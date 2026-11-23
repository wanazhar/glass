#!/usr/bin/env python3
"""Live-host E2E harness for the Glass iOS app.

Starts a real glass-core gateway in a disposable workspace in two modes:
  * remote: behind a TLS proxy that plays Tailscale Serve (injects
    Tailscale-User-Login, terminates HTTPS for GLASS_E2E_HOST with a local CA);
  * local: `connect --local` on loopback.
Then serves a control API the UI tests call:
  GET /link/owner | /link/secret | /link/local, POST /revoke (restart = new token),
  POST /thread/new | /thread/send | /thread/seed | /thread/components | /thread/wait, POST /net/drop | /net/stall | /net/restore,
  POST /control/claim | /control/release (a second session client, like another device taking control),
  GET /thread/context,
  POST /desktop/fake?on=1&kbps=N&rtt=MS | /desktop/mark, GET /desktop/report (a stand-in desktop host, below).
A scripted OpenAI-compatible provider answers every turn so threads stream real events.
"""
import http.client, http.server, json, os, re, select, signal, socket, ssl, subprocess, sys, threading, time, uuid
import urllib.error, urllib.parse, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.environ["GLASS_E2E_ROOT"]
CORE = os.environ["GLASS_CORE_BIN"]
HOST = os.environ.get("GLASS_E2E_HOST", "glass-e2e.tail0e2e.ts.net")
CONTROL_PORT = int(os.environ.get("GLASS_E2E_CONTROL_PORT", "51999"))
PROVIDER_PORT = int(os.environ.get("GLASS_E2E_PROVIDER_PORT", "52011"))
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
    env["GLASS_E2E_PROVIDER_KEY"] = "e2e"
    config = os.path.join(base, "xdg", "config", "glass-core")
    os.makedirs(config, exist_ok=True)
    with open(os.path.join(config, "config.toml"), "w") as f:
        f.write(f'provider = "openai"\nbase_url = "http://127.0.0.1:{PROVIDER_PORT}/v1"\n'
                'model = "e2e-scripted"\napi_key_env = "GLASS_E2E_PROVIDER_KEY"\n')
    return env


def data_dir(name):
    return os.path.join(ROOT, name, "xdg", "data", "glass-core")


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


