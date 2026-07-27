import { spawn, type ChildProcessByStdio } from "node:child_process";
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
  [field: string]: unknown;
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
  env?: Record<string, string>;
  cwd?: string;
  maxFrameBytes?: number;
}

/** Small MCP client with typed helpers for the stable Glass surface. */
export class GlassClient {
  private readonly child: ChildProcessByStdio<Writable, Readable, null>;
  private readonly maxFrameBytes: number;
  private readonly pending = new Map<number, { resolve: (value: any) => void; reject: (error: Error) => void }>();
  private buffer = Buffer.alloc(0);
  private nextId = 1;
  private initialized = false;

  constructor(options: GlassClientOptions = {}) {
    this.maxFrameBytes = options.maxFrameBytes ?? 4 * 1024 * 1024;
    this.child = spawn(options.command ?? "glass", options.args ?? ["--mcp"], {
      cwd: options.cwd,
      env: options.env ? { ...process.env, ...options.env } : process.env,
      stdio: ["pipe", "pipe", "inherit"],
    });
    this.child.stdout.on("data", (chunk: Buffer) => this.consume(chunk));
    this.child.on("error", (error) => this.rejectAll(error));
    this.child.on("exit", (code, signal) => this.rejectAll(new Error(`Glass exited (${code ?? signal})`)));
  }

  async initialize(): Promise<unknown> {
    if (this.initialized) return undefined;
    const result = await this.request("initialize", {
      protocolVersion: "2024-11-05",
      capabilities: {},
      clientInfo: { name: "glass-typescript-client", version: "0.1.0" },
    });
    this.send({ jsonrpc: "2.0", method: "notifications/initialized", params: {} });
    this.initialized = true;
    return result;
  }

  async call<T = ToolCallResult>(name: string, args: Record<string, unknown> = {}): Promise<T> {
    await this.initialize();
    const response = await this.request("tools/call", { name, arguments: args }) as { content?: Array<{ text?: string }> };
    const text = response.content?.find((part) => typeof part.text === "string")?.text;
    if (text) {
      try { return JSON.parse(text) as T; } catch { /* plain text is still a valid MCP result */ }
    }
    return response as T;
  }

  observe<T = Record<string, unknown>>(): Promise<T> { return this.call<T>("observe"); }
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

  close(): void {
    for (const pending of this.pending.values()) pending.reject(new Error("Glass client closed"));
    this.pending.clear();
    this.child.stdin.end();
    this.child.kill();
  }

  private request(method: string, params: Record<string, unknown>): Promise<unknown> {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.send({ jsonrpc: "2.0", id, method, params });
    });
  }

  private send(message: Record<string, unknown>): void {
    this.child.stdin.write(`${JSON.stringify(message)}\n`);
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
