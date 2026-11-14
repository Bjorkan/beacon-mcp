"""Host-side, no-dependency health and MCP smoke checks for the container."""

import json
import os
from http.client import HTTPResponse
from typing import Any
from urllib.request import Request, urlopen

path: str
response: HTTPResponse

base: str = f"http://127.0.0.1:{os.environ['PORT_TO_TEST']}"
for path in ("/healthz", "/readyz"):
    with urlopen(base + path, timeout=3) as response:
        assert response.status == 200

meta: dict[str, object] = {
    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
    "io.modelcontextprotocol/clientInfo": {
        "name": "container-test",
        "version": "1.0.0",
    },
    "io.modelcontextprotocol/clientCapabilities": {},
}
payload: dict[str, object] = {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/call",
    "params": {
        "name": "beacon_search_nodes",
        "arguments": {"limit": 20},
        "_meta": meta,
    },
}
headers: dict[str, str] = {
    "Content-Type": "application/json",
    "Accept": "application/json, text/event-stream",
    "MCP-Protocol-Version": "2026-07-28",
    "Mcp-Method": "tools/call",
    "Mcp-Name": "beacon_search_nodes",
}
with urlopen(Request(base + "/", json.dumps(payload).encode(), headers), timeout=10) as response:
    result: dict[str, Any] = json.load(response)["result"]
    assert result.get("isError") is False
    assert result["structuredContent"]["items"][0]["id"] == "container-node"
