#!/usr/bin/env node
// Glass-owned Pi AgentSession SDK runtime. Protocol: 4-byte BE length + JSON.

import { createRequire } from "node:module";
import { realpath } from "node:fs/promises";
import { dirname, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";

const sdkEntry = process.env.GLASS_PI_SDK_ENTRY;
const cwd = process.env.GLASS_PI_CWD;
const sessionDir = process.env.GLASS_PI_SESSION_DIR;
const agentDir = process.env.GLASS_PI_AGENT_DIR;
if (!sdkEntry || !cwd || !sessionDir || !agentDir) {
  throw new Error("Glass Pi runtime requires SDK, cwd, session, and agent paths");
}

const sdk = await import(pathToFileURL(sdkEntry).href);
const require = createRequire(sdkEntry);
const typeboxEntry = require.resolve("typebox");
const { Type } = await import(pathToFileURL(typeboxEntry).href);
const {
  createAgentSession,
  DefaultResourceLoader,
  SessionManager,
} = sdk;

let input = Buffer.alloc(0);
let nextToolRequest = 1;
const toolResponses = new Map();
let runtime;
let unsubscribe;

process.stdout.on("error", (error) => {
  if (error?.code === "EPIPE") process.exit(0);
  throw error;
});

function safe(value) {
  return JSON.parse(JSON.stringify(value, (_key, item) =>
    typeof item === "bigint" ? item.toString() : item));
}

function send(value) {
  const body = Buffer.from(JSON.stringify(safe(value)), "utf8");
  if (body.length > 16 * 1024 * 1024) {
    throw new Error("Glass Pi runtime frame exceeds 16 MiB");
  }
  const header = Buffer.allocUnsafe(4);
  header.writeUInt32BE(body.length);
  process.stdout.write(Buffer.concat([header, body]));
}

function callGlass(toolCallId, params, signal) {
  const id = `tool-${nextToolRequest++}`;
  return new Promise((resolvePromise, reject) => {
    const abort = () => {
      toolResponses.delete(id);
      reject(new Error("Glass tool call aborted"));
    };
    signal?.addEventListener("abort", abort, { once: true });
    toolResponses.set(id, {
      resolve(value) {
        signal?.removeEventListener("abort", abort);
        resolvePromise(value);
      },
      reject(error) {
        signal?.removeEventListener("abort", abort);
        reject(error);
      },
    });
    send({
      type: "toolCall",
      id,
      call: {
        id: toolCallId,
        name: params.name,
        arguments: params.arguments ?? {},
      },
    });
  });
}

function toolResult(result) {
  const payload = result?.ok === true && Object.hasOwn(result, "result")
    ? result.result
    : result;
  const text = JSON.stringify(payload);
  return {
    content: [{ type: "text", text }],
    details: payload,
    ...(result?.ok === false ? { isError: true } : {}),
  };
}

const glassTool = {
  name: "glass_tool",
  label: "Glass Tool",
  description: "Call one governed Glass capability by its exact glass.* name and JSON arguments. Use this for browser, Git, processes, tests, debugging, workflows, and other Glass services.",
  promptSnippet: "glass_tool({name, arguments}): governed Glass workspace/browser capability",
  promptGuidelines: [
    "Use the familiar Glass-backed coding tools for local source work; use glass_tool for browser, Git, process, test, debugger, and evidence operations.",
    "Example: glass_tool({name: \"glass.browser.observe\", arguments: {}}).",
    "Do not claim a mutation succeeded until the Glass tool result confirms it.",
  ],
  parameters: Type.Object({
    name: Type.String({ minLength: 1, maxLength: 256 }),
    arguments: Type.Optional(Type.Record(Type.String(), Type.Unknown())),
  }, { additionalProperties: false }),
  executionMode: "sequential",
  async execute(toolCallId, params, signal) {
    return toolResult(await callGlass(toolCallId, params, signal));
  },
};

function glassBackedTool(name, label, description, parameters, mapArguments) {
  return {
    name,
    label,
    description,
    promptSnippet: `${name}({ ... }): Glass-governed workspace operation`,
    parameters,
    executionMode: "sequential",
    async execute(toolCallId, params, signal) {
      return toolResult(await callGlass(toolCallId, {
        name: mapArguments.name,
        arguments: mapArguments.arguments(params, toolCallId),
      }, signal));
    },
  };
}

const nativeTools = [
  glassBackedTool(
    "delegate",
    "Temporary external agent",
    "Delegate one bounded prompt to Codex, Claude Code, or OpenCode through Glass. Read-only is the default and every delegation requires Glass approval.",
    Type.Object({
      harness: Type.String({ minLength: 1 }),
      prompt: Type.String({ minLength: 1, maxLength: 65536 }),
      sandbox: Type.Optional(Type.String()),
      timeoutSeconds: Type.Optional(Type.Integer({ minimum: 1, maximum: 3600 })),
    }, { additionalProperties: false }),
    { name: "glass.agent.delegate", arguments: (params) => params },
  ),
  glassBackedTool(
    "read",
    "Read",
    "Read a bounded UTF-8 project file through Glass. Paths stay inside the current workspace.",
    Type.Object({
      path: Type.String({ minLength: 1 }),
      offset: Type.Optional(Type.Integer({ minimum: 1 })),
      limit: Type.Optional(Type.Integer({ minimum: 1 })),
    }, { additionalProperties: false }),
    { name: "glass.file.read", arguments: (params) => params },
  ),
  glassBackedTool(
    "write",
    "Write",
    "Create or replace a bounded project file through Glass. Mutations require Glass approval.",
    Type.Object({
      path: Type.String({ minLength: 1 }),
      content: Type.String(),
    }, { additionalProperties: false }),
    { name: "glass.file.write", arguments: (params) => params },
  ),
  glassBackedTool(
    "edit",
    "Edit",
    "Apply exact non-overlapping oldText/newText replacements through Glass. Mutations require Glass approval.",
    Type.Object({
      path: Type.String({ minLength: 1 }),
      edits: Type.Array(Type.Object({
        oldText: Type.String(),
        newText: Type.String(),
      }, { additionalProperties: false })),
    }, { additionalProperties: false }),
    { name: "glass.file.edit", arguments: (params) => params },
  ),
  glassBackedTool(
    "bash",
    "Bash",
    "Run one bounded workspace-confined command through Glass. Mutations require Glass approval.",
    Type.Object({
      command: Type.String({ minLength: 1 }),
      timeout: Type.Optional(Type.Integer({ minimum: 1, maximum: 300 })),
    }, { additionalProperties: false }),
    {
      name: "glass.command.run",
      arguments: (params, toolCallId) => ({
        name: `pi-${toolCallId.replace(/[^A-Za-z0-9_-]/g, "-").slice(0, 48)}`,
        command: params.command,
        timeoutSeconds: params.timeout,
      }),
    },
  ),
  glassBackedTool(
    "grep",
    "Grep",
    "Search bounded UTF-8 project files through Glass.",
    Type.Object({
      pattern: Type.String({ minLength: 1 }),
      path: Type.Optional(Type.String()),
      glob: Type.Optional(Type.String()),
      ignoreCase: Type.Optional(Type.Boolean()),
      context: Type.Optional(Type.Integer({ minimum: 0, maximum: 20 })),
      limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 512 })),
    }, { additionalProperties: false }),
    { name: "glass.file.grep", arguments: (params) => params },
  ),
  glassBackedTool(
    "find",
    "Find",
    "Find bounded project paths through Glass using shell-style matching.",
    Type.Object({
      pattern: Type.String({ minLength: 1 }),
      path: Type.Optional(Type.String()),
      limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 512 })),
    }, { additionalProperties: false }),
    { name: "glass.file.find", arguments: (params) => params },
  ),
  glassBackedTool(
    "ls",
    "List files",
    "List bounded project files through Glass.",
    Type.Object({
      path: Type.Optional(Type.String()),
      limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 512 })),
    }, { additionalProperties: false }),
    { name: "glass.file.list", arguments: (params) => params },
  ),
];

