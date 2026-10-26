#!/usr/bin/env python3
"""Small stdio LSP fixture that returns a two-file rename WorkspaceEdit."""

import json
import sys
from pathlib import Path
from urllib.parse import quote, unquote, urlparse


def read_message():
    content_length = None
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        if line in (b"\r\n", b"\n"):
            break
        name, _, value = line.decode("ascii").partition(":")
        if name.lower() == "content-length":
            content_length = int(value.strip())
    if content_length is None:
        raise RuntimeError("LSP request omitted Content-Length")
    return json.loads(sys.stdin.buffer.read(content_length))


def write_message(message):
    payload = json.dumps(message, separators=(",", ":")).encode("utf-8")
    sys.stdout.buffer.write(f"Content-Length: {len(payload)}\r\n\r\n".encode("ascii"))
    sys.stdout.buffer.write(payload)
    sys.stdout.buffer.flush()


documents = {}

while True:
    request = read_message()
    if request is None:
        break
    method = request.get("method")
    request_id = request.get("id")
    if request_id is None:
        params = request.get("params", {})
        if method == "textDocument/didOpen":
            document = params.get("textDocument", {})
            documents[document.get("uri")] = document.get("text", "")
        elif method == "textDocument/didChange":
            document = params.get("textDocument", {})
            changes = params.get("contentChanges", [])
            if changes:
                documents[document.get("uri")] = changes[0].get("text", "")
        continue
    if method == "initialize":
        result = {"capabilities": {"renameProvider": True, "textDocumentSync": 1}}
    elif method == "textDocument/rename":
        source_uri = request["params"]["textDocument"]["uri"]
        source_path = Path(unquote(urlparse(source_uri).path))
        call_path = source_path.with_name("second.rs")
        source_uri = "file://" + quote(str(source_path))
        call_uri = "file://" + quote(str(call_path))
        if "// unsaved first\n" not in documents.get(source_uri, ""):
            write_message({
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {"code": -32602, "message": "rename did not receive the unsaved source buffer"},
            })
            continue
        if "// unsaved second\n" not in documents.get(call_uri, ""):
            write_message({
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {"code": -32602, "message": "rename did not receive the unsaved call-site buffer"},
            })
            continue
        result = {
            "documentChanges": [
                {
                    "textDocument": {"uri": source_uri, "version": 1},
                    "edits": [
                        {
                            "range": {
                                "start": {"line": 1, "character": 3},
                                "end": {"line": 1, "character": 11},
                            },
                            "newText": "new_name",
                        }
                    ],
                },
                {
                    "textDocument": {"uri": call_uri, "version": 1},
                    "edits": [
                        {
                            "range": {
                                "start": {"line": 1, "character": 0},
                                "end": {"line": 1, "character": 8},
                            },
                            "newText": "new_name",
                        }
                    ],
                },
            ]
        }
    elif method == "textDocument/rangeFormatting":
        source_uri = request["params"]["textDocument"]["uri"]
        selection = request["params"]["range"]
        if "// unsaved first\n" not in documents.get(source_uri, ""):
            write_message({
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {"code": -32602, "message": "range format did not receive the unsaved source buffer"},
            })
            continue
        if selection != {
            "start": {"line": 1, "character": 3},
            "end": {"line": 1, "character": 11},
        }:
            write_message({
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {"code": -32602, "message": "range format received the wrong selected range"},
            })
            continue
        result = [{"range": selection, "newText": "formatted"}]
    elif method == "shutdown":
        result = None
    else:
        result = None
    write_message({"jsonrpc": "2.0", "id": request_id, "result": result})
