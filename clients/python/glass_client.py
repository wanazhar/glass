"""Dependency-free Python thin client for the Glass MCP control plane."""

from __future__ import annotations

import json
import os
import socket
import subprocess
from typing import Any, BinaryIO, Literal, Optional, TypedDict


class GlassError(RuntimeError):
    """A stable Glass failure with machine-readable recovery guidance."""

    def __init__(
        self,
        message: str,
        *,
        code: str = "client.error",
        phase: str = "transport",
        mutation_possible: bool = False,
        retry_classification: str = "unknown",
        recommended_operation: str = "inspect_page",
        details: Any = None,
    ) -> None:
        super().__init__(message)
        self.code = code
        self.phase = phase
        self.mutation_possible = mutation_possible
        self.retry_classification = retry_classification
        self.recommended_operation = recommended_operation
        self.details = details

    @classmethod
    def from_error(cls, error: dict[str, Any]) -> "GlassError":
        data = error.get("data")
        if not isinstance(data, dict):
            return cls(f"{error.get('code')}: {error.get('message')}")
        retry = data.get("retry")
        if not isinstance(retry, dict):
            retry = {}
        return cls(
            str(data.get("message", error.get("message", "Glass request failed"))),
            code=str(data.get("code", error.get("code", "protocol.error"))),
            phase=str(data.get("phase", "transport")),
            mutation_possible=bool(data.get("mutationPossible", False)),
            retry_classification=str(retry.get("classification", "unknown")),
            recommended_operation=str(
                retry.get("recommendedOperation", "inspect_page")
            ),
            details=data.get("details"),
        )

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


class GlassCapabilityConstraints(TypedDict):
    platform: str
    browserFamily: Literal["chromium"]
    policy: str
    maxSessions: int