async function create(manager) {
  const resourceLoader = new DefaultResourceLoader({
    cwd,
    agentDir,
    noExtensions: true,
    noSkills: true,
    noPromptTemplates: true,
    noThemes: true,
    noContextFiles: true,
    systemPrompt: process.env.GLASS_PI_SYSTEM_PROMPT || undefined,
  });
  await resourceLoader.reload();
  const result = await createAgentSession({
    cwd,
    sessionManager: manager,
    resourceLoader,
    noTools: "builtin",
    tools: ["glass_tool", "delegate", "read", "write", "edit", "bash", "grep", "find", "ls"],
    customTools: [glassTool, ...nativeTools],
    thinkingLevel: process.env.GLASS_PI_THINKING || undefined,
  });
  const configuredModel = process.env.GLASS_PI_MODEL;
  if (configuredModel) {
    const split = configuredModel.indexOf("/");
    if (split <= 0) throw new Error("Glass Pi model must be provider/model");
    const model = result.session.modelRuntime.getModel(
      configuredModel.slice(0, split),
      configuredModel.slice(split + 1),
    );
    if (!model) throw new Error(`unknown Pi model ${configuredModel}`);
    await result.session.setModel(model);
  }
  const name = process.env.GLASS_PI_SESSION_NAME;
  if (name && !result.session.sessionName) {
    result.session.sessionManager.appendSessionInfo(name);
  }
  return result;
}

