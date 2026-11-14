"""Language-independent assertions against the pre-migration synthetic baseline."""

import hashlib
import json
from pathlib import Path

import httpx
import pytest
from conftest import gateway, rpc

BASELINE = json.loads(Path("tests/fixtures/behavior-baseline.json").read_text())


def test_frozen_contract_bytes():
    for name, digest in BASELINE["contractHashes"].items():
        assert (
            hashlib.sha256(Path("src/beacon_mcp/contracts", name).read_bytes()).hexdigest()
            == digest
        )


@pytest.mark.parametrize("case", BASELINE["cases"], ids=lambda case: case["name"])
async def test_reference_result_and_upstream_requests(case):
    seen = []

    def upstream(request):
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
    result = raw["result"]
    # SDK-owned resultType/serverInfo stamps deliberately follow SDK v2.
    actual = {
        key: result[key] for key in ("isError", "content", "structuredContent") if key in result
    }
    if case["expected"]["isError"] and "scenario" not in case:
        # Deliberate SDK/JSON Schema validation change: retain rejection and no GETs,
        # not the reference implementation's hand-formatted validation messages.
        assert actual["isError"] is True and actual["content"]
        error = actual.get("structuredContent", {}).get("error")
        if error is not None:
            assert error["code"] == "INVALID_ARGUMENT"
    else:
        assert actual == case["expected"]
    assert sorted(seen, key=lambda value: json.dumps(value, sort_keys=True)) == case["requests"]
