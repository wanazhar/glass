#!/usr/bin/env python3
"""Regression for staircase output in a real raw PTY, including SSH-sized screens."""
import http.server
import importlib.util
import json
import os
from pathlib import Path
import re
import tempfile
import termios
import threading

spec = importlib.util.spec_from_file_location("features", Path(__file__).with_name("smoke-features.py"))
f = importlib.util.module_from_spec(spec)
spec.loader.exec_module(f)

class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_POST(self):
        self.rfile.read(int(self.headers["Content-Length"]))
        chunks = ["Hello! I work in your workspace.\n\n", "How can I help you today?\n", "- Reading and editing files\n", "- Building and testing code\n", "- Version control operations\n\n", "What would you like to work on?\n"]
        frames = [{"choices": [{"delta": {"content": text}}]} for text in chunks]
        frames.append({"choices": [], "usage": {"prompt_tokens": 773, "completion_tokens": 151}})
        data = ("".join("data: " + json.dumps(frame) + "\n\n" for frame in frames) + "data: [DONE]\n\n").encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

with tempfile.TemporaryDirectory(prefix="gc-mobile-") as tmp:
    root = Path(tmp)
    config = root / "config/glass-core"
    config.mkdir(parents=True)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Endpoint)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    (config / "config.toml").write_text(f'provider = "openai"\nbase_url = "http://127.0.0.1:{server.server_port}/v1"\nmodel = "mock"\napi_key_env = "GC_MOBILE_KEY"\n')
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"), XDG_DATA_HOME=str(root / "data"), GC_MOBILE_KEY="test")
    try:
        for width, height in [(65, 30), (40, 24), (24, 16), (20, 12)]:
            terminal = f.Terminal(root, env, size=(width, height))
            try:
                terminal.wait("\x1b[?25h")
                attrs = termios.tcgetattr(terminal.master)
                assert not attrs[3] & termios.ICANON, "Input must remain raw"
                assert attrs[1] & termios.OPOST and attrs[1] & termios.ONLCR, "Raw input must preserve normal newline output"
                terminal.received = ""
                os.write(terminal.master, b"hello\r")
                output = terminal.wait("0 tools")
                terminal.idle()
                output += terminal.received
                assert not re.search(r"(?<!\r)\n", output), repr(output)
                plain = re.sub(r"\x1b\[[0-9;?]*[ -/]*[@-~]", "", output)
                # Activity updates now use a separate owned row; inspect projected
                # body rows rather than flattening cursor-addressed status into prose.
                body = "\n".join(terminal.screen.history) + "\n" + terminal.screen.text()
                assert re.search(r"(?m)^ {1,2}How can I help", body), body
                assert re.search(r"(?m)^ {1,2}• Building", body), body
                assert re.search(r"(?m)^ {1,2}What would", body), body
                footer = plain[plain.index("773 in"):]
                footer = footer.split("\r\n\r\n", 1)[0]
                for line in footer.split("\r\n"):
                    assert len(line) < width, (width, line)
                assert "session cost unavailable" not in footer
            finally:
                probe = os.dup(terminal.master)
                try:
                    terminal.close()
                    restored = termios.tcgetattr(probe)
                    assert restored[3] & termios.ICANON, "Canonical input must be restored on exit"
                    assert restored[1] & termios.OPOST and restored[1] & termios.ONLCR
                finally:
                    os.close(probe)
    finally:
        server.shutdown()
    print("PASS: raw PTY preserves newline-to-column-one; multiline mobile chat and bounded status at 65x30, 40x24, 24x16, 20x12")
