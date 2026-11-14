from typing import Never

import httpx
import pytest

from beacon_mcp.handlers.activity import OPERATIONS
from tests.python.conftest import gateway, rpc

ID = "123e4567-e89b-42d3-a456-426614174000"
KEY = "0123456789abcdef0123456789abcdef"
CASES: list[tuple[str, dict[str, object], str]] = [
    (
        "beacon_backfill_messages",
        {"afterId": 2147483649, "region": "sverige"},
        "/api/v1/messages/backfill",
    ),
    ("beacon_get_node_activity", {"id": ID, "range": "48h"}, f"/api/v1/nodes/{ID}/activity"),
    ("beacon_get_node_neighbors", {"id": ID, "limit": 1}, f"/api/v1/nodes/{ID}/neighbors"),
    (
        "beacon_get_node_observations",
        {"id": ID, "cursor": 2147483649, "limit": 1},
        f"/api/v1/nodes/{ID}/observations",
    ),
    ("beacon_get_observer_adverts", {"id": ID}, f"/api/v1/observers/{ID}/adverts"),
    (
        "beacon_get_observer_telemetry",
        {"id": ID, "range": "24h"},
        f"/api/v1/observers/{ID}/telemetry",
    ),
    (
        "beacon_backfill_packets",
        {"afterObservationId": 2147483649, "iatas": ["arn", "mmx"]},
        "/api/v1/packets/backfill",
    ),
    (
        "beacon_get_route_observations",
        {"iata": "arn", "pathKey": KEY, "range": "24h"},
        f"/api/v1/routes/ARN/{KEY}/observations",
    ),
]


@pytest.mark.parametrize("name,args,path", CASES)
async def test_all_eight_activity_tools(name: str, args: dict[str, object], path: str) -> None:
    seen: list[httpx.Request] = []

    def upstream(request: httpx.Request) -> httpx.Response:
        seen.append(request)
        assert request.url.path == path
        shape = OPERATIONS[name][1]
        data = (
            [{"id": 9}, {"id": 8}]
            if shape == "array"
            else {
                "items": [{"id": 9}, {"id": 8}],
                "hasMore": True,
                "nextCursor": 8,
                "points": [],
                "matchAvailable": True,
            }
        )
        return httpx.Response(200, json=data)

    async with gateway(upstream) as http:
        result = (await rpc(http, "tools/call", name=name, arguments=args))["result"]
    assert not result["isError"], result
    if name == "beacon_get_node_observations":
        assert seen[0].url.params["cursor"] == "2147483649"
        assert result["structuredContent"]["pagination"]["nextCursor"] == {"cursor": 8}
        assert len(result["structuredContent"]["items"]) == 1
    if name == "beacon_backfill_packets":
        assert seen[0].url.params["afterObservationId"] == "2147483649"
        assert seen[0].url.params["iatas"] == "ARN,MMX"


@pytest.mark.parametrize(
    "args",
    [
        {"range": "24h", "since": "2026-11-01T00:00:00Z", "until": "2026-11-02T00:00:00Z"},
        {"range": "24h", "pageCursor": "opaque"},
        {"since": "2026-11-01T00:00:00Z"},
        {"range": "721h"},
    ],
)
async def test_route_window_restrictions(args: dict[str, object]) -> None:
    def forbidden(_request: httpx.Request) -> Never:
        pytest.fail("invalid route evidence window")

    async with gateway(forbidden) as http:
        result = await rpc(
            http,
            "tools/call",
            name="beacon_get_route_observations",
            arguments={"iata": "ARN", "pathKey": KEY, **args},
        )
    assert result["result"]["isError"]
