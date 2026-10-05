#!/usr/bin/env python3
"""Minimal reference extension for glass.

Demonstrates every part of the extension protocol:
  - the handshake manifest (tools, commands, system prompt)
  - a tool the model can call
  - a slash command the user can invoke
  - rewriting tool output
  - authorizing or blocking a call
  - reacting to turn end

Copy this file, rename it, and edit from here. Any language that can read
lines on stdin and write lines on stdout works the same way.
"""
import json
import sys


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def reply(req_id, **fields):
    obj = {"id": req_id}
    obj.update(fields)
    send(obj)


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue

        kind = req.get("type")
        rid = req.get("id")

        if kind == "hello":
            reply(
                rid,
                protocol=1,
                name="hello",
                version="1.0.0",
                system_prompt="An extension named `hello` is loaded.",
                tools=[
                    {
                        "name": "hello",
                        "description": "Greet someone. Demonstrates an extension-provided tool.",
                        "schema": {
                            "type": "object",
                            "properties": {"who": {"type": "string", "description": "Name to greet"}},
                            "required": ["who"],
                        },
                    }
                ],
                commands=[{"name": "hello", "description": "Print a greeting from the extension"}],
            )

        elif kind == "run_tool":
            who = "world"
            try:
                who = json.loads(req.get("arguments") or "{}").get("who", "world")
            except json.JSONDecodeError:
                pass
            reply(rid, output=f"Hello, {who}! (from the hello extension)")

        elif kind == "command":
            if req.get("name") == "hello":
                reply(rid, output=f"hello extension v1.0.0, args={req.get('args', '')!r}")
            else:
                # Always answer, even for a command we do not own. Staying
                # silent would make the host wait out its timeout.
                reply(rid, output="")

        elif kind == "system_prompt":
            reply(rid, text="An extension named `hello` is loaded.")

        elif kind == "authorize":
            # Demonstrates the hook a permission-gate extension would use.
            name = req.get("name")
            args = req.get("arguments") or ""
            if name == "bash" and "rm -rf /" in args:
                reply(rid, allow=False, reason="refusing a recursive delete of /")
            else:
                reply(rid, allow=True)

        elif kind == "tool_result":
            body = req.get("body", "")
            reply(rid, body=body.replace("SECRET", "[redacted by hello ext]"))

        elif kind == "turn_end":
            reply(rid)

        elif kind == "call_tool_result":
            pass  # the host answering one of our own requests

        elif kind == "goodbye":
            break


if __name__ == "__main__":
    main()
