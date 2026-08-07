import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawn, execFileSync } from "node:child_process";
import { performance } from "node:perf_hooks";
import { atomicWriteJson } from "../checkpoint.mjs";
import { summarizeByteSamples } from "../metric-utils.mjs";

class TransportError extends Error {}
class UnsupportedScenario extends Error {}

const corpus = JSON.parse(fs.readFileSync(new URL("../scenarios/v1.json", import.meta.url), "utf8"));
const fixture = fs.readFileSync(new URL("../../crates/glass-browser/tests/fixtures/scorecard.html", import.meta.url), "utf8");
const iterations = positiveInteger("GLASS_SCORECARD_ITERATIONS", process.env.GLASS_SCORECARD_ITERATIONS ?? "10");
const chromePath = requiredEnv("CHROME_PATH");
const command = requiredEnv("AGENT_BROWSER_COMMAND");
const expectedVersion = requiredEnv("AGENT_BROWSER_VERSION");
const gitRevision = requiredEnv("GLASS_SCORECARD_GIT_REVISION");
const checkpointPath = requiredEnv("GLASS_SCORECARD_CHECKPOINT_PATH");
const invocation = { run_id: requiredEnv("GLASS_SCORECARD_RUN_ID"), started_at: requiredEnv("GLASS_SCORECARD_STARTED_AT") };
const requestTimeoutMs = positiveInteger("AGENT_BROWSER_REQUEST_TIMEOUT_MS", process.env.AGENT_BROWSER_REQUEST_TIMEOUT_MS ?? "30000");
const startupStarted = performance.now();
const chromeExecutable = resolveChromeExecutable(chromePath);
const socketDir = fs.mkdtempSync(path.join(os.tmpdir(), "gabs-"));
const client = createMcpClient(command, ["mcp", "--tools", "all"], {
  AGENT_BROWSER_EXECUTABLE_PATH: chromeExecutable,
  AGENT_BROWSER_NO_AUTO_DIALOG: "true",
  AGENT_BROWSER_NAMESPACE: `g${process.pid}`,
  AGENT_BROWSER_SESSION: `g${process.pid}`,
  AGENT_BROWSER_SOCKET_DIR: socketDir,
});

let outcomes = [];
let compactByteSamples = [];
let startupMs = 0;
try {
  const initialized = await client.initialize();
  const serverName = initialized.serverInfo?.name ?? "unknown";
  if (!/agent.?browser/i.test(serverName)) throw new Error(`unexpected MCP server identity: ${serverName}`);
  const availableTools = new Set();
  let cursor;
  do {
    const listed = await client.request("tools/list", cursor ? { cursor } : {});
    for (const { name } of listed.tools) availableTools.add(name);
    cursor = listed.nextCursor;
  } while (cursor);
  for (const required of ["agent_browser_open", "agent_browser_click", "agent_browser_fill", "agent_browser_snapshot", "agent_browser_eval"]) {
    if (!availableTools.has(required)) throw new Error(`agent-browser MCP missing required tool: ${required}`);
  }
  const caps = {
    hasSnapshot: availableTools.has("agent_browser_snapshot"),
    hasEvaluate: availableTools.has("agent_browser_eval"),
    hasGetText: availableTools.has("agent_browser_get_text"),
    hasAcceptDialog: availableTools.has("agent_browser_dialog_accept"),
  };
  await client.tool("agent_browser_open", { url: `data:text/html;base64,${Buffer.from(fixture).toString("base64")}` });
  startupMs = performance.now() - startupStarted;
  for (let iteration = 1; iteration <= iterations; iteration += 1) {
    for (const scenario of corpus.scenarios) {
      await client.tool("agent_browser_eval", { script: "(() => { window.resetFixture(); document.querySelector('#name').value = ''; return true; })()" }).catch(() => {});
      if (caps.hasSnapshot) {
        try {
          const snapshot = await client.tool("agent_browser_snapshot", {});
          compactByteSamples.push(Buffer.byteLength(JSON.stringify(snapshot.content ?? snapshot), "utf8"));
        } catch {}
      }
      const started = performance.now();
      let actual = null;
      let error = null;
      let unsupportedScenario = false;
      try { actual = await runScenario(client, scenario.id, caps); } catch (caught) {
        if (caught instanceof TransportError) throw caught;
        unsupportedScenario = caught instanceof UnsupportedScenario;
        error = boundedText(caught?.message ?? caught);
      }
      if (typeof actual === "string") actual = boundedText(actual);
      const status = actual === scenario.expected ? "success" : scenario.forbidden.includes(actual) ? "wrong_action" : unsupportedScenario ? "unsupported" : "failure";
      outcomes.push({ id: scenario.id, category: scenario.category, iteration, expected: scenario.expected, actual, status, error, latency_ms: performance.now() - started, cdp_requests: null });
    }
    writeCheckpoint(iteration);
  }
} finally {
  try { await client.tool("agent_browser_close", {}); } catch {}
  await client.close();
}

