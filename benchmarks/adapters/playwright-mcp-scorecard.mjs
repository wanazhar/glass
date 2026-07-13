import fs from "node:fs";
import os from "node:os";
import process from "node:process";
import { spawn, execFileSync } from "node:child_process";
import { performance } from "node:perf_hooks";

const corpus = JSON.parse(fs.readFileSync(new URL("../scenarios/v1.json", import.meta.url), "utf8"));
const fixture = fs.readFileSync(new URL("../../tests/fixtures/scorecard.html", import.meta.url), "utf8");
const iterations = positiveInteger("GLASS_SCORECARD_ITERATIONS", process.env.GLASS_SCORECARD_ITERATIONS ?? "10");
const chromePath = requiredEnv("CHROME_PATH");
const command = requiredEnv("PLAYWRIGHT_MCP_COMMAND");
const expectedVersion = requiredEnv("PLAYWRIGHT_MCP_VERSION");
const requestTimeoutMs = positiveInteger("PLAYWRIGHT_MCP_REQUEST_TIMEOUT_MS", process.env.PLAYWRIGHT_MCP_REQUEST_TIMEOUT_MS ?? "30000");
const outputDir = fs.mkdtempSync(`${os.tmpdir()}/glass-playwright-mcp-`);
const startupStarted = performance.now();
const client = new McpClient(command, [
  "--headless", "--isolated", "--executable-path", chromePath,
  "--output-dir", outputDir,
]);

let outcomes = [];
let startupMs = 0;
try {
  const initialized = await client.initialize();
  if (initialized.serverInfo?.name !== "Playwright") throw new Error("unexpected MCP server identity");
  const listed = await client.request("tools/list", {});
  const availableTools = new Set(listed.tools.map(({ name }) => name));
  for (const required of ["browser_navigate", "browser_click", "browser_evaluate", "browser_fill_form", "browser_run_code_unsafe"]) {
    if (!availableTools.has(required)) throw new Error(`released MCP surface is missing ${required}`);
  }
  await client.tool("browser_resize", { width: 1280, height: 720 });
  await client.tool("browser_navigate", { url: `data:text/html;base64,${Buffer.from(fixture).toString("base64")}` });
  startupMs = performance.now() - startupStarted;

  for (let iteration = 1; iteration <= iterations; iteration += 1) {
    for (const scenario of corpus.scenarios) {
      await client.tool("browser_evaluate", { function: "() => { window.resetFixture(); document.querySelector('#name').value = ''; return true; }" });
      const started = performance.now();
      let actual = null;
      let error = null;
      try {
        actual = await runScenario(client, scenario.id);
      } catch (caught) {
        error = String(caught?.message ?? caught);
      }
      const status = actual === scenario.expected ? "success" : scenario.forbidden.includes(actual) ? "wrong_action" : "failure";
      outcomes.push({ id: scenario.id, category: scenario.category, iteration, expected: scenario.expected,
        actual, status, error, latency_ms: performance.now() - started, cdp_requests: null });
    }
  }
} finally {
  await client.close();
  fs.rmSync(outputDir, { recursive: true, force: true });
}

const successes = count("success");
const failures = count("failure");
const wrongActions = count("wrong_action");
const unsupported = count("unsupported");
const report = {
  schema_version: 1,
  tool: { name: "playwright-mcp", version: expectedVersion },
  run: { corpus: corpus.corpus, corpus_fixture: corpus.fixture, iterations, temperature: "warm",
    profile: process.env.GLASS_SCORECARD_PROFILE ?? "fresh-ephemeral-single-session", viewport: { width: 1280, height: 720 } },
  environment: { os: process.platform, architecture: process.arch, rust: null,
    chrome: commandVersion(chromePath), machine: `${os.hostname()} ${os.release()}` },
  resources: {
    scope: "Runner RSS is the released MCP server process only; client and Chrome process-tree metrics are unavailable and reported as null",
    runner: { pid: client.pid, rss_start_bytes: null, rss_end_bytes: null, peak_rss_bytes: client.peakRss },
    chrome: { root_pid: null, rss_end_bytes: null, peak_process_tree_rss_bytes: null },
    binary_size_bytes: fs.statSync(process.execPath).size, compact_context_bytes: null,
    cdp_requests: null, startup_ms: startupMs,
  },
  summary: { successes, failures, wrong_actions: wrongActions, unsupported,
    task_success_rate: outcomes.length ? successes / outcomes.length : 0,
    hard_gate_passed: successes === outcomes.length },
  scenarios: outcomes,
};
process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);