class FakeDesktop:
    """Stands in for a running Linux desktop behind the same TLS front end, so the app's desktop path
    (stream, decode, input, reconnects) can be timed on a Mac. Frames are real 30 fps JPEG video from
    ffmpeg's test source, pre-encoded at each stream width; `kbps` caps the stream's bandwidth and `rtt`
    delays every response, like a slow tailnet path."""

    def __init__(self):
        self.on = False
        self.kbps = 0
        self.rtt = 0
        self.lease = "agent"
        self.paused = False
        self.frames = {}
        self.seq = 0
        self.lock = threading.Lock()
        self.reset()

    def reset(self):
        with self.lock:
            self.mark = None
            self.resumed_ms = None
            self.streams = []
            self.inputs = 0
            self.actions = []
            self.sent = 0

    def jpegs(self, width):
        if width not in self.frames:
            out = os.path.join(ROOT, "desktop", str(width))
            os.makedirs(out, exist_ok=True)
            height = width * 800 // 1280
            subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-f", "lavfi", "-i",
                            "testsrc2=size=1280x800:rate=30", "-frames:v", "60", "-vf", f"scale={width}:{height}",
                            "-q:v", "4", os.path.join(out, "f%03d.jpg")], check=True)
            self.frames[width] = [open(os.path.join(out, n), "rb").read() for n in sorted(os.listdir(out))]
        return self.frames[width]

    def status(self):
        return {"available": True, "sessionId": "fake-desktop", "environment": "xfce", "lifecycle": "disposable",
                "lease": self.lease, "width": 1280, "height": 800, "frameSeq": self.seq, "frameAgeMs": 1,
                "regionUpdates": 0,
                "capabilities": [{"environment": "xfce", "lifecycle": "disposable", "label": "Quick desktop",
                                  "available": True, "profileExists": False}]}

    def report(self):
        with self.lock:
            return {"resumedMs": self.resumed_ms, "streams": len(self.streams), "inputs": self.inputs, "actions": self.actions[-40:],
                    "streamsAfterMark": len([t for t in self.streams if self.mark and t >= self.mark])}

    def serve(self, client, method, target, body, frozen):
        url = urllib.parse.urlparse(target)
        q = {k: v[0] for k, v in urllib.parse.parse_qs(url.query).items()}
        if self.rtt:
            time.sleep(self.rtt / 1000)

        def send_json(value, status=200):
            data = json.dumps(value).encode()
            client.sendall(f"HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {len(data)}\r\n"
                           "Connection: close\r\n\r\n".encode() + data)

        if url.path == "/api/desktop/status":
            return send_json(self.status())
        if url.path == "/api/desktop/lease":
            action = json.loads(body or b"{}").get("action")
            self.lease = {"take-control": "human", "return-to-agent": "agent", "read-only": "read-only"}.get(action, self.lease)
            return send_json({"lease": self.lease})
        if url.path == "/api/desktop/input":
            event = json.loads(body or b"{}")
            with self.lock:
                self.inputs += 1
                self.actions.append(f"{event.get('action')}:{event.get('button', '')}")
            return send_json({"ok": True})
        if url.path == "/api/desktop/frame":
            jpeg = self.jpegs(1280)[self.seq % 60]
            client.sendall(f"HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {len(jpeg)}\r\n"
                           "Connection: close\r\n\r\n".encode() + jpeg)
            return
        if url.path != "/api/desktop/stream":
            return send_json({"error": "not in the stand-in desktop"}, 404)
        width = max(320, min(1280, int(q.get("width", "1280"))))
        frames = self.jpegs(width)
        opened = time.monotonic()
        with self.lock:
            self.streams.append(opened)
        client.sendall(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nCache-Control: no-store\r\n"
                       b"Connection: close\r\n\r\n")
        tick = time.monotonic()
        send_ms = 0
        while True:
            if client in frozen():
                time.sleep(0.2)
                continue
            if self.paused:
                client.sendall(b"--glassframe\r\nX-Glass-Heartbeat: 1\r\nContent-Length: 0\r\n\r\n\r\n")
                time.sleep(1)
                tick = time.monotonic()
                continue
            self.seq += 1
            jpeg = frames[self.seq % len(frames)]
            part = (f"--glassframe\r\nContent-Type: image/jpeg\r\nContent-Length: {len(jpeg)}\r\nX-Frame-Seq: {self.seq}\r\n"
                    f"X-Frame-Time-Ms: {int(time.time() * 1000)}\r\nX-Frame-Age-Ms: 1\r\nX-Frame-Encode-Ms: 0\r\n"
                    f"X-Frame-Send-Ms: {send_ms}\r\n\r\n").encode() + jpeg + b"\r\n"
            started = time.monotonic()
            for i in range(0, len(part), 16384):
                client.sendall(part[i:i + 16384])
                if self.kbps:
                    due = started + (i + 16384) * 8 / (self.kbps * 1000)
                    time.sleep(max(0, due - time.monotonic()))
            send_ms = round((time.monotonic() - started) * 1000)
            with self.lock:
                self.sent += 1
                if self.mark and opened >= self.mark and self.resumed_ms is None:
                    self.resumed_ms = round((time.monotonic() - self.mark) * 1000)
            tick += 1 / 30
            time.sleep(max(0, tick - time.monotonic()))
            tick = max(tick, time.monotonic() - 1 / 30)


desktop = FakeDesktop()


