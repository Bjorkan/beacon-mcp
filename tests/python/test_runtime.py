import json
from pathlib import Path

import pytest

from beacon_mcp.client import GatewayError
from beacon_mcp.runtime import CONTRACTS, Registry
from tests.python.conftest import gateway, rpc


async def test_sdk_discovery_and_frozen_tool_registration():
    registry = Registry()

    async def implementation(_client, _args):
        return {"items": ["not-duplicated-in-text"]}

    registry.register("beacon_list_iatas", implementation)
    async with gateway(registry=registry) as http:
        discovered = await rpc(http, "server/discover")
        assert "tools" in discovered["result"]["capabilities"]
        assert "extensions" not in discovered["result"]["capabilities"]
        listed = await rpc(http, "tools/list")
        assert listed["result"]["tools"] == [CONTRACTS["beacon_list_iatas"]]
        called = await rpc(http, "tools/call", name="beacon_list_iatas", arguments={})
        assert called["result"]["structuredContent"] == {"items": ["not-duplicated-in-text"]}
        assert called["result"]["content"] == [
            {"type": "text", "text": "Beacon returned 1 item(s)."}
        ]


def test_contract_copies_and_unique_inventory():
    frozen = json.loads(Path("tests/fixtures/tool-names.json").read_text())
    assert len(CONTRACTS) == len(frozen) == 51
    assert set(CONTRACTS) == set(frozen)


@pytest.mark.parametrize(
    "code,kind",
    [
        ("NOT_FOUND", "BeaconNotFoundError"),
        ("RESOURCE_EXHAUSTED", "BeaconRateLimitError"),
        ("UNAVAILABLE", "BeaconUpstreamError"),
    ],
)
async def test_classified_errors(code, kind):
    registry = Registry()

    async def failure(_client, _args):
        raise GatewayError(code, "redacted", status=503, retry_after="10")

    registry.register("beacon_get_server_info", failure)
    async with gateway(registry=registry) as http:
        payload = await rpc(http, "tools/call", name="beacon_get_server_info", arguments={})
    assert payload["result"]["isError"] is True
    assert payload["result"]["structuredContent"]["error"] == {
        "code": code,
        "type": kind,
        "status": 503,
        "retryAfter": "10",
    }


async def test_sdk_owns_malformed_call_parameters():
    async with gateway() as http:
        missing_name = await rpc(http, "tools/call", arguments={})
        wrong_arguments = await rpc(http, "tools/call", name="beacon_list_iatas", arguments=[])
    assert missing_name["error"]["code"] == -32602
    assert wrong_arguments["error"]["code"] == -32602


async def test_json_schema_validation_and_pydantic_integer_arguments():
    registry = Registry()
    seen = []

    async def implementation(_client, args):
        assert type(args["limit"]) is int
        assert all(type(value) is int for value in args["routeTypes"])
        seen.append(args)
        return {"items": []}

    registry.register("beacon_search_packets", implementation)
    async with gateway(registry=registry) as http:
        valid = await rpc(
            http,
            "tools/call",
            name="beacon_search_packets",
            arguments={"limit": 1.0, "routeTypes": [1.0, 2]},
        )
        invalid = await rpc(
            http, "tools/call", name="beacon_search_packets", arguments={"limit": 1.5}
        )
    assert not valid["result"]["isError"]
    assert invalid["result"]["isError"]
    assert invalid["result"]["content"][0]["text"].startswith("Invalid arguments:")
    assert len(seen) == 1


async def test_sdk_handles_unexpected_handler_exceptions():
    registry = Registry()

    async def failure(_client, _args):
        raise RuntimeError("synthetic detail must stay off the wire")

    registry.register("beacon_get_server_info", failure)
    async with gateway(registry=registry) as http:
        response = await rpc(http, "tools/call", name="beacon_get_server_info", arguments={})
    assert response["error"]["code"] == -32603
    assert "synthetic detail" not in json.dumps(response)
