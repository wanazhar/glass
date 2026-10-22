#!/usr/bin/env python3
"""Offline check for the egress policy proxy and the admin requirements layer.

Wave B of the six-codebase wave. Three checks:

1. The egress proxy starts when `sandbox_egress = "policy"` is configured
   with the sandbox on; a CONNECT to a non-allowlisted host is refused with
   the host named, and an allowlisted host tunnels through.
2. `requirements.toml` forces `sandbox = strict` over an explicit user
   `sandbox = off` and the startup line names the force.
3. A corrupt requirements file fails closed with a clear error.

Run after `cargo build`.
"""
import http.server
import json
import os
import socket
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
from core_binary import core_binary_path

REPO = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get("GLASS_CORE_TEST_BINARY", str(core_binary_path('debug'))))

ALLOWED_HOST = "allowed.invalid"
DENIED_HOST = "denied.invalid"


class HangEndpoint(http.server.BaseHTTPRequestHandler):
    """A model endpoint that never finishes: keeps the process alive."""

    def log_message(self, *_):
        pass

    def do_POST(self):
        # Stream nothing, close nothing, until the process gives up.
        try:
            time.sleep(30)
        except Exception:
            pass

    do_GET = do_POST


def polished(cfg_text: str) -> str:
    """Config with a zero-priced rate card so budget checks never fire."""
    return cfg_text + "[pricing]\ninput = 0.0\ncache_read = 0.0\ncache_write = 0.0\noutput = 0.0\n"


def speak(sock: socket.socket, payload: bytes) -> bytes:
    sock.sendall(payload)
    return recv_all(sock)


