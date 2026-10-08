#!/usr/bin/env python3
"""A tiny stdio MCP server used by the bridge tests. Not a product."""
import json
import sys


def main():
    count = 2
    if "--tools" in sys.argv:
        count = int(sys.argv[sys.argv.index("--tools") + 1])
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        msg = json.loads(line)
        method = msg.get("method")
        mid = msg.get("id")
        if method == "initialize":
            reply = {
                "jsonrpc": "2.0",
                "id": mid,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "fixture", "version": "1"},
                },
            }
        elif method == "notifications/initialized":
            continue
        elif method == "tools/list":
            tools = [
                {
                    "name": "echo",
                    "description": "Echo text back.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {"text": {"type": "string"}},
                        "required": ["text"],
                    },
                },
                {
                    "name": "fail",
                    "description": "Always fails.",
                    "inputSchema": {"type": "object"},
                },
            ]
            for i in range(max(0, count - 2)):
                tools.append(
                    {
                        "name": f"tool_{i}",
                        "description": f"Dummy tool number {i}.",
                        "inputSchema": {"type": "object"},
                    }
                )
            reply = {"jsonrpc": "2.0", "id": mid, "result": {"tools": tools}}
        elif method == "tools/call":
            params = msg.get("params") or {}
            name = params.get("name")
            arguments = params.get("arguments") or {}
            if name == "echo":
                result = {
                    "content": [
                        {"type": "text", "text": "echo: " + str(arguments.get("text", ""))}
                    ]
                }
            elif name == "fail":
                result = {
                    "content": [{"type": "text", "text": "it broke"}],
                    "isError": True,
                }
            else:
                result = {"content": [{"type": "text", "text": f"ran {name}"}]}
            reply = {"jsonrpc": "2.0", "id": mid, "result": result}
        else:
            reply = {
                "jsonrpc": "2.0",
                "id": mid,
                "error": {"code": -32601, "message": "method not found"},
            }
        sys.stdout.write(json.dumps(reply) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
