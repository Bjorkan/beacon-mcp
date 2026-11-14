"""Compose public SDK HTTP and application lifespans without changing SDK internals."""

import asyncio
from collections.abc import AsyncGenerator
from contextlib import asynccontextmanager
from typing import Any

from mcp.server.lowlevel import Server
from mcp.server.subscriptions import ListenHandler
from mcp.server.transport_security import TransportSecuritySettings
from starlette.applications import Starlette

from beacon_mcp.asgi import create_asgi_app
from beacon_mcp.client import BeaconClient
from beacon_mcp.config import Config
from beacon_mcp.runtime import Registry, create_server
from beacon_mcp.subscriptions import Live, bridge

# Native SDK body admission replaces the former 20-URI subscription wrapper cap.
MAX_MCP_REQUEST_BYTES = 131072


def create_app(
    config: Config, *, client: BeaconClient | None = None, registry: Registry | None = None
) -> Starlette:
    beacon = client or BeaconClient(config)
    live = Live(config) if config.subscriptions_enabled else None
    listener = (
        ListenHandler(live.bus, max_subscriptions=config.max_streams, max_buffered_events=1024)
        if live is not None
        else None
    )

    @asynccontextmanager
    async def beacon_lifespan(_server: Server) -> AsyncGenerator[dict[str, Any], None]:
        task = asyncio.create_task(bridge(beacon, live)) if live is not None else None
        app.state.bridge_task = task
        try:
            yield {}
        finally:
            if listener is not None:
                listener.close()
            if task is not None:
                task.cancel()
                await asyncio.gather(task, return_exceptions=True)
            if live is not None:
                await live.close()
            await beacon.aclose()

    server = create_server(
        beacon, registry, on_subscriptions_listen=listener, lifespan=beacon_lifespan
    )
    endpoint = server.streamable_http_app(
        streamable_http_path="/",
        json_response=True,
        stateless_http=True,
        max_request_body_size=MAX_MCP_REQUEST_BYTES,
        # Public unauthenticated gateway: access/Host policy belongs to its TLS proxy.
        transport_security=TransportSecuritySettings(enable_dns_rebinding_protection=False),
    )

    @asynccontextmanager
    async def lifespan(_application: Starlette) -> AsyncGenerator[None, None]:
        # Documented SDK composition for a mounted ASGI application.
        async with server.session_manager.run():
            yield

    app = create_asgi_app(endpoint, lifespan=lifespan)
    app.state.server = server
    app.state.live = live
    app.state.listener = listener
    app.state.beacon = beacon
    app.state.bridge_task = None
    return app