class GlassCapabilityManifest(TypedDict):
    protocolVersion: Literal[1]
    glassVersion: str
    schemas: dict[str, list[int]]
    capabilities: dict[str, bool]
    capabilityStatuses: dict[str, str]
    constraints: GlassCapabilityConstraints

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
        daemon_socket: Optional[str] = None,
        max_frame_bytes: int = 4 * 1024 * 1024,
    ) -> None:
        self._max_frame_bytes = max_frame_bytes
        self._socket: Optional[socket.socket] = None
        self._process: Optional[subprocess.Popen[bytes]] = None
        if daemon_socket is not None:
            self._socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            self._socket.connect(daemon_socket)
            self._stdin: BinaryIO = self._socket.makefile("wb")
            self._stdout: BinaryIO = self._socket.makefile("rb")
        else:
            merged_env = os.environ | (env or {})
            self._process = subprocess.Popen(
                [command, *(args or ["--mcp"])],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=None,
                cwd=cwd,
                env=merged_env,
            )
            if self._process.stdin is None or self._process.stdout is None:
                raise GlassError("Glass MCP pipes are unavailable")
            self._stdin = self._process.stdin
            self._stdout = self._process.stdout
        self._next_id = 1
        self._initialized = False
        self.capabilities: Optional[GlassCapabilityManifest] = None
        self._lease_token: Optional[str] = None

    def initialize(self) -> Any:
        if self._initialized:
            return None
        result = self._request(
            "initialize",
            {
                "protocolVersion": "2024-11-05",
                "glass": {
                    "protocolVersion": 1,
                    "schemas": {"action": [1], "observation": [1], "workflow": [1], "checkpoint": [1]},
                },
                "capabilities": {},
                "clientInfo": {"name": "glass-python-client", "version": "0.2.2"},
            },
        )
        manifest = result.get("glass") if isinstance(result, dict) else None
        if not isinstance(manifest, dict):
            raise GlassError("Glass capability manifest missing from initialize response")
        self.capabilities = manifest
        self._send({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}})
        self._initialized = True
        return manifest

    def supports_capability(self, capability: str) -> bool:
        """Return whether the negotiated runtime enables a capability."""
        manifest = self.initialize() or self.capabilities
        return bool(isinstance(manifest, dict) and manifest.get("capabilities", {}).get(capability))

    def capability_status(self, capability: str) -> Optional[str]:
        """Return the explicit availability status when the server provides one."""
        manifest = self.initialize() or self.capabilities
        statuses = manifest.get("capabilityStatuses", {}) if isinstance(manifest, dict) else {}
        status = statuses.get(capability)
        return status if isinstance(status, str) else None

    def supports_schema(self, schema: str, version: int) -> bool:
        """Return whether the negotiated runtime supports a schema version."""
        manifest = self.initialize() or self.capabilities
        versions = manifest.get("schemas", {}).get(schema, []) if isinstance(manifest, dict) else []
        return version in versions

    def require_capability(self, capability: str) -> None:
        """Raise before dispatch when an optional capability is unavailable."""
        if not self.supports_capability(capability):
            raise GlassError(f"Glass capability is unavailable: {capability}")

    def list_tools(self) -> list[dict[str, Any]]:
        """Return the negotiated MCP tool inventory."""
        self.initialize()
        result = self._request("tools/list", {})
        tools = result.get("tools") if isinstance(result, dict) else None
        if not isinstance(tools, list) or not all(isinstance(tool, dict) for tool in tools):
            raise GlassError("MCP tools/list returned an invalid tool inventory")
        return tools

    def call(self, name: str, arguments: Optional[dict[str, Any]] = None) -> Any:
        self.initialize()
        call_arguments = dict(arguments or {})
        if self._lease_token is not None:
            call_arguments.setdefault("leaseToken", self._lease_token)
        result = self._request("tools/call", {"name": name, "arguments": call_arguments})
        for part in result.get("content", []) if isinstance(result, dict) else []:
            text = part.get("text") if isinstance(part, dict) else None
            if isinstance(text, str):
                try:
                    return json.loads(text)
                except json.JSONDecodeError:
                    return text
        return result

    def acquire_mutation_lease(self, ttl_ms: int = 60_000) -> dict[str, Any]:
        """Acquire the daemon mutation lease for this connection."""
        self.initialize()
        result = self._request("glass/lease/acquire", {"ttlMs": ttl_ms})
        if not isinstance(result, dict) or not isinstance(result.get("token"), str):
            raise GlassError("daemon lease response did not include a token")
        self._lease_token = result["token"]
        return result

    def renew_mutation_lease(self, ttl_ms: int = 60_000) -> dict[str, Any]:
        """Renew the daemon mutation lease held by this connection."""
        self.initialize()
        if self._lease_token is None:
            raise GlassError("no daemon mutation lease is held")
        result = self._request(
            "glass/lease/renew", {"token": self._lease_token, "ttlMs": ttl_ms}
        )
        if not isinstance(result, dict):
            raise GlassError("daemon lease renewal returned an invalid response")
        return result

    def release_mutation_lease(self) -> None:
        """Release the daemon mutation lease held by this connection."""
        self.initialize()
        if self._lease_token is None:
            return
        self._request("glass/lease/release", {"token": self._lease_token})
        self._lease_token = None

    def observe(
        self,
        level: Optional[SemanticObservationLevel] = None,
        region: Optional[str] = None,
        include_dom: Optional[bool] = None,
        include_screenshot: Optional[bool] = None,
        include_form_values: Optional[bool] = None,
    ) -> Any:
        args: dict[str, Any] = {}
        if level is not None:
            args["level"] = level
        if region is not None:
            args["region"] = region
        if include_dom is not None:
            args["includeDom"] = include_dom
        if include_screenshot is not None:
            args["includeScreenshot"] = include_screenshot
        if include_form_values is not None:
            args["includeFormValues"] = include_form_values
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

    def hover(self, target: str, expected_revision: Optional[int] = None) -> Any:
        return self._guarded("hover", {"target": target}, expected_revision)

    def drag(self, source: str, destination: str, expected_revision: Optional[int] = None) -> Any:
        return self._guarded(
            "drag", {"source": source, "destination": destination}, expected_revision
        )

    def key_down(self, key: str, expected_revision: Optional[int] = None) -> Any:
        return self._guarded("keyDown", {"key": key}, expected_revision)

    def key_up(self, key: str, expected_revision: Optional[int] = None) -> Any:
        return self._guarded("keyUp", {"key": key}, expected_revision)

    def shortcut(self, shortcut: str, expected_revision: Optional[int] = None) -> Any:
        return self._guarded("shortcut", {"shortcut": shortcut}, expected_revision)

    def upload(
        self,
        target: str,
        files: list[str],
        expected_revision: Optional[int] = None,
    ) -> Any:
        return self._guarded("upload", {"target": target, "files": files}, expected_revision)

    def screenshot(self, arguments: Optional[dict[str, Any]] = None) -> Any:
        return self.call("screenshot", arguments or {})

    def observe_knowledge(self, arguments: Optional[dict[str, Any]] = None) -> Any:
        return self.call("observeKnowledge", arguments or {})

    def resolve_intent_with_knowledge(self, arguments: dict[str, Any]) -> SemanticIntentResult:
        return self.call("resolveIntentWithKnowledge", arguments)

    def knowledge_list(self) -> Any:
        return self.call("knowledgeList")

    def knowledge_show(self, record_id: str) -> Any:
        return self.call("knowledgeShow", {"recordId": record_id})

    def knowledge_stats(self) -> Any:
        return self.call("knowledgeStats")

    def knowledge_invalidate(
        self,
        record_id: str,
        state: str,
        reason: Optional[str] = None,
        observed_at: Optional[str] = None,
    ) -> Any:
        arguments: dict[str, Any] = {"recordId": record_id, "state": state}
        if reason is not None:
            arguments["reason"] = reason
        if observed_at is not None:
            arguments["observedAt"] = observed_at
        return self.call("knowledgeInvalidate", arguments)

    def knowledge_purge(self, origin: str) -> Any:
        return self.call("knowledgePurge", {"origin": origin})

    def preflight(self, target: str, action: str = "click") -> Any:
        return self.call("preflight", {"target": target, "action": action})

    def click_at(self, x: float, y: float) -> Any:
        return self.call("clickAt", {"x": x, "y": y})

    def dom(self) -> Any:
        return self.call("getDOM")

    def text(self) -> Any:
        return self.call("getText")

    def reconcile_references(
        self,
        from_revision: int,
        refs: list[str],
        hints: Optional[list[str]] = None,
        scope_ref: Optional[str] = None,
    ) -> Any:
        arguments: dict[str, Any] = {"fromRevision": from_revision, "refs": refs}
        if hints is not None:
            arguments["hints"] = hints
        if scope_ref is not None:
            arguments["scopeRef"] = scope_ref
        return self.call("reconcileReferences", arguments)

    def observe_delta(self) -> Any:
        return self.call("observeDelta")

    def set_network_conditions(self, arguments: dict[str, Any]) -> Any:
        return self.call("setNetworkConditions", arguments)

    def clear_network_conditions(self) -> Any:
        return self.call("clearNetworkConditions")

    def set_cpu_throttling(self, rate: float) -> Any:
        return self.call("setCpuThrottling", {"rate": rate})

    def clear_cpu_throttling(self) -> Any:
        return self.call("clearCpuThrottling")

    def set_user_agent(
        self,
        user_agent: str,
        accept_language: Optional[str] = None,
        platform: Optional[str] = None,
    ) -> Any:
        arguments: dict[str, Any] = {"userAgent": user_agent}
        if accept_language is not None:
            arguments["acceptLanguage"] = accept_language
        if platform is not None:
            arguments["platform"] = platform
        return self.call("setUserAgent", arguments)

    def clear_user_agent(self) -> Any:
        return self.call("clearUserAgent")

    def export_checkpoint(self) -> Any:
        return self.call("exportCheckpoint")

    def import_checkpoint(self, checkpoint: dict[str, Any]) -> Any:
        return self.call("importCheckpoint", checkpoint)

    def evaluate(self, expression: str) -> Any:
        return self.call("evaluate", {"expression": expression})

    def diagnostics(self, duration_ms: int = 1_000) -> Any:
        return self.call("diagnostics", {"durationMs": duration_ms})

    def accept_dialog(self) -> Any:
        return self.call("acceptDialog")

    def dismiss_dialog(self) -> Any:
        return self.call("dismissDialog")

    def dismiss_consent(self) -> Any:
        return self.call("dismissConsent")

    def download(self, destination: str, timeout_ms: int = 30_000) -> Any:
        return self.call("download", {"destination": destination, "timeoutMs": timeout_ms})

    def list_targets(self) -> Any:
        return self.call("listTargets")

    def create_target(self, url: str) -> Any:
        return self.call("createTarget", {"url": url})

    def select_target(self, target_id: str) -> Any:
        return self.call("selectTarget", {"id": target_id})

    def close_target(self, target_id: str) -> Any:
        return self.call("closeTarget", {"id": target_id})

    def list_frames(self) -> Any:
        return self.call("listFrames")

    def select_frame(self, frame_id: str) -> Any:
        return self.call("selectFrame", {"id": frame_id})

    def cookies(self) -> Any:
        return self.call("cookies")

    def set_cookies(self, cookies: Any) -> Any:
        return self.call("setCookies", {"cookies": cookies})

    def clear_cookies(self) -> Any:
        return self.call("clearCookies")

    def local_storage(self) -> Any:
        return self.call("localStorage")

    def session_storage(self) -> Any:
        return self.call("sessionStorage")

    def print_to_pdf(self, options: Optional[dict[str, Any]] = None) -> Any:
        return self.call("printToPdf", options or {})


    def clipboard_read(self) -> Any:
        return self.call("clipboardRead")

    def clipboard_write(self, text: str) -> Any:
        return self.call("clipboardWrite", {"text": text})

    def set_geolocation(self, latitude: float, longitude: float) -> Any:
        return self.call("setGeolocation", {"latitude": latitude, "longitude": longitude})

    def clear_geolocation(self) -> Any:
        return self.call("clearGeolocation")

    def set_timezone(self, timezone_id: str) -> Any:
        return self.call("setTimezone", {"timezoneId": timezone_id})

    def close(self) -> None:
        if self._process is not None:
            if self._process.poll() is None:
                self._process.terminate()
                self._process.wait(timeout=5)
        else:
            self._stdin.close()
            self._stdout.close()
            if self._socket is not None:
                self._socket.close()

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
                raise GlassError.from_error(error)
            return message.get("result")

    def _send(self, message: dict[str, Any]) -> None:
        self._stdin.write((json.dumps(message, separators=(",", ":")) + "\n").encode())
        self._stdin.flush()

    def _read_message(self) -> dict[str, Any]:
        first = self._stdout.readline(self._max_frame_bytes + 1)
        if not first:
            raise GlassError("Glass exited before returning an MCP response")
        if len(first) > self._max_frame_bytes:
            raise GlassError("MCP frame exceeds client limit")
        if first.lower().startswith(b"content-length:"):
            headers = first + self._stdout.readline(8192)
            while not headers.endswith(b"\r\n\r\n"):
                headers += self._stdout.readline(8192)
            length = next((int(line.split(b":", 1)[1]) for line in headers.splitlines() if line.lower().startswith(b"content-length:")), -1)
            if length < 0 or length > self._max_frame_bytes:
                raise GlassError("invalid MCP Content-Length")
            body = self._stdout.read(length)
        else:
            body = first.strip()
        try:
            return json.loads(body)
        except json.JSONDecodeError as error:
            raise GlassError("invalid MCP JSON response") from error
