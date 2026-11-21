#!/usr/bin/env python3
"""Live end-to-end check for a Marble host.

Python stdlib only. Each step prints PASS/FAIL plus timing; exits non-zero on
any FAIL. The token is read from --token-file and never printed.

The free-tier model provider rate-limits hard (HTTP 429 + ~60s cooldowns), so
model-backed steps retry with backoff instead of failing on the first window.
"""

import argparse
import hashlib
import hmac as hmac_mod
import json
import secrets
import socket
import ssl
import sys
import threading
import time
import urllib.request
import urllib.error

HOST = None
TOKEN = None
SSL_CTX = None
FORCE_QUIESCE = False
CREATED = []  # conversationIds created by this script, deleted at the end
RESULTS = []
RATE_LIMIT_WAIT = 65  # provider cooldown is ~60s
STEP_ATTEMPTS = 4


def request(path, method="GET", body=None, token=True, timeout=30):
    req = urllib.request.Request(HOST + path, method=method)
    req.add_header("Accept", "application/json")
    if token and TOKEN:
        req.add_header("Authorization", "Bearer " + TOKEN)
    data = None
    if body is not None:
        req.add_header("Content-Type", "application/json")
        data = json.dumps(body).encode()
    try:
        with urllib.request.urlopen(req, data=data, timeout=timeout, context=SSL_CTX) as resp:
            raw = resp.read().decode()
            return resp.status, (json.loads(raw) if raw else {})
    except urllib.error.HTTPError as e:
        raw = e.read().decode()
        try:
            return e.code, json.loads(raw)
        except json.JSONDecodeError:
            return e.code, {"error": raw}


def state_doc():
    _, doc = request("/api/state?lean=1")
    return doc


def busy():
    """Running tasks, admission queue, pending approvals."""
    doc = state_doc()
    running = [t for t in doc.get("tasks", []) if t.get("status") == "running"]
    return running, doc.get("control", {}).get("queue", []), doc.get("approvals", [])


