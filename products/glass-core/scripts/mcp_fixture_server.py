#!/usr/bin/env python3
"""A tiny stdio MCP server used by the bridge tests. Not a product."""
import json
import sys


def main():
    count = 2
    if "--tools" in sys.argv:
        count = int(sys.argv[sys.argv.index("--tools") + 1])
    elicitation = "--elicitation" in sys.argv
    ttl_ms = None
    if "--ttl-ms" in sys.argv:
        ttl_ms = int(sys.argv[sys.argv.index("--ttl-ms") + 1])
    drop_after_first = None
    if "--drop-after-first" in sys.argv:
        drop_after_first = sys.argv[sys.argv.index("--drop-after-first") + 1]
    lists = 0
    elicitation_ids = []
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
            lists += 1
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
            if elicitation:
                tools.append({"name": "ask", "description": "Ask for reviewed input.", "inputSchema": {"type": "object", "properties": {"original": {"type": "string"}}}})
            for i in range(max(0, count - 2)):
                tools.append(
                    {
                        "name": f"tool_{i}",
                        "description": f"Dummy tool number {i}.",
                        "inputSchema": {"type": "object"},
                    }
                )
            if drop_after_first is not None and lists > 1:
                tools = [t for t in tools if t["name"] != drop_after_first]
            result = {"tools": tools}
            if ttl_ms is not None:
                result["ttlMs"] = ttl_ms
            reply = {"jsonrpc": "2.0", "id": mid, "result": result}
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
            elif name == "ask" and elicitation:
                elicitation_ids.append(mid)
                if "inputResponses" not in params:
                    result = {"resultType": "input_required", "inputRequests": {
                        "opaque-form-key": {"method": "elicitation/create", "params": {
                            "mode": "form", "message": "Choose a label",
                            "requestedSchema": {"type": "object", "properties": {"label": {"type": "string", "title": "Label"}}, "required": ["label"]}
                        }}
                    }, "requestState": "fixture-state-exact"}
                elif (len(elicitation_ids) == 2 and elicitation_ids[0] != mid
                      and params.get("requestState") == "fixture-state-exact"
                      and set(params.get("inputResponses", {})) == {"opaque-form-key"}
                      and params["inputResponses"]["opaque-form-key"].get("action") == "accept"
                      and params.get("arguments") == {"original": "unchanged"}):
                    result = {"content": [{"type": "text", "text": "finished without echoing input"}]}
                else:
                    # Only structural metadata is included so protocol tests
                    # can diagnose failures without echoing submitted values.
                    result = {"isError": True, "content": [{"type": "text", "text": (
                        "retry contract mismatch: " + json.dumps({
                            "call_id": mid, "prior_ids": elicitation_ids[:-1],
                            "state": params.get("requestState"),
                            "response_keys": sorted((params.get("inputResponses") or {}).keys()),
                            "action": (params.get("inputResponses") or {}).get("opaque-form-key", {}).get("action"),
                            "arguments": arguments,
                        })
                    )}]}
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
