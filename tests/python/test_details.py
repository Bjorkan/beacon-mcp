from typing import Never

import httpx
import pytest

from tests.python.conftest import gateway, rpc

DETAIL_CASES: list[tuple[str, dict[str, object], str]] = [
    ("beacon_list_brokers", {}, "/api/v1/brokers"),
    ("beacon_get_channel", {"id": "7"}, "/api/v1/channels/7"),
    ("beacon_get_iata", {"id": "arn"}, "/api/v1/iatas/ARN"),
    ("beacon_get_iata_border", {"id": "arn"}, "/api/v1/iatas/ARN/border"),
    (
        "beacon_list_observer_directory",
        {"iatas": ["arn"]},
        "/api/v1/observers/directory",
    ),
    ("beacon_get_region", {"id": "3"}, "/api/v1/regions/3"),
    ("beacon_get_scope", {"id": "#Sweden"}, "/api/v1/scopes/#Sweden"),
]


@pytest.mark.parametrize("name,args,path", DETAIL_CASES)
async def test_all_seven_detail_reads(name: str, args: dict[str, object], path: str) -> None:
    def upstream(request: httpx.Request) -> httpx.Response:
        assert request.url.path == path
        if name == "beacon_list_brokers":
            return httpx.Response(200, json=[{"id": 1}])
        return httpx.Response(
            200,
            json={
                "items": [1],
                "hasMore": True,
                "nextCursor": 123,
                "coverage": {"complete": True},
                "windowStart": 1000,
                "observerTypes": ["mqtt"],
                "maxObservationCount": 100,
            },
        )

    async with gateway(upstream) as http:
        result = (await rpc(http, "tools/call", name=name, arguments=args))["result"]
    assert not result["isError"]
    if name == "beacon_list_observer_directory":
        data = result["structuredContent"]
        assert data["pagination"]["nextCursor"] == {"cursor": 123}
        assert data["coverage"] == {"complete": True} and data["windowStart"] == 1000


@pytest.mark.parametrize("id", ["..", ".", "../admin", "%23scope", "foo/bar", "foo\\bar"])
async def test_scope_path_restrictions(id: str) -> None:
    def forbidden(_request: httpx.Request) -> Never:
        pytest.fail("path injection reached upstream")

    async with gateway(forbidden) as http:
        result = await rpc(http, "tools/call", name="beacon_get_scope", arguments={"id": id})
    assert result["result"]["isError"]


@pytest.mark.parametrize("sort", ["traffic", "name"])
async def test_directory_continuation_preserves_returned_window(sort: str) -> None:
    requests: list[dict[str, str]] = []
    start_ms, end_ms = 1794009600123, 1794096000987

    def upstream(request: httpx.Request) -> httpx.Response:
        params = dict(request.url.params)
        requests.append(params)
        assert params["iatas"] == "ARN" and params["sort"] == sort
        assert params["limit"] == "2"
        if "cursor" in params:
            # Beacon requires the original window with every nonzero cursor.
            assert params["cursor"] == "2"
            assert params["since"] == str(start_ms)
            assert params["until"] == str(end_ms)
            return httpx.Response(
                200,
                json={
                    "items": [{"id": "observer-c"}],
                    "hasMore": False,
                    "windowStart": start_ms,
                    "windowEnd": end_ms,
                },
            )
        return httpx.Response(
            200,
            json={
                "items": [{"id": "observer-a"}, {"id": "observer-b"}],
                "hasMore": True,
                "nextCursor": 2,
                "windowStart": start_ms,
                "windowEnd": end_ms,
            },
        )

    async with gateway(upstream) as http:
        first = (
            await rpc(
                http,
                "tools/call",
                name="beacon_list_observer_directory",
                arguments={"iatas": ["arn"], "sort": sort, "limit": 2},
            )
        )["result"]["structuredContent"]
        second = (
            await rpc(
                http,
                "tools/call",
                name="beacon_list_observer_directory",
                arguments={
                    "iatas": ["arn"],
                    "sort": sort,
                    "limit": 2,
                    **first["pagination"]["nextCursor"],
                    "since": "2026-11-07T00:00:00.123Z",
                    "until": "2026-11-08T00:00:00.987Z",
                },
            )
        )["result"]
    assert not second["isError"]
    assert second["structuredContent"]["items"] == [{"id": "observer-c"}]
    assert second["structuredContent"]["pagination"] == {"hasMore": False}
    assert len(requests) == 2
