#!/usr/bin/env python3
"""Local fixture site for the browser and computer-use tool-selection eval.

Serves static pages from ./fixtures and records every POST to /api/<event> so
the eval runner can check task outcomes without trusting the model's answer.
"""

import json
import sys
import threading
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

FIXTURES = Path(__file__).resolve().parent / "fixtures"
SALES_CSV = "branch,region,revenue_rm\nIpoh,North,18250\nKlang,Central,22410\nMelaka,South,15980\nPenang,North,27135\nSeremban,South,12225\n"
STATE: dict[str, list] = {}
LOCK = threading.Lock()


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(FIXTURES), **kwargs)

    def log_message(self, *_args):
        pass

    def _json(self, value, status=200):
        body = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/api/state":
            with LOCK:
                return self._json(STATE)
        if self.path == "/files/branch-sales.csv":
            body = SALES_CSV.encode()
            self.send_response(200)
            self.send_header("content-type", "text/csv")
            self.send_header("content-disposition", 'attachment; filename="branch-sales.csv"')
            self.send_header("content-length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        return super().do_GET()

    def do_POST(self):
        if not self.path.startswith("/api/"):
            return self._json({"error": "not found"}, 404)
        length = int(self.headers.get("content-length") or 0)
        raw = self.rfile.read(length) if length else b"{}"
        event = self.path.removeprefix("/api/")
        with LOCK:
            if event == "reset":
                STATE.clear()
            else:
                try:
                    payload = json.loads(raw or b"{}")
                except json.JSONDecodeError:
                    payload = {"raw": raw.decode(errors="replace")}
                STATE.setdefault(event, []).append(payload)
        return self._json({"ok": True})


def serve(port: int) -> ThreadingHTTPServer:
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8765
    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
