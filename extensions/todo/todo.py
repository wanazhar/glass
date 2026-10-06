#!/usr/bin/env python3
"""glass todo extension — plan tracking, as t3 codes ships it.

The model keeps its plan in a todo list it manages itself: add
items as it breaks a task down, check them off as it finishes
them. The list lives in this process for the session, so it
costs nothing to keep and disappears when the session ends.

The tool always returns the whole list, so the model sees the
current state after every change without a separate read.
"""

import json
import sys

PROTOCOL = 1

# Each item is [description, done]. Order is insertion order,
# so the list reads as the plan was made.
todos = []


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def render():
    if not todos:
        return "(no todos)"
    lines = []
    for i, (desc, done) in enumerate(todos, 1):
        mark = "x" if done else " "
        lines.append(f"{i}. [{mark}] {desc}")
    pending = sum(1 for _, d in todos if not d)
    return "\n".join(lines) + f"\n{pending} pending, {len(todos) - pending} done"


def handle(args):
    action = (args.get("action") or "list").strip().lower()
    if action == "add":
        item = (args.get("item") or "").strip()
        if not item:
            return "todo: add needs an item"
        todos.append([item, False])
        return "added:\n" + render()
    if action == "check":
        return _mark(args, True)
    if action == "uncheck":
        return _mark(args, False)
    if action == "remove":
        idx = args.get("index")
        if not isinstance(idx, int) or not (1 <= idx <= len(todos)):
            return f"todo: index must be 1..{len(todos)}"
        removed = todos.pop(idx - 1)[0]
        return f"removed: {removed}\n" + render()
    if action == "list":
        return render()
    return f"todo: unknown action `{action}` (add, check, uncheck, remove, list)"


def _mark(args, done):
    idx = args.get("index")
    if not isinstance(idx, int) or not (1 <= idx <= len(todos)):
        return f"todo: index must be 1..{len(todos)}"
    todos[idx - 1][1] = done
    return render()


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
            "version": "1.0.0",
            "system_prompt": (
                "A `todo` tool tracks the plan. For any task with more than "
                "two steps, break it into todos with action `add`, then check "
                "them off (`check`) as each finishes. The tool returns the "
                "whole list after every change, so read that, not a separate "
                "call. Keep items small enough that each is one turn of work, "
                "and add new ones when the plan changes."
            ),
            "tools": [
                {
                    "name": "todo",
                    "description": (
                        "Manage the plan. Actions: add (needs `item`), "
                        "check/uncheck/remove (need 1-based `index`), list. "
                        "Always returns the full current list."
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
                send({"id": rid, "output": f"todo: {e}"})
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
