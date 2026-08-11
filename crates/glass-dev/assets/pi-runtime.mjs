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

const glassTool = {
  name: "glass_tool",
  label: "Glass Tool",
  description: "Invoke one governed Glass development or browser capability.",
  promptSnippet: "glass_tool: call authoritative Glass workspace capabilities",
  promptGuidelines: [
    "Use Glass tools for files, processes, Git, tests, debugger, browser, and evidence.",
    "Do not claim a mutation succeeded until the Glass tool result confirms it.",
  ],
  parameters: Type.Object({
    name: Type.String({ minLength: 1, maxLength: 256 }),
    arguments: Type.Optional(Type.Record(Type.String(), Type.Unknown())),
  }, { additionalProperties: false }),
  executionMode: "sequential",
  async execute(toolCallId, params, signal) {
    const result = await callGlass(toolCallId, params, signal);
    return {
      content: [{ type: "text", text: JSON.stringify(result) }],
      details: result,
    };
  },
};

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
    noTools: "all",
    customTools: [glassTool],
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

async function operation(name, params = {}) {
  const session = runtime.session;
  switch (name) {
    case "hello":
      return {
        protocol: "glass-pi-sdk-v1",
        sdk: "AgentSession",
        capabilities: [
          "prompt", "steer", "followUp", "abort", "compact", "models",
          "thinking", "newSession", "cloneSession", "fork", "switchSession",
          "messages", "entries", "tree", "stats", "name", "glassTool",
        ],
      };
    case "state": return snapshot();
    case "prompt": await session.prompt(params.text); return snapshot();
    case "steer": await session.steer(params.text); return snapshot();
    case "followUp": await session.followUp(params.text); return snapshot();
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

const initialManager = process.env.GLASS_PI_RESUME
  ? SessionManager.continueRecent(cwd, sessionDir)
  : SessionManager.create(cwd, sessionDir);
bind(await create(initialManager));
send({ type: "ready", state: snapshot() });
