"""Dependency-free Python thin client for the Glass MCP control plane."""

from __future__ import annotations

import json
import os
import subprocess
from typing import Any, Literal, Optional, TypedDict


class GlassError(RuntimeError):
    """A bounded MCP or process failure."""


VerificationPredicate = dict[str, Any]
BatchMode = str
WorkflowValueType = Literal["string", "integer", "number", "boolean", "url"]
WorkflowTransaction = Literal[
    "read_only", "idempotent", "conditionally_idempotent", "non_idempotent", "unknown"
]


class WorkflowInput(TypedDict, total=False):
    valueType: WorkflowValueType
    required: bool
    maxLength: int
    sensitive: bool


class WorkflowBudgets(TypedDict):
    maxSteps: int
    maxDurationMs: int
    maxRetries: int
    maxExtractedBytes: int


class WorkflowStep(TypedDict, total=False):
    id: str
    action: str
    transaction: WorkflowTransaction
    when: VerificationPredicate
    expect: VerificationPredicate
    beforeRetry: VerificationPredicate
    idempotencyKey: str
    maxRetries: int
    repeat: int


class WorkflowOutputDeclaration(TypedDict, total=False):
    valueType: WorkflowValueType
    source: Literal["page_url", "page_title", "visible_text"]
    required: bool
    sensitive: bool


class WorkflowDefinition(TypedDict, total=False):
    schemaVersion: Literal[1]
    name: str
    workflowVersion: str
    description: str
    inputs: dict[str, WorkflowInput]
    budgets: WorkflowBudgets
    preconditions: list[VerificationPredicate]
    steps: list[WorkflowStep]
    terminalCondition: VerificationPredicate
    outputs: dict[str, WorkflowOutputDeclaration]


WorkflowInputs = dict[str, Any]

SemanticObservationLevel = Literal["summary", "interactive", "structured", "detailed", "raw"]
SemanticConfidence = Literal["exact", "high", "medium", "low", "unknown"]


class SemanticTarget(TypedDict, total=False):
    reference: str
    role: str
    name: str
    inputType: str


class SemanticRegion(TypedDict, total=False):
    id: str
    kind: str
    label: str
    interactiveCount: int
    itemCount: int
    confidence: SemanticConfidence
    evidence: list[str]
    targets: list[SemanticTarget]
    expansion: dict[str, Any]


class SemanticObservation(TypedDict, total=False):
    schemaVersion: Literal[1]
    revision: int
    level: SemanticObservationLevel
    route: dict[str, str]
    page: dict[str, Any]
    regions: list[SemanticRegion]
    text: str
    accessibility: list[dict[str, Any]]
    rawAccessibility: list[dict[str, Any]]
    changes: dict[str, Any]
    limits: dict[str, Any]


SemanticIntentAction = Literal[
    "click", "type", "clear", "check", "uncheck", "select", "submit", "open", "close",
    "search", "filter", "sort", "paginate", "toggle", "expand", "collapse", "download",
    "upload", "inspect", "extract",
]
SemanticResolutionPolicy = Literal[
    "reportOnly", "requireExact", "requireUniqueHighConfidence",
    "allowUniqueMediumConfidence", "interactiveConfirmation",
]


class SemanticIntentRequest(TypedDict, total=False):
    schemaVersion: Literal[1]
    intent: str
    action: SemanticIntentAction
    scope: dict[str, Any]
    constraints: dict[str, Any]
    resolutionPolicy: SemanticResolutionPolicy
    expectedRevision: int


class SemanticIntentExecutionRequest(SemanticIntentRequest, total=False):
    candidateId: str
    value: str


class SemanticIntentResult(TypedDict, total=False):
    schemaVersion: Literal[1]
    intent: str
    action: SemanticIntentAction
    normalizedIntent: str
    resolution: str
    policyDecision: str
    revision: int
    candidates: list[dict[str, Any]]
    excludedCandidates: list[dict[str, Any]]
    excludedCount: int
    selectedCandidate: str
    suggestedConstraints: list[dict[str, Any]]
    reason: str
    route: dict[str, str]


class SemanticIntentExecutionResult(TypedDict, total=False):
    resolutionId: str
    candidateId: str
    status: Literal["executed", "not_executed"]
    resolution: SemanticIntentResult
    action: dict[str, Any]
    executionId: str
    reason: str


class WorkflowCheckpointStep(TypedDict, total=False):
    id: str
    state: str
    attempts: int
    history: list[str]
    executionIds: list[str]
    dispatchAcknowledged: bool
    effectObserved: bool
    postconditionVerified: bool
    retrySafe: bool
    previousRevision: int
    currentRevision: int
    branchDecision: VerificationPredicate


class WorkflowCheckpoint(TypedDict, total=False):
    schemaVersion: int
    runId: str
    workflowName: str
    workflowVersion: str
    definitionHash: str
    status: str
    nextStepIndex: int
    steps: list[WorkflowCheckpointStep]


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

    def observe(
        self,
        level: Optional[SemanticObservationLevel] = None,
        region: Optional[str] = None,
    ) -> Any:
        args: dict[str, Any] = {}
        if level is not None:
            args["level"] = level
        if region is not None:
            args["region"] = region
        return self.call("observe", args)

    def observe_semantic(
        self,
        level: SemanticObservationLevel,
        region: Optional[str] = None,
    ) -> SemanticObservation:
        return self.observe(level, region)

    def resolve_intent(self, request: SemanticIntentRequest) -> SemanticIntentResult:
        return self.call("resolveIntent", dict(request))

    def execute_intent(
        self, request: SemanticIntentExecutionRequest
    ) -> SemanticIntentExecutionResult:
        return self.call("executeIntent", dict(request))

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
        definition: WorkflowDefinition,
        inputs: Optional[WorkflowInputs] = None,
        checkpoint: Optional[WorkflowCheckpoint] = None,
    ) -> Any:
        args: dict[str, Any] = {"workflow": definition, "inputs": inputs or {}}
        if checkpoint is not None:
            args["checkpoint"] = checkpoint
        return self.call("workflow", args)

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