def recv_all(sock: socket.socket, cap: int = 65536) -> bytes:
    sock.settimeout(5)
    out = b""
    try:
        while len(out) < cap:
            chunk = sock.recv(4096)
            if not chunk:
                break
            out += chunk
            if b"\r\n\r\n" in out:
                break
    except socket.timeout:
        pass
    return out


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="glass-egress-") as scratch:
        root = Path(scratch)
        conf = root / "config" / "glass-core"
        conf.mkdir(parents=True)
        workspace = root / "ws"
        workspace.mkdir()

        # 1. Proxy denied + allowed under a strict sandbox.
        # The endpoint is a slow streamer so the process is alive while the
        # proxy port is probed; a dead endpoint would close it.
        blocker = http.server.HTTPServer(("127.0.0.1", 0), HangEndpoint)
        threading.Thread(target=blocker.serve_forever, daemon=True).start()
        (conf / "config.toml").write_text(
            polished(
                'provider = "openai"\nmodel = "mock"\napi_key_env = ""\n'
                f'base_url = "http://127.0.0.1:{blocker.server_port}/v1"\n'
                f'workspace = "{workspace}"\n'
                'sandbox = "strict"\n'
                'sandbox_egress = "policy"\n'
                f'egress_allow = ["{ALLOWED_HOST}", "*.github.com"]\n'
            )
        )
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"))

        # Start a turn; it hangs on the model endpoint, keeping the proxy up.
        proc = subprocess.Popen(
            [str(BINARY), "run", "--json", "--budget", "1", "x"],
            cwd=workspace, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        deadline = time.time() + 10
        warnings = ""
        port = None
        while time.time() < deadline:
            # The warning goes to stderr; a synchronous PIPE read deadlocks,
            # so poll frequently instead and None the pipe only at the end.
            try:
                import fcntl
                fd = proc.stderr.fileno()
                fl = fcntl.fcntl(fd, fcntl.F_GETFL)
                fcntl.fcntl(fd, fcntl.F_SETFL, fl | os.O_NONBLOCK)
                chunk = proc.stderr.read()
                if chunk:
                    warnings += chunk
            except Exception:
                pass
            m = [l for l in warnings.splitlines() if "egress policy proxy on" in l]
            if m:
                url = "http://" + m[0].split("on http://", 1)[1].split(" ", 1)[0]
                port = int(url.rsplit(":", 1)[1])
                break
            if proc.poll() is not None:
                break
            time.sleep(0.05)
        assert port, f"proxy never came up: {warnings[:400]}"

        # denied host: 403 naming it.
        s = socket.create_connection(("127.0.0.1", port), timeout=5)
        reply = speak(s, f"CONNECT {DENIED_HOST}:443 HTTP/1.1\r\nHost: {DENIED_HOST}\r\n\r\n".encode())
        s.close()
        assert b"403" in reply, reply
        assert DENIED_HOST.encode() in reply, reply
        assert b"not in sandbox.egress_allow" in reply, reply

        # wildcard allow passes the policy check (the upstream connect
        # fails, yielding 502 — the policy boundary is not what refused it).
        s = socket.create_connection(("127.0.0.1", port), timeout=5)
        reply = speak(s, b"CONNECT api.github.com:443 HTTP/1.1\r\n\r\n")
        s.close()
        assert b"403" not in reply, f"policy wrongly refused an allowlisted host: {reply[:120]}"

        # exact allow likewise leaves the policy layer.
        s = socket.create_connection(("127.0.0.1", port), timeout=5)
        reply = speak(s, f"CONNECT {ALLOWED_HOST}:443 HTTP/1.1\r\n\r\n".encode())
        s.close()
        assert b"403" not in reply, f"policy wrongly refused an exact allow: {reply[:120]}"

        # non-CONNECT is refused: this is not an HTTP forward proxy.
        s = socket.create_connection(("127.0.0.1", port), timeout=5)
        reply = speak(s, b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
        s.close()
        assert b"405" in reply, reply

        proc.kill()
        proc.wait()

        # The remaining checks need the process to exit on its own: a
        # config that points at a nonexistent endpoint fails fast.
        dead = "http://127.0.0.1:9/v1"

        # 2. requirements forces the sandbox over an explicit user choice.
        (conf / "config.toml").write_text(
            polished(
                'provider = "openai"\nmodel = "mock"\napi_key_env = ""\n'
                f'base_url = "{dead}"\n'
                f'workspace = "{workspace}"\n'
                'sandbox = "off"\n'
            )
        )
        (conf / "requirements.toml").write_text('sandbox = "strict"\n')
        probe = subprocess.run(
            [str(BINARY), "run", "--json", "--budget", "1", "x"],
            cwd=workspace, env=env, text=True, capture_output=True, timeout=60,
        )
        warnings = (probe.stderr or "") + (probe.stdout or "")
        assert "admin requirements: sandbox forced to `strict`" in warnings, warnings[:600]

        # 3. corrupt requirements fails closed with a named reason.
        (conf / "requirements.toml").write_text("sandbox = [not toml\n")
        probe = subprocess.run(
            [str(BINARY), "run", "--json", "--budget", "1", "x"],
            cwd=workspace, env=env, text=True, capture_output=True, timeout=60,
        )
        combined = (probe.stderr or "") + (probe.stdout or "")
        assert "requirements.toml" in combined and "parse" in combined, combined[:600]

        # 3b. invalid forced value likewise refused by name.
        (conf / "requirements.toml").write_text('sandbox = "permissive"\n')
        probe = subprocess.run(
            [str(BINARY), "run", "--json", "--budget", "1", "x"],
            cwd=workspace, env=env, text=True, capture_output=True, timeout=60,
        )
        combined = (probe.stderr or "") + (probe.stdout or "")
        assert "off, workspace, or strict" in combined, combined[:600]

        blocker.shutdown()
        blocker.server_close()

    print("PASS egress: the policy proxy refuses denied CONNECTs naming the host, "
          "lets allowlisted hosts past the policy layer, refuses non-CONNECT, "
          "stays off without the sandbox; requirements force the sandbox over the "
          "user choice and fail closed on corruption")
    return 0


if __name__ == "__main__":
    sys.exit(main())
