#!/usr/bin/env python3
"""Offline stand-in for `glass-core runtime`.

Speaks the documented newline-delimited JSON-RPC 2.0 host runtime so Glass Dev's
resident harness adapter can be exercised without a provider account, a model
call, or a network. It mirrors the real engine's ordering:

* `initialize` reports protocol v2 with negotiated mixed tool ownership.
* `auth/status` reports Core's `credentialReady` provider-readiness field.
* `session/new` returns a fixed session id.
* `turn/prompt` emits `turn_start`, requests exactly one host tool, waits for
  the host result, streams text, and only then answers the prompt request.

Usage: `python3 fake_glass_core_runtime.py runtime` (the `runtime` argument is
accepted and ignored so the fixture matches the real argv shape).
"""

from __future__ import annotations

import json
import os
import select
import sys
import time

SESSION_ID = "fixture-session"
STORAGE_DIR = None
MODE = "mixed"


def log_request(method: str, params: dict) -> None:
    if STORAGE_DIR is None:
        return
    with open(os.path.join(STORAGE_DIR, "fixture-requests.jsonl"), "a", encoding="utf-8") as log:
        log.write(json.dumps({"method": method, "params": params}, separators=(",", ":")) + "\n")
    workspace_log = os.path.join(os.getcwd(), ".glass", "e2e-core-requests.jsonl")
    if os.path.exists(os.path.join(os.getcwd(), ".glass", "e2e-core-request-log")):
        with open(workspace_log, "a", encoding="utf-8") as log:
            log.write(json.dumps({"method": method, "params": params}, separators=(",", ":")) + "\n")


