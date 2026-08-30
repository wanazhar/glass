#!/usr/bin/env node
// Glass-owned Pi AgentSession SDK runtime. Protocol: 4-byte BE length + JSON.

import { createRequire } from "node:module";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
} from "node:fs";
import { spawn } from "node:child_process";
import { realpath } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";

const sdkEntry = process.env.GLASS_PI_SDK_ENTRY;
const cwd = process.env.GLASS_PI_CWD;
const sessionDir = process.env.GLASS_PI_SESSION_DIR;
const agentDir = process.env.GLASS_PI_AGENT_DIR;
if (!sdkEntry || !cwd || !sessionDir || !agentDir) {
  throw new Error("Glass Pi runtime requires SDK, cwd, session, and agent paths");
}

const sdk = await import(pathToFileURL(sdkEntry).href);
const { BUILTIN_SLASH_COMMANDS } = await import(
  new URL("./core/slash-commands.js", pathToFileURL(sdkEntry)).href,
);
const require = createRequire(sdkEntry);
const typeboxEntry = require.resolve("typebox");
const { Type } = await import(pathToFileURL(typeboxEntry).href);
const {
  copyToClipboard,
  createAgentSession,
  DefaultResourceLoader,
  ProjectTrustStore,
  SessionManager,
  SettingsManager,
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
    "Use the familiar Glass-backed coding tools for local source work; use glass_tool for browser, Git, process, test, debugger, editor review, task, and evidence operations.",
    "Example: glass_tool({name: \"glass.browser.observe\", arguments: {}}).",
    "Canonical glass_tool names include glass.browser.verify, glass.editor.fim, glass.editor.proposal.accept_pack, glass.task.crew, glass.github.ship, glass.git.merge, glass.git.rebase, glass.git.push, glass.todo.write, glass.lsp.inlay_hints, glass.file.search, and glass.workflow.record.",
    "Follow context.playbook: editor uses glass.editor.*; browser observe/act/verify; git uses glass.git.* and glass.github.review — never bash git.",
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
  const trustedResources = process.env.GLASS_PI_TRUSTED_RESOURCES === "1";
  const settingsManager = SettingsManager.create(cwd, agentDir, {
    projectTrusted: trustedResources,
  });
  const resourceLoader = new DefaultResourceLoader({
    cwd,
    agentDir,
    settingsManager,
    noExtensions: !trustedResources,
    noSkills: !trustedResources,
    noPromptTemplates: !trustedResources,
    noThemes: !trustedResources,
    noContextFiles: !trustedResources,
    systemPrompt: process.env.GLASS_PI_SYSTEM_PROMPT || undefined,
  });
  await resourceLoader.reload();
  const result = await createAgentSession({
    cwd,
    sessionManager: manager,
    settingsManager,
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

const MAX_SLASH_DISPLAY_BYTES = 128 * 1024;

function modelInfo(model) {
  return model ? {
    provider: model.provider,
    id: model.id,
    name: model.name,
  } : null;
}

function modelList(models) {
  return [...models].map((model) => modelInfo(model));
}

function extensionSlashCommands(session) {
  return session.extensionRunner.getRegisteredCommands().map((command) => ({
    name: command.invocationName || command.name,
    description: command.description || "Extension command",
    source: "extension",
  }));
}

function slashCommandCatalog(session) {
  const commands = BUILTIN_SLASH_COMMANDS.map((command) => ({
    ...command,
    source: "builtin",
  }));
  const seen = new Set(commands.map((command) => command.name));
  const add = (command) => {
    if (!seen.has(command.name)) {
      seen.add(command.name);
      commands.push(command);
    }
  };
  for (const command of extensionSlashCommands(session)) add(command);
  for (const prompt of session.resourceLoader.getPrompts().prompts) {
    add({
      name: prompt.name,
      description: prompt.description || "Prompt template",
      ...(prompt.argumentHint ? { argumentHint: prompt.argumentHint } : {}),
      source: "prompt",
    });
  }
  if (session.settingsManager.getEnableSkillCommands()) {
    for (const skill of session.resourceLoader.getSkills().skills) {
      add({
        name: `skill:${skill.name}`,
        description: skill.description || "Skill",
        source: "skill",
      });
    }
  }
  return commands;
}

function slashArgs(args) {
  const trimmed = String(args || "").trim();
  return trimmed ? trimmed.split(/\s+/) : [];
}

function parseBoolean(value, label = "value") {
  if (/^(1|true|yes|on)$/i.test(value)) return true;
  if (/^(0|false|no|off)$/i.test(value)) return false;
  throw new Error(`${label} must be yes or no`);
}

function parseModelRef(session, first, second) {
  const reference = second === undefined
    ? String(first || "")
    : `${String(first || "")}/${String(second || "")}`;
  const split = reference.indexOf("/");
  if (split <= 0 || split === reference.length - 1) {
    throw new Error("expected PROVIDER/MODEL or PROVIDER MODEL");
  }
  const provider = reference.slice(0, split);
  const modelId = reference.slice(split + 1);
  const model = session.modelRuntime.getModel(provider, modelId);
  if (!model) throw new Error(`unknown Pi model ${provider}/${modelId}`);
  return model;
}

function settingsSnapshot(session) {
  const manager = session.settingsManager;
  return {
    global: safe(manager.getGlobalSettings()),
    project: safe(manager.getProjectSettings()),
    projectTrusted: manager.isProjectTrusted(),
    effective: {
      provider: manager.getDefaultProvider(),
      model: manager.getDefaultModel(),
      thinking: manager.getDefaultThinkingLevel(),
      steeringMode: manager.getSteeringMode(),
      followUpMode: manager.getFollowUpMode(),
      theme: manager.getTheme(),
      compaction: manager.getCompactionSettings(),
      skillCommands: manager.getEnableSkillCommands(),
      enabledModels: manager.getEnabledModels(),
    },
  };
}

function sessionInfo(session) {
  return {
    ...safe(session.getSessionStats()),
    sessionName: session.sessionManager.getSessionName(),
    sessionFile: session.sessionFile,
    entryCount: session.sessionManager.getEntries().length,
    leafId: session.sessionManager.getLeafId(),
    model: modelInfo(session.model),
    thinking: session.thinkingLevel,
  };
}

function displayResult(name, value) {
  if (value === undefined) return "done";
  if (typeof value === "string") return value;
  let text;
  try {
    text = JSON.stringify(safe(value), null, 2);
  } catch {
    text = String(value);
  }
  if (Buffer.byteLength(text, "utf8") > MAX_SLASH_DISPLAY_BYTES) {
    let end = Math.min(text.length, MAX_SLASH_DISPLAY_BYTES);
    while (end > 0 && Buffer.byteLength(text.slice(0, end), "utf8") > MAX_SLASH_DISPLAY_BYTES) {
      end -= 1;
    }
    text = `${text.slice(0, end)}\n… /${name} output truncated`;
  }
  return text;
}

function emitSlashResult(name, args, result) {
  send({
    type: "glass_pi_command_result",
    command: normalizeSlashCommand(name) || "command",
    args: args || "",
    text: displayResult(name, result),
  });
}

function normalizeSlashCommand(value) {
  return String(value || "").trim().replace(/^\/+/, "");
}

function runCommand(command, args, timeoutMs = 30_000) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, args, {
      cwd,
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    let settled = false;
    const append = (target, chunk) => {
      const text = chunk.toString("utf8");
      return target.length >= 256 * 1024
        ? target
        : `${target}${text}`.slice(0, 256 * 1024);
    };
    const timer = setTimeout(() => {
      if (settled) return;
      child.kill();
      settled = true;
      reject(new Error(`${command} timed out after ${timeoutMs}ms`));
    }, timeoutMs);
    child.stdout.on("data", (chunk) => { stdout = append(stdout, chunk); });
    child.stderr.on("data", (chunk) => { stderr = append(stderr, chunk); });
    child.on("error", (error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      reject(new Error(`${command} could not start: ${error.message}`));
    });
    child.on("close", (code) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (code === 0) {
        resolvePromise(stdout.trim());
      } else {
        const detail = (stderr || stdout).trim() || `exit code ${code ?? "unknown"}`;
        reject(new Error(`${command} failed: ${detail}`));
      }
    });
  });
}

