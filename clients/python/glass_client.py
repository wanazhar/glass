"""Dependency-free Python thin client for the Glass MCP control plane."""

from __future__ import annotations

import json
import os
import subprocess
from typing import Any, Optional


class GlassError(RuntimeError):
    """A bounded MCP or process failure."""


VerificationPredicate = dict[str, Any]
BatchMode = str


class GlassClient:
    def __init__(
        self,
        command: str = "glass",
        args: Optional[list[str]] = None,
        *,
        env: Optional[dict[str, str]] = None,
        cwd: Optional[str] = None,
        max_frame_bytes: int = 4 * 1024 * 1024,
    ) -> None:
        self._max_frame_bytes = max_frame_bytes
        merged_env = os.environ | (env or {})
        self._process = subprocess.Popen(
            [command, *(args or ["--mcp"])],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=None,
            cwd=cwd,
            env=merged_env,
        )
        self._next_id = 1
        self._initialized = False

    def initialize(self) -> Any:
        if self._initialized:
            return None
        result = self._request(
            "initialize",
            {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "glass-python-client", "version": "0.1.0"},
            },
        )
        self._send({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}})
        self._initialized = True
        return result

    def call(self, name: str, arguments: Optional[dict[str, Any]] = None) -> Any:
        self.initialize()
        result = self._request("tools/call", {"name": name, "arguments": arguments or {}})
        for part in result.get("content", []) if isinstance(result, dict) else []:
            text = part.get("text") if isinstance(part, dict) else None
            if isinstance(text, str):
                try:
                    return json.loads(text)
                except json.JSONDecodeError:
                    return text
        return result

    def observe(self) -> Any:
        return self.call("observe")

    def navigate(
        self,
        url: str,
        timeout_ms: Optional[int] = None,
        expected_revision: Optional[int] = None,
    ) -> Any:
        args: dict[str, Any] = {"url": url}
        if timeout_ms is not None:
            args["timeoutMs"] = timeout_ms
        if expected_revision is not None:
            args["expectedRevision"] = expected_revision
        return self.call("navigate", args)

    def click(self, target: str, expected_revision: Optional[int] = None) -> Any:
        args: dict[str, Any] = {"target": target}
        if expected_revision is not None:
            args["expectedRevision"] = expected_revision
        return self.call("click", args)

    def _guarded(self, name: str, arguments: dict[str, Any], expected_revision: Optional[int]) -> Any:
        if expected_revision is not None:
            arguments["expectedRevision"] = expected_revision
        return self.call(name, arguments)

    def click_expect_popup(self, target: str, expected_revision: Optional[int] = None) -> Any:
        return self._guarded("clickExpectPopup", {"target": target}, expected_revision)

    def double_click(self, target: str, expected_revision: Optional[int] = None) -> Any:
        return self._guarded("doubleClick", {"target": target}, expected_revision)

    def type_text(
        self,
        text: str,
        target: Optional[str] = None,
        expected_revision: Optional[int] = None,
    ) -> Any:
        args: dict[str, Any] = {"text": text}
        if target is not None:
            args["target"] = target
        return self._guarded("type", args, expected_revision)

    def clear(self, target: str, expected_revision: Optional[int] = None) -> Any:
        return self._guarded("clear", {"target": target}, expected_revision)

    def check(self, target: str, expected_revision: Optional[int] = None) -> Any:
        return self._guarded("check", {"target": target}, expected_revision)

    def uncheck(self, target: str, expected_revision: Optional[int] = None) -> Any:
        return self._guarded("uncheck", {"target": target}, expected_revision)

    def select(self, target: str, value: str, expected_revision: Optional[int] = None) -> Any:
        return self._guarded("select", {"target": target, "value": value}, expected_revision)

    def scroll(self, dx: float = 0, dy: float = 600, expected_revision: Optional[int] = None) -> Any:
        return self._guarded("scroll", {"dx": dx, "dy": dy}, expected_revision)

    def key(self, key: str, expected_revision: Optional[int] = None) -> Any:
        return self._guarded("key", {"key": key}, expected_revision)

    def fill_form(
        self,
        fields: list[dict[str, str]],
        expected_revision: Optional[int] = None,
    ) -> Any:
        return self._guarded("fillForm", {"fields": fields}, expected_revision)

    def verify(
        self,
        predicate: VerificationPredicate,
        timeout_ms: Optional[int] = None,
    ) -> Any:
        args: dict[str, Any] = {"predicate": predicate}
        if timeout_ms is not None:
            args["timeoutMs"] = timeout_ms
        return self.call("verify", args)

    def batch(
        self,
        steps: list[dict[str, Any]],
        mode: BatchMode = "unguarded",
        expected_revision: Optional[int] = None,
        atomic: bool = False,
    ) -> Any:
        args: dict[str, Any] = {"steps": steps, "mode": mode, "atomic": atomic}
        if expected_revision is not None:
            args["expectedRevision"] = expected_revision
        return self.call("batch", args)

    def workflow(
        self,
        definition: dict[str, Any],
        inputs: Optional[dict[str, Any]] = None,
    ) -> Any:
        return self.call("workflow", {"workflow": definition, "inputs": inputs or {}})

    def wait(self, condition: str, timeout_ms: Optional[int] = None) -> Any:
        args: dict[str, Any] = {"condition": condition}
        if timeout_ms is not None:
            args["timeoutMs"] = timeout_ms
        return self.call("wait", args)

    def close(self) -> None:
        if self._process.poll() is None:
            self._process.terminate()
            self._process.wait(timeout=5)

    def _request(self, method: str, params: dict[str, Any]) -> Any:
        request_id = self._next_id
        self._next_id += 1
        self._send({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params})
        while True:
            message = self._read_message()
            if message.get("id") != request_id:
                continue
            if "error" in message:
                error = message["error"]
                raise GlassError(f"{error.get('code')}: {error.get('message')}")
            return message.get("result")

    def _send(self, message: dict[str, Any]) -> None:
        if self._process.stdin is None:
            raise GlassError("Glass stdin is closed")
        self._process.stdin.write((json.dumps(message, separators=(",", ":")) + "\n").encode())
        self._process.stdin.flush()

    def _read_message(self) -> dict[str, Any]:
        if self._process.stdout is None:
            raise GlassError("Glass stdout is closed")
        first = self._process.stdout.readline(self._max_frame_bytes + 1)
        if not first:
            raise GlassError("Glass exited before returning an MCP response")
        if len(first) > self._max_frame_bytes:
            raise GlassError("MCP frame exceeds client limit")
        if first.lower().startswith(b"content-length:"):
            headers = first + self._process.stdout.readline(8192)
            while not headers.endswith(b"\r\n\r\n"):
                headers += self._process.stdout.readline(8192)
            length = next((int(line.split(b":", 1)[1]) for line in headers.splitlines() if line.lower().startswith(b"content-length:")), -1)
            if length < 0 or length > self._max_frame_bytes:
                raise GlassError("invalid MCP Content-Length")
            body = self._process.stdout.read(length)
        else:
            body = first.strip()
        try:
            return json.loads(body)
        except json.JSONDecodeError as error:
            raise GlassError("invalid MCP JSON response") from error
