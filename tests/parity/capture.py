"""One-time migration baseline capture from the unchanged reference executable.

Only local synthetic HTTP data is used. Retire this helper with the reference
implementation; tests consume the resulting language-independent fixture.
"""
import hashlib
import json
import os
import socket
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qsl, unquote, urlsplit
from urllib.request import Request, urlopen

ID = "123e4567-e89b-42d3-a456-426614174000"
B = "123e4567-e89b-42d3-a456-426614174001"
KEY = "0123456789abcdef0123456789abcdef"
ROOT = Path(__file__).resolve().parents[2]
CONTRACT_FILES = sorted((ROOT / "src/beacon_mcp/contracts").glob("*-tool-definitions.json"))
CONTRACTS = {k: v for file in CONTRACT_FILES for k, v in json.loads(file.read_text()).items()}
META = {"io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientInfo": {"name": "parity", "version": "1"}, "io.modelcontextprotocol/clientCapabilities": {}}

# Explicit fixture routes; never discover them from the implementation under test.
ROUTES = {
 "beacon_get_server_info": "info", "beacon_list_iatas": "iatas", "beacon_list_regions": "regions", "beacon_list_scopes": "scopes",
 "beacon_search_nodes": "nodes", "beacon_get_node": f"nodes/{ID}", "beacon_search_observers": "observers", "beacon_get_observer": f"observers/{ID}", "beacon_get_observer_activity": f"observers/{ID}/activity",
 "beacon_search_packets": "packets", "beacon_get_packet": "packets/aabbccddeeff0011", "beacon_search_messages": "messages", "beacon_list_channels": "channels", "beacon_get_channel_messages": "channels/7/messages",
 "beacon_list_routes": "routes", "beacon_search_routes": "routes/search", "beacon_find_cross_iata_routes": "routes/cross", "beacon_search_traces": "traces", "beacon_get_trace": "traces/aabbccdd",
 "beacon_get_top_advertisers": "stats/top-advertisers", "beacon_get_network_overview": "stats/overview", "beacon_get_network_series": "stats/series", "beacon_compare_observers": "stats/observer-comparison",
 "beacon_list_brokers": "brokers", "beacon_get_channel": "channels/7", "beacon_get_iata": "iatas/ARN", "beacon_get_iata_border": "iatas/ARN/border", "beacon_backfill_messages": "messages/backfill",
 "beacon_get_node_activity": f"nodes/{ID}/activity", "beacon_get_node_neighbors": f"nodes/{ID}/neighbors", "beacon_get_node_observations": f"nodes/{ID}/observations",
 "beacon_get_observer_adverts": f"observers/{ID}/adverts", "beacon_get_observer_telemetry": f"observers/{ID}/telemetry", "beacon_list_observer_directory": "observers/directory", "beacon_backfill_packets": "packets/backfill",
 "beacon_get_region": "regions/3", "beacon_get_route_observations": f"routes/ARN/{KEY}/observations", "beacon_get_scope": "scopes/#Sweden",
 **{f"beacon_get_{name}": "stats/" + endpoint for name, endpoint in [
 ("clock_drift", "clock-drift"), ("node_types", "node-types"), ("observation_stats", "observations"), ("path_stats", "paths"), ("payload_breakdown", "payload-breakdown"), ("radio_presets", "radio-presets"), ("scope_stats", "scopes"), ("signal_stats", "signal"), ("top_nodes", "top-nodes"), ("top_observers", "top-observers"), ("top_talkers", "top-talkers")]},
}
ARRAY_TOOLS = {"beacon_list_iatas", "beacon_list_regions", "beacon_list_brokers", "beacon_search_routes", "beacon_find_cross_iata_routes", "beacon_get_top_advertisers", "beacon_backfill_messages", "beacon_get_node_neighbors", "beacon_backfill_packets", "beacon_get_clock_drift", "beacon_get_node_types", "beacon_get_observation_stats", "beacon_get_payload_breakdown", "beacon_get_radio_presets", "beacon_get_scope_stats", "beacon_get_top_nodes", "beacon_get_top_observers", "beacon_get_top_talkers"}
PAGE_TOOLS = {"beacon_search_nodes", "beacon_search_observers", "beacon_search_packets", "beacon_search_messages", "beacon_list_channels", "beacon_get_channel_messages", "beacon_get_node_observations", "beacon_get_observer_adverts", "beacon_list_observer_directory"}
ROWS = [{"id": i, "lastSeen": i * 1000, "lastHeardAt": i * 2000, "traceTag": "aabbccdd", "name": "synthetic", "hops": ["ab", "abcdef"]} for i in (9, 8, 7)]
UPSTREAM = {}
for name, route in ROUTES.items():
    if name in ARRAY_TOOLS or name in ("beacon_list_routes", "beacon_search_traces"):
        value = ROWS
    elif name == "beacon_list_scopes":
        value = ["#a", "#b", "#c"]
    elif name in PAGE_TOOLS:
        value = {"items": ROWS, "hasMore": True, "nextCursor": 8, "nextPageCursor": "v1:exact:opaque", "sort": "name", "order": "asc", "coverage": {"complete": True}, "windowStart": 1000, "windowEnd": 2000, "maxObservationCount": 100, "observerTypes": ["mqtt"], "effectiveSort": "traffic"}
    else:
        value = {"id": ID, "points": [], "totalPackets": 12, "matchAvailable": True, "serverVersion": "2.2.0", "minAppVersion": None, "minWebVersion": None}
    UPSTREAM["/api/v1/" + route] = value


