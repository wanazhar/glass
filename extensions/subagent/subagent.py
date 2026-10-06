#!/usr/bin/env python3
"""glass subagent extension.

Adds a `subagent` tool that delegates a self-contained task to a
fresh agent with its own context. The sub-agent runs with the core
tools (bash, state) and reports back; its intermediate steps never
enter the parent's conversation, so delegation is cheap in context.

The extension is a thin proxy: it declares the tool and forwards the
task to the host, which runs the nested agent. Only the host has the
provider credentials and the tools, so the nested loop lives there.

Concurrency is `serial`: a sub-agent runs to completion before the
parent continues, and only one runs at a time.
"""

import json
import sys

# The wire version this extension speaks. It must match
# what glass expects, and it goes at the top level of the
# handshake reply, not nested inside the manifest.
PROTOCOL = 1


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def read_line():
    line = sys.stdin.readline()
    if not line:
        return None
    line = line.strip()
    return json.loads(line) if line else None


MANIFEST = {
    "name": "subagent",
    "version": "1.0.0",
    "system_prompt": (
        "You can delegate self-contained tasks to a sub-agent with the "
        "`subagent` tool. The sub-agent runs in its own context with the "
        "bash and state tools, then reports back; its steps do not enter "
        "your conversation. Delegate tasks that are independent of your "
        "current working context — research, exploration, bulk file "
        "operations. Keep the task description self-contained: the "
        "sub-agent cannot see your conversation, so spell out the goal, "
        "the paths involved, and what a finished result looks like."
    ),
    "tools": [
        {
            "name": "subagent",
            "description": (
                "Run a sub-agent on a self-contained task and return its "
                "report. The sub-agent has its own context and the bash and "
                "state tools. Use for research, exploration, or bulk work "
                "that does not need the current conversation. The task must "
                "be fully self-contained."
            ),
            "schema": {
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": (
                            "The task to delegate, written so someone with "
                            "no prior context can do it: goal, paths, and "
                            "the expected result."
                        ),
                    },
                },
                "required": ["task"],
            },
            "concurrency": "serial",
        }
    ],
}


def handle_run_tool(req):
    args = json.loads(req.get("arguments") or "{}")
    task = args.get("task") or ""
    if not task.strip():
        return "subagent: missing `task`"
    # Ask the host to run a nested agent. This blocks until the
    # sub-agent finishes: the host runs it to completion before
    # writing the report back, so there is nothing to poll.
    send({"type": "run_subagent", "task": task})
    reply = read_line()
    if reply is None:
        return "subagent: host closed the stream"
    if reply.get("type") == "subagent_result":
        return reply.get("output") or "(sub-agent returned nothing)"
    return "subagent: unexpected reply " + json.dumps(reply)


def main():
    while True:
        req = read_line()
        if req is None:
            break
        kind = req.get("type")
        if kind == "hello":
            # The manifest fields live at the top level of
            # the reply; glass reads the protocol version
            # from there.
            reply = {"id": req.get("id"), "protocol": PROTOCOL}
            reply.update(MANIFEST)
            send(reply)
        elif kind == "run_tool":
            output = handle_run_tool(req)
            send({"id": req.get("id"), "output": output})
        elif kind == "command":
            send(
                {
                    "id": req.get("id"),
                    "output": "subagent: no interactive commands",
                }
            )
        elif kind in ("goodbye", "shutdown"):
            break
        # turn_end, authorize, and anything else need no reply.


if __name__ == "__main__":
    main()
