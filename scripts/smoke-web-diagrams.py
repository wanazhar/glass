#!/usr/bin/env python3
"""Render chart fixtures against the built browser bundle on an isolated server."""
import http.server
import subprocess
import threading
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(ROOT / "web/dist"), **kwargs)

    def log_message(self, *args):
        pass

    def do_GET(self):
        if self.path != "/fixture":
            return super().do_GET()
        body = b'<html class="light"><head><link rel="stylesheet" href="/style.css"><link rel="stylesheet" href="/app.css"></head><body></body></html>'
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
try:
    result = subprocess.run([
        "node", str(ROOT / "web/test/diagrams.mjs"),
        f"http://127.0.0.1:{server.server_port}/fixture",
    ], timeout=120)
    raise SystemExit(result.returncode)
finally:
    server.shutdown()
    server.server_close()
