#!/usr/bin/env python3
import json
import os
import subprocess
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
WAIT_FOR_STEER = "--wait-for-steer" in sys.argv or os.environ.get("MARBLE_FIXTURE_WAIT_FOR_STEER") == "1"

SESSION_ID = "fixture-session"
STORAGE_DIR = None
INPUT_TEXT = {"value": ""}
CONVERSATION_SEQ = 0
TASK_SEQ = {}
SUBMISSIONS = {}
TASK_COUNTER = [0]
HISTORY = {"sequence": 0, "conversations": {}}
KNOWN_CONVERSATIONS = set()


def save_history():
    if STORAGE_DIR:
        with open(os.path.join(STORAGE_DIR, "fixture-history.json"), "w", encoding="utf-8") as handle:
            json.dump(HISTORY, handle)


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
                SUBMISSIONS = {tuple(k.split("|", 1)): v for k, v in json.load(handle).items()}
                TASK_COUNTER[0] = max([int(v.rsplit("-", 1)[-1]) for v in SUBMISSIONS.values()] or [0])


def save_submissions():
    if not STORAGE_DIR:
        return
    path = os.path.join(STORAGE_DIR, SUBMISSIONS_FILE)
    with open(path, "w", encoding="utf-8") as handle:
        json.dump({f"{k[0]}|{k[1]}": v for k, v in SUBMISSIONS.items()}, handle)


