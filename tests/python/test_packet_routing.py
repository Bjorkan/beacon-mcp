import json
from pathlib import Path
from typing import Never, TypedDict

import httpx
import pytest
from pydantic import TypeAdapter
from conftest import gateway, rpc

from beacon_mcp.handlers import packet, routing

CASES: list[tuple[str, dict[str, object], str]] = [
    (
        "beacon_search_packets",
        {"payloadTypeName": "reserved", "hashPrefix": "aBcDe", "iatas": ["arn"]},
        "/api/v1/packets",
    ),
    ("beacon_get_packet", {"hash": "AABBCCDDEEFF0011"}, "/api/v1/packets/aabbccddeeff0011"),
    ("beacon_search_messages", {"channelId": 7}, "/api/v1/messages"),
    ("beacon_list_channels", {"keyKnown": True}, "/api/v1/channels"),
    ("beacon_get_channel_messages", {"channelId": 7}, "/api/v1/channels/7/messages"),
    ("beacon_list_routes", {"limit": 1, "cursor": 500, "cursorId": 5}, "/api/v1/routes"),
    ("beacon_search_routes", {"from": "AB", "to": "abcdef"}, "/api/v1/routes/search"),
    (
        "beacon_find_cross_iata_routes",
        {"fromHash": "ab", "toHash": "cd", "fromIata": "arn", "toIata": "mmx"},
        "/api/v1/routes/cross",
    ),
    ("beacon_search_traces", {"limit": 1, "type": "PING"}, "/api/v1/traces"),
    ("beacon_get_trace", {"tag": "AABBCCDD"}, "/api/v1/traces/aabbccdd"),
]


@pytest.mark.parametrize("name,args,path", CASES)
async def test_all_ten_and_cursors(name: str, args: dict[str, object], path: str) -> None:
    seen: list[httpx.Request] = []

    def upstream(request: httpx.Request) -> httpx.Response:
        seen.append(request)
        assert request.url.path == path
        if name in ("beacon_list_routes", "beacon_search_traces"):
            data = [{"lastSeen": 1000, "id": 7, "lastHeardAt": 123, "traceTag": "aabbccdd"}]
        elif name in ("beacon_search_routes", "beacon_find_cross_iata_routes"):
            data = [{"hops": ["ab", "abcdef"]}]
        elif name in ("beacon_get_packet", "beacon_get_trace"):
            data = {"detail": True}
        else:
            data = {"items": [1], "hasMore": True, "nextCursor": 42, "nextPageCursor": "opaque"}
        return httpx.Response(200, json=data)

    async with gateway(upstream) as http:
        result = (await rpc(http, "tools/call", name=name, arguments=args))["result"]
    assert result["isError"] is False and len(seen) == 1
    data = result["structuredContent"]
    if name == "beacon_search_packets":
        assert data["pagination"]["nextCursor"] == {"cursor": 42}
        assert seen[0].url.params["payloadTypes"] == "12,13,14"
    if name == "beacon_list_channels":
        assert data["pagination"]["nextCursor"] == {"pageCursor": "opaque"}
    if name == "beacon_search_messages":
        assert seen[0].url.params["channelID"] == "7"
    if name == "beacon_list_routes":
        assert data["pagination"]["nextCursor"] == {"cursor": 1000, "cursorId": 7}
    if name == "beacon_search_traces":
        assert data["pagination"]["nextCursor"] == {"cursor": 123, "cursorTag": "aabbccdd"}


class NegativeCase(TypedDict):
    name: str
    arguments: dict[str, object]


# Validate fixture records before selecting tool-specific negative tests.
_negative_cases = TypeAdapter(list[NegativeCase]).validate_python(
    json.loads(Path("tests/fixtures/invalid-tool-cases.json").read_text())
)
NEGATIVE: list[NegativeCase] = [
    case for case in _negative_cases if case["name"] in {*packet.OPERATIONS, *routing.OPERATIONS}
]


@pytest.mark.parametrize("case", NEGATIVE)
async def test_packet_routing_negative_arguments_rejected(case: NegativeCase) -> None:
    def forbidden(_request: httpx.Request) -> Never:
        pytest.fail("invalid request reached upstream")

    async with gateway(forbidden) as http:
        result = (await rpc(http, "tools/call", name=case["name"], arguments=case["arguments"]))[
            "result"
        ]
    assert result["isError"] is True
    assert result["content"][0]["text"]
    error = result.get("structuredContent", {}).get("error")
    if error is not None:
        assert error["code"] == "INVALID_ARGUMENT"


@pytest.mark.parametrize(
    "name,args",
    [
        ("beacon_search_packets", {"scopes": ["å" * 128]}),
        ("beacon_search_packets", {"pageCursor": "å" * 257}),
        ("beacon_search_traces", {"region": "å" * 128}),
    ],
)
async def test_utf8_packet_routing_bounds(name: str, args: dict[str, object]) -> None:
    def forbidden(_request: httpx.Request) -> Never:
        pytest.fail("oversized UTF-8 filter reached Beacon")

    async with gateway(forbidden) as http:
        assert (await rpc(http, "tools/call", name=name, arguments=args))["result"]["isError"]
