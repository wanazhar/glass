#!/usr/bin/env python3
"""Exercise an isolated Connect owner capability through a real local TLS proxy.

Uses a temporary CA certificate and private XDG roots. Does not configure
Tailscale, contact a provider, start a desktop, or alter user services.
"""
import argparse
import http.client
import http.server
import hashlib
import hmac
import json
import os
from pathlib import Path
import secrets
import socket
import ssl
import subprocess
import tempfile
import threading
import time


def free_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/glass-core"))
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="gc-secret-tls-") as temp:
        root = Path(temp)
        cert, key = root / "cert.pem", root / "key.pem"
        subprocess.run([
            "openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
            "-keyout", str(key), "-out", str(cert), "-days", "1", "-subj",
            "/CN=localhost", "-addext", "subjectAltName=DNS:localhost",
        ], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        key.chmod(0o600)
        port, backend_port = free_port(), free_port()
        while backend_port == port:
            backend_port = free_port()

        class Proxy(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def forward(self):
                body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
                headers = dict(self.headers)
                headers["Tailscale-User-Login"] = "owner@example.test"
                connection = http.client.HTTPConnection("127.0.0.1", backend_port, timeout=5)
                try:
                    connection.request(self.command, self.path, body=body, headers=headers)
                    result = connection.getresponse()
                    data = result.read()
                    self.send_response(result.status)
                    self.send_header("Content-Type", "application/json")
                    self.send_header("Content-Length", str(len(data)))
                    self.end_headers()
                    self.wfile.write(data)
                finally:
                    connection.close()

            do_GET = forward
            do_POST = forward

        proxy = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Proxy)
        server_tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        server_tls.load_cert_chain(cert, key)
        proxy.socket = server_tls.wrap_socket(proxy.socket, server_side=True)
        thread = threading.Thread(target=proxy.serve_forever, daemon=True)
        thread.start()
        origin = f"https://localhost:{proxy.server_port}"
        owner_token = secrets.token_hex(32)
        data = root / "data" / "glass-core"
        data.mkdir(parents=True, mode=0o700)
        state = data / "connect.json"
        assets = root / "assets"
        assets.mkdir()
        (assets / "index.html").write_text("<!doctype html><title>Isolated Connect fixture</title>")
        settings = dict(
            port=port, https_backend_port=backend_port, https_active=True,
            https=proxy.server_port, http=0, http_host="", host="localhost",
            login="owner@example.test", token=owner_token, pid=0, stamp="",
            local=False, workspace=str(root), assets=str(assets), security=None,
        )
        state.write_text(json.dumps(settings))
        state.chmod(0o600)
        env = dict(os.environ, HOME=str(root), XDG_DATA_HOME=str(root / "data"),
                   XDG_CONFIG_HOME=str(root / "config"), XDG_CACHE_HOME=str(root / "cache"))
        log = root / "gateway.log"
        client_tls = ssl.create_default_context(cafile=str(cert))

        access_capability = None

        def request(path, body=None, capability=None, plain=False, direct_backend=False, authenticated=True):
            headers = {"Host": f"localhost:{proxy.server_port}", "Origin": origin,
                       "Content-Type": "application/json"}
            if capability:
                headers["X-Glass-Secret-Capability"] = capability
            if access_capability and authenticated:
                headers["X-Glass-Owner-Capability"] = access_capability
            if plain or direct_backend:
                headers.update({"Tailscale-User-Login": "owner@example.test",
                                "X-Forwarded-Proto": "https"})
                connection = http.client.HTTPConnection("127.0.0.1", backend_port if direct_backend else port, timeout=5)
            else:
                connection = http.client.HTTPSConnection("localhost", proxy.server_port,
                                                         context=client_tls, timeout=5)
            try:
                connection.request("POST" if body is not None else "GET", path,
                                   json.dumps(body) if body is not None else None, headers)
                response = connection.getresponse()
                result = response.read().decode()
                assert "Set-Cookie" not in response.headers, "Capabilities must not cross port boundaries in cookies"
                return response.status, json.loads(result)
            finally:
                connection.close()

        with log.open("wb") as output:
            gateway = subprocess.Popen([str(binary), "connect", "_serve"], env=env,
                                       cwd=root, stdin=subprocess.DEVNULL, stdout=output, stderr=output)
            try:
                deadline = time.monotonic() + 10
                while True:
                    assert gateway.poll() is None, "Isolated Connect gateway exited"
                    try:
                        with socket.create_connection(("127.0.0.1", backend_port), timeout=.2):
                            break
                    except OSError:
                        assert time.monotonic() < deadline, "Isolated Connect startup timed out"
                        time.sleep(.02)
                settings.update(pid=gateway.pid, stamp=Path(f"/proc/{gateway.pid}/stat")
                                .read_text().rsplit(") ", 1)[1].split()[19])
                state.write_text(json.dumps(settings))
                link = subprocess.run([str(binary), "connect", "secret-link"], env=env,
                                      cwd=root, capture_output=True, text=True, check=True, timeout=5)
                secret_bootstrap = hmac.new(owner_token.encode(),
                    f"glass-connect-secret-bootstrap-v1\0{origin}".encode(), hashlib.sha256).hexdigest()
                assert link.stdout.strip() == f"{origin}#secret={secret_bootstrap}"
                assert owner_token not in link.stdout
                # Static assets must never include the private XDG state tree.
                attacker = http.client.HTTPConnection("127.0.0.1", backend_port, timeout=5)
                try:
                    attacker.request("GET", "/data/glass-core/connect.json", headers={
                        "Host": f"localhost:{proxy.server_port}",
                        "Tailscale-User-Login": "owner@example.test",
                    })
                    try:
                        response = attacker.getresponse()
                        exposed = response.read().decode()
                        assert response.status != 200 and owner_token not in exposed
                    except http.client.RemoteDisconnected:
                        pass  # The missing asset fails closed before writing a body.
                finally:
                    attacker.close()
                assert request("/api/status")[0] == 403
                for endpoint in ["/api/send", "/api/desktop/input", "/api/command"]:
                    assert request(endpoint, {}, direct_backend=True)[0] == 403
                assert request("/api/file?workspace=/tmp&path=secret", direct_backend=True)[0] == 403
                assert request("/api/local", {"token": "0" * 64})[0] == 403
                assert request("/api/local", {"token": secret_bootstrap}, plain=True)[0] == 403
                assert request("/api/local", {"token": owner_token})[0] == 403
                code, unlocked = request("/api/local", {"token": secret_bootstrap})
                assert code == 200
                access_capability = unlocked["ownerCapability"]
                capability = unlocked["secretCapability"]
                assert len(capability) == 64 and capability != owner_token
                assert request("/api/status", capability=capability)[1]["secret_submission_unlocked"] is True
                sentinel = "isolated-secret-value-must-not-be-echoed"
                body = {"secret": sentinel}
                assert request("/api/secret", body)[0] == 403
                assert request("/api/secret", body, direct_backend=True, authenticated=False)[0] == 403
                assert request("/api/secret", body, capability, plain=True)[0] == 403
                code, result = request("/api/secret", body, capability)
                assert code == 400 and result.get("error") == "Missing thread", result
                assert sentinel not in json.dumps(result)
                assert sentinel not in log.read_text(errors="replace")
            finally:
                gateway.terminate()
                try:
                    gateway.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    gateway.kill()
                    gateway.wait(timeout=5)
                proxy.shutdown()
                proxy.server_close()
                thread.join(timeout=5)
    print("PASS real TLS owner unlock; forged HTTP/header denial; in-memory capability; non-echo")


if __name__ == "__main__":
    main()
