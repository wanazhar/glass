#!/usr/bin/env python3
import json
import os
import sys
import time

def argument(flag, default=None):
    if flag in sys.argv:
        index = sys.argv.index(flag)
        if index + 1 < len(sys.argv):
            return sys.argv[index + 1]
    return default


# Tests can drive one governed call without editing the fixture.
TOOL_NAME = argument("--tool", "glass.workspace.list")
TOOL_ARGUMENTS = {}
if argument("--path") is not None:
    TOOL_ARGUMENTS["path"] = argument("--path")
if argument("--content") is not None:
    TOOL_ARGUMENTS["content"] = argument("--content")

# Tests can slow one turn so two lanes can be observed running together.
DELAY_MS = int(argument("--delay-ms", "0") or "0")
# Each engine is its own process, so a test can tell their ids apart.
ID_PREFIX = argument("--id-prefix", "") or ""
# A lane that dies with a turn in flight, for the isolation tests.
DIE_ON_RUN = "--die-on-run" in sys.argv

# A cancelled turn leaves replies in flight. In this mode the fixture parks the
# first durable run until an abort arrives, then answers the run before the
# abort so the abort acknowledgement is stranded for whatever turn comes next.
LATE_ABORT_ACK = "--late-abort-ack" in sys.argv
PARKED_RUN = {}

SESSION_ID = "fixture-session"
STORAGE_DIR = None
INPUT_TEXT = {"value": ""}
CONVERSATION_SEQ = 0
TASK_SEQ = {}
SUBMISSIONS = {}
TASK_COUNTER = [0]


