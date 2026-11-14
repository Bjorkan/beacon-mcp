import httpx
import pytest

from beacon_mcp.handlers.core import OPERATIONS
from beacon_mcp.runtime import CONTRACTS
from tests.python.conftest import gateway, rpc

ID = "123e4567-e89b-42d3-a456-426614174000"


@pytest.mark.parametrize("name", OPERATIONS)
async def test_all_nine_core_tools(name):
    seen = []

    def upstream(request):
        seen.append(request)
        path = request.url.path
        if path in ("/api/v1/iatas", "/api/v1/regions", "/api/v1/scopes"):
            return httpx.Response(200, json=[{"value": 1}, {"value": 2}])
        if path in ("/api/v1/nodes", "/api/v1/observers"):
            return httpx.Response(
                200,
                json={
                    "items": [{"id": ID}],
                    "hasMore": True,
                    "nextCursor": 1000,
                    "nextPageCursor": "v1:exact",
                },
            )
        return httpx.Response(200, json={"id": ID, "points": [], "serverVersion": "2.2.0"})

    args = {"id": ID} if "id" in CONTRACTS[name]["inputSchema"]["properties"] else {}
    async with gateway(upstream) as http:
        result = (await rpc(http, "tools/call", name=name, arguments=args))["result"]
    assert result["isError"] is False and len(seen) == 1
    assert result["content"][0]["text"].startswith("Beacon")


async def test_cursors_normalization_and_scope_offset():
    seen = []

    def upstream(request):
        seen.append(request)
        return httpx.Response(
            200,
            json=["#a", "#b", "#c"]
            if request.url.path.endswith("scopes")
            else {
                "items": [1],
                "hasMore": True,
                "nextCursor": 1000,
                "nextPageCursor": "v1:opaque",
                "sort": "name",
                "order": "asc",
            },
        )

    async with gateway(upstream) as http:
        node = (
            await rpc(
                http,
                "tools/call",
                name="beacon_search_nodes",
                arguments={"iatas": ["arn"], "limit": 1},
            )
        )["result"]["structuredContent"]
        scope = (
            await rpc(
                http, "tools/call", name="beacon_list_scopes", arguments={"cursor": 2, "limit": 2}
            )
        )["result"]["structuredContent"]
    assert seen[0].url.params["iatas"] == "ARN"
    assert node["pagination"]["nextCursor"] == {"cursor": 1000, "pageCursor": "v1:opaque"}
    assert node["sort"] == "name" and scope == {"items": ["#c"], "pagination": {"hasMore": False}}


@pytest.mark.parametrize(
    "name,args",
    [
        ("beacon_search_nodes", {"type": 9}),
        ("beacon_search_nodes", {"region": "se", "regionId": 1}),
        ("beacon_search_nodes", {"cursor": 1, "sort": "name"}),
        ("beacon_search_observers", {"type": " "}),
        ("beacon_get_observer_activity", {"id": ID, "range": "72h", "interval": "5m"}),
    ],
)
async def test_invalid_core_never_contacts_beacon(name, args):
    def forbidden(_):
        pytest.fail("invalid input reached upstream")

    async with gateway(forbidden) as http:
        assert (await rpc(http, "tools/call", name=name, arguments=args))["result"]["isError"]


@pytest.mark.parametrize(
    "name,args",
    [
        ("beacon_search_nodes", {"region": "å" * 128}),
        ("beacon_search_nodes", {"pageCursor": "å" * 257}),
        ("beacon_search_observers", {"type": "å" * 128}),
    ],
)
async def test_utf8_byte_bounds_match_reference(name, args):
    def forbidden(_):
        pytest.fail("oversized UTF-8 filter reached Beacon")

    async with gateway(forbidden) as http:
        assert (await rpc(http, "tools/call", name=name, arguments=args))["result"]["isError"]
