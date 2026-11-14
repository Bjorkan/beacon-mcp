"""Minimal local public Beacon mock used by the hardened container test."""

import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class Handler(BaseHTTPRequestHandler):
    def do_GET(self) -> None:
        if self.path.startswith("/api/v1/nodes"):
            body = {
                "items": [{"id": "container-node", "name": "Container test"}],
                "hasMore": False,
            }
        else:
            body = []
        data = json.dumps(body).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, format: str, *args: object) -> None:
        # Match BaseHTTPRequestHandler's public override signature.
        pass


ThreadingHTTPServer(("0.0.0.0", 8080), Handler).serve_forever()
