"""Typed shared ASGI/HTTPX fixtures for the frozen MCP contract tests."""

from collections.abc import AsyncGenerator, Awaitable, Callable
from contextlib import asynccontextmanager
from typing import Any, cast

import httpx
from asgi_lifespan import LifespanManager

from beacon_mcp.app import create_app
from beacon_mcp.client import BeaconClient
from beacon_mcp.config import Config
from beacon_mcp.runtime import Registry

# HTTPX supports both synchronous and asynchronous MockTransport callbacks.
MockHandler = (
    Callable[[httpx.Request], httpx.Response]
    | Callable[[httpx.Request], Awaitable[httpx.Response]]
)

META = {
    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
    "io.modelcontextprotocol/clientInfo": {"name": "parity-test", "version": "1"},
    "io.modelcontextprotocol/clientCapabilities": {},
}


def default_handler(_request: httpx.Request) -> httpx.Response:
    return httpx.Response(200, json={})


@asynccontextmanager
async def gateway(
    handler: MockHandler | None = None, registry: Registry | None = None, **env: str
) -> AsyncGenerator[httpx.AsyncClient]:
    config = Config.from_env(
        {"BEACON_BASE_URL": "https://beacon.test", "BEACON_SUBSCRIPTIONS_ENABLED": "false", **env}
    )
    beacon = BeaconClient(
        config, transport=httpx.MockTransport(handler if handler is not None else default_handler)
    )
    app = create_app(config, client=beacon, registry=registry)
    async with LifespanManager(app):
        async with httpx.AsyncClient(
            transport=httpx.ASGITransport(app=app), base_url="http://gateway.test"
        ) as http:
            yield http


async def rpc(http: httpx.AsyncClient, method: str, **params: Any) -> dict[str, Any]:
    identifier: object = params.get("name", params.get("uri"))
    headers = {
        "Accept": "application/json, text/event-stream",
        "MCP-Protocol-Version": "2026-07-28",
        "Mcp-Method": method,
    }
    if isinstance(identifier, str):
        headers["Mcp-Name"] = identifier
    response = await http.post(
        "/",
        headers=headers,
        json={"jsonrpc": "2.0", "id": 1, "method": method, "params": {"_meta": META, **params}},
    )
    decoded: object = response.json()
    if not isinstance(decoded, dict) or not all(isinstance(key, str) for key in decoded):
        raise AssertionError("MCP response must be a JSON object")
    # The test suite deliberately exercises invalid parameter shapes; validate the
    # top-level JSON-RPC envelope while leaving individual payloads dynamic.
    return cast(dict[str, Any], decoded)