class Proxy:
    """TLS front end standing in for Tailscale Serve on the HTTPS port."""

    def __init__(self):
        self.server = None
        self.port = None
        self.backend = None
        self.dropped = False
        self.live = set()
        self.live_lock = threading.Lock()
        self.event_requests = []
        self.frozen = set()

    def stall(self):
        """Simulates a phone that slept: open tunnels stop carrying bytes but stay open, like a socket
        iOS suspended whose NAT mapping is gone. New connections work normally."""
        with self.live_lock:
            self.frozen = set(self.live)

    def drop(self):
        """Simulates the network going away: closes every tunnel and refuses new ones."""
        self.dropped = True
        with self.live_lock:
            for s in list(self.live):
                try:
                    s.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
                s.close()
            self.live.clear()

    def start(self):
        routes = json.load(open(os.path.join(ROOT, "serve.json")))
        self.port, target = next(iter(routes.items()))
        backend = self.backend = target.split("//", 1)[1]
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
        if self.dropped:
            raw.close()
            return
        client = upstream = None
        with self.live_lock:
            self.live.add(raw)
        try:
            # wrap_socket detaches `raw`, so drop() must see the TLS socket itself.
            client = context.wrap_socket(raw, server_side=True)
            with self.live_lock:
                self.live.discard(raw)
                self.live.add(client)
            head = b""
            while b"\r\n\r\n" not in head:
                chunk = client.recv(65536)
                if not chunk:
                    return
                head += chunk
            headers, rest = head.split(b"\r\n\r\n", 1)
            lines = [l for l in headers.split(b"\r\n") if not l.lower().startswith(b"tailscale-")]
            request = lines[0].decode(errors="replace").split(" ") if lines else []
            if desktop.on and len(request) > 1 and request[1].startswith("/api/desktop/"):
                length = next((int(l.split(b":", 1)[1]) for l in lines if l.lower().startswith(b"content-length:")), 0)
                while len(rest) < length:
                    chunk = client.recv(65536)
                    if not chunk:
                        return
                    rest += chunk
                return desktop.serve(client, request[0], request[1], rest[:length], lambda: self.frozen)
            if lines and lines[0].startswith(b"GET /api/events"):
                resume = next((l.split(b":", 1)[1].strip().decode() for l in lines
                               if l.lower().startswith(b"last-event-id:")), "")
                with self.live_lock:
                    self.event_requests.append(resume)
            lines.insert(1, f"Tailscale-User-Login: {LOGIN}".encode())
            host, port = backend.rsplit(":", 1)
            upstream = socket.create_connection((host, int(port)))
            with self.live_lock:
                self.live.add(upstream)
            upstream.sendall(b"\r\n".join(lines) + b"\r\n\r\n" + rest)
            pair = {client: upstream, upstream: client}
            while True:
                if client in self.frozen:
                    time.sleep(0.2)
                    continue
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
            with self.live_lock:
                for s in (raw, client, upstream):
                    self.live.discard(s)
            for s in (raw, client, upstream):
                if s is not None:
                    s.close()


