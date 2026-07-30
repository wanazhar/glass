import { spawn } from "node:child_process";
import { createConnection, type Socket } from "node:net";
import type { Readable, Writable } from "node:stream";

export interface JsonRpcResult<T> { result: T; }
export interface McpErrorShape { code: number; message: string; data?: unknown; }
export interface ToolCallResult<T = unknown> {
  content?: Array<{ type: string; text?: string; [key: string]: unknown }>;
  isError?: boolean;
  [key: string]: unknown;
}

export type VerificationPredicate =
  | { urlEquals: string }
  | { titleContains: string }
  | { visible: string }
  | { textContains: string }
  | { popupOpened: boolean }
  | { dialogOpen: boolean }
  | { downloadStarted: boolean }
  | { revisionEquals: number }
  | { all: VerificationPredicate[] }
  | { any: VerificationPredicate[] }
  | { not: VerificationPredicate };

export type BatchMode = "fixed" | "chain" | "unguarded";
export type WorkflowValueType = "string" | "integer" | "number" | "boolean" | "url";
export type WorkflowTransaction = "read_only" | "idempotent" | "conditionally_idempotent" | "non_idempotent" | "unknown";
export type WorkflowOutputSource = "page_url" | "page_title" | "visible_text";
export type WorkflowStepState =
  | "pending" | "ready" | "preflight" | "resolving" | "not_dispatched"
  | "dispatched" | "effect_observed" | "verified" | "outputs_extracted"
  | "committed" | "failed_before_dispatch" | "failed_after_dispatch"
  | "indeterminate" | "skipped";

export type SemanticObservationLevel = "summary" | "interactive" | "structured" | "detailed" | "raw";
export type SemanticConfidence = "exact" | "high" | "medium" | "low" | "unknown";
export type SemanticPageKind =
  | "generic" | "home" | "search" | "searchResults" | "article" | "documentation"
  | "listing" | "detail" | "form" | "authentication" | "checkout" | "confirmation"
  | "dashboard" | "settings" | "error" | "accessDenied" | "unknown";
export type SemanticRegionKind =
  | "navigation" | "main" | "search" | "form" | "dialog" | "alert" | "status"
  | "toolbar" | "filterPanel" | "results" | "collection" | "table" | "pagination"
  | "article" | "sidebar" | "checkoutSummary" | "authentication" | "footer" | "unknown";

export interface SemanticRouteIdentity { targetId: string; frameId: string; url: string; }
export interface SemanticTarget { reference: string; role: string; name?: string; inputType?: string; }
export interface SemanticAccessibilityNode {
  role: string;
  name?: string;
  children?: SemanticAccessibilityNode[];
  interactive?: boolean;
}
export interface SemanticRegion {
  id: string;
  kind: SemanticRegionKind;
  label: string;
  interactiveCount: number;
  itemCount?: number;
  confidence: SemanticConfidence;
  evidence?: string[];
  targets?: SemanticTarget[];
  expansion?: { regionId: string; revision: number; route: SemanticRouteIdentity };
}
export interface SemanticChangeSet {
  fromRevision: number;
  toRevision: number;
  route: SemanticRouteIdentity;
  regions?: Array<{ id: string; kind: "added" | "removed" | "updated"; previousId?: string }>;
  targets?: Array<{ regionId: string; targetId: string; kind: "added" | "removed" | "updated"; previousTargetId?: string }>;
  continuity?: Array<{
    previousReference: string;
    currentReference: string;
    confidence: SemanticConfidence;
    evidence: string;
  }>;
}
export interface SemanticObservation {
  schemaVersion: 1;
  revision: number;
  level: SemanticObservationLevel;
  route: SemanticRouteIdentity;
  page: {
    kind: SemanticPageKind;
    title: string;
    url: string;
    targetId: string;
    frameId: string;
    confidence: SemanticConfidence;
    evidence?: string[];
  };
  regions: SemanticRegion[];
  text?: string;
  accessibility?: SemanticAccessibilityNode[];
  rawAccessibility?: SemanticAccessibilityNode[];
  changes?: SemanticChangeSet;
  limits: { truncated: boolean; omittedRegions: number; omittedTargets?: number; omittedBytes?: number };
}

