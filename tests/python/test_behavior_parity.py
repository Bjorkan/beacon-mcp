"""Language-independent assertions against the pre-migration synthetic baseline."""

import hashlib
import json
from pathlib import Path
from typing import NotRequired, TypedDict

import httpx

from tests.python.conftest import gateway, rpc
import pytest
from pydantic import TypeAdapter


class UpstreamRequest(TypedDict):
    path: str
    query: dict[str, str]


class Scenario(TypedDict, total=False):
    status: int
    body: object


class ExpectedResult(TypedDict):
    isError: bool
    content: list[dict[str, object]]
    structuredContent: NotRequired[object]


class ReferenceCase(TypedDict):
    name: str
    arguments: dict[str, object]
    expected: ExpectedResult
    requests: list[UpstreamRequest]
    scenario: NotRequired[Scenario]


class BehaviorBaseline(TypedDict):
    contractHashes: dict[str, str]
    upstream: dict[str, object]
    cases: list[ReferenceCase]


# The immutable fixture is untrusted JSON to the type checker. Validate its
# structure once rather than spreading Unknown or unsafe casts across assertions.
BASELINE = TypeAdapter(BehaviorBaseline).validate_python(
    json.loads(Path("tests/fixtures/behavior-baseline.json").read_text())
)
RESULT = TypeAdapter(dict[str, object])


def test_frozen_contract_bytes() -> None:
    for name, digest in BASELINE["contractHashes"].items():
        assert (
            hashlib.sha256(Path("src/beacon_mcp/contracts", name).read_bytes()).hexdigest()
            == digest
        )


@pytest.mark.parametrize("case", BASELINE["cases"], ids=lambda case: case["name"])
async def test_reference_result_and_upstream_requests(case: ReferenceCase) -> None:
    seen: list[UpstreamRequest] = []

    def upstream(request: httpx.Request) -> httpx.Response:
        seen.append({"path": request.url.path, "query": dict(request.url.params)})
        scenario = case.get("scenario", {})
        status = scenario.get("status", 200)
        return httpx.Response(
            status,
            headers={"Retry-After": "10"} if status >= 400 else {},
            json=scenario.get("body", BASELINE["upstream"].get(request.url.path, {})),
        )

    async with gateway(upstream) as http:
        raw = await rpc(http, "tools/call", name=case["name"], arguments=case["arguments"])
    result = RESULT.validate_python(raw["result"])
    # SDK-owned resultType/serverInfo stamps deliberately follow SDK v2.
    actual = {
        key: result[key] for key in ("isError", "content", "structuredContent") if key in result
    }
    if case["expected"]["isError"] and "scenario" not in case:
        # Deliberate SDK/JSON Schema validation change: retain rejection and no GETs,
        # not the reference implementation's hand-formatted validation messages.
        assert actual["isError"] is True and actual["content"]
        # Keep the original parity invariant: a present error field must
        # be a valid object with the expected classification; never skip it.
        payload = RESULT.validate_python(actual.get("structuredContent", {}))
        error = payload.get("error")
        if error is not None:
            assert RESULT.validate_python(error).get("code") == "INVALID_ARGUMENT"
    else:
        assert actual == case["expected"]
    assert sorted(seen, key=lambda value: json.dumps(value, sort_keys=True)) == case["requests"]
