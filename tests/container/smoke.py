"""Host-side, no-dependency health and MCP smoke checks for the container."""

import json
import os
from urllib.request import Request, urlopen

base = f"http://127.0.0.1:{os.environ['PORT_TO_TEST']}"
for path in ("/healthz", "/readyz"):
    with urlopen(base + path, timeout=3) as response:
        assert response.status == 200

meta = {
    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
    "io.modelcontextprotocol/clientInfo": {
        "name": "container-test",
        "version": "1.0.0",
    },
    "io.modelcontextprotocol/clientCapabilities": {},
}
payload = {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/call",
    "params": {
        "name": "beacon_search_nodes",
        "arguments": {"limit": 20},
        "_meta": meta,
    },
}
headers = {
    "Content-Type": "application/json",
    "Accept": "application/json, text/event-stream",
    "MCP-Protocol-Version": "2026-07-28",
    "Mcp-Method": "tools/call",
    "Mcp-Name": "beacon_search_nodes",
}
with urlopen(
    Request(base + "/", json.dumps(payload).encode(), headers), timeout=10
) as response:
    result = json.load(response)["result"]
    assert result.get("isError") is False
    assert result["structuredContent"]["items"][0]["id"] == "container-node"