export type SemanticIntentAction =
  | "click" | "type" | "clear" | "check" | "uncheck" | "select" | "submit"
  | "open" | "close" | "search" | "filter" | "sort" | "paginate" | "toggle"
  | "expand" | "collapse" | "download" | "upload" | "inspect" | "extract";
export type SemanticResolutionPolicy =
  | "reportOnly" | "requireExact" | "requireUniqueHighConfidence"
  | "allowUniqueMediumConfidence" | "interactiveConfirmation";
export interface SemanticIntentRequest {
  schemaVersion: 1;
  intent: string;
  action: SemanticIntentAction;
  scope?: { pageKind?: string; regionKind?: string; regionId?: string; formLabel?: string };
  constraints?: {
    role?: string;
    name?: string;
    nameContains?: string;
    mustBeVisible?: boolean;
    mustBeEnabled?: boolean;
    excludeText?: string[];
    maxCandidates?: number;
  };
  resolutionPolicy: SemanticResolutionPolicy;
  expectedRevision?: number;
}
export interface SemanticIntentExecutionRequest extends SemanticIntentRequest {
  candidateId: string;
  value?: string;
}
export interface SemanticIntentResult {
  schemaVersion: 1;
  intent: string;
  action: SemanticIntentAction;
  normalizedIntent: string;
  resolution: "exact" | "uniqueHighConfidence" | "uniqueLowConfidence" | "ambiguous" | "notFound" | "staleRevision" | "policyRejected" | "unsupportedIntent";
  policyDecision: "allowed" | "reportOnly" | "confirmationRequired" | "rejected";
  revision?: number;
  candidates?: SemanticIntentCandidate[];
  excludedCandidates?: Array<{ id: string; reason: SemanticEvidence }>;
  excludedCount: number;
  selectedCandidate?: string;
  suggestedConstraints?: Array<{ regionKind?: string; nameContains?: string; role?: string }>;
  reason?: string;
  route?: SemanticRouteIdentity;
}
export interface SemanticEvidence { category: string; detail: string; }
export interface SemanticIntentCandidate {
  id: string;
  reference: string;
  role: string;
  name: string;
  inputType?: string;
  regionId?: string;
  regionKind?: SemanticRegionKind;
  confidence: "exact" | "high" | "medium" | "low" | "insufficient";
  evidence?: SemanticEvidence[];
  fingerprint?: Record<string, unknown>;
}
export interface SemanticIntentExecutionResult {
  resolutionId: string;
  candidateId: string;
  status: "executed" | "not_executed";
  resolution: SemanticIntentResult;
  action?: Record<string, unknown>;
  executionId?: string;
  reason?: string;
}

export interface WorkflowInput {
  valueType: WorkflowValueType;
  required?: boolean;
  maxLength?: number;
  sensitive?: boolean;
}

export interface WorkflowBudgets {
  maxSteps: number;
  maxDurationMs: number;
  maxRetries: number;
  maxExtractedBytes: number;
}

export interface WorkflowStep {
  id: string;
  action: string;
  transaction?: WorkflowTransaction;
  when?: VerificationPredicate;
  expect?: VerificationPredicate;
  beforeRetry?: VerificationPredicate;
  idempotencyKey?: string;
  maxRetries?: number;
  repeat?: number;
  [field: string]: unknown;
}

export interface WorkflowOutputDeclaration {
  valueType: WorkflowValueType;
  source: WorkflowOutputSource;
  required?: boolean;
  sensitive?: boolean;
}

export interface WorkflowDefinition {
  schemaVersion: 1;
  name: string;
  workflowVersion: string;
  description?: string;
  inputs: Record<string, WorkflowInput>;
  budgets: WorkflowBudgets;
  preconditions?: VerificationPredicate[];
  steps: WorkflowStep[];
  terminalCondition: VerificationPredicate;
  outputs: Record<string, WorkflowOutputDeclaration>;
}
export type WorkflowInputs = Record<string, unknown>;
export interface WorkflowCheckpoint {
  schemaVersion: number;
  runId?: string;
  workflowName: string;
  workflowVersion: string;
  definitionHash: string;
  status: string;
  nextStepIndex: number;
  steps: WorkflowCheckpointStep[];
  [field: string]: unknown;
}

