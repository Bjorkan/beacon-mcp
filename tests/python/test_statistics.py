import httpx
import pytest

from beacon_mcp.handlers.statistics import OPERATIONS
from tests.python.conftest import gateway, rpc


@pytest.mark.parametrize("name", OPERATIONS)
async def test_all_eleven_statistics_and_bounds(name):
    seen = []

    def upstream(request):
        seen.append(request)
        assert request.url.params.get("iatas") == "ARN,MMX"
        assert request.extensions["timeout"]["read"] == 20
        return httpx.Response(
            200,
            json=[{"value": 1}, {"value": 2}] if OPERATIONS[name][1] == "array" else {"total": 2},
        )

    args = {"iatas": ["arn", "mmx"]}
    if OPERATIONS[name][1] == "array":
        args["limit"] = 1
    if OPERATIONS[name][1] == "object":
        args.update(since="2026-11-01T00:00:00Z", until="2026-11-02T00:00:00Z")
    if name == "beacon_get_radio_presets":
        args["preset"] = "910.525,62.5,7"
    async with gateway(upstream) as http:
        result = (await rpc(http, "tools/call", name=name, arguments=args))["result"]
    assert not result["isError"], result
    assert len(seen) == 1
    if OPERATIONS[name][1] == "array":
        assert result["structuredContent"] == {
            "items": [{"value": 1}],
            "pagination": {"hasMore": False, "truncated": True},
        }
    if name == "beacon_get_radio_presets":
        assert seen[0].url.params["preset"] == "910.525,62.5,7"
