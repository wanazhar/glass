#!/usr/bin/env python3
import json
import os
import sys

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
    task = "generation-fixture-" + str(TASK_COUNTER[0])
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
                "name": "glass.workspace.list",
                "arguments": {},
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
        if "method" not in message and message.get("id") == frame_id:
            result = message.get("result") or {}
            break
    if STORAGE_DIR:
        with open(os.path.join(STORAGE_DIR, "fixture-tool-outcome.json"), "w", encoding="utf-8") as handle:
            handle.write(json.dumps({"isError": bool(result.get("isError")), "text": str(result.get("text") or "")}))
    event(turn, "tool_end", {"callId": "call-1", "isError": bool(result.get("isError")), "text": str(result.get("text") or "")})
    event(turn, "text", {"text": "handled " + str(text).strip()})
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
            reply(
                identifier,
                {
                    "protocolVersion": 1,
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
        elif method == "durable/create":
            global CONVERSATION_SEQ
            CONVERSATION_SEQ += 1
            conversation_id = "durable-fixture-" + str(CONVERSATION_SEQ)
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
            reply(
                identifier,
                {"submission": {"id": task_id, "status": "pending"}, "duplicate": duplicate},
            )
        elif method == "durable/run":
            task_id = params.get("taskId") or task_id_for(params.get("conversationId"), params.get("requestId"))
            run_turn(identifier, task_id, INPUT_TEXT.get("value", ""), params.get("conversationId") or SESSION_ID, params.get("requestId") or "")
        elif method == "shutdown":
            reply(identifier, {})
            return
        else:
            send({"jsonrpc": "2.0", "id": identifier, "error": {"code": -32601, "message": "method not found: " + str(method)}})


main()