export interface WorkflowCheckpointStep {
  id: string;
  state: WorkflowStepState;
  attempts: number;
  history?: WorkflowStepState[];
  executionIds?: string[];
  dispatchAcknowledged?: boolean;
  effectObserved?: boolean;
  postconditionVerified?: boolean;
  retrySafe?: boolean;
  previousRevision?: number;
  currentRevision?: number;
  branchDecision?: Record<string, unknown>;
}

export interface WorkflowRunResult {
  runId: string;
  name: string;
  workflowVersion: string;
  status: "completed" | "failed" | "budget_exhausted" | "resume_required";
  steps: Array<Record<string, unknown>>;
  trace: Record<string, unknown>;
  outputs: Record<string, unknown>;
  [field: string]: unknown;
}

export interface FormField {
  target: string;
  value?: string;
}

export interface GlassClientOptions {
  command?: string;
  args?: string[];
  daemonSocket?: string;
  env?: Record<string, string>;
  cwd?: string;
  maxFrameBytes?: number;
}

export interface GlassCapabilityConstraints {
  platform: string;
  browserFamily: "chromium";
  policy: string;
  maxSessions: number;
}

export type GlassCapabilityStatus =
  | "available" | "availableUncertified" | "experimental" | "disabledByPolicy"
  | "unavailableOnPlatform" | "missingRuntimeDependency" | "blockedBySecurityGate";

export interface GlassCapabilityManifest {
  protocolVersion: 1;
  glassVersion: string;
  schemas: Record<string, number[]>;
  capabilities: Record<string, boolean>;
  capabilityStatuses?: Record<string, GlassCapabilityStatus>;
  constraints: GlassCapabilityConstraints;
}

/** Small MCP client with typed helpers for the stable Glass surface. */
export class GlassClient {
  private readonly input: Writable;
  private readonly output: Readable;
  private readonly closeTransport: () => void;
  private readonly maxFrameBytes: number;
  private readonly pending = new Map<number, { resolve: (value: any) => void; reject: (error: Error) => void }>();
  private buffer = Buffer.alloc(0);
  private nextId = 1;
  private initialized = false;
  private manifest?: GlassCapabilityManifest;
  private leaseToken?: string;

  constructor(options: GlassClientOptions = {}) {
    this.maxFrameBytes = options.maxFrameBytes ?? 4 * 1024 * 1024;
    if (options.daemonSocket) {
      const socket: Socket = createConnection(options.daemonSocket);
      this.input = socket;
      this.output = socket;
      this.closeTransport = () => socket.destroy();
      socket.on("error", (error) => this.rejectAll(error));
      socket.on("close", () => this.rejectAll(new Error("Glass daemon socket closed")));
    } else {
      const child = spawn(options.command ?? "glass", options.args ?? ["--mcp"], {
        cwd: options.cwd,
        env: options.env ? { ...process.env, ...options.env } : process.env,
        stdio: ["pipe", "pipe", "inherit"],
      });
      this.input = child.stdin;
      this.output = child.stdout;
      this.closeTransport = () => child.kill();
      child.on("error", (error) => this.rejectAll(error));
      child.on("exit", (code, signal) => this.rejectAll(new Error(`Glass exited (${code ?? signal})`)));
    }
    this.output.on("data", (chunk: Buffer) => this.consume(chunk));
  }

  async initialize(): Promise<GlassCapabilityManifest | undefined> {
    if (this.initialized) return undefined;
    const result = await this.request("initialize", {
      protocolVersion: "2024-11-05",
      glass: {
        protocolVersion: 1,
        schemas: { action: [1], observation: [1], workflow: [1], checkpoint: [1] },
      },
      capabilities: {},
      clientInfo: { name: "glass-typescript-client", version: "0.2.1" },
    });
    const manifest = (result as { glass?: GlassCapabilityManifest }).glass;
    if (!manifest) throw new Error("Glass capability manifest missing from initialize response");
    this.manifest = manifest;
    this.send({ jsonrpc: "2.0", method: "notifications/initialized", params: {} });
    this.initialized = true;
    return manifest;
  }

