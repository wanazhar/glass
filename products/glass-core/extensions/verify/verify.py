#!/usr/bin/env python3
"""glass verify extension: run the project's verifier after writes.

Aider measured its lint-and-revert guardrail as worth more than its edit
tool, and Claude Code routes the same duty through a project `verify`
skill. This is that idea in glass-core's architecture: after a bash command
writes files, the verifier runs through the host's own bash tool — so the
sandbox policy, timeout, and output caps apply — and a failure is appended
to the tool result the model is about to read, before it can build three
turns of work on a broken file.

Config at ~/.config/glass-core/verify.json (or GC_VERIFY_CONFIG):

    {
      "command": "cargo test --quiet",   // required to enable, unless detected
      "timeout_s": 90,                   // host bash timeout for the run
      "debounce_s": 8                    // at most one verify per window
    }

With no config, detection is deliberately small: `Cargo.toml` ->
`cargo check --quiet`; `package.json` with `scripts.typecheck` ->
`npm run --silent typecheck`. Anything else stays silent until configured.
"""

import json
import os
import sys
import time

PROTOCOL = 1
WRITE_MARKER = "[wrote "
OUTPUT_CAP = 4096
HEAD_CAP = 1200


def config_path():
    override = os.environ.get("GC_VERIFY_CONFIG")
    if override:
        return override
    xdg = os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config")
    return os.path.join(xdg, "glass-core", "verify.json")


def load_config():
    try:
        with open(config_path(), "r", encoding="utf-8") as fh:
            data = json.load(fh)
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def detect(workspace):
    """Cheap, honest defaults. Everything else stays opt-in."""
    try:
        if os.path.isfile(os.path.join(workspace, "Cargo.toml")):
            return "cargo check --quiet"
        package = os.path.join(workspace, "package.json")
        if os.path.isfile(package):
            with open(package, "r", encoding="utf-8", errors="replace") as fh:
                data = json.load(fh)
            if isinstance(data.get("scripts"), dict) and data["scripts"].get("typecheck"):
                return "npm run --silent typecheck"
    except (OSError, ValueError):
        return None
    return None


def parse_exit(body):
    """Split the host bash body `[exit N]\\n<output>` into (code, output)."""
    first, _, rest = body.partition("\n")
    prefix = first.strip()
    if prefix.startswith("[exit ") and prefix.endswith("]"):
        try:
            return int(prefix[6:-1]), rest
        except ValueError:
            return None, body
    return None, body


def clip(text):
    """Head and tail of a failure, bounded; the middle is rarely the fix."""
    text = text.strip("\n")
    if len(text) <= OUTPUT_CAP:
        return text
    head = text[:HEAD_CAP]
    tail = text[-(OUTPUT_CAP - HEAD_CAP - 40):]
    return f"{head}\n[… {len(text) - OUTPUT_CAP} bytes elided …]\n{tail}"


class Verifier:
    def __init__(self, workspace):
        cfg = load_config()
        command = cfg.get("command")
        self.command = (command.strip() if isinstance(command, str) else "") or detect(workspace)
        timeout = cfg.get("timeout_s")
        self.timeout_s = max(5, min(int(timeout) if timeout is not None else 90, 600))
        debounce = cfg.get("debounce_s")
        # `or` would treat an explicit 0 as missing; testing for None keeps
        # "verify after every write" expressible.
        self.debounce_s = max(0.0, min(float(debounce) if debounce is not None else 8, 3600))
        self.last_run = 0.0
        self.last_failed = False
        self.reported_unavailable = False

    def run_command(self):
        """Run the verifier through the host's bash tool (nested request)."""
        send(
            {
                "type": "call_tool",
                "call_id": "verify",
                "name": "bash",
                "arguments": json.dumps(
                    {"command": self.command, "timeout_ms": self.timeout_s * 1000}
                ),
            }
        )
        for _ in range(100000):
            reply = read_line()
            if reply is None:
                return None
            if reply.get("type") == "call_tool_result":
                return str(reply.get("body") or "")
        return None

    def maybe_verify(self, body):
        """The block to append to a tool result, or None to leave it alone."""
        if not self.command or WRITE_MARKER not in body:
            return None
        now = time.monotonic()
        if now - self.last_run < self.debounce_s:
            return None
        self.last_run = now
        output = self.run_command()
        if output is None:
            if self.reported_unavailable:
                return None
            self.reported_unavailable = True
            return "\n[verify unavailable: the host refused the nested bash call]"
        code, text = parse_exit(output)
        if code is None:
            # No exit marker means the host answered with an error body.
            if self.reported_unavailable:
                return None
            self.reported_unavailable = True
            return f"\n[verify unavailable] {clip(text)[:400]}"
        if code == 0:
            if not self.last_failed:
                return None
            self.last_failed = False
            return "\n[verify ok]"
        self.last_failed = True
        return f"\n[verify exit {code}] {self.command}\n{clip(text)}"


