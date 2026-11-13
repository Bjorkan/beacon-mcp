from contextlib import asynccontextmanager

import httpx
from asgi_lifespan import LifespanManager

from beacon_mcp.app import create_app
from beacon_mcp.client import BeaconClient
from beacon_mcp.config import Config

META = {
    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
    "io.modelcontextprotocol/clientInfo": {"name": "parity-test", "version": "1"},
    "io.modelcontextprotocol/clientCapabilities": {},
}


@asynccontextmanager
async def gateway(handler=None, registry=None, **env):
    config = Config.from_env(
        {"BEACON_BASE_URL": "https://beacon.test", "BEACON_SUBSCRIPTIONS_ENABLED": "false", **env}
    )
    beacon = BeaconClient(
        config, transport=httpx.MockTransport(handler or (lambda _: httpx.Response(200, json={})))
    )
    app = create_app(config, client=beacon, registry=registry)
    async with LifespanManager(app):
        async with httpx.AsyncClient(
            transport=httpx.ASGITransport(app=app), base_url="http://gateway.test"
        ) as http:
            yield http


async def rpc(http, method, **params):
    response = await http.post(
        "/",
        headers={
            "Accept": "application/json, text/event-stream",
            "MCP-Protocol-Version": "2026-07-28",
            "Mcp-Method": method,
            **(
                {"Mcp-Name": params.get("name", params.get("uri"))}
                if "name" in params or "uri" in params
                else {}
            ),
        },
        json={"jsonrpc": "2.0", "id": 1, "method": method, "params": {"_meta": META, **params}},
    )
    return response.json()