class Provider(http.server.BaseHTTPRequestHandler):
    last_system = ""

    """Scripted OpenAI chat-completions stream. `flood N` streams N one-char deltas as fast as it can;
    `slow N` streams N short words half a second apart, a long turn that stays cheap to render."""
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", "0"))) or b"{}")
        last, tool_result = "", None
        system = [m.get("content") for m in body.get("messages", []) if m.get("role") == "system"]
        if system and isinstance(system[0], str):
            Provider.last_system = system[0]
        for message in body.get("messages", []):
            content = message.get("content")
            text = content if isinstance(content, str) else " ".join(
                part.get("text", "") for part in content or [] if isinstance(part, dict))
            if message.get("role") == "user":
                last, tool_result = text, None
            elif message.get("role") == "tool":
                tool_result = text
        flood = re.search(r"flood (\d+)", last)
        pause = re.search(r"pause (\d+)", last)
        if pause and tool_result is None:
            time.sleep(int(pause.group(1)))
        slow = re.search(r"slow (\d+)", last)
        lines = re.search(r"lines (\d+)", last)
        call = None
        if tool_result is None and "ask question" in last:
            call = ("ask_user_question", {"questions": [{"question": "Which color?", "multiSelect": False, "options": [
                {"label": "Blue", "description": "Calm"}, {"label": "Green", "description": "Fresh"}]}]})
        elif tool_result is None and "need secret" in last:
            call = ("request_secret", {"purpose": "Call the E2E API", "destination": "https://api.example.test",
                                       "tool": "web_secret_request", "operation": "GET /v1/ping"})
        if call:
            return self.tool_call(*call)
        if tool_result is not None:
            reply = "Tool said: " + re.sub(r"\s+", " ", tool_result)[:300]
            deltas, pause = [reply[i:i + 6] for i in range(0, len(reply), 6)], 0.02
        elif flood:
            deltas, pause = ["x"] * int(flood.group(1)), 0.001
        elif slow:
            deltas, pause = ["tick "] * int(slow.group(1)), 0.5
        elif lines:
            words = [f"Line {i + 1} of a long streamed reply keeps going with enough words to wrap. " for i in range(int(lines.group(1)))]
            deltas, pause = [w + ("\n\n" if i % 2 else "") for i, w in enumerate(words)], 0.15
        else:
            words = re.sub(r"\s+", " ", last).strip()[-200:]
            reply = f"Echo from the host: **{words}**\n\n```swift\nlet glass = \"live\"\n```"
            deltas, pause = [reply[i:i + 6] for i in range(0, len(reply), 6)], 0.03
        if not body.get("stream"):
            data = json.dumps({"choices": [{"message": {"role": "assistant", "content": "".join(deltas)}, "finish_reason": "stop"}],
                               "usage": {"prompt_tokens": 1, "completion_tokens": 1}}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Connection", "close")
        self.end_headers()
        self.close_connection = True

        def chunk(payload):
            self.wfile.write(f"data: {json.dumps(payload)}\n\n".encode())
            self.wfile.flush()

        for delta in deltas:
            chunk({"choices": [{"index": 0, "delta": {"content": delta}}]})
            time.sleep(pause)
        chunk({"choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]})
        chunk({"choices": [], "usage": {"prompt_tokens": 10, "completion_tokens": len(deltas)}})
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()


    def tool_call(self, name, arguments):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Connection", "close")
        self.end_headers()
        self.close_connection = True
        for payload in (
            {"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "id": "call_" + uuid.uuid4().hex[:8], "type": "function",
                                                                "function": {"name": name, "arguments": json.dumps(arguments)}}]}}]},
            {"choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]},
            {"choices": [], "usage": {"prompt_tokens": 10, "completion_tokens": 5}},
        ):
            self.wfile.write(f"data: {json.dumps(payload)}\n\n".encode())
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()


class SessionClient:
    """A second raw client on a thread's serve socket, standing in for another device."""
    clients = {}

    @classmethod
    def control(cls, thread, action):
        client = cls.clients.get(thread)
        if client is None:
            client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            client.connect(os.path.join(data_dir("remote"), f"serve-{thread}.sock"))
            client.settimeout(10)
            cls.clients[thread] = (client, client.makefile("rb"))
        sock, reader = cls.clients[thread]
        sock.sendall(json.dumps({"t": "control", "action": action}).encode() + b"\n")
        while True:
            line = reader.readline()
            if not line:
                raise RuntimeError("session socket closed")
            message = json.loads(line)
            if message.get("t") == "control":
                if action == "release":
                    sock.close()
                    del cls.clients[thread]
                return json.dumps({"can_write": message.get("can_write")})