def send(message: dict) -> None:
    sys.stdout.write(json.dumps(message, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def reply(identifier, result: dict) -> None:
    send({"jsonrpc": "2.0", "id": identifier, "result": result})


def event(kind: str, data: dict, turn: str) -> None:
    send(
        {
            "jsonrpc": "2.0",
            "method": "runtime/event",
            "params": {"sessionId": SESSION_ID, "turnId": turn, "type": kind, "data": data},
        }
    )


def request_tool(call_id: str, name: str, arguments: dict) -> dict:
    """Send one host tool request and return the matching result object."""
    frame_id = f"host:{SESSION_ID}:1:{call_id}"
    send(
        {
            "jsonrpc": "2.0",
            "id": frame_id,
            "method": "tool/execute",
            "params": {
                "sessionId": SESSION_ID,
                "turnId": "turn-1",
                "callId": call_id,
                "name": name,
                "arguments": arguments,
            },
        }
    )
    while True:
        line = sys.stdin.readline()
        if not line:
            raise SystemExit(0)
        line = line.strip()
        if not line:
            continue
        message = json.loads(line)
        if message.get("id") == frame_id and "method" not in message:
            return message.get("result", {})


def run_turn(identifier, params: dict) -> None:
    text = str(params.get("text", ""))
    turn = "turn-1"
    event("turn_start", {}, turn)
    gate_root = os.path.join(os.getcwd(), ".glass")
    if "issue64-task-recovery-prompt" in text:
        if os.path.exists(os.path.join(gate_root, "e2e-core-task-recovery-gate")):
            with open(
                os.path.join(gate_root, "e2e-core-task-recovery-pid"), "w", encoding="utf-8"
            ) as marker:
                marker.write(str(os.getpid()))
            with open(
                os.path.join(gate_root, "e2e-core-task-recovery-started"), "w", encoding="utf-8"
            ) as marker:
                marker.write("started")
            deadline = time.monotonic() + 30
            while not os.path.exists(os.path.join(gate_root, "e2e-core-task-recovery-release")):
                if time.monotonic() >= deadline:
                    raise TimeoutError("timed out waiting for task recovery test release")
                time.sleep(0.02)
        if os.path.exists(os.path.join(gate_root, "e2e-core-task-recovery-resumed")):
            with open(
                os.path.join(gate_root, "e2e-core-task-recovery-resumed-pid"),
                "w",
                encoding="utf-8",
            ) as marker:
                marker.write(str(os.getpid()))
            if os.path.exists(
                os.path.join(gate_root, "e2e-core-task-recovery-resume-gate")
            ):
                with open(
                    os.path.join(gate_root, "e2e-core-task-recovery-resume-started"),
                    "w",
                    encoding="utf-8",
                ) as marker:
                    marker.write("started")
                deadline = time.monotonic() + 30
                while not os.path.exists(
                    os.path.join(gate_root, "e2e-core-task-recovery-resume-release")
                ):
                    if time.monotonic() >= deadline:
                        raise TimeoutError("timed out waiting for resumed task test release")
                    time.sleep(0.02)
    if "active-run-queue-target" in text and os.path.exists(
        os.path.join(gate_root, "e2e-core-active-steer")
    ):
        with open(
            os.path.join(gate_root, "e2e-core-active-steer-pid"), "w", encoding="utf-8"
        ) as marker:
            marker.write(str(os.getpid()))
        with open(
            os.path.join(gate_root, "e2e-core-active-steer-started"), "w", encoding="utf-8"
        ) as marker:
            marker.write("started")
        deadline = time.monotonic() + 30
        while not os.path.exists(os.path.join(gate_root, "e2e-core-active-steer-release")):
            if time.monotonic() >= deadline:
                raise TimeoutError("timed out waiting for active queue steering test release")
            readable, _, _ = select.select([sys.stdin], [], [], 0.02)
            if not readable:
                continue
            line = sys.stdin.readline()
            if not line:
                raise SystemExit(0)
            try:
                incoming = json.loads(line)
            except json.JSONDecodeError:
                continue
            if incoming.get("method") == "turn/steer":
                steer_params = incoming.get("params") or {}
                log_request("turn/steer", steer_params)
                reply(incoming.get("id"), {"accepted": True})
                with open(
                    os.path.join(gate_root, "e2e-core-active-steer-received"),
                    "w",
                    encoding="utf-8",
                ) as marker:
                    marker.write(json.dumps(steer_params, separators=(",", ":")))
    gate_tag = next((tag for tag in ("alpha-job", "beta-job", "gamma-job") if tag in text), None)
    if "live-response-crash-window" in text and os.path.exists(
        os.path.join(gate_root, "e2e-core-recovery-gate")
    ):
        with open(
            os.path.join(gate_root, "e2e-core-recovery-pid"), "w", encoding="utf-8"
        ) as marker:
            marker.write(str(os.getpid()))
        with open(
            os.path.join(gate_root, "e2e-core-recovery-started"), "w", encoding="utf-8"
        ) as marker:
            marker.write("started")
        deadline = time.monotonic() + 30
        while not os.path.exists(os.path.join(gate_root, "e2e-core-recovery-release")):
            if time.monotonic() >= deadline:
                raise TimeoutError("timed out waiting for recovery test release")
            time.sleep(0.02)
    if gate_tag and os.path.exists(os.path.join(gate_root, "e2e-core-concurrency")):
        with open(os.path.join(gate_root, f"e2e-core-started-{gate_tag}"), "w", encoding="utf-8") as marker:
            marker.write("started")
        deadline = time.monotonic() + 30
        while not os.path.exists(os.path.join(gate_root, f"e2e-core-release-{gate_tag}")):
            if time.monotonic() >= deadline:
                raise TimeoutError(f"timed out waiting to release {gate_tag}")
            time.sleep(0.02)
    if "write" in text:
        result = request_tool(
            "call-write",
            "glass.git.commit",
            {"path": "notes.md", "content": "fixture\n"},
        )
    else:
        result = request_tool(
            "call-read",
            "glass.workspace.list",
            {},
        )
    tool_error = bool(result.get("isError"))
    event("tool_end", {"callId": "call-1", "isError": tool_error}, turn)
    event("text", {"text": f"handled {text.strip() or 'the request'}"}, turn)
    event("usage", {"input": 12, "output": 8}, turn)
    event("settled", {"reason": "completed"}, turn)
    reply(identifier, {"sessionId": SESSION_ID, "settled": True})
    if gate_tag and os.path.exists(os.path.join(gate_root, "e2e-core-concurrency")):
        with open(os.path.join(gate_root, f"e2e-core-completed-{gate_tag}"), "w", encoding="utf-8") as marker:
            marker.write("completed")


def main() -> None:
    global STORAGE_DIR, MODE, SESSION_ID
    # Glass appends `runtime` to every engine invocation. Ignore that command
    # argument when no fixture mode was explicitly supplied.
    mode = sys.argv[1] if len(sys.argv) > 1 and sys.argv[1] != "runtime" else "mixed"
    MODE = mode
    if os.path.exists(os.path.join(os.getcwd(), ".glass", "e2e-core-unique-session-ids")):
        SESSION_ID = f"fixture-session-{os.getpid()}"
    runtime_version = "0.1.0" if mode == "older-version-v2" else "0.3.14"
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        method = message.get("method")
        identifier = message.get("id")
        params = message.get("params") or {}
        if method != "shutdown":
            log_request(method, params)
        if method == "initialize":
            STORAGE_DIR = params.get("storageDir")
            if STORAGE_DIR:
                os.makedirs(STORAGE_DIR, exist_ok=True)
            if mode == "legacy-v1":
                send({
                    "jsonrpc": "2.0",
                    "id": identifier,
                    "error": {"code": -32602, "message": "protocolVersion must be 1"},
                })
                continue
            if params.get("protocolVersion") != 2:
                send(
                    {
                        "jsonrpc": "2.0",
                        "id": identifier,
                        "error": {"code": -32602, "message": "protocolVersion must be 2"},
                    }
                )
                continue
            reply(
                identifier,
                {
                    "protocolVersion": 2,
                    "runtime": {"name": "glass-core", "version": runtime_version},
                    "capabilities": {"hostTools": True, "localTools": True, "mixedTools": mode != "missing-mixed", "steering": True, "cancel": True},
                    "limits": {"frameBytes": 1024 * 1024, "toolResultBytes": 512 * 1024},
                    "storageDir": params.get("storageDir"),
                },
            )
        elif method == "auth/status":
            reply(
                identifier,
                {
                    "credentialReady": True,
                    "requiresRefresh": False,
                    "provider": "fixture",
                    "model": "fixture-model",
                    "detail": "fixture provider configuration",
                },
            )
        elif method == "session/new":
            assert params.get("toolMode") == "mixed"
            assert params.get("tools") and all(
                tool.get("name", "").startswith("glass.") for tool in params["tools"]
            )
            assert not any(tool.get("name") == "glass.file.read" for tool in params["tools"])
            assert not any(
                key in params for key in ("provider", "baseUrl", "connection", "apiKeyEnv", "apiKey")
            )
            reply(identifier, {"sessionId": SESSION_ID})
        elif method == "session/resume":
            assert params.get("expectedToolMode") == "mixed"
            if mode == "missing-session":
                send({"jsonrpc": "2.0", "id": identifier, "error": {"code": -32000, "message": "session not found"}})
                continue
            # The real engine returns the stored id. The fixture prefixes it so
            # a test can tell resume apart from a fresh session/new.
            resumed = params.get("sessionId", SESSION_ID)
            preserve_resume_id = os.path.exists(
                os.path.join(os.getcwd(), ".glass", "e2e-core-preserve-resume-session")
            )
            if mode == "resume-mismatch":
                resumed = "different-session"
            elif mode == "mixed" and not preserve_resume_id:
                resumed = f"resumed-{resumed}"
            reply(identifier, {"sessionId": resumed})
        elif method == "session/name":
            reply(identifier, {})
        elif method == "session/list":
            # Mirrors the real shape: ascending page plus an optional cursor.
            reply(
                identifier,
                {
                    "sessions": [
                        {
                            "sessionId": SESSION_ID,
                            "open": True,
                            "name": "fixture",
                            "cwd": os.getcwd(),
                            "toolMode": "mixed",
                        },
                        {
                            "sessionId": "zz-legacy-host-session",
                            "open": False,
                            "name": "legacy host session",
                            "cwd": os.getcwd(),
                            "toolMode": "host",
                        },
                    ],
                    "nextAfterSessionId": None,
                },
            )
        elif method == "session/describe":
            reply(
                identifier,
                {
                    "sessionId": "different-session" if mode == "describe-mismatch" else params.get("sessionId", SESSION_ID),
                    "cwd": os.path.join(os.getcwd(), "..") if mode == "wrong-cwd" else os.getcwd(),
                    "toolMode": "host" if mode == "wrong-tool-mode" else "mixed",
                    "provider": "must-not-be-copied",
                    "apiKeyEnv": "FIXTURE_SECRET_ENV",
                },
            )
        elif method == "session/entries":
            assert params.get("sessionId") == SESSION_ID
            if mode == "endless-empty-entries":
                reply(identifier, {
                    "entries": [],
                    "cursor": params.get("cursor", 0) + 1,
                    "hasMore": True,
                })
            elif params.get("cursor", 0) > 0:
                reply(identifier, {"entries": [], "cursor": params["cursor"], "hasMore": False})
            else:
                reply(identifier, {
                    "entries": [
                        {"entryId": f"{SESSION_ID}@0", "cursor": 10, "record": {"k": "user", "t": "prior question"}, "truncatedFields": []},
                        {"entryId": f"{SESSION_ID}@10", "cursor": 20, "record": {"k": "assistant", "t": "prior answer"}, "truncatedFields": []},
                    ],
                    "cursor": 20,
                    "hasMore": False,
                })
        elif method == "models/list":
            assert params.get("sessionId") == SESSION_ID
            assert params.get("cursor") == 0
            reply(
                identifier,
                {
                    "provider": "anthropic",
                    "models": [
                        {"id": "claude-opus-4", "label": "Claude Opus 4"},
                        {"id": "claude-sonnet-4", "label": "claude-sonnet-4"},
                    ],
                    "nextCursor": None,
                },
            )
        elif method in ("turn/prompt", "turn/followUp"):
            if mode == "prompt-error":
                send(
                    {
                        "jsonrpc": "2.0",
                        "id": identifier,
                        "error": {
                            "code": -32000,
                            "message": "fixture provider detail that must not be shown",
                            "data": {"kind": "provider", "retryable": False},
                        },
                    }
                )
                continue
            run_turn(identifier, params)
        elif method == "turn/steer":
            reply(identifier, {"accepted": True})
        elif method == "turn/cancel":
            reply(identifier, {"accepted": False, "reason": "idle"})
        elif method == "shutdown":
            reply(identifier, {})
            return
        else:
            send(
                {
                    "jsonrpc": "2.0",
                    "id": identifier,
                    "error": {"code": -32601, "message": f"method not found: {method}"},
                }
            )


if __name__ == "__main__":
    main()