def send(message):
    sys.stdout.write(json.dumps(message, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def reply(identifier, result):
    send({"jsonrpc": "2.0", "id": identifier, "result": result})


def note_steer(message):
    """Record one correction in this engine's own store, so a test can tell
    which lane the person's steer reached."""
    if not STORAGE_DIR:
        return
    params = message.get("params") or {}
    # `turn/steer` carries the turn input directly, beside the session id.
    text = str(params.get("text") or "")
    with open(os.path.join(STORAGE_DIR, "fixture-steer.json"), "w", encoding="utf-8") as handle:
        json.dump({"sessionId": params.get("sessionId"), "text": text}, handle)


def event(turn, kind, data):
    send(
        {
            "jsonrpc": "2.0",
            "method": "runtime/event",
            "params": {"sessionId": SESSION_ID, "turnId": turn, "type": kind, "data": data},
        }
    )


def task_id_for(conversation, request):
    key = (conversation, request)
    task = TASK_SEQ.get(key)
    if task is not None:
        return task
    TASK_COUNTER[0] += 1
    task = ID_PREFIX + "generation-fixture-" + str(TASK_COUNTER[0])
    TASK_SEQ[key] = task
    return task


SUBMISSIONS_FILE = "fixture-submissions.json"


def load_submissions():
    global SUBMISSIONS
    SUBMISSIONS = {}
    if STORAGE_DIR:
        path = os.path.join(STORAGE_DIR, SUBMISSIONS_FILE)
        if os.path.exists(path):
            with open(path, "r", encoding="utf-8") as handle:
                SUBMISSIONS = {(k[0], k[1]): v for k, v in json.load(handle).items()}


def save_submissions():
    if not STORAGE_DIR:
        return
    path = os.path.join(STORAGE_DIR, SUBMISSIONS_FILE)
    with open(path, "w", encoding="utf-8") as handle:
        json.dump({f"{k[0]}|{k[1]}": v for k, v in SUBMISSIONS.items()}, handle)


def run_turn(identifier, task, text, conversation, request):
    turn = "turn-1"
    event(turn, "turn_start", {})
    if argument("--pre-tool-text"):
        event(turn, "text", {"text": argument("--pre-tool-text")})
    if DELAY_MS > 0:
        time.sleep(DELAY_MS / 1000)
    # A prompt of "ASK <name>: <instruction>" makes this engine ask another
    # marble for help, so handoff tests need no provider.
    tool_name = TOOL_NAME
    tool_args = TOOL_ARGUMENTS
    stripped = str(text).strip()
    ask_at = stripped.find("ASK ")
    if ask_at >= 0:
        ask_text = stripped[ask_at + 4:]
        target, _, instruction = ask_text.partition(":")
        tool_name = "glass.marble.ask"
        tool_args = {"to": target.strip(), "instruction": instruction.strip() or "help"}
    frame_id = "host:" + SESSION_ID + ":" + task + ":call-1"
    send(
        {
            "jsonrpc": "2.0",
            "id": frame_id,
            "method": "tool/execute",
            "params": {
                "sessionId": SESSION_ID,
                "turnId": turn,
                "callId": "call-1",
                "name": tool_name,
                "arguments": tool_args,
            },
        }
    )
    result = None
    while True:
        line = sys.stdin.readline()
        if not line:
            raise SystemExit(0)
        line = line.strip()
        if not line:
            continue
        message = json.loads(line)
        if message.get("method") == "turn/steer":
            note_steer(message)
            continue
        if "method" not in message and message.get("id") == frame_id:
            result = message.get("result") or {}
            break
    if STORAGE_DIR:
        try:
            with open(os.path.join(STORAGE_DIR, "fixture-tool-outcome.json"), "w", encoding="utf-8") as handle:
                handle.write(json.dumps({"isError": bool(result.get("isError")), "text": str(result.get("text") or "")}))
        except OSError:
            pass
    event(turn, "tool_end", {"callId": "call-1", "isError": bool(result.get("isError")), "text": str(result.get("text") or "")})
    chunks = argument("--text-chunks")
    for chunk in json.loads(chunks) if chunks else ["handled " + str(text).strip()]:
        event(turn, "text", {"text": chunk})
        if chunks:
            time.sleep(0.3)
    event(turn, "usage", {"input": 1, "output": 1})
    event(turn, "settled", {"reason": "completed"})
    reply(identifier, {"sessionId": SESSION_ID, "settled": True})


def main():
    global STORAGE_DIR
    while True:
        line = sys.stdin.readline()
        if not line:
            return
        line = line.strip()
        if not line:
            continue
        message = json.loads(line)
        method = message.get("method")
        identifier = message.get("id")
        params = message.get("params") or {}
        if method == "initialize":
            STORAGE_DIR = params.get("storageDir")
            if STORAGE_DIR:
                os.makedirs(STORAGE_DIR, exist_ok=True)
            load_submissions()
            protocol = params.get("protocolVersion")
            required = params.get("requiredCapabilities") or []
            if protocol == 2 and "mixedTools" not in required:
                send(
                    {
                        "jsonrpc": "2.0",
                        "id": identifier,
                        "error": {"code": -32602, "message": "protocol v2 requires mixedTools"},
                    }
                )
                continue
            reply(
                identifier,
                {
                    "protocolVersion": protocol or 1,
                    "runtime": {"name": "glass-core", "version": "0.3.14"},
                    "capabilities": {
                        "hostTools": True,
                        "localTools": True,
                        "mixedTools": True,
                        "maskedSecretInput": True,
                        "steering": True,
                        "cancel": True,
                        "durable": True,
                        "resume": True,
                    },
                    "limits": {"frameBytes": 1048576, "toolResultBytes": 524288},
                    "storageDir": params.get("storageDir"),
                },
            )
            if "--exit-after-initialize" in sys.argv:
                return
        elif method == "durable/create":
            global CONVERSATION_SEQ
            if STORAGE_DIR:
                with open(os.path.join(STORAGE_DIR, "fixture-session-options.json"), "w", encoding="utf-8") as handle:
                    json.dump(params, handle)
            if params.get("toolMode") != "mixed":
                send(
                    {
                        "jsonrpc": "2.0",
                        "id": identifier,
                        "error": {"code": -32602, "message": "fixture requires mixed tool ownership"},
                    }
                )
                continue
            for tool in params.get("tools") or []:
                if not str(tool.get("name") or "").startswith("glass."):
                    send(
                        {
                            "jsonrpc": "2.0",
                            "id": identifier,
                            "error": {"code": -32602, "message": "mixed tools must be namespaced"},
                        }
                    )
                    break
            else:
                CONVERSATION_SEQ += 1
                conversation_id = ID_PREFIX + "durable-fixture-" + str(CONVERSATION_SEQ)
                reply(identifier, {"conversationId": conversation_id, "sequence": CONVERSATION_SEQ})
        elif method == "durable/submit":
            conversation_id = params.get("conversationId")
            request_id = params.get("requestId")
            key = (conversation_id, request_id)
            task_id = SUBMISSIONS.get(key)
            duplicate = task_id is not None
            if task_id is None:
                task_id = task_id_for(conversation_id, request_id)
                SUBMISSIONS[key] = task_id
                save_submissions()
            if isinstance(params.get("input"), dict):
                INPUT_TEXT["value"] = str(params["input"].get("text") or "")
                if STORAGE_DIR:
                    with open(os.path.join(STORAGE_DIR, "fixture-input.json"), "w", encoding="utf-8") as handle:
                        json.dump(params, handle)
            reply(
                identifier,
                {"submission": {"id": task_id, "status": "pending"}, "duplicate": duplicate},
            )
        elif method == "durable/run":
            if DIE_ON_RUN:
                # The engine dies with this turn in flight; another lane must
                # keep working.
                sys.exit(0)
            if LATE_ABORT_ACK and not PARKED_RUN.get("used"):
                PARKED_RUN["used"] = True
                PARKED_RUN["id"] = identifier
                continue
            task_id = params.get("taskId") or task_id_for(params.get("conversationId"), params.get("requestId"))
            run_turn(identifier, task_id, INPUT_TEXT.get("value", ""), params.get("conversationId") or SESSION_ID, params.get("requestId") or "")
        elif method == "durable/abort":
            if PARKED_RUN.get("id"):
                reply(PARKED_RUN.pop("id"), {"sessionId": SESSION_ID, "settled": True})
            reply(identifier, {"aborted": True, "taskId": params.get("taskId")})
        elif method == "turn/steer":
            note_steer(message)
            reply(identifier, {"steered": True})
        elif method == "shutdown":
            reply(identifier, {})
            return
        else:
            send({"jsonrpc": "2.0", "id": identifier, "error": {"code": -32601, "message": "method not found: " + str(method)}})


main()