def run_turn(identifier, task, text, conversation, request):
    HISTORY["sequence"] += 1
    HISTORY["conversations"].setdefault(conversation, []).append({"sequence": HISTORY["sequence"], "text": str(text)})
    save_history()
    if "--transient-error-always" in sys.argv or "--permanent-error" in sys.argv:
        message = "HTTP 400 provider_rejected secret-token" if "--permanent-error" in sys.argv else "connection reset secret-token"
        send({"jsonrpc": "2.0", "id": identifier, "error": {"code": -32000, "message": message}})
        return
    if "--empty-reply" in sys.argv:
        reply(identifier, {"settled": True})
        return
    failed = os.path.join(STORAGE_DIR, "fixture-failed-once") if STORAGE_DIR else None
    if "--fail-once" in sys.argv and "FAIL" in text and failed and not os.path.exists(failed):
        with open(failed, "w", encoding="utf-8") as handle:
            handle.write("failed")
        if "--error-once" in sys.argv and "--generation-failure-once" not in sys.argv:
            send({"jsonrpc": "2.0", "id": identifier, "error": {"code": -32000, "message": "connection reset secret-token"}})
            return
        event("turn-1", "text", {"text": "[Generation failed] Reason: provider's requirements secret-token"})
        reply(identifier, {"settled": True})
        return
    text = str(text).split("[/Marble host context]\n\n[User message]\n", 1)[-1]
    turn = "turn-1"
    event(turn, "turn_start", {})
    if os.environ.get("MARBLE_FIXTURE_EVERYONE_THREADS") == "1":
        with open(os.path.join(STORAGE_DIR, "fixture-session-options.json"), "r", encoding="utf-8") as handle:
            name = json.load(handle)["instructions"].split("You are ", 1)[1].split(",", 1)[0].split(".", 1)[0]
        attempt_tool(turn, task, "glass.workspace.list", {})
        answer = name + " says hello."
        if name == "Dietrich":
            answer = "The user is asking for a greeting. I need to say hello. I should keep it short. I'll output only the final answer.\n\n" + answer
        event(turn, "text", {"text": answer})
        event(turn, "settled", {"reason": "completed"})
        reply(identifier, {"settled": True})
        return
    if "--isolated-greeting" in sys.argv and text == "@Ada hello":
        if not any("remark.pdf" in row["text"] for row in HISTORY["conversations"][conversation]):
            event(turn, "text", {"text": "Hello!"})
            event(turn, "settled", {"reason": "completed"})
            reply(identifier, {"settled": True})
            return
    if "--everyone-reasoning" in sys.argv:
        with open(os.path.join(STORAGE_DIR, "fixture-session-options.json"), "r", encoding="utf-8") as handle:
            name = json.load(handle)["instructions"].split("You are ", 1)[1].split(",", 1)[0].split(".", 1)[0]
        event(turn, "reasoning", {"text":"Channel for " + name + "."})
        event(turn, "tool_start", {"callId":"reasoning-tool","name":"bash"})
        event(turn, "tool_end", {"callId":"reasoning-tool","text":"done","isError":False})
        if name != "Ada":
            event(turn, "text", {"text":"Planning for " + name + "."})
            event(turn, "text_block_start", {"blockId":"answer"})
        event(turn, "text", {"text":"Public answer."})
        event(turn, "settled", {"reason":"completed"})
        reply(identifier, {"settled":True})
        return
    if "--text-blocks" in sys.argv:
        if "--text-blocks-open-tag" in sys.argv:
            event(turn, "text", {"text":"<thinking>"})
        for delta in ["Planning ", "secret."]:
            event(turn, "text", {"text": delta})
        if "--text-blocks-tool-empty" in sys.argv:
            event(turn, "tool_start", {"callId":"planning-tool","name":"bash"})
            event(turn, "tool_end", {"callId":"planning-tool","text":"done","isError":False})
            event(turn, "settled", {"reason":"completed"})
            reply(identifier, {"settled":True})
            return
        event(turn, "text_block_start", {"blockId":"block-1"})
        event(turn, "text", {"text":"More planning."})
        event(turn, "text_block_start", {"blockId":"block-2"})
        if "--text-blocks-error" in sys.argv:
            send({"jsonrpc":"2.0","id":identifier,"error":{"code":-32000,"message":"HTTP 400 provider_rejected"}})
            return
        for delta in ([] if "--text-blocks-empty" in sys.argv else ["Public ", "answer."]):
            event(turn, "text", {"text": delta})
        event(turn, "settled", {"reason":"completed"})
        reply(identifier, {"settled":True})
        return
    if "--reasoning-channels" in sys.argv:
        event(turn, "reasoning", {"text": "Channel secret."})
        for tag in ["think", "thinking", "thought", "reasoning"]:
            for chunk in ["<" + tag[:2], tag[2:] + ">", "Tagged secret.", "</" + tag[:2], tag[2:] + ">"]:
                event(turn, "text", {"text": chunk})
        event(turn, "text", {"text": "Public answer."})
        event(turn, "settled", {"reason": "completed"})
        reply(identifier, {"settled": True})
        return
    if argument("--pre-tool-text"):
        event(turn, "text", {"text": argument("--pre-tool-text")})
    if "--native-date" in sys.argv:
        with open(os.path.join(STORAGE_DIR, "fixture-session-options.json"), "r", encoding="utf-8") as handle:
            options = json.load(handle)
        name = options["instructions"].split("You are ", 1)[1].split(",", 1)[0].split(".", 1)[0]
        executions = []
        for index in range(2):
            if index:
                event(turn, "text", {"text": "Plan two."})
            call = "date-" + str(index)
            event(turn, "tool_start", {"callId": call, "name": "bash", "title": "date -u -Iseconds"})
            result = subprocess.run(["date", "-u", "-Iseconds"], capture_output=True, text=True, check=True)
            executions.append({"task": task, "callId": call, "output": result.stdout.strip()})
            event(turn, "tool_end", {"callId": call, "text": result.stdout, "isError": False})
        with open(os.path.join(STORAGE_DIR, "fixture-date-executions.jsonl"), "a", encoding="utf-8") as handle:
            for execution in executions:
                handle.write(json.dumps(execution) + "\n")
        for chunk in ["@" + name + " The system time is ", result.stdout.strip(), "."]:
            event(turn, "text", {"text": chunk})
        event(turn, "settled", {"reason": "completed"})
        reply(identifier, {"settled": True})
        return
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
    attempt_tool(turn, task, tool_name, tool_args)
    chunks = argument("--text-chunks")
    for chunk in json.loads(chunks) if chunks else ["handled " + str(text).strip()]:
        event(turn, "text", {"text": chunk})
        if chunks:
            time.sleep(0.3)
    event(turn, "usage", {"input": 1, "output": 1})
    event(turn, "settled", {"reason": "completed"})
    reply(identifier, {"sessionId": SESSION_ID, "settled": True})


def attempt_tool(turn, task, tool_name, tool_args):
    frame_id = "host:" + SESSION_ID + ":" + task + ":call-1"
    if "--core-tool-progress" in sys.argv:
        event(turn, "tool_start", {"callId": "call-1", "name": tool_name})
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