async function runScenario(mcp, id) {
  switch (id) {
    case "duplicate-label":
      await mcp.tool("browser_click", { element: "exact Delete button", target: "#duplicate-right" });
      return result(mcp);
    case "overlay":
      await mcp.tool("browser_evaluate", { function: "() => { document.querySelector('#overlay').style.display='block'; return true; }" });
      try { await mcp.tool("browser_click", { element: "covered target", target: "#overlay-target" }); } catch {}
      return (await result(mcp)) === "idle" ? "blocked" : result(mcp);
    case "reflow":
      try { await mcp.tool("browser_click", { element: "moving target", target: "#moving" }); } catch {}
      return (await result(mcp)) === "idle" ? "blocked" : result(mcp);
    case "delayed-content":
      await mcp.tool("browser_evaluate", { function: "async () => { window.scheduleDelayed(); await new Promise(r => setTimeout(r, 200)); return true; }" });
      return evaluate(mcp, "() => document.querySelector('#delayed')?.textContent ?? 'missing'");
    case "spa-navigation":
      await mcp.tool("browser_click", { element: "SPA navigation", target: "#spa" });
      return result(mcp);
    case "form":
      await mcp.tool("browser_fill_form", { fields: [{ name: "Name", type: "textbox", target: "#name", value: "Glass" }] });
      await mcp.tool("browser_click", { element: "Submit", target: "#form button" });
      return result(mcp);
    case "popup":
      return runCode(mcp, "async (page) => { const opened = page.waitForEvent('popup'); await page.locator('#popup').click(); const popup = await opened; await popup.waitForLoadState(); await popup.close(); return 'popup-controlled'; }");
    case "frame":
      return runCode(mcp, "async (page) => { await page.frameLocator('#frame').locator('#frame-action').click(); return 'frame-clicked'; }");
    case "dialog":
      return runCode(mcp, "async (page) => { page.once('dialog', dialog => dialog.accept()); await page.locator('#dialog').click(); return await page.locator('#result').inputValue(); }");
    case "download":
      return runCode(mcp, "async (page) => { const event = page.waitForEvent('download'); await page.locator('#download').click(); const download = await event; await download.createReadStream(); return 'download-complete'; }");
    case "failure-recovery":
      return runCode(mcp, "async (page) => { try { await page.getByText('Definitely missing', { exact: true }).click({ timeout: 100 }); return 'unexpected-action'; } catch { await page.locator('#result').evaluate(node => node.value='recovered'); return 'recovered'; } }");
    default: throw new Error(`unknown scenario ${id}`);
  }
}

async function result(mcp) { return evaluate(mcp, "() => document.querySelector('#result').value"); }
async function evaluate(mcp, fn) { return parseResult(await mcp.tool("browser_evaluate", { function: fn })); }
async function runCode(mcp, code) { return parseResult(await mcp.tool("browser_run_code_unsafe", { code })); }
function parseResult(value) {
  const text = value.content?.find(({ type }) => type === "text")?.text;
  const match = text?.match(/^### Result\n([^\n]*)/m);
  if (!match) throw new Error("MCP tool response omitted a scalar result");
  return JSON.parse(match[1]);
}
function count(status) { return outcomes.filter((outcome) => outcome.status === status).length; }
function positiveInteger(name, value) { const parsed = Number(value); if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${name} must be a positive integer`); return parsed; }
function requiredEnv(name) { const value = process.env[name]; if (!value) throw new Error(`${name} is required`); return value; }
function commandVersion(command) { try { return execFileSync(command, ["--version"], { encoding: "utf8" }).trim(); } catch { return null; } }
function processRss(pid) {
  try { const match = fs.readFileSync(`/proc/${pid}/status`, "utf8").match(/^VmRSS:\s+(\d+)\s+kB$/m); return match ? Number(match[1]) * 1024 : null; } catch { return null; }
}

class McpClient {
  constructor(command, args) {
    this.child = spawn(command, args, { stdio: ["pipe", "pipe", "inherit"] });
    this.pid = this.child.pid;
    this.nextId = 0;
    this.pending = new Map();
    this.buffer = Buffer.alloc(0);
    this.failed = null;
    this.peakRss = processRss(this.pid);
    this.sampler = setInterval(() => { const rss = processRss(this.pid); if (rss !== null) this.peakRss = Math.max(this.peakRss ?? 0, rss); }, 10);
    this.child.stdout.on("data", (chunk) => this.onData(chunk));
    this.child.once("error", (error) => this.fail(error));
    this.child.once("exit", (code, signal) => this.fail(new Error(`MCP server exited (${code ?? signal})`)));
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
    try { message = JSON.parse(line); } catch { return this.fail(new Error("MCP server emitted malformed JSON")); }
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
        reject(new Error(`MCP ${method} exceeded ${requestTimeoutMs} ms`));
        this.child.kill("SIGKILL");
      }, requestTimeoutMs);
      this.pending.set(id, { resolve, reject, timer });
      this.child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
    });
  }
  fail(error) {
    if (this.failed) return;
    this.failed = error;
    for (const pending of this.pending.values()) { clearTimeout(pending.timer); pending.reject(error); }
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
    if (!this.child.killed) this.child.kill("SIGTERM");
    await Promise.race([new Promise((resolve) => this.child.once("exit", resolve)), new Promise((resolve) => setTimeout(resolve, 2000))]);
    if (this.child.exitCode === null) this.child.kill("SIGKILL");
  }
}