function trustSnapshot() {
  const store = new ProjectTrustStore(agentDir);
  return {
    cwd,
    saved: store.getEntry(cwd),
    effective: runtime.session.settingsManager.isProjectTrusted(),
  };
}

function hotkeysText(session) {
  const extensionShortcuts = session.extensionRunner.getRegisteredCommands().length > 0
    ? "Extension commands are available through the same `/` modal."
    : "No extension commands are currently loaded.";
  return `Navigation\n- Arrow keys: move through the Glass modal or editor\n- Page Up/Page Down: move through long command output\n\nEditing\n- Enter: run the selected slash command\n- Tab: complete the selected command\n- Ctrl-U: clear the slash input\n- Escape: close the modal\n\nAgent surface\n- /: open native Pi slash commands\n- :: open Glass workspace actions\n- /copy: copy the last assistant message\n- /quit: leave Glass Dev\n\n${extensionShortcuts}`;
}

async function forkSession(entryId, position = "before") {
  const session = runtime.session;
  const selected = session.sessionManager.getEntry(entryId);
  if (!selected) throw new Error("invalid entry ID for forking");
  let targetLeafId;
  if (position === "at") {
    targetLeafId = selected.id;
  } else {
    if (selected.type !== "message" || selected.message.role !== "user") {
      throw new Error("fork requires a user-message entry ID");
    }
    targetLeafId = selected.parentId;
  }
  if (!targetLeafId) {
    const next = SessionManager.create(cwd, sessionDir);
    next.newSession({ parentSession: session.sessionFile });
    return replace(next);
  }
  const path = session.sessionManager.createBranchedSession(targetLeafId);
  if (!path) throw new Error("Pi could not persist the forked session");
  return replace(SessionManager.open(path, sessionDir, cwd));
}