def quiesce(timeout=300):
    """Wait for the host to go idle. Only with --force-quiesce does it deny
    pending approvals and cancel running work, which may belong to a person."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        running, queue, approvals = busy()
        if not FORCE_QUIESCE:
            if not running and not queue:
                return True
            time.sleep(5)
            continue
        for approval in approvals:
            request(
                f"/api/approvals/{approval['requestId']}/deny", "POST", {}
            )
        if not running and not queue:
            return True
        for task in running:
            request(f"/api/tasks/{task['id']}/cancel", "POST", {})
        time.sleep(5)
    running, queue, _ = busy()
    return not running and not queue


def sse_events(since, stop_event, sink):
    """Read /api/events until stop_event set; sink receives decoded frames."""
    req = urllib.request.Request(f"{HOST}/api/events?since={since}")
    if TOKEN:
        req.add_header("Authorization", "Bearer " + TOKEN)
    try:
        with urllib.request.urlopen(req, timeout=600, context=SSL_CTX) as resp:
            data_lines = []
            for raw in resp:
                if stop_event.is_set():
                    return
                line = raw.decode().rstrip("\n").rstrip("\r")
                if line == "":
                    if data_lines:
                        try:
                            sink.append(json.loads("\n".join(data_lines)))
                        except json.JSONDecodeError:
                            pass
                    data_lines = []
                    continue
                if line.startswith(":"):
                    continue
                if line.startswith("data:"):
                    data_lines.append(line[5:].strip())
    except Exception:
        return


class Stream:
    """Background SSE reader starting at the current sequence."""

    def __init__(self, since=0):
        self.events = []
        self.lock = threading.Lock()
        self.stop = threading.Event()
        self.thread = threading.Thread(
            target=sse_events, args=(since, self.stop, self), daemon=True
        )
        self.thread.start()

    def append(self, event):
        with self.lock:
            self.events.append(event)

    def find(self, kind, predicate=lambda e: True, since_index=0):
        with self.lock:
            for i, e in enumerate(self.events[since_index:], since_index):
                if e.get("kind") == kind and predicate(e):
                    return i, e
        return None, None

    def wait_for(self, kind, predicate=lambda e: True, timeout=120):
        deadline = time.time() + timeout
        index = 0
        while time.time() < deadline:
            idx, event = self.find(kind, predicate, index)
            if event is not None:
                return event, idx + 1
            index = len(self.events)
            time.sleep(0.25)
        return None, index

    def rate_limited_recently(self):
        with self.lock:
            return any(
                "429" in (e.get("data", {}).get("reason") or "")
                for e in self.events[-20:]
                if e.get("kind") == "task_retry"
            )

    def close(self):
        self.stop.set()


def step(name):
    print(f"-- {name}", flush=True)
    return time.time()


def done(start, ok, detail=""):
    RESULTS.append(bool(ok))
    print(
        f"{'PASS' if ok else 'FAIL'} ({time.time() - start:5.1f}s) {detail}",
        flush=True,
    )
    return ok


def send_turn(text):
    status, doc = request("/api/turns", "POST", {"text": text})
    if status != 202 or not doc.get("conversationId"):
        return None
    CREATED.append(doc["conversationId"])
    return doc


def wait_task(stream, task_id, timeout):
    """Wait for the settled event of one task; None on timeout."""
    deadline = time.time() + timeout
    index = 0
    while time.time() < deadline:
        _, e = stream.find("settled", lambda e: e.get("taskId") == task_id, index)
        if e is not None:
            return e
        index = len(stream.events)
        time.sleep(0.25)
    return None


def run_model_step(name, attempt_fn, timeout=180):
    """Run a model-backed step; on failure while the provider is
    rate-limiting, back off and retry the whole step."""
    t = step(name)
    for attempt in range(1, STEP_ATTEMPTS + 1):
        if attempt > 1:
            print(f"   attempt {attempt} after {RATE_LIMIT_WAIT}s backoff", flush=True)
            time.sleep(RATE_LIMIT_WAIT)
            quiesce(120)
        ok, detail = attempt_fn(timeout)
        if ok:
            return done(t, True, detail)
        # A settled turn that failed on a provider error, or no turn at all
        # while the provider is cooling down, is worth another attempt.
        last_chance = attempt >= STEP_ATTEMPTS
        if last_chance:
            return done(t, False, detail)
        print(f"   attempt {attempt} failed ({detail})", flush=True)
    return done(t, False, detail)


def chat_to_settled(stream, prompt, timeout):
    turn = send_turn(prompt)
    if not turn:
        return False, "turn not accepted"
    event = wait_task(stream, turn["taskId"], timeout)
    if event is None:
        return False, "no settled event"
    reason = event.get("data", {}).get("reason") or event.get("data", {}).get("status")
    return reason == "completed", f"settled reason={reason} conv={turn['conversationId']}"


def tls_pin_check():
    """Verify the cert the client saw against the host's HMAC proof."""
    from urllib.parse import urlparse
    parsed = urlparse(HOST)
    host = parsed.hostname
    port = parsed.port or 443
    ctx = ssl._create_unverified_context()
    with socket.create_connection((host, port), timeout=15) as sock:
        with ctx.wrap_socket(sock, server_hostname=host) as secure:
            der = secure.getpeercert(binary_form=True)
    if not der:
        return False, "no peer certificate"
    observed = hashlib.sha256(der).hexdigest()
    nonce = secrets.token_hex(32)
    status, body = request(f"/api/tls-proof?nonce={nonce}", token=False)
    if status != 200:
        return False, f"tls-proof status={status} body={body}"
    if body.get("certSha256") != observed:
        return False, "server certSha256 did not match the observed leaf"
    message = (
        b"marble-tls-pin-v1\0" + nonce.encode() + b"\0" + observed.encode()
    )
    expected = hmac_mod.new(
        TOKEN.encode(), message, hashlib.sha256
    ).hexdigest()
    if not hmac_mod.compare_digest(expected, body.get("proof", "")):
        return False, "proof HMAC mismatch"
    status, _ = request("/api/state?lean=1")
    if status != 200:
        return False, f"state over TLS status={status}"
    return True, "pin verified and state answers over TLS"


