import httpx
import pytest
from conftest import gateway, rpc

DETAIL_CASES = [
    ("beacon_list_brokers", {}, "/api/v1/brokers"),
    ("beacon_get_channel", {"id": "7"}, "/api/v1/channels/7"),
    ("beacon_get_iata", {"id": "arn"}, "/api/v1/iatas/ARN"),
    ("beacon_get_iata_border", {"id": "arn"}, "/api/v1/iatas/ARN/border"),
    (
        "beacon_list_observer_directory",
        {"iatas": ["arn"], "cursor": 12},
        "/api/v1/observers/directory",
    ),
    ("beacon_get_region", {"id": "3"}, "/api/v1/regions/3"),
    ("beacon_get_scope", {"id": "#Sweden"}, "/api/v1/scopes/#Sweden"),
]


@pytest.mark.parametrize("name,args,path", DETAIL_CASES)
async def test_all_seven_detail_reads(name, args, path):
    def upstream(request):
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
async def test_scope_path_restrictions(id):
    def forbidden(_):
        pytest.fail("path injection reached upstream")

    async with gateway(forbidden) as http:
        result = await rpc(http, "tools/call", name="beacon_get_scope", arguments={"id": id})
    assert result["result"]["isError"]
