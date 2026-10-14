#!/usr/bin/env python3
"""Compatibility adapter for GC's native, durable state.todo checklist.

Legacy 1-based indices are resolved against a revision-guarded native read.
The extension keeps no task state of its own.
"""

import json
import sys

PROTOCOL = 1

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def host(args):
    send({"type": "call_tool", "call_id": "todo-native", "name": "state", "arguments": json.dumps(args)})
    for line in sys.stdin:
        reply = json.loads(line)
        if reply.get("type") == "call_tool_result" and reply.get("call_id") == "todo-native":
            return str(reply.get("body") or "")
    raise RuntimeError("Native checklist writer unavailable")


def handle(args):
    action = (args.get("action") or "list").strip().lower()
    if action == "add":
        return host({"todo": {"ops": [{"op": "add", "items": [{"title": args.get("item", "")}]}]}})
    snapshot = host({"todo": {}})
    if snapshot.startswith("error:"):
        return snapshot
    value = json.loads(snapshot)
    tasks = value["tasks"]
    if action == "list":
        return snapshot
    index = args.get("index")
    if isinstance(index, bool) or not isinstance(index, int) or not 1 <= index <= len(tasks):
        return f"error: todo index must be 1..{len(tasks)}"
    task = tasks[index - 1]
    if action == "remove":
        op = {"op": "remove", "id": task["id"]}
    elif action in ("check", "uncheck"):
        op = {"op": "update", "id": task["id"], "status": "completed" if action == "check" else "pending"}
    else:
        return f"error: unknown todo action {action}"
    return host({"todo": {"ops": [op]}, "expected": value["revision"]})


for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        req = json.loads(line)
    except json.JSONDecodeError:
        continue
    rid = req.get("id")
    kind = req.get("type")

    if kind == "hello":
        send({
            "id": rid,
            "protocol": PROTOCOL,
            "name": "todo",
            "version": "1.1.0",
            "system_prompt": (
                "The compatibility `todo` tool uses native state.todo storage. Prefer batching with state.todo. For any task with more than "
                "two steps, break it into todos with action `add`, then check "
                "them off (`check`) as each finishes. The tool returns the "
                "changed tasks after every change; list reads the full list. "
                "Keep items small enough to track concrete work, "
                "and add new ones when the plan changes."
            ),
            "tools": [
                {
                    "name": "todo",
                    "description": (
                        "Manage the plan. Actions: add (needs `item`), "
                        "check/uncheck/remove (need 1-based `index`), list. "
                        "Writes return changed tasks; list reads the full native checklist."
                    ),
                    "schema": {
                        "type": "object",
                        "properties": {
                            "action": {
                                "type": "string",
                                "enum": ["add", "check", "uncheck", "remove", "list"],
                            },
                            "item": {"type": "string", "description": "what to add"},
                            "index": {
                                "type": "integer",
                                "description": "1-based position",
                            },
                        },
                        "required": ["action"],
                    },
                    # Mutates the plan, so it must not run alongside
                    # another todo call.
                    "concurrency": "serial",
                }
            ],
        })
        continue

    if kind == "run_tool":
        name = req.get("name")
        try:
            args = json.loads(req.get("arguments") or "{}")
        except json.JSONDecodeError:
            args = {}
        if name == "todo":
            try:
                send({"id": rid, "output": handle(args)})
            except Exception as e:  # noqa: BLE001
                send({"id": rid, "output": f"error: todo: {e}"})
        else:
            send({"id": rid, "output": f"todo: unknown tool {name}"})
        continue

    if kind in ("system_prompt", "authorize"):
        send({"id": rid, "allow": True, "text": ""})
        continue
    if kind == "tool_result":
        send({"id": rid, "body": req.get("body", "")})
        continue
    if kind == "turn_end":
        continue
    if kind == "goodbye":
        send({"id": rid})
        break
    # Unknown messages are ignored: the host treats silence as
    # "no opinion" and moves on.