class Gateway:
    """Talks to the remote gateway the way the web UI does (through the TLS front end)."""

    def __init__(self):
        self.capability = None
        self.context = ssl.create_default_context(cafile=os.path.join(ROOT, "tls", "ca.pem"))

    def request(self, method, path, body=None):
        if self.capability is None:
            token = link("remote").split("#owner=", 1)[1]
            self.capability = self.raw("POST", "/api/local", {"token": token}, {})["ownerCapability"]
        return self.raw(method, path, body, {"X-Glass-Owner-Capability": self.capability})

    def raw(self, method, path, body, headers):
        """Goes straight to the gateway with the headers Serve would add, so /net/drop
        only cuts the app off, not the "web UI" this harness plays."""
        origin = f"https://{HOST}" + ("" if str(proxy.port) == "443" else f":{proxy.port}")
        host, port = proxy.backend.rsplit(":", 1)
        data = json.dumps(body).encode() if body is not None else None
        connection = http.client.HTTPConnection(host, int(port), timeout=60)
        try:
            connection.request(method, path, body=data, headers={
                "Host": origin.split("//", 1)[1], "Origin": origin, "Content-Type": "application/json",
                "Tailscale-User-Login": LOGIN, **headers})
            response = connection.getresponse()
            payload = response.read()
        finally:
            connection.close()
        if response.status != 200:
            raise RuntimeError(f"{method} {path}: HTTP {response.status} {payload[:300]!r}")
        return json.loads(payload or b"null")

    def assistant_count(self, thread):
        records = self.request("GET", f"/api/history?id={thread}&compact=1")["records"]
        return sum(1 for r in records if r["record"].get("k") == "assistant")

    def wait_idle(self, thread, after=0, timeout=90):
        """Waits until a turn has added an assistant record beyond `after` and settled."""
        deadline = time.time() + timeout
        while time.time() < deadline:
            records = self.request("GET", f"/api/history?id={thread}&compact=1")["records"]
            kinds = [r["record"] for r in records]
            answers = [r for r in kinds if r.get("k") == "assistant"]
            requests = [r for r in kinds if r.get("k") == "serve_request"]
            if len(answers) > after and (not requests or requests[-1].get("status") != "accepted"):
                return answers[-1]["t"]
            time.sleep(0.5)
        raise RuntimeError("turn did not finish")