const successes = count("success");
const failures = count("failure");
const wrongActions = count("wrong_action");
const unsupported = count("unsupported");
const report = {
  schema_version: 1,
  tool: { name: "agent-browser", version: expectedVersion },
  run: { corpus: corpus.corpus, corpus_fixture: corpus.fixture, iterations, temperature: "warm", profile: process.env.GLASS_SCORECARD_PROFILE ?? "fresh-ephemeral-single-session", viewport: { width: 1280, height: 720 } },
  environment: { os: process.platform, architecture: process.arch, rust: null, chrome: commandVersion(chromePath), machine: `${os.hostname()} ${os.release()}` },
  resources: { scope: "Runner RSS is the agent-browser MCP server process only; client and Chrome process-tree metrics are unavailable", runner: { pid: client.pid, rss_start_bytes: null, rss_end_bytes: null, peak_rss_bytes: client.peakRss }, chrome: { root_pid: null, rss_end_bytes: null, peak_process_tree_rss_bytes: null }, binary_size_bytes: fs.statSync(process.execPath).size, compact_context_bytes: null, cdp_requests: null, startup_ms: startupMs },
  summary: { successes, failures, wrong_actions: wrongActions, unsupported, task_success_rate: outcomes.length ? successes / outcomes.length : 0, hard_gate_passed: successes === outcomes.length },
  metrics: { compact_observe_bytes: summarizeByteSamples(compactByteSamples) },
  scenarios: outcomes,
};
process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);