  get capabilities(): GlassCapabilityManifest | undefined { return this.manifest; }

  supportsCapability(capability: string): boolean {
    return this.manifest?.capabilities[capability] === true;
  }
  capabilityStatus(capability: string): GlassCapabilityStatus | undefined {
    return this.manifest?.capabilityStatuses?.[capability];
  }
  supportsSchema(schema: string, version: number): boolean {
    return this.manifest?.schemas[schema]?.includes(version) === true;
  }
  requireCapability(capability: string): void {
    if (!this.supportsCapability(capability)) throw new Error(`Glass capability is unavailable: ${capability}`);
  }

  async listTools(): Promise<Array<Record<string, unknown>>> {
    await this.initialize();
    const result = await this.request("tools/list", {}) as { tools?: unknown };
    if (!Array.isArray(result.tools) || !result.tools.every((tool) => typeof tool === "object" && tool !== null)) {
      throw new Error("MCP tools/list returned an invalid tool inventory");
    }
    return result.tools as Array<Record<string, unknown>>;
  }

  async call<T = ToolCallResult>(name: string, args: Record<string, unknown> = {}): Promise<T> {
    await this.initialize();
    const callArgs = { ...args, ...(this.leaseToken === undefined ? {} : { leaseToken: this.leaseToken }) };
    const response = await this.request("tools/call", { name, arguments: callArgs }) as { content?: Array<{ text?: string }> };
    const text = response.content?.find((part) => typeof part.text === "string")?.text;
    if (text) {
      try { return JSON.parse(text) as T; } catch { /* plain text is still a valid MCP result */ }
    }
    return response as T;
  }

  async acquireMutationLease(ttlMs = 60_000): Promise<Record<string, unknown>> {
    await this.initialize();
    const lease = await this.request("glass/lease/acquire", { ttlMs }) as Record<string, unknown>;
    if (typeof lease.token !== "string") throw new Error("daemon lease response did not include a token");
    this.leaseToken = lease.token;
    return lease;
  }

  async renewMutationLease(ttlMs = 60_000): Promise<Record<string, unknown>> {
    await this.initialize();
    if (this.leaseToken === undefined) throw new Error("no daemon mutation lease is held");
    return await this.request("glass/lease/renew", { token: this.leaseToken, ttlMs }) as Record<string, unknown>;
  }

  async releaseMutationLease(): Promise<void> {
    await this.initialize();
    if (this.leaseToken === undefined) return;
    await this.request("glass/lease/release", { token: this.leaseToken });
    this.leaseToken = undefined;
  }