async function importSession(inputPath) {
  const source = resolve(cwd, inputPath);
  if (!existsSync(source)) throw new Error(`session file not found: ${source}`);
  mkdirSync(sessionDir, { recursive: true });
  const destination = join(sessionDir, basename(source));
  if (resolve(destination) !== source) copyFileSync(source, destination);
  return replace(SessionManager.open(destination, sessionDir, cwd));
}

async function confinedSessionPath(path) {
  const canonicalDir = await realpath(sessionDir);
  const sessionCandidate = resolve(sessionDir, path);
  const candidate = existsSync(sessionCandidate) ? sessionCandidate : resolve(cwd, path);
  const canonical = await realpath(candidate);
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
          "glassTool", "slashCommands", "slashCommand",
        ],
      };
    case "state": return snapshot();
    case "slashCommands": return slashCommandCatalog(session);
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
    case "slashCommand": {
      const command = normalizeSlashCommand(params.name);
      const args = String(params.args || "").trim();
      if (!command) throw new Error("Pi slash command name is required");
      const tokens = slashArgs(args);
      switch (command) {
        case "settings": {
          if (tokens.length === 0) return { kind: "settings", settings: settingsSnapshot(session) };
          const manager = session.settingsManager;
          const values = [...tokens];
          if (values[0].toLowerCase() === "set") values.shift();
          const key = values.shift()?.toLowerCase();
          const value = values.join(" ").trim();
          if (!key || !value) {
            throw new Error("/settings accepts `set KEY VALUE`; supported keys: theme, model, thinking, steering, follow-up, compaction, skill-commands");
          }
          switch (key) {
            case "theme": manager.setTheme(value); break;
            case "model": {
              const modelTokens = slashArgs(value);
              const model = parseModelRef(session, modelTokens[0], modelTokens[1]);
              manager.setDefaultModelAndProvider(model.provider, model.id);
              break;
            }
            case "thinking": manager.setDefaultThinkingLevel(value); break;
            case "steering":
              if (value !== "all" && value !== "one-at-a-time") throw new Error("steering must be all or one-at-a-time");
              manager.setSteeringMode(value);
              break;
            case "follow-up":
            case "followup":
              if (value !== "all" && value !== "one-at-a-time") throw new Error("follow-up must be all or one-at-a-time");
              manager.setFollowUpMode(value);
              break;
            case "compaction": manager.setCompactionEnabled(parseBoolean(value, "compaction")); break;
            case "skill-commands": manager.setEnableSkillCommands(parseBoolean(value, "skill-commands")); break;
            default: throw new Error(`unsupported Pi setting ${key}`);
          }
          await manager.flush();
          return { kind: "settings", settings: settingsSnapshot(session) };
        }
        case "model": {
          if (tokens.length === 0) {
            return {
              kind: "models",
              current: modelInfo(session.model),
              models: modelList(session.modelRuntime.getAvailableSnapshot()),
            };
          }
          if (tokens.length > 2) throw new Error("/model accepts PROVIDER/MODEL or PROVIDER MODEL");
          const model = parseModelRef(session, tokens[0], tokens[1]);
          await session.setModel(model, { persist: false });
          return { kind: "model", current: modelInfo(session.model), snapshot: snapshot() };
        }
        case "tree": {
          if (tokens.length === 0) {
            return {
              kind: "tree",
              leafId: session.sessionManager.getLeafId(),
              tree: session.sessionManager.getTree(),
            };
          }
          if (tokens.length !== 1) throw new Error("/tree accepts one entry ID");
          const result = await session.navigateTree(tokens[0], { summarize: false });
          return { kind: "tree", result, snapshot: snapshot() };
        }
        case "thinking": {
          const levels = session.getAvailableThinkingLevels();
          if (tokens.length === 0) return { kind: "thinking", current: session.thinkingLevel, levels };
          if (tokens.length !== 1 || !levels.includes(tokens[0])) {
            throw new Error(`unknown thinking level; available: ${levels.join(", ")}`);
          }
          session.setThinkingLevel(tokens[0], { persist: false });
          return { kind: "thinking", current: session.thinkingLevel, levels, snapshot: snapshot() };
        }
        case "scoped-models": {
          if (tokens.length === 0) {
            return {
              kind: "scoped-models",
              configured: session.settingsManager.getEnabledModels(),
              current: modelList(session.scopedModels.map(({ model }) => model)),
            };
          }
          if (tokens.length === 1 && /^(all|off|none)$/i.test(tokens[0])) {
            session.setScopedModels([]);
            session.settingsManager.setEnabledModels(undefined);
          } else {
            const models = tokens.map((token) => parseModelRef(session, token));
            session.setScopedModels(models.map((model) => ({ model })));
            session.settingsManager.setEnabledModels(tokens);
          }
          await session.settingsManager.flush();
          return {
            kind: "scoped-models",
            configured: session.settingsManager.getEnabledModels(),
            current: modelList(session.scopedModels.map(({ model }) => model)),
          };
        }
        case "export": {
          const target = args ? resolve(cwd, args) : undefined;
          const path = target?.toLowerCase().endsWith(".jsonl")
            ? session.exportToJsonl(target)
            : await session.exportToHtml(target);
          return { kind: "export", path };
        }
        case "import": {
          if (!args) throw new Error("/import requires a JSONL path");
          const result = await importSession(args);
          return { kind: "import", snapshot: result };
        }
        case "share": {
          const temporary = mkdtempSync(join(tmpdir(), "glass-pi-share-"));
          try {
            const html = join(temporary, "session.html");
            await session.exportToHtml(html);
            const url = await runCommand("gh", ["gist", "create", "--public=false", html]);
            return { kind: "share", url };
          } finally {
            rmSync(temporary, { recursive: true, force: true });
          }
        }
        case "copy": {
          const text = session.getLastAssistantText();
          if (!text) throw new Error("there is no assistant message to copy");
          await copyToClipboard(text);
          return { kind: "copy", characters: text.length };
        }
        case "name": {
          if (!args) return { kind: "name", session: sessionInfo(session) };
          await session.setSessionName(args);
          return { kind: "name", snapshot: snapshot() };
        }
        case "session": return { kind: "session", session: sessionInfo(session) };
        case "changelog": {
          const path = join(dirname(sdkEntry), "..", "CHANGELOG.md");
          const text = existsSync(path) ? readFileSync(path, "utf8") : "Pi changelog is unavailable";
          return { kind: "changelog", text };
        }
        case "hotkeys": return { kind: "hotkeys", text: hotkeysText(session) };
        case "fork": {
          if (tokens.length === 0) {
            return { kind: "fork", messages: session.getUserMessagesForForking() };
          }
          if (tokens.length !== 1) throw new Error("/fork accepts one user-message entry ID");
          return { kind: "fork", snapshot: await forkSession(tokens[0], "before") };
        }
        case "clone": {
          if (tokens.length !== 0) throw new Error("/clone does not accept arguments");
          const leafId = session.sessionManager.getLeafId();
          if (!leafId) throw new Error("there is no session content to clone");
          return { kind: "clone", snapshot: await forkSession(leafId, "at") };
        }
        case "trust": {
          if (tokens.length === 0) return { kind: "trust", trust: trustSnapshot() };
          if (tokens.length !== 1 || !/^(yes|no|reset|true|false)$/i.test(tokens[0])) {
            throw new Error("/trust accepts yes, no, or reset");
          }
          const decision = /^reset$/i.test(tokens[0]) ? null : parseBoolean(tokens[0], "trust");
          new ProjectTrustStore(agentDir).set(cwd, decision);
          session.settingsManager.setProjectTrusted(decision === true);
          await session.settingsManager.flush();
          await session.reload();
          return { kind: "trust", trust: trustSnapshot() };
        }
        case "login": {
          const providers = session.modelRuntime.getProviders().map((provider) => ({
            id: provider.id,
            name: provider.name,
          }));
          if (args && !providers.some((provider) =>
            provider.id.toLowerCase() === args.toLowerCase() || provider.name.toLowerCase() === args.toLowerCase())) {
            throw new Error(`unknown Pi login provider ${args}`);
          }
          return {
            kind: "login",
            provider: args || null,
            providers,
            message: "Pi login requires terminal handoff; Glass will open Pi for the human login flow",
          };
        }
        case "logout": {
          if (!args) {
            const credentials = await session.modelRuntime.listCredentials({ signal: AbortSignal.timeout(15_000) });
            return {
              kind: "logout",
              credentials: credentials.map(({ providerId, type }) => ({ providerId, type })),
            };
          }
          if (tokens.length !== 1) throw new Error("/logout accepts one provider ID");
          await session.modelRuntime.logout(tokens[0], { signal: AbortSignal.timeout(15_000) });
          return { kind: "logout", provider: tokens[0], removed: true };
        }
        case "new": {
          if (tokens.length !== 0) throw new Error("/new does not accept arguments");
          return { kind: "new", snapshot: await replace(SessionManager.create(cwd, sessionDir)) };
        }
        case "compact": {
          const result = await session.compact(args || undefined);
          return { kind: "compact", result, snapshot: snapshot() };
        }
        case "resume": {
          if (!args) {
            return { kind: "sessions", sessions: await SessionManager.list(cwd, sessionDir) };
          }
          return {
            kind: "resume",
            snapshot: await replace(SessionManager.open(await confinedSessionPath(args), sessionDir, cwd)),
          };
        }
        case "reload":
          if (tokens.length !== 0) throw new Error("/reload does not accept arguments");
          await session.reload();
          return { kind: "reload", commands: slashCommandCatalog(session), snapshot: snapshot() };
        case "quit":
          if (tokens.length !== 0) throw new Error("/quit does not accept arguments");
          return { kind: "quit", message: "Glass will close the Agent surface" };
        default:
          break;
      }

      const extension = session.extensionRunner.getCommand(command);
      if (extension) {
        const result = await extension.handler(args, session.extensionRunner.createCommandContext());
        return { kind: "extension", command, result };
      }
      const prompt = session.resourceLoader.getPrompts().prompts
        .find((candidate) => candidate.name === command);
      const skill = session.resourceLoader.getSkills().skills
        .find((candidate) => `skill:${candidate.name}` === command);
      if (prompt || skill) {
        await session.prompt(`/${command}${args ? ` ${args}` : ""}`);
        return { kind: skill ? "skill" : "prompt", command, snapshot: snapshot() };
      }
      throw new Error(`unknown Pi slash command /${command}`);
    }
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
    if (message.operation === "slashCommand") {
      emitSlashResult(message.params?.name, message.params?.args, result);
    }
    send({ type: "response", id: message.id, operation: message.operation, ok: true, result });
  } catch (error) {
    if (message.operation === "slashCommand") {
      send({
        type: "glass_pi_command_result",
        command: normalizeSlashCommand(message.params?.name) || "command",
        args: message.params?.args || "",
        ok: false,
        text: error instanceof Error ? error.message : String(error),
      });
    }
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
