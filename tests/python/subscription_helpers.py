"""Real local HTTP server and SSE helpers shared by subscription/lifecycle tests."""

import asyncio
import json
import socket
from collections.abc import AsyncGenerator, AsyncIterator, Sequence
from contextlib import AbstractAsyncContextManager, asynccontextmanager
from typing import Any, cast

import httpx
import uvicorn
from starlette.applications import Starlette

from beacon_mcp.app import create_app
from beacon_mcp.client import BeaconClient
from beacon_mcp.config import Config
from tests.python.helpers import META

ID: str = "123e4567-e89b-42d3-a456-426614174000"
SUBSCRIPTION_ID: str = "io.modelcontextprotocol/subscriptionId"


@asynccontextmanager
async def running_gateway(
    max_streams: int = 2, traffic: bool = False
) -> AsyncGenerator[tuple[httpx.AsyncClient, Starlette]]:
    http: httpx.AsyncClient
    config: Config = Config.from_env(
        {
            "BEACON_BASE_URL": "http://127.0.0.1:9",
            "BEACON_SUBSCRIPTIONS_MAX_STREAMS": str(max_streams),
            "BEACON_SUBSCRIPTIONS_DEBOUNCE_MS": "0",
            "BEACON_SUBSCRIPTIONS_TRAFFIC_ENABLED": str(traffic).lower(),
        }
    )
    beacon: BeaconClient = BeaconClient(
        config, transport=httpx.MockTransport(lambda _: httpx.Response(404))
    )
    app: Starlette = create_app(config, client=beacon)
    sock: socket.socket = socket.socket()
    sock.bind(("127.0.0.1", 0))
    port: int = sock.getsockname()[1]
    sock.setblocking(False)
    server: uvicorn.Server = uvicorn.Server(
        uvicorn.Config(
            app, log_config=None, access_log=False, proxy_headers=False, timeout_graceful_shutdown=1
        )
    )
    task: asyncio.Task[None] = asyncio.create_task(server.serve(sockets=[sock]))
    try:
        async with asyncio.timeout(3):
            while not server.started:  # noqa: ASYNC110 - Uvicorn exposes a readiness flag
                await asyncio.sleep(0.005)
        if app.state.bridge_task:
            app.state.bridge_task.cancel()
            await asyncio.gather(app.state.bridge_task, return_exceptions=True)
        async with httpx.AsyncClient(base_url=f"http://127.0.0.1:{port}", timeout=3) as http:
            yield http, app
    finally:
        app.state.listener.close()
        server.should_exit = True
        await asyncio.wait_for(task, 4)
        sock.close()
    assert beacon.http.is_closed
    assert app.state.live.closed and not app.state.live.pending


def listen(
    http: httpx.AsyncClient, uris: Sequence[str], *, request_id: str | int = "listen-1"
) -> AbstractAsyncContextManager[httpx.Response]:
    return http.stream(
        "POST",
        "/",
        headers={
            "Accept": "application/json, text/event-stream",
            "MCP-Protocol-Version": "2026-07-28",
            "Mcp-Method": "subscriptions/listen",
        },
        json={
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "subscriptions/listen",
            "params": {"notifications": {"resourceSubscriptions": uris}, "_meta": META},
        },
    )


async def event(lines: AsyncIterator[str], method: str) -> dict[str, Any]:
    line: str
    async with asyncio.timeout(2):
        async for line in lines:
            if line.startswith("data:"):
                decoded: object = json.loads(line[5:].strip())
                if isinstance(decoded, dict) and all(
                    isinstance(key, str) for key in cast(dict[object, object], decoded)
                ):
                    value: dict[str, Any] = cast(dict[str, Any], decoded)
                    if value.get("method") == method:
                        return value
    raise AssertionError("stream ended before notification")
