import json
from typing import Never

import httpx
import pytest
from conftest import gateway, rpc

from beacon_mcp.resources import parse_uri

ID = "123e4567-e89b-42d3-a456-426614174000"


@pytest.mark.parametrize(
    "uri",
    [
        f"beacon://nodes/{ID.upper()}",
        f"beacon://observers/{ID}",
        "beacon://packets/AABBCCDDEEFF0011",
        "beacon://channels/25",
        "beacon://traces/ABCDEF01",
        "beacon://iatas/arn",
        "beacon://regions/3",
    ],
)
async def test_all_seven_resources_fresh_and_canonical(uri: str) -> None:
    calls = 0

    def upstream(request: httpx.Request) -> httpx.Response:
        nonlocal calls
        calls += 1
        return httpx.Response(200, json={"path": request.url.path, "sequence": calls})

    async with gateway(upstream) as http:
        templates = await rpc(http, "resources/templates/list")
        assert len(templates["result"]["resourceTemplates"]) == 7
        for sequence in (1, 2):
            result = await rpc(http, "resources/read", uri=uri)
            content = result["result"]["contents"][0]
            assert content["mimeType"] == "application/json"
            assert json.loads(content["text"])["sequence"] == sequence
            assert parse_uri(content["uri"]) == parse_uri(uri)
    assert calls == 2


@pytest.mark.parametrize(
    "uri",
    [
        "beacon://nodes/../admin",
        f"beacon://nodes/{ID}/extra",
        "beacon://nodes/00000000-0000-0000-0000-000000000000",
        "beacon://channels/0001",
        "beacon://channels/0",
        "beacon://iatas/ARN?x=1",
        "beacon://packets/%2e%2e",
        "beacon://scopes/foo",
    ],
)
async def test_injected_uris_rejected(uri: str) -> None:
    assert parse_uri(uri) is None

    def forbidden(_request: httpx.Request) -> Never:
        pytest.fail("unsafe URI reached Beacon")

    async with gateway(forbidden) as http:
        assert "error" in await rpc(http, "resources/read", uri=uri)


@pytest.mark.parametrize(
    "name,args,contains",
    [
        ("beacon_node_diagnostics", {"id": ID}, "beacon_diagnose_node"),
        ("beacon_region_comparison", {"iataA": "arn", "iataB": "mmx"}, "iataA=ARN and iataB=MMX"),
        ("beacon_route_review", {"iata": "arn", "pathKey": "AB" * 16}, "pathKey " + "ab" * 16),
    ],
)
async def test_all_three_prompts(name: str, args: dict[str, str], contains: str) -> None:
    def forbidden(_request: httpx.Request) -> Never:
        pytest.fail("prompts must not contact Beacon")

    async with gateway(forbidden) as http:
        assert len((await rpc(http, "prompts/list"))["result"]["prompts"]) == 3
        result = await rpc(http, "prompts/get", name=name, arguments=args)
    message = result["result"]["messages"][0]
    assert message["role"] == "user" and contains in message["content"]["text"]
