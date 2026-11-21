#!/usr/bin/env python3
"""Live-host E2E harness for the Glass iOS app.

Starts a real glass-core gateway in a disposable workspace in two modes:
  * remote: behind a TLS proxy that plays Tailscale Serve (injects
    Tailscale-User-Login, terminates HTTPS for GLASS_E2E_HOST with a local CA);
  * local: `connect --local` on loopback.
Then serves a control API the UI tests call:
  GET /link/owner | /link/secret | /link/local, POST /revoke (restart = new token).
"""
import http.server, json, os, re, select, signal, socket, ssl, subprocess, sys, threading, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.environ["GLASS_E2E_ROOT"]
CORE = os.environ["GLASS_CORE_BIN"]
HOST = os.environ.get("GLASS_E2E_HOST", "glass-e2e.tail0e2e.ts.net")
CONTROL_PORT = int(os.environ.get("GLASS_E2E_CONTROL_PORT", "51999"))
LOGIN = "owner@example.test"
URL = re.compile(r"(https?://\S+)")


def env_for(name):
    base = os.path.join(ROOT, name)
    env = dict(os.environ)
    for var in ("DATA", "CONFIG", "STATE", "CACHE"):
        path = os.path.join(base, "xdg", var.lower())
        os.makedirs(path, exist_ok=True)
        env[f"XDG_{var}_HOME"] = path
    env["PATH"] = os.path.join(ROOT, "bin") + os.pathsep + env["PATH"]
    env["GC_TAILSCALE_SOCKET"] = os.path.join(ROOT, "no-tailscaled.sock")
    return env


WS = os.path.join(ROOT, "ws")


def core(name, *args):
    out = subprocess.run([CORE, "connect", *args], cwd=WS, env=env_for(name), capture_output=True, text=True, timeout=60)
    if out.returncode != 0:
        raise RuntimeError(f"connect {' '.join(args)} failed: {out.stderr or out.stdout}")
    return out.stdout


def link(name, *args):
    match = URL.search(core(name, *args))
    if not match:
        raise RuntimeError("no link in connect output")
    return match.group(1)


class Proxy:
    """TLS front end standing in for Tailscale Serve on the HTTPS port."""

    def __init__(self):
        self.server = None
        self.port = None

    def start(self):
        routes = json.load(open(os.path.join(ROOT, "serve.json")))
        self.port, target = next(iter(routes.items()))
        backend = target.split("//", 1)[1]
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(os.path.join(ROOT, "tls", "host.pem"), os.path.join(ROOT, "tls", "host.key"))
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        # macOS lets unprivileged processes bind low ports only on the wildcard address.
        sock.bind(("0.0.0.0", int(self.port)))
        sock.listen(64)
        self.server = sock
        threading.Thread(target=self.accept, args=(sock, context, backend), daemon=True).start()

    def stop(self):
        if self.server:
            self.server.close()
            self.server = None

    def accept(self, sock, context, backend):
        while True:
            try:
                client, _ = sock.accept()
            except OSError:
                return
            threading.Thread(target=self.handle, args=(client, context, backend), daemon=True).start()

    def handle(self, raw, context, backend):
        try:
            client = context.wrap_socket(raw, server_side=True)
            head = b""
            while b"\r\n\r\n" not in head:
                chunk = client.recv(65536)
                if not chunk:
                    return
                head += chunk
            headers, rest = head.split(b"\r\n\r\n", 1)
            lines = [l for l in headers.split(b"\r\n") if not l.lower().startswith(b"tailscale-")]
            lines.insert(1, f"Tailscale-User-Login: {LOGIN}".encode())
            host, port = backend.rsplit(":", 1)
            upstream = socket.create_connection((host, int(port)))
            upstream.sendall(b"\r\n".join(lines) + b"\r\n\r\n" + rest)
            pair = {client: upstream, upstream: client}
            while True:
                readable, _, _ = select.select(list(pair), [], [], 120)
                if not readable:
                    return
                for s in readable:
                    data = s.recv(65536)
                    if not data:
                        return
                    pair[s].sendall(data)
        except (OSError, ssl.SSLError):
            pass
        finally:
            raw.close()


proxy = Proxy()
lock = threading.Lock()


def start_remote():
    link("remote")
    proxy.start()


def revoke():
    with lock:
        core("remote", "stop")
        proxy.stop()
        start_remote()


class Control(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def reply(self, body, status=200):
        data = body.encode()
        self.send_response(status)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        try:
            with lock:
                if self.path == "/link/owner":
                    return self.reply(link("remote"))
                if self.path == "/link/secret":
                    return self.reply(link("remote", "secret-link"))
                if self.path == "/link/local":
                    return self.reply(link("local", "--local"))
                if self.path == "/health":
                    return self.reply("ok")
            self.reply("not found", 404)
        except Exception as error:
            self.reply(str(error), 500)

    def do_POST(self):
        if self.path != "/revoke":
            return self.reply("not found", 404)
        try:
            revoke()
            self.reply("revoked")
        except Exception as error:
            self.reply(str(error), 500)


def shutdown(*_):
    for name in ("remote", "local"):
        try:
            core(name, "stop")
        except Exception:
            pass
    proxy.stop()
    sys.exit(0)


def main():
    os.makedirs(os.path.join(ROOT, "bin"), exist_ok=True)
    tailscale = os.path.join(ROOT, "bin", "tailscale")
    if not os.path.exists(tailscale):
        os.symlink(os.path.join(HERE, "fake-tailscale"), tailscale)
    os.makedirs(WS, exist_ok=True)
    subprocess.run(["git", "init", "-q", WS], check=True)
    signal.signal(signal.SIGTERM, shutdown)
    signal.signal(signal.SIGINT, shutdown)
    start_remote()
    link("local", "--local")
    server = http.server.ThreadingHTTPServer(("127.0.0.1", CONTROL_PORT), Control)
    print(f"ready https://{HOST}:{proxy.port} control http://127.0.0.1:{CONTROL_PORT}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