  observe<T = Record<string, unknown>>(
    level?: SemanticObservationLevel,
    region?: string,
    options: { includeDom?: boolean; includeScreenshot?: boolean; includeFormValues?: boolean } = {},
  ): Promise<T> {
    return this.call<T>("observe", {
      ...(level === undefined ? {} : { level }),
      ...(region === undefined ? {} : { region }),
      ...options,
    });
  }
  observeSemantic<T = SemanticObservation>(level: SemanticObservationLevel, region?: string): Promise<T> {
    return this.observe<T>(level, region);
  }
  resolveIntent<T = SemanticIntentResult>(request: SemanticIntentRequest): Promise<T> {
    return this.call<T>("resolveIntent", request as unknown as Record<string, unknown>);
  }
  executeIntent<T = SemanticIntentExecutionResult>(request: SemanticIntentExecutionRequest): Promise<T> {
    return this.call<T>("executeIntent", request as unknown as Record<string, unknown>);
  }
  navigate<T = Record<string, unknown>>(url: string, timeoutMs?: number, expectedRevision?: number): Promise<T> {
    return this.call<T>("navigate", { url, ...(timeoutMs === undefined ? {} : { timeoutMs }), ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  click<T = Record<string, unknown>>(target: string, expectedRevision?: number): Promise<T> {
    return this.call<T>("click", { target, ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  clickExpectPopup<T = Record<string, unknown>>(target: string, expectedRevision?: number): Promise<T> {
    return this.call<T>("clickExpectPopup", { target, ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  doubleClick<T = Record<string, unknown>>(target: string, expectedRevision?: number): Promise<T> {
    return this.call<T>("doubleClick", { target, ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  typeText<T = Record<string, unknown>>(text: string, target?: string, expectedRevision?: number): Promise<T> {
    return this.call<T>("type", { text, ...(target === undefined ? {} : { target }), ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  clear<T = Record<string, unknown>>(target: string, expectedRevision?: number): Promise<T> {
    return this.call<T>("clear", { target, ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  check<T = Record<string, unknown>>(target: string, expectedRevision?: number): Promise<T> {
    return this.call<T>("check", { target, ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  uncheck<T = Record<string, unknown>>(target: string, expectedRevision?: number): Promise<T> {
    return this.call<T>("uncheck", { target, ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  select<T = Record<string, unknown>>(target: string, value: string, expectedRevision?: number): Promise<T> {
    return this.call<T>("select", { target, value, ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  scroll<T = Record<string, unknown>>(dx = 0, dy = 600, expectedRevision?: number): Promise<T> {
    return this.call<T>("scroll", { dx, dy, ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  key<T = Record<string, unknown>>(key: string, expectedRevision?: number): Promise<T> {
    return this.call<T>("key", { key, ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  fillForm<T = Record<string, unknown>>(fields: FormField[], expectedRevision?: number): Promise<T> {
    return this.call<T>("fillForm", { fields, ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  verify<T = Record<string, unknown>>(predicate: VerificationPredicate, timeoutMs?: number): Promise<T> {
    return this.call<T>("verify", { predicate, ...(timeoutMs === undefined ? {} : { timeoutMs }) });
  }
  batch<T = Record<string, unknown>>(steps: unknown[], mode: BatchMode = "unguarded", expectedRevision?: number, atomic = false): Promise<T> {
    return this.call<T>("batch", { steps, mode, atomic, ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  workflow<T = WorkflowRunResult>(definition: WorkflowDefinition, inputs: WorkflowInputs = {}, checkpoint?: WorkflowCheckpoint): Promise<T> {
    return this.call<T>("workflow", { workflow: definition, inputs, ...(checkpoint === undefined ? {} : { checkpoint }) });
  }
  wait<T = Record<string, unknown>>(condition: string, timeoutMs?: number): Promise<T> {
    return this.call<T>("wait", { condition, ...(timeoutMs === undefined ? {} : { timeoutMs }) });
  }
  hover<T = Record<string, unknown>>(target: string, expectedRevision?: number): Promise<T> {
    return this.call<T>("hover", { target, ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  drag<T = Record<string, unknown>>(source: string, destination: string, expectedRevision?: number): Promise<T> {
    return this.call<T>("drag", { source, destination, ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  keyDown<T = Record<string, unknown>>(key: string, expectedRevision?: number): Promise<T> {
    return this.call<T>("keyDown", { key, ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  keyUp<T = Record<string, unknown>>(key: string, expectedRevision?: number): Promise<T> {
    return this.call<T>("keyUp", { key, ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  shortcut<T = Record<string, unknown>>(shortcut: string, expectedRevision?: number): Promise<T> {
    return this.call<T>("shortcut", { shortcut, ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  upload<T = Record<string, unknown>>(target: string, files: string[], expectedRevision?: number): Promise<T> {
    return this.call<T>("upload", { target, files, ...(expectedRevision === undefined ? {} : { expectedRevision }) });
  }
  screenshot<T = Record<string, unknown>>(args: Record<string, unknown> = {}): Promise<T> {
    return this.call<T>("screenshot", args);
  }
  observeKnowledge<T = Record<string, unknown>>(args: Record<string, unknown> = {}): Promise<T> {
    return this.call<T>("observeKnowledge", args);
  }
  resolveIntentWithKnowledge<T = SemanticIntentResult>(args: Record<string, unknown>): Promise<T> {
    return this.call<T>("resolveIntentWithKnowledge", args);
  }
  knowledgeList<T = Record<string, unknown>>(): Promise<T> {
    return this.call<T>("knowledgeList");
  }
  knowledgeShow<T = Record<string, unknown>>(recordId: string): Promise<T> {
    return this.call<T>("knowledgeShow", { recordId });
  }
  knowledgeStats<T = Record<string, unknown>>(): Promise<T> {
    return this.call<T>("knowledgeStats");
  }
  knowledgeInvalidate<T = Record<string, unknown>>(recordId: string, state: string, reason?: string, observedAt?: string): Promise<T> {
    return this.call<T>("knowledgeInvalidate", {
      recordId,
      state,
      ...(reason === undefined ? {} : { reason }),
      ...(observedAt === undefined ? {} : { observedAt }),
    });
  }
  knowledgePurge<T = Record<string, unknown>>(origin: string): Promise<T> {
    return this.call<T>("knowledgePurge", { origin });
  }
  preflight<T = Record<string, unknown>>(target: string, action = "click"): Promise<T> {
    return this.call<T>("preflight", { target, action });
  }
  clickAt<T = Record<string, unknown>>(x: number, y: number): Promise<T> {
    return this.call<T>("clickAt", { x, y });
  }
  dom<T = Record<string, unknown>>(): Promise<T> {
    return this.call<T>("getDOM");
  }
  text<T = string>(): Promise<T> {
    return this.call<T>("getText");
  }
  reconcileReferences<T = Record<string, unknown>>(fromRevision: number, refs: string[], hints: string[] = [], scopeRef?: string): Promise<T> {
    return this.call<T>("reconcileReferences", {
      fromRevision,
      refs,
      hints,
      ...(scopeRef === undefined ? {} : { scopeRef }),
    });
  }
  observeDelta<T = Record<string, unknown>>(): Promise<T> {
    return this.call<T>("observeDelta");
  }
  setNetworkConditions<T = Record<string, unknown>>(args: Record<string, unknown>): Promise<T> {
    return this.call<T>("setNetworkConditions", args);
  }
  clearNetworkConditions<T = Record<string, unknown>>(): Promise<T> {
    return this.call<T>("clearNetworkConditions");
  }
  setCpuThrottling<T = Record<string, unknown>>(rate: number): Promise<T> {
    return this.call<T>("setCpuThrottling", { rate });
  }
  clearCpuThrottling<T = Record<string, unknown>>(): Promise<T> {
    return this.call<T>("clearCpuThrottling");
  }
  setUserAgent<T = Record<string, unknown>>(userAgent: string, acceptLanguage?: string, platform?: string): Promise<T> {
    return this.call<T>("setUserAgent", {
      userAgent,
      ...(acceptLanguage === undefined ? {} : { acceptLanguage }),
      ...(platform === undefined ? {} : { platform }),
    });
  }
  clearUserAgent<T = Record<string, unknown>>(): Promise<T> {
    return this.call<T>("clearUserAgent");
  }
  exportCheckpoint<T = Record<string, unknown>>(): Promise<T> {
    return this.call<T>("exportCheckpoint");
  }
  importCheckpoint<T = Record<string, unknown>>(checkpoint: Record<string, unknown>): Promise<T> {
    return this.call<T>("importCheckpoint", checkpoint);
  }
  evaluate<T = unknown>(expression: string): Promise<T> {
    return this.call<T>("evaluate", { expression });
  }
  diagnostics<T = Record<string, unknown>>(durationMs = 1_000): Promise<T> {
    return this.call<T>("diagnostics", { durationMs });
  }
  acceptDialog<T = Record<string, unknown>>(): Promise<T> {
    return this.call<T>("acceptDialog");
  }
  dismissDialog<T = Record<string, unknown>>(): Promise<T> {
    return this.call<T>("dismissDialog");
  }
  dismissConsent<T = Record<string, unknown>>(): Promise<T> {
    return this.call<T>("dismissConsent");
  }
  download<T = Record<string, unknown>>(destination: string, timeoutMs = 30_000): Promise<T> {
    return this.call<T>("download", { destination, timeoutMs });
  }
  listTargets<T = Record<string, unknown>>(): Promise<T> {
    return this.call<T>("listTargets");
  }
  createTarget<T = Record<string, unknown>>(url: string): Promise<T> {
    return this.call<T>("createTarget", { url });
  }
  selectTarget<T = Record<string, unknown>>(id: string): Promise<T> {
    return this.call<T>("selectTarget", { id });
  }
  closeTarget<T = Record<string, unknown>>(id: string): Promise<T> {
    return this.call<T>("closeTarget", { id });
  }
  listFrames<T = Record<string, unknown>>(): Promise<T> {
    return this.call<T>("listFrames");
  }
  selectFrame<T = Record<string, unknown>>(id: string): Promise<T> {
    return this.call<T>("selectFrame", { id });
  }
  cookies<T = Record<string, unknown>>(): Promise<T> {
    return this.call<T>("cookies");
  }
  setCookies<T = Record<string, unknown>>(cookies: unknown): Promise<T> {
    return this.call<T>("setCookies", { cookies });
  }
  clearCookies<T = Record<string, unknown>>(): Promise<T> {
    return this.call<T>("clearCookies");
  }
  localStorage<T = Record<string, unknown>>(): Promise<T> {
    return this.call<T>("localStorage");
  }
  sessionStorage<T = Record<string, unknown>>(): Promise<T> {
    return this.call<T>("sessionStorage");
  }
  printToPdf<T = string>(options: Record<string, unknown> = {}): Promise<T> {
    return this.call<T>("printToPdf", options);
  }
  clipboardRead<T = string>(): Promise<T> {
    return this.call<T>("clipboardRead");
  }
  clipboardWrite<T = Record<string, unknown>>(text: string): Promise<T> {
    return this.call<T>("clipboardWrite", { text });
  }
  setGeolocation<T = Record<string, unknown>>(latitude: number, longitude: number): Promise<T> {
    return this.call<T>("setGeolocation", { latitude, longitude });
  }
  clearGeolocation<T = Record<string, unknown>>(): Promise<T> {
    return this.call<T>("clearGeolocation");
  }
  setTimezone<T = Record<string, unknown>>(timezoneId: string): Promise<T> {
    return this.call<T>("setTimezone", { timezoneId });
  }
  close(): void {
    for (const pending of this.pending.values()) pending.reject(new Error("Glass client closed"));
    this.pending.clear();
    this.input.end();
    this.closeTransport();
  }

  private request(method: string, params: Record<string, unknown>): Promise<unknown> {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.send({ jsonrpc: "2.0", id, method, params });
    });
  }

  private send(message: Record<string, unknown>): void {
    this.input.write(`${JSON.stringify(message)}\n`);
  }

  private consume(chunk: Buffer): void {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    if (this.buffer.length > this.maxFrameBytes) { this.rejectAll(new Error("MCP frame exceeds client limit")); return; }
    while (this.buffer.length) {
      let body: Buffer | undefined;
      if (this.buffer.subarray(0, 15).toString("ascii").toLowerCase().startsWith("content-length:")) {
        const separator = this.buffer.indexOf("\r\n\r\n");
        if (separator < 0) return;
        const header = this.buffer.subarray(0, separator).toString("ascii");
        const match = header.match(/content-length:\s*(\d+)/i);
        if (!match) { this.rejectAll(new Error("invalid MCP Content-Length header")); return; }
        const length = Number(match[1]);
        if (this.buffer.length < separator + 4 + length) return;
        body = this.buffer.subarray(separator + 4, separator + 4 + length);
        this.buffer = this.buffer.subarray(separator + 4 + length);
      } else {
        const newline = this.buffer.indexOf(10);
        if (newline < 0) return;
        body = this.buffer.subarray(0, newline);
        this.buffer = this.buffer.subarray(newline + 1);
      }
      try {
        const message = JSON.parse(body.toString("utf8"));
        if (typeof message.id !== "number") continue;
        const pending = this.pending.get(message.id);
        if (!pending) continue;
        this.pending.delete(message.id);
        if (message.error) pending.reject(new Error(`${message.error.code}: ${message.error.message}`));
        else pending.resolve(message.result);
      } catch (error) { this.rejectAll(error instanceof Error ? error : new Error(String(error))); }
    }
  }

  private rejectAll(error: Error): void {
    for (const pending of this.pending.values()) pending.reject(error);
    this.pending.clear();
  }
}