def main():
    global STORAGE_DIR, HISTORY, ID_PREFIX
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
                if (os.environ.get("MARBLE_FIXTURE_EVERYONE_THREADS") == "1"
                        or os.path.basename(os.path.dirname(STORAGE_DIR)) == "conversation-sessions"):
                    ID_PREFIX = os.path.basename(STORAGE_DIR) + "-"
                os.makedirs(STORAGE_DIR, exist_ok=True)
            load_submissions()
            history_file = os.path.join(STORAGE_DIR, "fixture-history.json") if STORAGE_DIR else None
            if history_file and os.path.exists(history_file):
                with open(history_file, "r", encoding="utf-8") as handle:
                    HISTORY = json.load(handle)
            KNOWN_CONVERSATIONS.update(HISTORY["conversations"])
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
                        "textBlockBoundaries": (WAIT_FOR_STEER or any(flag in sys.argv for flag in ["--text-blocks", "--everyone-reasoning"])) and "textBlockBoundaries" in params.get("optionalCapabilities", []),
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
            marker = os.path.join(STORAGE_DIR, "fixture-create-rejected")
            if "--reject-create-once" in sys.argv and not os.path.exists(marker):
                with open(marker, "w", encoding="utf-8") as handle:
                    handle.write("rejected")
                message = "connection reset during admission" if "--transient-create-error" in sys.argv else "fixture refuses first admission"
                send({"jsonrpc": "2.0", "id": identifier, "error": {"code": -32000, "message": message}})
                continue
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
                KNOWN_CONVERSATIONS.add(conversation_id)
                HISTORY["conversations"].setdefault(conversation_id, [])
                save_history()
                reply(identifier, {"conversationId": conversation_id, "sequence": CONVERSATION_SEQ})
        elif method == "durable/configure":
            if STORAGE_DIR:
                with open(os.path.join(STORAGE_DIR, "fixture-tool-policy.json"), "w", encoding="utf-8") as handle:
                    json.dump(params["agent"], handle)
            reply(identifier, {"sequence": HISTORY["sequence"]})
        elif method == "durable/inspect":
            reply(identifier, {"sequence": HISTORY["sequence"], "items": [], "hasMore": False})
        elif method == "durable/fork":
            HISTORY["sequence"] += 1
            conversation_id = ID_PREFIX + "fork-" + str(HISTORY["sequence"])
            KNOWN_CONVERSATIONS.add(conversation_id)
            HISTORY["conversations"][conversation_id] = [entry for entry in HISTORY["conversations"].get(params["conversationId"], []) if entry["sequence"] <= params["asOf"]]
            save_history()
            reply(identifier, {"conversationId": conversation_id})
        elif method == "durable/submit":
            marker = os.path.join(STORAGE_DIR, "fixture-submit-rejected")
            reject = "--reject-submit-once" in sys.argv or ("--reject-retry-submit-once" in sys.argv and os.path.exists(os.path.join(STORAGE_DIR, "fixture-failed-once")))
            if reject and not os.path.exists(marker):
                with open(marker, "w", encoding="utf-8") as handle:
                    handle.write("rejected")
                send({"jsonrpc": "2.0", "id": identifier, "error": {"code": -32000, "message": "fixture refuses first submission"}})
                continue
            conversation_id = params.get("conversationId")
            if "--strict-conversations" in sys.argv and conversation_id not in KNOWN_CONVERSATIONS:
                send({"jsonrpc": "2.0", "id": identifier, "error": {"code": -32000, "message": "unknown conversation"}})
                continue
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
                INPUT_TEXT["conversation"] = conversation_id
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
            if count := argument("--budget-tools"):
                PARKED_RUN["id"] = identifier
                event("turn-1", "turn_start", {})
                for index in range(int(count)):
                    call = "budget-" + str(index)
                    event("turn-1", "tool_start", {"callId": call, "name": "read"})
                    event("turn-1", "tool_end", {"callId": call, "isError": False})
                continue
            if WAIT_FOR_STEER:
                PARKED_RUN["id"] = identifier
                event("turn-1", "turn_start", {})
                event("turn-1", "text_block_start", {"blockId": "answer"})
                event("turn-1", "text", {"text": "For the walk, I’d start with the trail by the river."})
                continue
            run_turn(identifier, task_id, INPUT_TEXT.get("value", ""), INPUT_TEXT.get("conversation") or SESSION_ID, params.get("requestId") or "")
        elif method == "durable/abort":
            if STORAGE_DIR and argument("--budget-tools"):
                with open(os.path.join(STORAGE_DIR, "fixture-aborted-task"), "w", encoding="utf-8") as handle:
                    handle.write(params["taskId"])
            if PARKED_RUN.get("id"):
                reply(PARKED_RUN.pop("id"), {"sessionId": SESSION_ID, "settled": True})
            reply(identifier, {"aborted": True, "taskId": params.get("taskId")})
        elif method == "turn/steer":
            note_steer(message)
            reply(identifier, {"steered": True})
            if WAIT_FOR_STEER and (running := PARKED_RUN.pop("id", None)):
                event("turn-1", "text", {"text": " With your correction in mind: " + params["text"]})
                event("turn-1", "settled", {"reason": "completed"})
                reply(running, {"sessionId": SESSION_ID, "settled": True})
        elif method == "shutdown":
            reply(identifier, {})
            return
        else:
            send({"jsonrpc": "2.0", "id": identifier, "error": {"code": -32601, "message": "method not found: " + str(method)}})


main()