def main():
    global HOST, TOKEN, FORCE_QUIESCE, SSL_CTX
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", required=True)
    parser.add_argument("--token-file", required=True)
    parser.add_argument(
        "--force-quiesce",
        action="store_true",
        help="deny pending approvals and cancel running tasks so the run starts idle",
    )
    parser.add_argument(
        "--tls-pin-check",
        action="store_true",
        help="verify the certificate pin proof, then run the quick steps over https",
    )
    args = parser.parse_args()
    FORCE_QUIESCE = args.force_quiesce
    HOST = args.host.rstrip("/")
    TOKEN = open(args.token_file).read().strip()
    SSL_CTX = (
        ssl._create_unverified_context() if HOST.startswith("https") else None
    )

    if args.tls_pin_check:
        t = step("tls-proof pins the observed certificate")
        ok, detail = tls_pin_check()
        done(t, ok, detail)
        if not ok:
            return finish(None)

        t = step("healthz over TLS")
        status, health = request("/healthz", token=False)
        done(t, status == 200 and health.get("ok") is True, f"version={health.get('version')}")

        t = step("unauthenticated state rejected over TLS")
        status, _ = request("/api/state?lean=1", token=False)
        done(t, status == 401, f"status={status}")

        t = step("host idle")
        done(t, quiesce(300), "")

        _, health_doc = request("/healthz", token=False)
        stream = Stream(health_doc.get("sequence", 0))
        run_model_step(
            "single chat over TLS streams to settled",
            lambda to: chat_to_settled(
                stream, "Reply with exactly the word: marble-tls-check", to
            ),
        )
        return finish(stream)

    t = step("healthz and version")
    status, health = request("/healthz", token=False)
    ok = bool(status == 200 and health.get("ok") and health.get("version"))
    done(t, ok, f"version={health.get('version')} authRequired={health.get('authRequired')}")
    if not ok:
        return finish(None)

    t = step("unauthenticated state rejected")
    status, _ = request("/api/state?lean=1", token=False)
    done(t, status == 401, f"status={status}")

    t = step("host idle")
    done(t, quiesce(300), "")

    _, health_doc = request("/healthz", token=False)
    stream = Stream(health_doc.get("sequence", 0))

    run_model_step(
        "single chat streams to settled",
        lambda to: chat_to_settled(stream, "Reply with exactly the word: marble-alpha-one", to),
    )

    def two_chats(to):
        t1 = send_turn("Reply with exactly the word: alpha-two-a")
        t2 = send_turn("Reply with exactly the word: alpha-two-b")
        if not t1 or not t2:
            return False, "turns not accepted"
        a = wait_task(stream, t1["taskId"], to)
        b = wait_task(stream, t2["taskId"], to)
        return bool(a and b), ""

    run_model_step("two concurrent chats both complete", two_chats)

    def cancel_turn(to):
        turn = send_turn(
            "Write a very long, detailed essay of at least 2000 words about "
            "the history of marbles. Do not stop early."
        )
        if not turn:
            return False, "turn not accepted"
        time.sleep(1)
        request(f"/api/tasks/{turn['taskId']}/cancel", "POST", {})
        event = wait_task(stream, turn["taskId"], 90)
        reason = (event or {}).get("data", {}).get("reason") or \
            (event or {}).get("data", {}).get("status")
        if reason == "cancelled":
            cancel_turn.task_id = turn["taskId"]
            return True, f"settled reason={reason}"
        if reason in ("completed",):
            # The model beat the cancel; try again on the next attempt.
            return False, "turn finished before cancel landed"
        return False, f"settled reason={reason}"

    run_model_step("cancel a running turn", cancel_turn)
    cancelled_task = getattr(cancel_turn, "task_id", None)

    def retry_turn(to):
        if not cancelled_task:
            return False, "no cancelled task to retry"
        status, doc = request("/api/turns/retry", "POST", {"taskId": cancelled_task})
        if status not in (200, 202):
            return False, f"retry status={status} body={doc}"
        conv = doc.get("conversationId")
        new_task = doc.get("taskId")
        if conv and conv not in CREATED:
            CREATED.append(conv)
        event = wait_task(stream, new_task, to) if new_task else None
        if event is None:
            return False, "no settled event for retried task"
        reason = event.get("data", {}).get("reason")
        return reason == "completed", f"settled={event.get('data', {})}"

    run_model_step("retry the cancelled turn", retry_turn, timeout=300)

    tag = str(int(time.time()) % 100000)
    prompts = [
        "Please create a file named {name} in the workspace containing the "
        "single word {stem} as a real deliverable.",
        "I need the file {name} in the workspace with the contents {stem}. "
        "Create it now, please.",
        "Create {name} containing {stem}.",
        "Write {stem} into a new workspace file called {name}.",
    ]

    def approval_round_trip(decision, letter, to):
        for i in range(len(prompts)):
            name = f"alpha-approval-{letter}-{tag}-{i}.txt"
            stem = name.rsplit(".", 1)[0]
            turn = send_turn(prompts[i].format(name=name, stem=stem))
            if not turn:
                continue
            event, _ = stream.wait_for(
                "approval_pending",
                lambda e: e.get("conversationId") == turn["conversationId"],
                timeout=to,
            )
            if event is None:
                continue
            req_id = event["data"].get("requestId")
            if not req_id:
                continue
            status, _ = request(
                f"/api/approvals/{req_id}/{decision}", "POST", {"remember": False}
            )
            resolved, _ = stream.wait_for(
                "approval_resolved",
                lambda e: e.get("data", {}).get("requestId") == req_id,
                timeout=90,
            )
            if status == 200 and resolved is not None:
                return True, f"requestId={req_id}"
            return False, f"decision status={status} resolved={resolved is not None}"
        return False, "no approval_pending produced"

    run_model_step(
        "approval pending → allow",
        lambda to: approval_round_trip("allow", "a", to),
    )
    run_model_step(
        "approval pending → deny",
        lambda to: approval_round_trip("deny", "b", to),
    )

    _, doc = request("/api/state?lean=1")
    if doc.get("control", {}).get("profile") != "workspace":
        request("/api/control/profile", "POST", {"profile": "workspace"})

    def marker_step(to):
        turn = send_turn("Reply with exactly the word: zqxwv-e2e-marker")
        if not turn:
            return False, "turn not accepted"
        event = wait_task(stream, turn["taskId"], to)
        if event is None:
            return False, "no settled event"
        status, doc = request("/api/transcript/search?q=zqxwv-e2e-marker")
        hits = doc.get("hits", [])
        found = status == 200 and any(
            "zqxwv-e2e-marker" in h.get("text", "") for h in hits
        )
        if found:
            marker_step.conv = turn["conversationId"]
        return found, f"hits={len(hits)}"

    run_model_step("transcript search finds marker", marker_step)
    marker_conv = getattr(marker_step, "conv", None)

    t = step("export conversation document")
    exported = None
    if marker_conv:
        for _ in range(5):
            status, doc = request(f"/api/export?conversationId={marker_conv}")
            exported = (
                status == 200
                and doc.get("format") == "glass-marble-conversation/v1"
                and doc.get("transcript")
            )
            if exported:
                break
            time.sleep(2)
    done(t, bool(exported), "")

    t = step("model endpoint round trip")
    status, doc = request("/api/state?lean=1")
    original = doc.get("model", "")
    status, _ = request("/api/model", "POST", {"model": ""})
    _, doc = request("/api/state?lean=1")
    done(t, status in (200, 202) and doc.get("model", "") == original,
         f"model={doc.get('model')!r}")

    return finish(stream)


def finish(stream):
    if stream:
        stream.close()
    t = step("cleanup conversations")
    failed = 0
    deadline = time.time() + 300
    for conv in dict.fromkeys(c for c in CREATED if c):
        while True:
            status, doc = request(f"/api/conversations/{conv}/delete", "POST", {})
            if status == 200:
                break
            if status == 409 and time.time() < deadline:
                time.sleep(5)
                continue
            failed += 1
            print(f"   delete {conv}: {status} {doc}", flush=True)
            break
    done(t, failed == 0, f"deleted {len(set(CREATED)) - failed}/{len(set(CREATED))}")

    print(f"== {sum(RESULTS)} passed, {len(RESULTS) - sum(RESULTS)} failed")
    return 0 if all(RESULTS) else 1


if __name__ == "__main__":
    sys.exit(main())