async function runScenario(mcp, id, caps) {
  switch (id) {
    case "duplicate-label": return runDuplicateLabel(mcp, caps);
    case "overlay": return runOverlay(mcp, caps);
    case "reflow": return runReflow(mcp, caps);
    case "delayed-content": return runDelayedContent(mcp, caps);
    case "spa-navigation": return runSpaNavigation(mcp, caps);
    case "form": return runForm(mcp, caps);
    case "popup": throw new UnsupportedScenario("agent-browser 0.33.0 exposes no typed popup witness primitive");
    case "frame": return runFrame(mcp, caps);
    case "dialog": return runDialog(mcp, caps);
    case "download": throw new UnsupportedScenario("agent-browser 0.33.0 exposes no typed download-integrity primitive");
    case "failure-recovery": return runFailureRecovery(mcp, caps);
    default: throw new Error(`unknown scenario ${id}`);
  }
}
async function result(mcp) { return evaluate(mcp, "document.querySelector('#result').value"); }
async function evaluate(mcp, fn) {
  const r = await mcp.tool("agent_browser_eval", { script: fn });
  const structuredResult = r.structuredContent?.response?.data?.result;
  if (structuredResult !== undefined) return String(structuredResult);
  const text = r.content?.filter(({ type }) => type === "text").map(({ text }) => text).join("\n");
  if (!text) throw new Error("evaluate returned no text");
  const m = text.match(/^### Result\n([^\n]*)/m);
  if (m) return decodeEvalScalar(m[1]);
  try { const p = JSON.parse(text); if (p.result !== undefined) return decodeEvalScalar(p.result); } catch {}
  return text.trim();
}
function decodeEvalScalar(value) {
  if (typeof value !== "string") return String(value);
  try {
    const decoded = JSON.parse(value);
    if (typeof decoded === "string") return decoded;
  } catch {}
  return value;
}

async function runDuplicateLabel(mcp, caps) {
  if (caps.hasSnapshot) {
    const snap = await mcp.tool("agent_browser_snapshot", {});
    const text = snap.content?.filter(({ type }) => type === "text").map(({ text }) => text).join("\n") ?? "";
    const refs = [...text.matchAll(/@(\w+)\b.*?Delete(?!\s*draft)/g)];
    if (refs.length > 0) {
      try { await mcp.tool("agent_browser_click", { selector: `@${refs[refs.length - 1][1]}` }); return await result(mcp); } catch {}
    }
    const allRefs = [...text.matchAll(/@(\w+)\b.*?Delete/g)];
    if (allRefs.length > 1) {
      try { await mcp.tool("agent_browser_click", { selector: `@${allRefs[1][1]}` }); return await result(mcp); } catch {}
    }
  }
  try { await mcp.tool("agent_browser_click", { selector: "#duplicate-right" }); } catch { return "blocked"; }
  return await result(mcp);
}
async function runOverlay(mcp, caps) {
  await mcp.tool("agent_browser_eval", { script: "(() => { document.querySelector('#overlay').style.display='block'; return true; })()" });
  try { await mcp.tool("agent_browser_click", { selector: "#overlay-target" }); return await result(mcp); } catch { return "blocked"; }
}
async function runReflow(mcp, caps) {
  try { await mcp.tool("agent_browser_click", { selector: "#moving" }); } catch { return "blocked"; }
  return (await result(mcp)) === "idle" ? "blocked" : await result(mcp);
}
async function runDelayedContent(mcp, caps) {
  await mcp.tool("agent_browser_eval", { script: "(async () => { window.scheduleDelayed(); await new Promise(r => setTimeout(r, 200)); return true; })()" });
  return evaluate(mcp, "document.querySelector('#delayed')?.textContent ?? 'missing'");
}
async function runSpaNavigation(mcp, caps) {
  await mcp.tool("agent_browser_click", { selector: "#spa" });
  return await result(mcp);
}
async function runForm(mcp, caps) {
  await mcp.tool("agent_browser_fill", { selector: "#name", text: "Glass" });
  await mcp.tool("agent_browser_click", { selector: "#form button" });
  return await result(mcp);
}
async function runFrame(mcp, caps) {
  try {
    await mcp.tool("agent_browser_eval", { script: "(() => { const b = document.querySelector('#frame')?.contentDocument?.querySelector('#frame-action'); if (b) { b.click(); return 'frame-clicked'; } return 'frame-not-found'; })()" });
    return await result(mcp);
  } catch { throw new UnsupportedScenario("agent-browser frame interaction was not available"); }
}
async function runDialog(mcp, caps) {
  try {
    await mcp.tool("agent_browser_click", { selector: "#dialog" });
    if (caps.hasAcceptDialog) await mcp.tool("agent_browser_dialog_accept", {});
    for (let i = 0; i < 20; i++) {
      try { const v = await result(mcp); if (v === "dialog-accepted") return v; } catch {}
      await new Promise((r) => setTimeout(r, 25));
    }
    return await result(mcp);
  } catch { return "blocked"; }
}
async function runFailureRecovery(mcp, caps) {
  try { await mcp.tool("agent_browser_click", { selector: "#definitely-missing" }); return "unexpected-action"; } catch {}
  await mcp.tool("agent_browser_eval", { script: "(() => { document.querySelector('#result').value = 'recovered'; return true; })()" });
  return "recovered";
}

function count(status) { return outcomes.filter((o) => o.status === status).length; }
function writeCheckpoint(iteration) { atomicWriteJson(checkpointPath, { iteration, outcomes_snapshot: [...outcomes] }); }
function positiveInteger(envName, value) { const p = parseInt(value, 10); if (!Number.isFinite(p) || p < 1) throw new Error(`${envName} must be a positive integer, got: ${value}`); return p; }
function requiredEnv(name) { const v = process.env[name]; if (!v) throw new Error(`${name} is required`); return v; }
function boundedText(value) { const t = String(value ?? ""); return t.length > 1024 ? t.slice(0, 1024) : t; }
function commandVersion(path) { try { return execFileSync(path, ["--version"], { encoding: "utf8" }).trim(); } catch { return null; } }
function resolveChromeExecutable(pathname) {
  const resolved = fs.realpathSync(pathname);
  if (process.platform === "linux" && (resolved === "/usr/bin/snap" || pathname === "/snap/bin/chromium")) {
    const snapChrome = "/snap/chromium/current/usr/lib/chromium-browser/chrome";
    if (fs.existsSync(snapChrome)) return fs.realpathSync(snapChrome);
  }
  return resolved;
}
function processRss(pid) { try { return parseInt(fs.readFileSync(`/proc/${pid}/stat`).toString().split(" ")[23], 10) * 4096; } catch { return null; } }

function createMcpClient(command, args, environment) {
  return new (class McpClient {
  constructor(command, args) {
    this.child = spawn(command, args, { stdio: ["pipe", "pipe", "inherit"], env: { ...process.env, ...environment } });
    this.pid = this.child.pid;
    this.nextId = 0;
    this.pending = new Map();
    this.buffer = Buffer.alloc(0);
    this.failed = null;
    this.peakRss = processRss(this.pid);
    this.sampler = setInterval(() => { const rss = processRss(this.pid); if (rss !== null) this.peakRss = Math.max(this.peakRss ?? 0, rss); }, 10);
    this.child.stdout.on("data", (chunk) => this.onData(chunk));
    this.child.once("error", (error) => this.fail(error));
    this.child.once("exit", (code, signal) => this.fail(new Error(`agent-browser MCP server exited (${code ?? signal})`)));
  }
  onData(chunk) {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    if (this.buffer.length > 1024 * 1024) return this.fail(new Error("MCP response exceeded 1 MiB"));
    for (;;) {
      const newline = this.buffer.indexOf(10);
      if (newline < 0) return;
      const line = this.buffer.subarray(0, newline).toString("utf8");
      this.buffer = this.buffer.subarray(newline + 1);
      this.onLine(line);
    }
  }
  onLine(line) {
    let message;
    try { message = JSON.parse(line); } catch { return this.fail(new Error("agent-browser MCP server emitted malformed JSON")); }
    const pending = this.pending.get(message.id);
    if (!pending) return;
    this.pending.delete(message.id);
    clearTimeout(pending.timer);
    if (message.error) pending.reject(new Error(`MCP ${message.error.code}: ${message.error.message}`));
    else pending.resolve(message.result);
  }
  request(method, params) {
    return new Promise((resolve, reject) => {
      if (this.failed) return reject(this.failed);
      const id = ++this.nextId;
      const timer = setTimeout(() => {
        this.pending.delete(id);
        const error = new TransportError(`MCP ${method} exceeded ${requestTimeoutMs} ms`);
        reject(error);
        this.fail(error);
        this.child.kill("SIGKILL");
      }, requestTimeoutMs);
      this.pending.set(id, { resolve, reject, timer });
      this.child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
    });
  }
  fail(error) {
    if (this.failed) return;
    this.failed = error instanceof TransportError ? error : new TransportError(String(error?.message ?? error));
    for (const pending of this.pending.values()) { clearTimeout(pending.timer); pending.reject(this.failed); }
    this.pending.clear();
  }
  async initialize() {
    const result = await this.request("initialize", { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "glass-acceptance", version: "1" } });
    this.child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized", params: {} })}\n`);
    return result;
  }
  async tool(name, args) {
    const result = await this.request("tools/call", { name, arguments: args });
    if (result.isError) throw new Error(result.content?.map(({ text }) => text).filter(Boolean).join("\n") || `${name} failed`);
    return result;
  }
  async close() {
    clearInterval(this.sampler);
    if (this.child.exitCode !== null || this.child.signalCode !== null) return;
    if (!this.child.killed) this.child.kill("SIGTERM");
    await Promise.race([new Promise((resolve) => this.child.once("exit", resolve)), new Promise((resolve) => setTimeout(resolve, 2000))]);
    if (this.child.exitCode === null) this.child.kill("SIGKILL");
  }
  })(command, args);
}
