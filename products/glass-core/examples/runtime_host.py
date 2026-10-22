#!/usr/bin/env python3
"""Small standard-library host for gc runtime; no terminal or GC UI required."""
import argparse
import json
import queue
import subprocess
import threading
from pathlib import Path

MAX_FRAME = 1024 * 1024


class RuntimeClient:
    def __init__(self, binary="gc", cwd=None, env=None):
        self.process = subprocess.Popen(
            [str(binary), "runtime"], cwd=cwd, env=env,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        )
        self.messages = queue.Queue(maxsize=64)
        self.sequence = 0

        def read():
            try:
                while True:
                    line = self.process.stdout.readline(MAX_FRAME + 1)
                    if not line:
                        raise EOFError("runtime disconnected")
                    if len(line) > MAX_FRAME:
                        raise ValueError("runtime frame exceeds 1 MiB")
                    self.messages.put(json.loads(line))
            except Exception as exc:
                self.messages.put(exc)

        self.reader = threading.Thread(target=read, daemon=True)
        self.reader.start()

    def send(self, value):
        body = (json.dumps(value, ensure_ascii=False) + "\n").encode()
        if len(body) > MAX_FRAME:
            raise ValueError("runtime frame exceeds 1 MiB")
        self.process.stdin.write(body)
        self.process.stdin.flush()

    def request(self, method, params):
        self.sequence += 1
        ident = f"host-{self.sequence}"
        self.send({"jsonrpc": "2.0", "id": ident, "method": method, "params": params})
        return ident

    def receive(self, timeout=30):
        value = self.messages.get(timeout=timeout)
        if isinstance(value, Exception):
            raise value
        return value

    def result(self, ident, text, is_error=False):
        result = text if isinstance(text, dict) else {"text": text, "isError": is_error}
        self.send({"jsonrpc": "2.0", "id": ident, "result": result})

    def wait(self, ident, tool=None, event=None, timeout=60):
        """Use request/receive directly for concurrent controls or UI polling."""
        while True:
            value = self.receive(timeout)
            if value.get("method") == "tool/execute":
                if tool is None:
                    self.result(value["id"], "host has no tool executor", True)
                else:
                    try:
                        text = tool(value["params"])
                        self.result(value["id"], text)
                    except Exception as exc:
                        self.result(value["id"], str(exc), True)
            elif value.get("method") == "runtime/event":
                if event:
                    event(value["params"])
            elif value.get("id") == ident:
                if "error" in value:
                    raise RuntimeError(value["error"]["message"])
                return value["result"]
            else:
                raise RuntimeError(f"unexpected response: {value}")

    def close(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
        self.process.stdin.close()
        self.process.stdout.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("prompt")
    parser.add_argument("--binary", default="gc")
    parser.add_argument("--workspace", type=Path, default=Path.cwd())
    parser.add_argument("--storage", type=Path, required=True)
    args = parser.parse_args()
    workspace = args.workspace.resolve()
    client = RuntimeClient(args.binary, workspace)
    tools = [
        {"name": "read_file", "description": "Read up to 64 KiB of a workspace file.",
         "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}},
                         "required": ["path"], "additionalProperties": False}},
        {"name": "ask_user_question",
         "description": "Ask the user a question; the answer may be any custom text.",
         "inputSchema": {"type": "object", "properties": {"question": {"type": "string"}},
                         "required": ["question"], "additionalProperties": False}},
    ]

    def execute(call):
        if call["name"] == "ask_user_question":
            return input(call["arguments"]["question"] + "\n> ")
        if call["name"] != "read_file":
            raise ValueError("unknown host tool")
        path = (workspace / call["arguments"]["path"]).resolve()
        if not path.is_relative_to(workspace):
            raise ValueError("path escapes reference host workspace")
        with path.open("rb") as source:
            data = source.read(65537)
        if len(data) > 65536:
            raise ValueError("file exceeds reference host read limit")
        return data.decode("utf-8")

    def render(event):
        if event["type"] == "text":
            print(event["data"]["text"], end="", flush=True)
        elif event["type"] in ("tool_start", "tool_end", "settled"):
            print(f"\n[{event['type']}] {event['data']}")

    try:
        client.wait(client.request("initialize", {
            "protocolVersion": 1, "storageDir": str(args.storage.resolve())}))
        session = client.wait(client.request("session/new", {
            "cwd": str(workspace), "toolMode": "host", "tools": tools,
            "instructions": "Help the user using the supplied tools. Ask when clarification is needed."}))
        client.wait(client.request("turn/prompt", {
            **session, "text": args.prompt}), tool=execute, event=render)
        client.wait(client.request("shutdown", {}))
    finally:
        client.close()


if __name__ == "__main__":
    main()
