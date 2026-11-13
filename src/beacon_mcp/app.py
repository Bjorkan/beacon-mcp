"""Compose public SDK HTTP and application lifespans without changing SDK internals."""

from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import Any

from mcp.server.lowlevel import Server
from mcp.server.transport_security import TransportSecuritySettings
from starlette.applications import Starlette

from beacon_mcp.asgi import create_asgi_app
from beacon_mcp.client import BeaconClient
from beacon_mcp.config import Config
from beacon_mcp.runtime import Registry, create_server

# Bound request memory through the public SDK transport option.
MAX_MCP_REQUEST_BYTES = 131072


def create_app(
    config: Config, *, client: BeaconClient | None = None, registry: Registry | None = None
) -> Starlette:
    beacon = client or BeaconClient(config)

    @asynccontextmanager
    async def beacon_lifespan(_server: Server) -> AsyncIterator[dict[str, Any]]:
        try:
            yield {}
        finally:
            await beacon.aclose()

    server = create_server(beacon, registry, lifespan=beacon_lifespan)
    endpoint = server.streamable_http_app(
        streamable_http_path="/",
        json_response=True,
        stateless_http=True,
        max_request_body_size=MAX_MCP_REQUEST_BYTES,
        # Public unauthenticated gateway: access/Host policy belongs to its TLS proxy.
        transport_security=TransportSecuritySettings(enable_dns_rebinding_protection=False),
    )

    @asynccontextmanager
    async def lifespan(_application: Starlette) -> AsyncIterator[None]:
        # Documented SDK composition for a mounted ASGI application.
        async with server.session_manager.run():
            yield

    app = create_asgi_app(endpoint, lifespan=lifespan)
    app.state.server = server
    app.state.beacon = beacon
    return app
