import asyncio
import json

import pytest
from mcp import ClientSession
from mcp.client.streamable_http import streamable_http_client
from mcp.server.subscriptions import ListenHandler

from beacon_mcp.app import MAX_MCP_REQUEST_BYTES
from tests.python.helpers import gateway, rpc
from tests.python.subscription_helpers import ID, SUBSCRIPTION_ID, event, listen, running_gateway


async def test_direct_sdk_handler_ack_and_exact_canonical_uri_delivery():
    canonical = f"beacon://nodes/{ID}"
    different_case = f"beacon://nodes/{ID.upper()}"
    unknown = "beacon://regions/3"
    async with running_gateway() as (http, app):
        handler = app.state.server.get_request_handler("subscriptions/listen")
        assert isinstance(handler.handler, ListenHandler)
        async with listen(http, [different_case, unknown, canonical]) as response:
            lines = response.aiter_lines()
            ack = await event(lines, "notifications/subscriptions/acknowledged")
            assert ack["params"]["notifications"]["resourceSubscriptions"] == [
                different_case,
                unknown,
                canonical,
            ]
            assert ack["params"]["_meta"][SUBSCRIPTION_ID] == "listen-1"
            app.state.live.received("observerStatus", {"observerId": ID})
            app.state.live.received("nodeUpdate", {"nodeId": ID, "content": "discarded"})
            update = await event(lines, "notifications/resources/updated")
            assert update["params"]["uri"] == canonical
            assert update["params"]["_meta"][SUBSCRIPTION_ID] == "listen-1"
            assert "discarded" not in json.dumps(update)
            # Close using the SDK public API, then verify no alias/observer event arrived.
            app.state.listener.close()
            remaining = [line async for line in lines if line.startswith("data:")]
            assert all('"notifications/resources/updated"' not in line for line in remaining)


async def test_sdk_stream_limit_releases_on_disconnect_and_closes_gracefully():
    uri = f"beacon://nodes/{ID}"
    async with running_gateway(max_streams=1) as (http, app):
        async with listen(http, [uri]) as first:
            await event(first.aiter_lines(), "notifications/subscriptions/acknowledged")
            async with listen(http, [uri]) as overflow:
                assert (
                    json.loads(await overflow.aread())["error"]["message"]
                    == "Subscription limit reached"
                )
        # Only public behavior: a disconnected listener must release its SDK slot.
        for _ in range(100):
            async with listen(http, [uri], request_id=42) as next_stream:
                if next_stream.status_code != 200:
                    await next_stream.aread()
                else:
                    lines = next_stream.aiter_lines()
                    ack = await event(lines, "notifications/subscriptions/acknowledged")
                    assert ack["params"]["_meta"][SUBSCRIPTION_ID] == 42
                    app.state.listener.close()
                    tail = [
                        json.loads(line[5:]) async for line in lines if line.startswith("data:")
                    ]
                    assert tail[-1]["result"]["_meta"][SUBSCRIPTION_ID] == 42
                    break
            await asyncio.sleep(0.01)
        else:
            pytest.fail("SDK did not release the disconnected stream")


@pytest.mark.parametrize("traffic", [False, True])
async def test_sdk_honors_more_than_twenty_uris_but_publication_is_opt_in(traffic: bool) -> None:
    uris = ["beacon://packets/aabbccddeeff0011", *[f"beacon://channels/{i}" for i in range(1, 30)]]
    async with running_gateway(traffic=traffic) as (http, app):
        async with listen(http, uris) as response:
            lines = response.aiter_lines()
            ack = await event(lines, "notifications/subscriptions/acknowledged")
            assert ack["params"]["notifications"]["resourceSubscriptions"] == uris
            app.state.live.received("packetObservation", {"packetHash": "AABBCCDDEEFF0011"})
            if traffic:
                assert (await event(lines, "notifications/resources/updated"))["params"][
                    "uri"
                ] == uris[0]
            app.state.listener.close()
            remaining = [line async for line in lines if line.startswith("data:")]
            assert all('"notifications/resources/updated"' not in line for line in remaining)


@pytest.mark.parametrize("version", ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"])
async def test_sdk_native_legacy_initialize(version: str) -> None:
    async with gateway() as http:
        response = await http.post(
            "/",
            headers={
                "Accept": "application/json, text/event-stream",
                "MCP-Protocol-Version": version,
            },
            json={
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": version,
                    "capabilities": {},
                    "clientInfo": {"name": "test", "version": "1"},
                },
            },
        )
        assert response.json()["result"]["protocolVersion"] == version
        assert not response.json()["result"]["capabilities"]["resources"].get("subscribe")
        assert "mcp-session-id" not in response.headers


async def test_disabled_and_invalid_protocol_requests():
    async with gateway() as http:
        discovered = (await rpc(http, "server/discover"))["result"]
        assert not discovered["capabilities"]["resources"].get("subscribe")
        for method in ("resources/subscribe", "subscriptions/listen", "invented/method"):
            assert "error" in await rpc(http, method)
        malformed = await http.post(
            "/", headers={"Accept": "application/json, text/event-stream"}, content="{"
        )
        assert malformed.status_code >= 400


async def test_sdk_request_body_limit_bounds_subscription_filters():
    async with gateway() as http:
        response = await http.post(
            "/",
            headers={
                "Accept": "application/json, text/event-stream",
                "MCP-Protocol-Version": "2026-07-28",
                "Mcp-Method": "subscriptions/listen",
            },
            content=b" " * (MAX_MCP_REQUEST_BYTES + 1),
        )
        assert response.status_code == 413


async def test_sdk_native_http_methods_and_client_disconnect():
    async with running_gateway() as (http, app):
        # Modern transport rejects standalone GET and DELETE itself.
        for method in ("GET", "DELETE"):
            response = await http.request(
                method, "/", headers={"MCP-Protocol-Version": "2026-07-28"}
            )
            assert response.status_code == 405
        # SDK's handshake-era transport exposes its own standalone GET SSE stream.
        async with http.stream(
            "GET",
            "/",
            headers={"MCP-Protocol-Version": "2025-11-25", "Accept": "text/event-stream"},
        ) as response:
            assert response.status_code == 200
            assert response.headers["content-type"].startswith("text/event-stream")
        response = await http.delete("/", headers={"MCP-Protocol-Version": "2025-11-25"})
        assert response.status_code == 405  # Stateless sessions cannot be deleted.
        assert not app.state.beacon.http.is_closed


async def test_official_sdk_client_discovers_and_calls_gateway():
    async with running_gateway() as (http, _app):
        async with streamable_http_client(str(http.base_url)) as streams:
            async with ClientSession(*streams, read_timeout_seconds=3) as session:
                await session.discover()
                assert session.server_info is not None
                assert session.server_info.name == "beacon-mcp"
                tools = await session.list_tools()
                assert len(tools.tools) == 51
                # The local mock backend returns 404; SDK results retain Beacon metadata.
                result = await session.call_tool("beacon_get_node", {"id": ID})
                assert result.is_error
                assert result.structured_content["error"]["code"] == "NOT_FOUND"
                templates = await session.list_resource_templates()
                assert len(templates.resource_templates) == 7
                prompts = await session.list_prompts()
                assert len(prompts.prompts) == 3
                prompt = await session.get_prompt("beacon_node_diagnostics", {"id": ID})
                assert prompt.messages