def arguments(name):
    schema = CONTRACTS[name]["inputSchema"]
    values = {"afterId": 2147483649, "afterObservationId": 2147483649, "id": ("7" if name == "beacon_get_channel" else "3" if name == "beacon_get_region" else "arn" if name.startswith("beacon_get_iata") else "#Sweden" if name == "beacon_get_scope" else ID), "hash": "AABBCCDDEEFF0011", "tag": "AABBCCDD", "channelId": 7, "iata": "arn", "pathKey": KEY, "from": "AB", "to": "abcdef", "fromHash": "AB", "toHash": "cd", "observerA": ID, "observerB": B, "iataA": "arn", "iataB": "mmx", "since": "2026-11-01T00:00:00Z", "until": "2026-11-02T00:00:00Z", "sort": "direct"}
    args = {key: values[key] for key in schema.get("required", [])}
    for key, value in (("iatas", ["arn", "mmx"]), ("limit", 2), ("range", "48h")):
        if key in schema["properties"]:
            args[key] = value
    return args


def capture(binary):
    seen = []
    scenario = {}
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            path = unquote(urlsplit(self.path).path)
            seen.append({"path": path, "query": dict(parse_qsl(urlsplit(self.path).query, keep_blank_values=True))})
            status = scenario.get("status", 200)
            body = scenario.get("body", UPSTREAM.get(path, {}))
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            if status >= 400:
                self.send_header("Retry-After", "10")
            raw = json.dumps(body).encode()
            self.send_header("Content-Length", str(len(raw)))
            self.end_headers()
            self.wfile.write(raw)
        def log_message(self, *_):
            pass
    upstream = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    worker = threading.Thread(target=upstream.serve_forever, daemon=True)
    worker.start()
    with socket.socket() as reserve:
        reserve.bind(("127.0.0.1", 0))
        port = reserve.getsockname()[1]
    env = {**os.environ, "HOST": "127.0.0.1", "PORT": str(port), "BEACON_BASE_URL": f"http://127.0.0.1:{upstream.server_port}", "BEACON_SUBSCRIPTIONS_ENABLED": "false", "LOG_LEVEL": "silent"}
    process = subprocess.Popen([binary], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    def rpc(name, args):
        request = Request(f"http://127.0.0.1:{port}/", json.dumps({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": args, "_meta": META}}).encode(), {"Content-Type": "application/json", "Accept": "application/json, text/event-stream", "MCP-Protocol-Version": "2026-07-28", "Mcp-Method": "tools/call", "Mcp-Name": name})
        with urlopen(request, timeout=5) as response:
            value = json.load(response)["result"]
        return {key: value[key] for key in ("isError", "content", "structuredContent") if key in value}
    try:
        for _ in range(100):
            try:
                with urlopen(f"http://127.0.0.1:{port}/healthz", timeout=0.1):
                    break
            except OSError:
                time.sleep(0.02)
        cases = []
        def add(name, args, error_scenario=None):
            seen.clear()
            scenario.clear()
            if error_scenario:
                scenario.update(error_scenario)
            result = rpc(name, args)
            cases.append({"name": name, "arguments": args, "expected": result, "requests": sorted(seen, key=lambda value: json.dumps(value, sort_keys=True)), **({"scenario": error_scenario} if error_scenario else {})})
        for name in json.loads((ROOT / "tests/fixtures/tool-names.json").read_text()):
            add(name, arguments(name))
            assert not cases[-1]["expected"]["isError"], name
        for case in json.loads((ROOT / "tests/fixtures/invalid-tool-cases.json").read_text()):
            add(case["name"], case["arguments"])
        for status, code in ((400,"bad_request"), (404,"not_found"), (429,"rate_limited"), (503,"deadline_exceeded"), (500,"internal_server_error")):
            add("beacon_get_server_info", {}, {"status": status, "body": {"error": {"code": code, "message": "synthetic error detail"}}})
        output = {"sourceCommit": "d12b0ba94868e03ad049e14596172d765c056274", "sourceImplementation": "Rust 2.2.0", "contractHashes": {file.name: hashlib.sha256(file.read_bytes()).hexdigest() for file in CONTRACT_FILES}, "upstream": UPSTREAM, "cases": cases}
        (ROOT / "tests/fixtures/behavior-baseline.json").write_text(json.dumps(output, indent=2) + "\n")
        print(f"Captured {len(cases)} synthetic reference cases.")
    finally:
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        upstream.shutdown()
        upstream.server_close()
        worker.join()


if __name__ == "__main__":
    capture(sys.argv[1])