function bind(result) {
  unsubscribe?.();
  runtime?.session?.dispose();
  runtime = result;
  unsubscribe = runtime.session.subscribe((event) => {
    send(event);
    if (event.type === "agent_end") send({ type: "agent_settled" });
  });
}

async function replace(manager) {
  const next = await create(manager);
  bind(next);
  return snapshot();
}

function snapshot() {
  const session = runtime.session;
  return {
    sessionId: session.sessionId,
    sessionFile: session.sessionFile,
    sessionName: session.sessionName,
    model: session.model ? {
      provider: session.model.provider,
      id: session.model.id,
    } : null,
    thinking: session.thinkingLevel,
    streaming: session.isStreaming,
    idle: session.isIdle,
    pendingMessages: session.pendingMessageCount,
  };
}

async function confinedSessionPath(path) {
  const canonicalDir = await realpath(sessionDir);
  const canonical = await realpath(resolve(path));
  if (canonical !== canonicalDir && !canonical.startsWith(canonicalDir + sep)) {
    throw new Error("session path is outside the Glass Pi session directory");
  }
  return canonical;
}

function lastAssistantText(target) {
  const messages = target.messages || [];
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (message?.role !== "assistant") continue;
    const parts = Array.isArray(message.content) ? message.content : [];
    const text = parts
      .filter((part) => part?.type === "text" && typeof part.text === "string")
      .map((part) => part.text)
      .join("");
    if (text.trim()) return text;
  }
  return "";
}

async function completeFill(parent, params = {}) {
  const prefix = String(params.prefix || "");
  const suffix = String(params.suffix || "");
  const prompt = `Fill in the middle of this source. Reply with ONLY the inserted text. No markdown fences, no explanation, and do not repeat PREFIX or SUFFIX.

PREFIX:
${prefix}

SUFFIX:
${suffix}`;
  const loader = new DefaultResourceLoader({
    cwd,
    agentDir,
    noExtensions: true,
    noSkills: true,
    noPromptTemplates: true,
    noThemes: true,
    noContextFiles: true,
    systemPrompt: "Return only the inserted source text.",
  });
  await loader.reload();
  let sessionManager;
  try {
    sessionManager = SessionManager.inMemory(cwd);
  } catch {
    sessionManager = SessionManager.inMemory();
  }
  const ghost = await createAgentSession({
    cwd,
    sessionManager,
    resourceLoader: loader,
    noTools: "builtin",
    tools: [],
    customTools: [],
    thinkingLevel: "off",
    model: parent.model,
    modelRuntime: parent.modelRuntime,
  });
  try {
    await ghost.session.prompt(prompt);
    return { text: lastAssistantText(ghost.session).trim() };
  } finally {
    ghost.session.dispose();
  }
}

async function attachContext(context, deliverAs = "nextTurn") {
  if (!context || typeof context !== "object") return;
  const text = JSON.stringify(safe(context));
  if (Buffer.byteLength(text, "utf8") > 64 * 1024) {
    throw new Error("Glass context attachment exceeds 64 KiB");
  }
  await runtime.session.sendCustomMessage({
    customType: "glass.context",
    content: [{ type: "text", text }],
    display: false,
    details: safe(context),
  }, { triggerTurn: false, deliverAs });
}

