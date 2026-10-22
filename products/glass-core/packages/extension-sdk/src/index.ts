/** Protocol 1 SDK. GC remains responsible for policy, rendering and process isolation. */
import { randomUUID } from 'node:crypto';
import { once } from 'node:events';
import type { Readable, Writable } from 'node:stream';

export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export interface ToolDefinition {
  name: string;
  description: string;
  schema: Record<string, Json>;
  concurrency?: 'parallel' | 'keyed' | 'serial';
}
export interface Lineage { origin: string; depth: number; path: string[] }
export interface Diagnostic {
  severity: 'error' | 'warning' | 'info';
  message: string;
  file?: string;
  line?: number;
  column?: number;
  code?: string;
}
export interface Preview { title?: string; language?: string; before?: string; after: string }
export interface ToolResult { text?: string; preview?: Preview; diagnostics?: Diagnostic[] }
export interface Context {
  workspace: string;
  model: string;
  callId: string;
  lineage?: Lineage;
  /** Aborted on transport close, shutdown or SIGTERM. GC currently cancels by stopping the process. */
  signal: AbortSignal;
  callTool(name: 'bash' | 'state', arguments_: Record<string, Json>): Promise<string>;
  appendCustom(kind: string, data: string): Promise<void>;
  readCustom(kind: string): Promise<string[]>;
  log(message: string): void;
}
export type Handler = (arguments_: Record<string, Json>, context: Context) => string | ToolResult | Promise<string | ToolResult>;
export interface Tool {
  definition: ToolDefinition;
  execute?: Handler;
  /** Imported only on the first call; retained for subsequent calls. */
  load?: () => Promise<{ execute: Handler }>;
}
export interface Command { name: string; description: string; execute(args: string, context: Context): string | ToolResult | Promise<string | ToolResult> }
export interface Extension {
  name: string;
  version: string;
  tools?: Tool[];
  commands?: Command[];
  systemPrompt?: string;
  authorize?: (name: string, arguments_: string, context: Context) => { allow: boolean; reason?: string } | Promise<{ allow: boolean; reason?: string }>;
  toolResult?: (name: string, body: string, context: Context) => string | Promise<string>;
  turnEnd?: (text: string, cacheablePrefixBytes: number, context: Context) => void | Promise<void>;
  seedCustom?: (entries: { kind: string; entries: string[] }[], context: Context) => void | Promise<void>;
  dispose?: () => void | Promise<void>;
}
export class ToolError extends Error {
  constructor(public readonly code: string, message: string) { super(message); this.name = 'ToolError'; }
}
/** Validate arguments explicitly: TypeScript types alone do not validate model input. */
export function stringArgument(args: Record<string, Json>, name: string): string {
  const value = args[name];
  if (typeof value !== 'string') throw new ToolError('INVALID_ARGUMENT', `${name} must be a string`);
  return value;
}
function cell(value: string): string { return value.replace(/[\r\n]+/g, ' ').replace(/\|/g, '\\|').replace(/`/g, '\\`'); }
function code(body: string, language = ''): string {
  let longest = 2;
  for (const match of body.matchAll(/`+/g)) longest = Math.max(longest, match[0].length);
  const fence = '`'.repeat(longest + 1);
  return `${fence}${language.replace(/[^a-zA-Z0-9_+-]/g, '')}\n${body}\n${fence}`;
}
/** Native Markdown previews: no terminal escape sequences or extension UI runtime. */
export function renderResult(result: string | ToolResult): string {
  if (typeof result === 'string') return result;
  const parts = result.text ? [result.text] : [];
  if (result.preview) {
    const p = result.preview;
    parts.push(`### ${cell(p.title ?? 'Preview')}`);
    if (p.before !== undefined) parts.push('**Before**', code(p.before, p.language));
    parts.push('**After**', code(p.after, p.language));
  }
  if (result.diagnostics?.length) {
    const rows = ['| Level | Location | Message |', '| --- | --- | --- |'];
    for (const d of result.diagnostics) {
      const location = d.file ? `${d.file}${d.line === undefined ? '' : `:${d.line}${d.column === undefined ? '' : `:${d.column}`}`}` : '—';
      rows.push(`| ${d.severity} | ${cell(location)} | ${cell(`${d.code ? `${d.code}: ` : ''}${d.message}`)} |`);
    }
    parts.push('### Diagnostics', rows.join('\n'));
  }
  return parts.join('\n\n');
}
const MAX_BYTES = 4 * 1024 * 1024;
type Wire = Record<string, unknown>;
type Pending = { resolve: (message: Wire) => void; reject: (error: Error) => void; timer: NodeJS.Timeout };
export interface TransportOptions { input?: Readable; output?: Writable; errors?: Writable; timeoutMs?: number }

/** Serve one persistent process. There is no polling loop, worker pool or token hook. */
export async function runExtension(extension: Extension, options: TransportOptions = {}): Promise<void> {
  const input = options.input ?? process.stdin;
  const output = options.output ?? process.stdout;
  const errors = options.errors ?? process.stderr;
  const timeoutMs = options.timeoutMs ?? 30_000;
  const tools = new Map<string, Tool>();
  const commands = new Map<string, Command>();
  const handlers = new Map<string, Promise<Handler>>();
  for (const tool of extension.tools ?? []) {
    if (tools.has(tool.definition.name) || (!tool.execute && !tool.load)) throw new Error(`Invalid or duplicate tool: ${tool.definition.name}`);
    tools.set(tool.definition.name, tool);
  }
  for (const command of extension.commands ?? []) {
    if (commands.has(command.name)) throw new Error(`Duplicate command: ${command.name}`);
    commands.set(command.name, command);
  }
  const pending = new Map<string, Pending>();
  const abort = new AbortController();
  let workspace = '', model = '', ready = false, closed = false, queued = 0;
  let chain = Promise.resolve();
  const stop = (reason: Error) => {
    if (closed) return;
    closed = true;
    abort.abort(reason);
    for (const item of pending.values()) { clearTimeout(item.timer); item.reject(reason); }
    pending.clear();
  };
  const outputFailed = (error: Error) => { stop(error); input.destroy(error); };
  const outputClosed = () => outputFailed(new ToolError('TRANSPORT_CLOSED', 'Extension output closed before shutdown'));
  output.once('error', outputFailed);
  output.once('close', outputClosed);
  const terminate = () => { stop(new Error('Extension terminated')); input.destroy(); };
  process.once('SIGTERM', terminate);
  const send = async (message: Wire) => {
    if (closed) throw new Error('Extension transport is closed');
    const line = JSON.stringify(message) + '\n';
    if (Buffer.byteLength(line) > MAX_BYTES) throw new ToolError('OUTPUT_LIMIT', 'Reply exceeds 4 MiB; return a bounded preview');
    if (!output.write(line)) await once(output, 'drain', { signal: abort.signal });
  };
  const nested = (type: string, fields: Wire): Promise<Wire> => {
    if (closed || pending.size >= 16) return Promise.reject(new ToolError('HOST_CALL_LIMIT', 'Host call unavailable or too many pending calls'));
    const call_id = randomUUID();
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { pending.delete(call_id); reject(new ToolError('HOST_TIMEOUT', `${type} did not reply`)); }, timeoutMs);
      pending.set(call_id, { resolve, reject, timer });
      void send({ type, call_id, ...fields }).catch(error => { clearTimeout(timer); pending.delete(call_id); reject(error); });
    });
  };
  const stateKind = (kind: string): string => {
    if (!kind.trim()) throw new ToolError('STATE_ERROR', 'State kind must not be empty');
    return kind.startsWith(`${extension.name}/`) ? kind : `${extension.name}/${kind}`;
  };
  const context = (request: Wire): Context => ({
    workspace, model, callId: typeof request.call_id === 'string' ? request.call_id : '',
    ...(request.lineage ? { lineage: request.lineage as Lineage } : {}), signal: abort.signal,
    async callTool(name, args) {
      const reply = await nested('call_tool', { name, arguments: JSON.stringify(args) });
      if (typeof reply.body !== 'string') throw new ToolError('HOST_PROTOCOL', 'Host tool reply has no body');
      return reply.body;
    },
    async appendCustom(kind, data) {
      const reply = await nested('append_custom', { kind: stateKind(kind), data });
      if (!reply.ok) throw new ToolError('STATE_ERROR', String(reply.error ?? 'Append rejected'));
    },
    async readCustom(kind) {
      const reply = await nested('read_custom', { kind: stateKind(kind) });
      if (!reply.ok || !Array.isArray(reply.entries) || !reply.entries.every(value => typeof value === 'string')) throw new ToolError('STATE_ERROR', String(reply.error ?? 'Invalid state reply'));
      return reply.entries as string[];
    },
    log(message) { errors.write(`${extension.name}: ${message}\n`); },
  });
  const handle = async (request: Wire) => {
    const id = request.id;
    try {
      if (request.type === 'hello') {
        if (request.protocol !== 1 || ready) throw new ToolError('PROTOCOL', 'Expected one protocol 1 handshake');
        workspace = String(request.workspace ?? ''); model = String(request.model ?? ''); ready = true;
        await send({ id, protocol: 1, name: extension.name, version: extension.version,
          tools: [...tools.values()].map(tool => tool.definition),
          commands: [...commands.values()].map(({ name, description }) => ({ name, description })),
          system_prompt: extension.systemPrompt ?? '', authorization_required: !!extension.authorize });
        return;
      }
      if (!ready) throw new ToolError('PROTOCOL', 'Handshake required');
      const ctx = context(request);
      switch (request.type) {
        case 'system_prompt': await send({ id, text: extension.systemPrompt ?? '' }); break;
        case 'authorize': await send({ id, ...(extension.authorize ? await extension.authorize(String(request.name), String(request.arguments ?? '{}'), ctx) : { allow: true }) }); break;
        case 'tool_result': await send({ id, body: extension.toolResult ? await extension.toolResult(String(request.name), String(request.body ?? ''), ctx) : String(request.body ?? '') }); break;
        case 'turn_end': await extension.turnEnd?.(String(request.text ?? ''), Number(request.cacheable_prefix_bytes ?? 0), ctx); await send({ id }); break;
        case 'seed_custom': await extension.seedCustom?.(request.entries as { kind: string; entries: string[] }[], ctx); await send({ id }); break;
        case 'command': {
          const command = commands.get(String(request.name));
          if (!command) throw new ToolError('UNKNOWN_COMMAND', String(request.name));
          await send({ id, output: renderResult(await command.execute(String(request.args ?? ''), ctx)) }); break;
        }
        case 'run_tool': {
          const name = String(request.name), tool = tools.get(name);
          if (!tool) throw new ToolError('UNKNOWN_TOOL', name);
          let args: unknown;
          try { args = JSON.parse(String(request.arguments ?? '{}')); } catch { throw new ToolError('INVALID_ARGUMENT', 'Arguments must be valid JSON'); }
          if (!args || typeof args !== 'object' || Array.isArray(args)) throw new ToolError('INVALID_ARGUMENT', 'Arguments must be an object');
          let handler = handlers.get(name);
          if (!handler) {
            handler = tool.execute ? Promise.resolve(tool.execute) : tool.load!().then(module => module.execute);
            handlers.set(name, handler);
            void handler.catch(() => handlers.delete(name));
          }
          await send({ id, output: renderResult(await (await handler)(args as Record<string, Json>, ctx)) }); break;
        }
        default: throw new ToolError('PROTOCOL', `Unknown request: ${String(request.type)}`);
      }
    } catch (error) {
      if (closed) return;
      const code = error instanceof ToolError ? error.code : 'EXTENSION_ERROR';
      const message = error instanceof Error ? error.message : String(error);
      ctxLog(`${code}: ${message}`);
      const failure = { id, error: { code, message }, output: `error: [${code}] ${message}` };
      if (request.type === 'authorize') await send({ ...failure, allow: false, reason: message });
      else await send(failure);
    }
  };
  const ctxLog = (message: string) => { errors.write(`${extension.name}: ${message}\n`); };
  let buffers: Buffer[] = [], bytes = 0;
  try {
    for await (const raw of input) {
      const chunk = Buffer.isBuffer(raw) ? raw : Buffer.from(raw);
      let start = 0;
      while (start < chunk.length) {
        const newline = chunk.indexOf(10, start);
        const end = newline < 0 ? chunk.length : newline;
        const segment = chunk.subarray(start, end);
        bytes += segment.length;
        if (bytes > MAX_BYTES) throw new ToolError('INPUT_LIMIT', 'Host frame exceeds 4 MiB');
        buffers.push(segment);
        start = end + 1;
        if (newline < 0) break;
        const line = Buffer.concat(buffers, bytes).toString('utf8'); buffers = []; bytes = 0;
        if (!line.trim()) continue;
        const request: unknown = JSON.parse(line);
        if (!request || typeof request !== 'object' || Array.isArray(request)) throw new ToolError('PROTOCOL', 'Host frame must be an object');
        const message = request as Wire;
        if (typeof message.call_id === 'string' && (message.type === 'call_tool_result' || message.type === 'custom_result')) {
          const item = pending.get(message.call_id);
          if (!item) continue;
          pending.delete(message.call_id); clearTimeout(item.timer); item.resolve(message); continue;
        }
        if (message.type === 'goodbye') { stop(new Error('Host shutdown')); input.destroy(); break; }
        if (queued >= 8) throw new ToolError('INPUT_LIMIT', 'Too many queued host requests');
        queued++;
        chain = chain.then(() => handle(message)).finally(() => { queued--; });
        void chain.catch(error => { stop(error instanceof Error ? error : new Error(String(error))); input.destroy(); });
      }
      if (closed) break;
    }
  } finally {
    stop(new Error('Host input closed'));
    try {
      await chain;
    } finally {
      process.removeListener('SIGTERM', terminate);
      output.removeListener('error', outputFailed);
      output.removeListener('close', outputClosed);
      await extension.dispose?.();
    }
  }
}