def seed(thread, pairs, size):
    """Appends long user/assistant records so history spans several 512 KiB windows."""
    path = os.path.join(data_dir("remote"), f"{thread}.jsonl")
    filler = ("lorem ipsum dolor sit amet " * (size // 27 + 1))[:size]
    with open(path, "a") as f:
        for i in range(pairs):
            f.write(json.dumps({"k": "user", "t": f"Seeded question {i + 1}"}) + "\n")
            f.write(json.dumps({"k": "assistant", "t": f"Seeded answer {i + 1}. {filler}"}) + "\n")


proxy = Proxy()
lock = threading.Lock()
gateway = Gateway()


def start_remote():
    link("remote")
    proxy.start()


def revoke():
    with lock:
        core("remote", "stop")
        proxy.stop()
        start_remote()
        gateway.capability = None


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
                if self.path == "/net/events":
                    return self.reply(json.dumps(proxy.event_requests))
                if self.path.startswith("/thread/context?"):
                    thread = urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query)["id"][0]
                    return self.reply(json.dumps(gateway.request("GET", f"/api/context?id={thread}")))
                if self.path == "/provider/system":
                    return self.reply(Provider.last_system)
                if self.path == "/desktop/report":
                    return self.reply(json.dumps(desktop.report()))
                if self.path == "/health":
                    return self.reply("ok")
            self.reply("not found", 404)
        except Exception as error:
            self.reply(str(error), 500)

    def do_POST(self):
        url = urllib.parse.urlparse(self.path)
        q = {k: v[0] for k, v in urllib.parse.parse_qs(url.query).items()}
        try:
            if url.path == "/revoke":
                revoke()
                return self.reply("revoked")
            if url.path == "/net/drop":
                proxy.drop()
                return self.reply("dropped")
            if url.path == "/net/stall":
                proxy.stall()
                return self.reply("stalled")
            if url.path == "/net/restore":
                proxy.dropped = False
                return self.reply("restored")
            if url.path == "/desktop/fake":
                desktop.on = q.get("on", "1") == "1"
                desktop.kbps = int(q.get("kbps", "0"))
                desktop.rtt = int(q.get("rtt", "0"))
                desktop.paused = q.get("paused") == "1"
                desktop.reset()
                for width in (1280, 960, 800, 640):
                    desktop.jpegs(width)
                return self.reply(json.dumps({w: sum(map(len, f)) // len(f) for w, f in desktop.frames.items()}))
            if url.path == "/desktop/pause":
                desktop.paused = q.get("on", "1") == "1"
                return self.reply("paused" if desktop.paused else "playing")
            if url.path == "/desktop/mark":
                desktop.reset()
                desktop.mark = time.monotonic()
                return self.reply("marked")
            if url.path == "/thread/new":
                thread = gateway.request("POST", "/api/new", {"workspace": ""})["id"]
                if "title" in q:
                    with open(os.path.join(data_dir("remote"), f"{thread}.jsonl"), "a") as f:
                        f.write(json.dumps({"k": "user", "t": q["title"]}) + "\n")
                        f.write(json.dumps({"k": "assistant", "t": "Ready when you are."}) + "\n")
                return self.reply(thread)
            if url.path == "/thread/rich":
                # A saved reply with a formula and a diagram, rendered offline by the app.
                with open(os.path.join(data_dir("remote"), f"{q['id']}.jsonl"), "a") as f:
                    f.write(json.dumps({"k": "user", "t": "Show the maths"}) + "\n")
                    f.write(json.dumps({"k": "assistant", "t": "Area:\n\n$$\\pi r^2$$\n\n```mermaid\ngraph TD\n  A[Pair] --> B[Work]\n```\n"}) + "\n")
                return self.reply("rich")
            if url.path == "/thread/markdown":
                # A saved reply using everyday Markdown the app must draw natively.
                markdown = "\n".join([
                    "## Tables", "", "| Name | Role | Status |", "|------|:----:|-------:|",
                    "| Alice | Admin | Active |", "| Bob | Editor | Inactive |", "",
                    "<details>", "<summary>Full report</summary>", "", "Hidden **details** inside.", "", "</details>", "",
                    "Inline math: $\\sum_{i=1}^{n} i = \\frac{n(n+1)}{2}$ costs $5 and $10.", "",
                    "- [x] Set up project", "- [ ] Deploy to production", "",
                    "> This is a blockquote.", "> > Level two", "",
                    "1. First item", "   - Nested bullet", "2. Second item", "",
                    "![Image](sample_gradient.png)", "", "Done."])
                with open(os.path.join(data_dir("remote"), f"{q['id']}.jsonl"), "a") as f:
                    f.write(json.dumps({"k": "user", "t": "Show markdown"}) + "\n")
                    f.write(json.dumps({"k": "assistant", "t": markdown}) + "\n")
                return self.reply("markdown")
            if url.path == "/thread/count":
                return self.reply(str(gateway.assistant_count(q["id"])))
            if url.path == "/thread/components":
                # A saved reply with reply components: drawn natively, or shown as their fallback.
                blocks = [
                    {"type": "table", "v": 1, "fallback": "| Service | Status |\n| --- | --- |\n| api | up |\n| web | down |",
                     "data": {"title": "Services", "columns": [{"key": "service", "label": "Service"}, {"key": "status", "label": "Status"}],
                              "rows": [{"service": "api", "status": "up"}, {"service": "web", "status": "down"}], "filter": "status"}},
                    {"type": "chart", "v": 1, "fallback": "Requests: Mon 120, Tue 180, Wed 150.",
                     "data": {"title": "Requests", "kind": "bar", "series": [{"name": "Requests", "points": [
                         {"x": "Mon", "y": 120}, {"x": "Tue", "y": 180}, {"x": "Wed", "y": 150}]}]}},
                    {"type": "form", "v": 1, "fallback": "1. Which host?\n2. Notes",
                     "data": {"title": "Deploy", "fields": [{"id": "host", "label": "Host", "kind": "select", "options": ["staging", "production"]},
                                                           {"id": "notes", "label": "Notes", "kind": "text"}], "submit": "Add Answers"}},
                    {"type": "writing", "v": 1, "fallback": "**Subject:** Deploy done\n\nThe deploy finished.",
                     "data": {"kind": "email", "to": "team@example.invalid", "subject": "Deploy done", "body": "The deploy finished."}},
                    {"type": "weather", "v": 1, "example": True, "fallback": "_Example data, not live._\n\nLondon: 12°C, cloudy.",
                     "data": {"location": "London", "unit": "C", "current": {"temp": 12, "condition": "Cloudy", "icon": "cloud.fill"}}},
                    {"type": "hologram", "v": 1, "fallback": "Fallback for a type this app does not know.", "data": {}},
                ]
                text = "Status:\n\n" + "\n\n".join("```glass-ui\n" + json.dumps(b) + "\n```" for b in blocks) + "\n"
                with open(os.path.join(data_dir("remote"), f"{q['id']}.jsonl"), "a") as f:
                    f.write(json.dumps({"k": "user", "t": "Show the services"}) + "\n")
                    f.write(json.dumps({"k": "assistant", "t": text}) + "\n")
                return self.reply("components")
            if url.path == "/workspace/change":
                # A committed file with an unstaged edit, so Changes and Files have real content.
                git = ["git", "-C", WS, "-c", "user.name=E2E", "-c", "user.email=e2e@example.invalid"]
                path = os.path.join(WS, "notes.md")
                with open(path, "w") as f:
                    f.write("# Notes\n\nfirst line\n")
                subprocess.run(git + ["add", "notes.md"], check=True)
                subprocess.run(git + ["commit", "-q", "-m", "seed"], check=True)
                with open(path, "a") as f:
                    f.write("added from the harness\n")
                return self.reply("changed")
            if url.path == "/workspace/read":
                with open(os.path.join(WS, os.path.basename(q["path"]))) as f:
                    return self.reply(f.read())
            if url.path == "/workspace/status":
                out = subprocess.run(["git", "-C", WS, "status", "--porcelain"], check=True, capture_output=True, text=True)
                return self.reply(out.stdout)
            if url.path == "/thread/seed":
                seed(q["id"], int(q.get("pairs", "40")), int(q.get("size", "20000")))
                return self.reply("seeded")
            if url.path == "/thread/send":
                before = gateway.assistant_count(q["id"])
                gateway.request("POST", "/api/send", {"id": q["id"], "message": {
                    "t": "prompt", "text": q["text"], "request_id": str(uuid.uuid4())}})
                if q.get("wait") == "1":
                    return self.reply(gateway.wait_idle(q["id"], before))
                return self.reply("sent")
            if url.path in ("/control/claim", "/control/release"):
                return self.reply(SessionClient.control(q["id"], url.path.rsplit("/", 1)[1]))
            if url.path == "/settings/set":
                # Another client saves a preference, so the app's next save hits a revision conflict.
                revision = gateway.request("GET", "/api/settings")["revision"]
                gateway.request("POST", "/api/settings", {"revision": revision, "action": "preference",
                                                          "key": q["key"], "value": json.loads(q["value"])})
                return self.reply("saved")
            if url.path == "/settings/get":
                return self.reply(json.dumps(gateway.request("GET", "/api/settings")["defaults"].get(q["key"])))
            if url.path == "/thread/wait":
                return self.reply(gateway.wait_idle(q["id"]))
            self.reply("not found", 404)
        except Exception as error:
            self.reply(f"{type(error).__name__}: {error}", 500)


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
    provider = http.server.ThreadingHTTPServer(("127.0.0.1", PROVIDER_PORT), Provider)
    threading.Thread(target=provider.serve_forever, daemon=True).start()
    start_remote()
    link("local", "--local")
    server = http.server.ThreadingHTTPServer(("127.0.0.1", CONTROL_PORT), Control)
    print(f"ready https://{HOST}:{proxy.port} control http://127.0.0.1:{CONTROL_PORT}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