async function operation(name, params = {}) {
  const session = runtime.session;
  switch (name) {
    case "hello":
      return {
        protocol: "glass-pi-sdk-v1",
        sdk: "AgentSession",
        capabilities: [
          "prompt", "steer", "followUp", "complete", "abort", "compact", "models",
          "thinking", "newSession", "cloneSession", "rewind", "fork",
          "switchSession", "messages", "entries", "tree", "stats", "name",
          "glassTool",
        ],
      };
    case "state": return snapshot();
    case "prompt":
      await attachContext(params.context);
      await session.prompt(params.text);
      return snapshot();
    case "complete":
      return await completeFill(session, params);
    case "steer":
      await attachContext(params.context, "steer");
      await session.steer(params.text);
      return snapshot();
    case "followUp":
      await attachContext(params.context, "followUp");
      await session.followUp(params.text);
      return snapshot();
    case "abort": await session.abort(); return snapshot();
    case "compact": return await session.compact(params.instructions);
    case "models": return session.modelRuntime.getModels().map((model) => ({
      provider: model.provider,
      id: model.id,
      name: model.name,
    }));
    case "setModel": {
      const model = session.modelRuntime.getModel(params.provider, params.modelId);
      if (!model) throw new Error(`unknown Pi model ${params.provider}/${params.modelId}`);
      await session.setModel(model);
      return snapshot();
    }
    case "setThinking": session.setThinkingLevel(params.level); return snapshot();
    case "newSession":
      return await replace(SessionManager.create(cwd, sessionDir));
    case "cloneSession": {
      if (!session.sessionFile) throw new Error("current Pi session is not persisted");
      return await replace(SessionManager.forkFrom(session.sessionFile, cwd, sessionDir));
    }
    case "rewind": {
      const path = session.sessionManager.createBranchedSession(params.entryId);
      if (!path) throw new Error("Pi could not persist the rewind branch");
      return await replace(SessionManager.open(path, sessionDir, cwd));
    }
    case "fork": {
      const path = session.sessionManager.createBranchedSession(params.entryId);
      if (!path) throw new Error("Pi could not persist the forked session");
      return await replace(SessionManager.open(path, sessionDir, cwd));
    }
    case "switchSession":
      return await replace(SessionManager.open(
        await confinedSessionPath(params.path), sessionDir, cwd));
    case "listSessions": return await SessionManager.list(cwd, sessionDir);
    case "messages": return session.messages;
    case "entries": {
      const entries = session.sessionManager.getEntries();
      if (!params.since) return entries;
      const index = entries.findIndex((entry) => entry.id === params.since);
      return index < 0 ? entries : entries.slice(index + 1);
    }
    case "tree": return session.sessionManager.getTree();
    case "stats": return session.getSessionStats();
    case "setName":
      session.sessionManager.appendSessionInfo(params.name);
      return snapshot();
    default: throw new Error(`unknown Glass Pi SDK operation ${name}`);
  }
}

async function handle(message) {
  if (message.operation === "toolResult") {
    const pending = toolResponses.get(message.id);
    if (!pending) return;
    toolResponses.delete(message.id);
    if (message.ok) pending.resolve(message.result);
    else pending.reject(new Error(message.error || "Glass tool call failed"));
    return;
  }
  try {
    const result = await operation(message.operation, message.params);
    send({ type: "response", id: message.id, operation: message.operation, ok: true, result });
  } catch (error) {
    send({
      type: "response",
      id: message.id,
      operation: message.operation,
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    });
  }
}

process.stdin.on("data", (chunk) => {
  input = Buffer.concat([input, chunk]);
  while (input.length >= 4) {
    const length = input.readUInt32BE(0);
    if (length === 0 || length > 16 * 1024 * 1024) {
      throw new Error("invalid Glass Pi runtime frame length");
    }
    if (input.length < 4 + length) break;
    const body = input.subarray(4, 4 + length);
    input = input.subarray(4 + length);
    let message;
    try {
      message = JSON.parse(body.toString("utf8"));
    } catch {
      throw new Error("invalid Glass Pi runtime JSON frame");
    }
    void handle(message);
  }
});

process.on("SIGTERM", async () => {
  try { await runtime?.session?.abort(); } catch {}
  unsubscribe?.();
  runtime?.session?.dispose();
  process.exit(0);
});

const initialManager = process.env.GLASS_PI_FORK_FROM
  ? SessionManager.forkFrom(process.env.GLASS_PI_FORK_FROM, cwd, sessionDir)
  : process.env.GLASS_PI_RESUME
    ? SessionManager.continueRecent(cwd, sessionDir)
    : SessionManager.create(cwd, sessionDir);
bind(await create(initialManager));
send({ type: "ready", state: snapshot() });
