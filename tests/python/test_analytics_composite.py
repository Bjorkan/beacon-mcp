import json
from pathlib import Path
from typing import Any, Never, TypedDict

import httpx
import mcp.types as mcp_types
import pytest
from pydantic import TypeAdapter

from beacon_mcp.runtime import CONTRACTS, default_registry
from tests.python.helpers import gateway, rpc


class NegativeCase(TypedDict):
    name: str
    arguments: dict[str, object]


NEGATIVE_CASES: list[NegativeCase] = TypeAdapter(list[NegativeCase]).validate_python(
    json.loads(Path("tests/fixtures/invalid-tool-cases.json").read_text())
)

ID: str = "123e4567-e89b-42d3-a456-426614174000"
B: str = "123e4567-e89b-42d3-a456-426614174001"


@pytest.mark.parametrize(
    "name,args,calls",
    [
        ("beacon_get_top_advertisers", {"sort": "direct", "limit": 1}, 1),
        ("beacon_get_network_overview", {"iatas": ["arn"]}, 1),
        (
            "beacon_get_network_series",
            {"since": "2026-11-01T00:00:00Z", "until": "2026-11-02T00:00:00Z"},
            1,
        ),
        (
            "beacon_compare_observers",
            {
                "observerA": ID,
                "observerB": B,
                "since": "2026-11-01T00:00:00Z",
                "until": "2026-11-02T00:00:00Z",
            },
            1,
        ),
        ("beacon_diagnose_node", {"id": ID, "range": "48h", "neighborLimit": 2}, 3),
        ("beacon_compare_regions", {"iataA": "arn", "iataB": "mmx"}, 2),
    ],
)
async def test_analytics_and_bounded_composites(
    name: str, args: dict[str, object], calls: int
) -> None:
    http: httpx.AsyncClient
    seen: list[httpx.Request] = []

    def upstream(request: httpx.Request) -> httpx.Response:
        seen.append(request)
        array: bool = request.url.path.endswith(("neighbors", "top-advertisers"))
        return httpx.Response(
            200, json=[{"id": i} for i in range(3)] if array else {"totalPackets": 12, "points": []}
        )

    async with gateway(upstream) as http:
        result: dict[str, Any] = (await rpc(http, "tools/call", name=name, arguments=args))[
            "result"
        ]
    assert not result["isError"], result
    assert len(seen) == calls
    assert all(
        request.method == "GET" and request.url.path.startswith("/api/v1/") for request in seen
    )
    if name == "beacon_diagnose_node":
        assert result["structuredContent"]["neighborsTruncated"]
        assert len(result["structuredContent"]["neighbors"]) == 2


def test_complete_inventory() -> None:
    tool: mcp_types.Tool
    expected: list[str] = TypeAdapter(list[str]).validate_python(
        json.loads(Path("tests/fixtures/tool-names.json").read_text())
    )
    registered: list[mcp_types.Tool] = default_registry().list_tools()
    assert len(registered) == 51
    assert {tool.name for tool in registered} == set(expected)
    for tool in registered:
        assert tool.model_dump(by_alias=True, exclude_none=True) == CONTRACTS[tool.name]


@pytest.mark.parametrize("case", NEGATIVE_CASES)
async def test_all_thirty_negative_argument_fixtures_rejected(case: NegativeCase) -> None:
    http: httpx.AsyncClient

    def forbidden(_request: httpx.Request) -> Never:
        pytest.fail("invalid request reached upstream")

    async with gateway(forbidden) as http:
        result: dict[str, Any] = (
            await rpc(http, "tools/call", name=case["name"], arguments=case["arguments"])
        )["result"]
    assert result["isError"] is True
    assert result["content"][0]["text"]
    error: dict[str, Any] | None = result.get("structuredContent", {}).get("error")
    if error is not None:
        assert error["code"] == "INVALID_ARGUMENT"


async def test_analytics_utf8_region_bound() -> None:
    http: httpx.AsyncClient

    def forbidden(_request: httpx.Request) -> Never:
        pytest.fail("oversized UTF-8 region reached Beacon")

    async with gateway(forbidden) as http:
        result: dict[str, Any] = (
            await rpc(
                http,
                "tools/call",
                name="beacon_get_network_overview",
                arguments={"region": "å" * 128},
            )
        )["result"]
    assert result["isError"] and result["content"][0]["text"].endswith("invalid region slug")