def system_prompt(verifier):
    if verifier is None or not verifier.command:
        return ""
    return (
        f"After a command writes files, `{verifier.command}` runs automatically "
        "(debounced) and its result is appended to the tool output as "
        "`[verify exit N]`. Treat that block as authoritative: fix what it "
        "reports before building further on the change."
    )


def manifest(verifier):
    return {
        "name": "verify",
        "version": "1.0.0",
        "system_prompt": system_prompt(verifier),
        "tools": [],
    }


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def read_line():
    line = sys.stdin.readline()
    if not line:
        return None
    line = line.strip()
    return json.loads(line) if line else None


def main():
    verifier = None
    while True:
        req = read_line()
        if req is None:
            break
        kind = req.get("type")
        if kind == "hello":
            verifier = Verifier(req.get("workspace") or os.getcwd())
            reply = {"id": req.get("id"), "protocol": PROTOCOL}
            reply.update(manifest(verifier))
            send(reply)
        elif kind == "system_prompt":
            send({"id": req.get("id"), "text": system_prompt(verifier)})
        elif kind == "authorize":
            send({"id": req.get("id"), "allow": True})
        elif kind == "tool_result":
            body = str(req.get("body") or "")
            addition = verifier.maybe_verify(body) if verifier else None
            send({"id": req.get("id"), "body": body + addition if addition else body})
        elif kind == "command":
            if req.get("name") == "verify":
                if verifier is None or not verifier.command:
                    output = "verify: inactive (no command configured or detected)"
                else:
                    state = "failed" if verifier.last_failed else "ok"
                    output = (
                        f"verify: {verifier.command}\n"
                        f"debounce {verifier.debounce_s:g}s · timeout {verifier.timeout_s}s · "
                        f"last result: {state}"
                    )
            else:
                output = "verify: unknown command"
            send({"id": req.get("id"), "output": output})
        elif kind in ("goodbye", "shutdown"):
            break


def selftest():
    import tempfile

    assert parse_exit("[exit 7]\nboom") == (7, "boom")
    assert parse_exit("plain body") == (None, "plain body")
    assert len(clip("x" * 20_000)) <= OUTPUT_CAP + 60
    assert clip("short") == "short"

    with tempfile.TemporaryDirectory() as tmp:
        assert detect(tmp) is None
        open(os.path.join(tmp, "Cargo.toml"), "w").close()
        assert detect(tmp) == "cargo check --quiet"
        os.remove(os.path.join(tmp, "Cargo.toml"))
        with open(os.path.join(tmp, "package.json"), "w") as fh:
            json.dump({"scripts": {"typecheck": "tsc --noEmit"}}, fh)
        assert detect(tmp) == "npm run --silent typecheck"

        verifier = Verifier(tmp)
        verifier.command = "fake"
        verifier.debounce_s = 0
        calls = {"n": 0}

        def fake_run():
            calls["n"] += 1
            return "[exit 1]\nBOOM"

        verifier.run_command = fake_run
        block = verifier.maybe_verify("[exit 0]\n[write verified]\n[wrote a.txt]")
        assert block and "[verify exit 1]" in block and "BOOM" in block, block
        # A failure stays sticky: the next success reports back-to-green once.
        verifier.run_command = lambda: "[exit 0]\n"
        recovered = verifier.maybe_verify("[wrote a.txt]")
        assert recovered == "\n[verify ok]", recovered
        assert verifier.maybe_verify("[wrote a.txt]") is None
        # No write marker: never runs.
        assert verifier.maybe_verify("[exit 0]\nhello") is None
        assert calls["n"] == 1
        # Debounce suppresses a second run inside the window.
        verifier.debounce_s = 3600
        verifier.run_command = fake_run
        assert verifier.maybe_verify("[wrote b.txt]") is None
    print("PASS verify extension selftest")


if __name__ == "__main__":
    if "--selftest" in sys.argv:
        selftest()
    else:
        main()
