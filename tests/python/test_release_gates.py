import asyncio
import importlib.util
import json
import logging
from pathlib import Path

import httpx
import pytest
from conftest import gateway, rpc

from beacon_mcp.__main__ import SafeDiagnostics
from beacon_mcp.client import BeaconClient
from beacon_mcp.config import Config
from beacon_mcp.handlers.composite import diagnose_node

EXPECTED_NAMES = json.loads(Path("tests/fixtures/tool-names.json").read_text())
ID = "123e4567-e89b-42d3-a456-426614174000"


def test_diagnostics_cannot_disclose_exception_headers_or_payloads():
    record = logging.LogRecord(
        "mcp.server.runner",
        logging.ERROR,
        "",
        0,
        "payload=%s",
        ("sensitive",),
        (ValueError, ValueError("sensitive traceback"), None),
    )
    record.exc_text = "sensitive traceback"
    assert SafeDiagnostics().filter(record)
    assert "sensitive" not in logging.Formatter("%(message)s").format(record)
    assert record.exc_info is None and record.exc_text is None


async def test_composite_failure_cancels_siblings_and_preserves_classification():
    cancelled = asyncio.Event()

    async def upstream(request):
        if request.url.path.endswith("/activity"):
            return httpx.Response(
                503, headers={"Retry-After": "10"}, json={"error": {"code": "deadline_exceeded"}}
            )
        try:
            await asyncio.Event().wait()
        finally:
            cancelled.set()

    from beacon_mcp.client import GatewayError

    client = BeaconClient(
        Config.from_env({"BEACON_BASE_URL": "https://beacon.test"}),
        transport=httpx.MockTransport(upstream),
    )
    try:
        with pytest.raises(GatewayError) as caught:
            await diagnose_node(client, {"id": ID})
        assert caught.value.code == "DEADLINE_EXCEEDED"
        assert cancelled.is_set()
    finally:
        await client.aclose()


async def test_inventory_order_and_forbidden_events():
    async with gateway() as http:
        assert [
            tool["name"] for tool in (await rpc(http, "tools/list"))["result"]["tools"]
        ] == EXPECTED_NAMES
        assert "events" not in (await rpc(http, "server/discover"))["result"]["capabilities"]
        assert (await rpc(http, "events/subscribe"))["error"]["code"] == -32601


async def test_invalid_calendar_still_rejected_with_beacon_classification():
    async with gateway() as http:
        result = (
            await rpc(
                http,
                "tools/call",
                name="beacon_search_packets",
                arguments={"since": "2026-02-01T00:00:00Z", "until": "2026-02-30T00:00:00Z"},
            )
        )["result"]
    assert result["isError"]
    assert result["structuredContent"]["error"] == {
        "code": "INVALID_ARGUMENT",
        "type": "BeaconInputError",
    }


def test_openapi_sync_requires_an_immutable_commit(monkeypatch):
    spec = importlib.util.spec_from_file_location("sync_openapi", "scripts/sync-openapi.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    for revision in ("main", "dev", "v2.2.0", "abcdef"):
        monkeypatch.setattr("sys.argv", ["sync-openapi.py", revision])
        with pytest.raises(SystemExit):
            module.main()
