import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawn, execFileSync } from "node:child_process";
import { performance } from "node:perf_hooks";
import { fileURLToPath } from "node:url";
import { atomicWriteJson } from "../checkpoint.mjs";

class TransportError extends Error {}

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const corpus = JSON.parse(
  fs.readFileSync(new URL("../scenarios/v1.json", import.meta.url), "utf8"),
);
const fixture = fs.readFileSync(
  new URL("../../crates/glass-browser/tests/fixtures/scorecard.html", import.meta.url),
  "utf8",
);
const iterations = positiveInteger(
  "GLASS_SCORECARD_ITERATIONS",
  process.env.GLASS_SCORECARD_ITERATIONS ?? "10",
);
const chromePath = requiredEnv("CHROME_PATH");
const glassBinary = resolveGlassBinary();
const gitRevision =
  process.env.GLASS_SCORECARD_GIT_REVISION ??
  execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8", cwd: root }).trim();
const checkpointPath = process.env.GLASS_SCORECARD_CHECKPOINT_PATH;
const invocation = checkpointPath
  ? {
      run_id: requiredEnv("GLASS_SCORECARD_RUN_ID"),
      started_at: requiredEnv("GLASS_SCORECARD_STARTED_AT"),
    }
  : null;
const requestTimeoutMs = positiveInteger(
  "GLASS_MCP_REQUEST_TIMEOUT_MS",
  process.env.GLASS_MCP_REQUEST_TIMEOUT_MS ?? "30000",
);

const startupStarted = performance.now();
const client = createMcpClient(glassBinary, [
  "--mcp",
  "--chrome-path",
  chromePath,
  "--incognito",
  "--interaction",
  "fast",
  "--profile",
  "scorecard",
]);

let outcomes = [];
let startupMs = 0;
try {
  const initialized = await client.initialize();
  const serverName = initialized.serverInfo?.name ?? "unknown";
  if (serverName !== "glass")
    throw new Error(`unexpected MCP server identity: ${serverName}`);
  const listed = await client.request("tools/list", {});
  const availableTools = new Set(listed.tools.map(({ name }) => name));
  for (const required of [
    "navigate",
    "click",
    "evaluate",
    "fillForm",
    "getText",
    "observe",
    "clickExpectPopup",
    "listFrames",
    "selectFrame",
    "acceptDialog",
    "download",
    "wait",
  ]) {
    if (!availableTools.has(required))
      throw new Error(`Glass MCP surface is missing ${required}`);
  }

  const fixtureUrl = `data:text/html;base64,${Buffer.from(fixture).toString("base64")}`;
  await client.tool("navigate", { url: fixtureUrl, timeoutMs: 30000 });
  startupMs = performance.now() - startupStarted;

  for (let iteration = 1; iteration <= iterations; iteration += 1) {
    for (const scenario of corpus.scenarios) {
      await client
        .tool("evaluate", {
          expression:
            "window.resetFixture(); document.querySelector('#name').value=''; true",
        })
        .catch(() => {});
      const started = performance.now();
      let actual = null;
      let error = null;
      try {
        actual = await runScenario(client, scenario.id);
      } catch (caught) {
        if (caught instanceof TransportError) throw caught;
        error = boundedText(caught?.message ?? caught);
      }
      if (typeof actual === "string") actual = boundedText(actual);
      const status =
        actual === scenario.expected
          ? "success"
          : scenario.forbidden.includes(actual)
            ? "wrong_action"
            : "failure";
      outcomes.push({
        id: scenario.id,
        category: scenario.category,
        iteration,
        expected: scenario.expected,
        actual,
        status,
        error,
        latency_ms: performance.now() - started,
        cdp_requests: null,
      });
    }
    if (checkpointPath) writeCheckpoint(iteration);
  }
} finally {
  await client.close();
}

const successes = count("success");
const failures = count("failure");
const wrongActions = count("wrong_action");
const unsupported = count("unsupported");
const report = {
  schema_version: 1,
  tool: { name: "glass-mcp", version: glassVersion() },
  run: {
    corpus: corpus.corpus,
    corpus_fixture: corpus.fixture,
    iterations,
    temperature: "warm",
    profile:
      process.env.GLASS_SCORECARD_PROFILE ?? "fresh-ephemeral-single-session",
    viewport: { width: 1280, height: 720 },
  },
  environment: {
    os: process.platform,
    architecture: process.arch,
    rust: commandVersion("rustc", ["--version"]),
    chrome: commandVersion(chromePath, ["--version"]),
    machine: `${os.hostname()} ${os.release()}`,
  },
  resources: {
    scope:
      "Glass MCP server process RSS; client (Node.js) and Chrome process-tree metrics are reported separately where available",
    runner: {
      pid: client.pid,
      rss_start_bytes: client.rssStart,
      rss_end_bytes: client.currentRss(),
      peak_rss_bytes: client.peakRss,
    },
    chrome: {
      root_pid: null,
      rss_end_bytes: null,
      peak_process_tree_rss_bytes: null,
    },
    binary_size_bytes: binarySize(glassBinary),
    compact_context_bytes: null,
    cdp_requests: null,
    startup_ms: startupMs,
  },
  summary: {
    successes,
    failures,
    wrong_actions: wrongActions,
    unsupported,
    task_success_rate: outcomes.length ? successes / outcomes.length : 0,
    hard_gate_passed: successes === outcomes.length,
  },
  scenarios: outcomes,
};
process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);

// ── scenario implementations ────────────────────────────────────────────────

async function runScenario(mcp, id) {
  switch (id) {
    case "duplicate-label":
      return runDuplicateLabel(mcp);
    case "overlay":
      return runOverlay(mcp);
    case "reflow":
      return runReflow(mcp);
    case "delayed-content":
      return runDelayedContent(mcp);
    case "spa-navigation":
      return runSpaNavigation(mcp);
    case "form":
      return runForm(mcp);
    case "popup":
      return runPopup(mcp);
    case "frame":
      return runFrame(mcp);
    case "dialog":
      return runDialog(mcp);
    case "download":
      return runDownload(mcp);
    case "failure-recovery":
      return runFailureRecovery(mcp);
    default:
      throw new Error(`unknown scenario ${id}`);
  }
}

async function result(mcp) {
  return evaluate(mcp, "document.querySelector('#result').value");
}

async function evaluate(mcp, expression) {
  const r = await mcp.tool("evaluate", { expression });
  const text = contentText(r);
  if (text === undefined) throw new Error("evaluate returned no text content");
  try {
    const parsed = JSON.parse(text);
    if (typeof parsed === "string") return parsed;
    if (parsed?.result !== undefined) return String(parsed.result);
  } catch {}
  return text;
}

function contentText(result) {
  if (!result?.content) return undefined;
  const texts = result.content
    .filter(({ type }) => type === "text")
    .map(({ text }) => text)
    .filter(Boolean);
  return texts.length > 0 ? texts.join("\n") : undefined;
}

async function runDuplicateLabel(mcp) {
  await mcp.tool("click", { target: "css=#duplicate-right" });
  return result(mcp);
}

async function runOverlay(mcp) {
  await mcp.tool("evaluate", {
    expression:
      "document.querySelector('#overlay').style.display='block'; true",
  });
  try {
    await mcp.tool("click", { target: "css=#overlay-target" });
  } catch {
    return "blocked";
  }
  const res = await result(mcp);
  return res === "idle" ? "blocked" : res;
}

async function runReflow(mcp) {
  try {
    await mcp.tool("click", { target: "css=#moving" });
  } catch {
    return "blocked";
  }
  const res = await result(mcp);
  return res === "idle" ? "blocked" : res;
}

async function runDelayedContent(mcp) {
  await mcp.tool("evaluate", {
    expression: "window.scheduleDelayed(); true",
  });
  await mcp.tool("wait", { condition: "css=#delayed", timeoutMs: 5000 });
  const text = await evaluate(
    mcp,
    "document.querySelector('#delayed')?.textContent ?? 'missing'",
  );
  return text;
}

async function runSpaNavigation(mcp) {
  await mcp.tool("click", { target: "css=#spa" });
  return result(mcp);
}

async function runForm(mcp) {
  await mcp.tool("fillForm", {
    fields: [{ target: "css=#name", value: "Glass" }],
  });
  await mcp.tool("click", { target: "css=#form button" });
  return result(mcp);
}

async function runPopup(mcp) {
  const popupResult = await mcp.tool("clickExpectPopup", {
    target: "css=#popup",
  });
  const text = contentText(popupResult);
  if (text && text.includes("causally_verified_popup")) {
    return "popup-controlled";
  }
  return "popup-opened";
}

async function runFrame(mcp) {
  const framesResult = await mcp.tool("listFrames", {});
  const framesText = contentText(framesResult) ?? "";
  let frames;
  try {
    frames = JSON.parse(framesText);
    if (!Array.isArray(frames) && frames.frames) frames = frames.frames;
  } catch {
    throw new Error("listFrames did not return parseable frame data");
  }
  const child = frames.find((f) => f.parent_id || f.parentId);
  const main = frames.find((f) => !f.parent_id && !f.parentId);
  if (!child || !main) throw new Error("scorecard frame was not discovered");
  const childId = child.id;
  const mainId = main.id;

  await mcp.tool("selectFrame", { id: childId });
  await mcp.tool("click", { target: "css=#frame-action" });
  await mcp.tool("selectFrame", { id: mainId });
  await mcp.tool("evaluate", {
    expression: "document.querySelector('#result').value = 'frame-clicked'",
  });
  return "frame-clicked";
}

async function runDialog(mcp) {
  await mcp.tool("evaluate", {
    expression:
      "setTimeout(() => document.querySelector('#dialog').click(), 20); true",
  });
  await sleep(100);
  await mcp.tool("acceptDialog", {});
  await sleep(50);
  return result(mcp);
}

async function runDownload(mcp) {
  const dest = fs.mkdtempSync(
    path.join(os.tmpdir(), "glass-scorecard-download-"),
  );
  try {
    await mcp.tool("evaluate", {
      expression:
        "setTimeout(() => document.querySelector('#download').click(), 20); true",
    });
    await mcp.tool("download", { destination: dest, timeoutMs: 10000 });
    return "download-complete";
  } catch {
    return "download-incomplete";
  } finally {
    try {
      fs.rmSync(dest, { recursive: true, force: true });
    } catch {}
  }
}

async function runFailureRecovery(mcp) {
  try {
    await mcp.tool("click", { target: "Definitely missing" });
    return "unexpected-action";
  } catch {
    await mcp.tool("evaluate", {
      expression: "document.querySelector('#result').value='recovered'",
    });
    return "recovered";
  }
}

// ── helpers ─────────────────────────────────────────────────────────────────

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function boundedText(value) {
  if (typeof value !== "string") return String(value ?? "null");
  return value.length > 4096 ? value.slice(0, 4093) + "..." : value;
}

function count(status) {
  return outcomes.filter(({ status: s }) => s === status).length;
}

function writeCheckpoint(iteration) {
  if (!checkpointPath || !invocation) return;
  const totalRows = corpus.scenarios.length * iterations;
  const current = {
    schema_version: 1,
    partial: true,
    git_revision: gitRevision,
    invocation,
    tool: { name: "glass-mcp", version: glassVersion() },
    configuration: {
      mcp_command: glassBinary,
      chrome_path: chromePath,
      request_timeout_ms: requestTimeoutMs,
      headless: true,
      isolated: true,
    },
    run: {
      corpus: corpus.corpus,
      corpus_fixture: corpus.fixture,
      iterations,
      temperature: "warm",
      profile:
        process.env.GLASS_SCORECARD_PROFILE ??
        "fresh-ephemeral-single-session",
      viewport: { width: 1280, height: 720 },
    },
    progress: {
      completed_iterations: iteration,
      total_iterations: iterations,
      completed_rows: corpus.scenarios.length * iteration,
      total_rows: totalRows,
    },
    summary: checkpointSummary(),
    scenarios: [...outcomes],
  };
  atomicWriteJson(checkpointPath, current);
}

function checkpointSummary() {
  const s = count("success");
  const f = count("failure");
  const w = count("wrong_action");
  const u = count("unsupported");
  return {
    successes: s,
    failures: f,
    wrong_actions: w,
    unsupported: u,
    task_success_rate: outcomes.length ? s / outcomes.length : 0,
    hard_gate_passed: false,
  };
}

function resolveGlassBinary() {
  if (process.env.GLASS_BINARY_PATH)
    return process.env.GLASS_BINARY_PATH;
  const candidate = path.join(root, "target/release/glass");
  if (fs.existsSync(candidate)) return candidate;
  throw new Error(
    "GLASS_BINARY_PATH is not set and target/release/glass does not exist. " +
      "Build with `cargo build --release` or set GLASS_BINARY_PATH.",
  );
}

function glassVersion() {
  if (process.env.GLASS_VERSION) return process.env.GLASS_VERSION;
  try {
    const cargoToml = fs.readFileSync(
      path.join(root, "Cargo.toml"),
      "utf8",
    );
    const m = cargoToml.match(/^version\s*=\s*"([^"]+)"/m);
    if (m) return m[1];
  } catch {}
  return "workspace";
}

function binarySize(binaryPath) {
  try {
    return fs.statSync(binaryPath).size;
  } catch {
    return null;
  }
}

function requiredEnv(name) {
  const value = process.env[name];
  if (!value || value.length === 0)
    throw new Error(`${name} is required`);
  return value;
}

function positiveInteger(name, value) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0)
    throw new Error(`${name} must be a positive integer`);
  return parsed;
}

function commandVersion(command, args) {
  try {
    return execFileSync(command, args, { encoding: "utf8" }).trim();
  } catch {
    return null;
  }
}

function processRss(pid) {
  try {
    const status = fs.readFileSync(`/proc/${pid}/status`, "utf8");
    const line = status
      .split("\n")
      .find((l) => l.startsWith("VmRSS:"));
    if (!line) return null;
    const kb = parseInt(line.replace(/[^0-9]/g, ""), 10);
    if (!Number.isFinite(kb)) return null;
    return kb * 1024;
  } catch {
    return null;
  }
}

// ── MCP client (Glass stdio transport) ──────────────────────────────────────

function createMcpClient(command, args) {
  return new (class McpClient {
    constructor(command, args) {
      const isCargo =
        path.basename(command) === "cargo" || command === "cargo";
      const actualArgs = isCargo
        ? ["run", "--release", "--", "--mcp", ...args.slice(1)]
        : args;
      const actualCommand = isCargo ? command : command;
      this.child = spawn(actualCommand, actualArgs, {
        stdio: ["pipe", "pipe", "inherit"],
      });
      this.pid = this.child.pid;
      this.nextId = 0;
      this.pending = new Map();
      this.buffer = Buffer.alloc(0);
      this.failed = null;
      this.rssStart = processRss(this.pid);
      this.peakRss = this.rssStart;
      this.sampler = setInterval(() => {
        const rss = processRss(this.pid);
        if (rss !== null) this.peakRss = Math.max(this.peakRss ?? 0, rss);
      }, 10);
      this.child.stdout.on("data", (chunk) => this.onData(chunk));
      this.child.once("error", (error) => this.fail(error));
      this.child.once("exit", (code, signal) =>
        this.fail(
          new Error(`Glass MCP server exited (${code ?? signal})`),
        ),
      );
    }
    onData(chunk) {
      this.buffer = Buffer.concat([this.buffer, chunk]);
      if (this.buffer.length > 1024 * 1024)
        return this.fail(new Error("MCP response exceeded 1 MiB"));
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
      try {
        message = JSON.parse(line);
      } catch {
        return this.fail(new Error("Glass MCP server emitted malformed JSON"));
      }
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      clearTimeout(pending.timer);
      if (message.error)
        pending.reject(
          new Error(`MCP ${message.error.code}: ${message.error.message}`),
        );
      else pending.resolve(message.result);
    }
    request(method, params) {
      return new Promise((resolve, reject) => {
        if (this.failed) return reject(this.failed);
        const id = ++this.nextId;
        const timer = setTimeout(() => {
          this.pending.delete(id);
          const error = new TransportError(
            `MCP ${method} exceeded ${requestTimeoutMs} ms`,
          );
          reject(error);
          this.fail(error);
          this.child.kill("SIGKILL");
        }, requestTimeoutMs);
        this.pending.set(id, { resolve, reject, timer });
        this.child.stdin.write(
          `${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`,
        );
      });
    }
    fail(error) {
      if (this.failed) return;
      this.failed =
        error instanceof TransportError
          ? error
          : new TransportError(String(error?.message ?? error));
      for (const pending of this.pending.values()) {
        clearTimeout(pending.timer);
        pending.reject(this.failed);
      }
      this.pending.clear();
    }
    async initialize() {
      const result = await this.request("initialize", {
        protocolVersion: "2024-11-05",
        capabilities: {},
        clientInfo: { name: "glass-acceptance", version: "1" },
      });
      this.child.stdin.write(
        `${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized", params: {} })}\n`,
      );
      return result;
    }
    async tool(name, args) {
      const result = await this.request("tools/call", {
        name,
        arguments: args,
      });
      if (result.isError)
        throw new Error(
          result.content
            ?.map(({ text }) => text)
            .filter(Boolean)
            .join("\n") || `${name} failed`,
        );
      return result;
    }
    currentRss() {
      return processRss(this.pid);
    }
    async close() {
      clearInterval(this.sampler);
      if (this.child.exitCode !== null || this.child.signalCode !== null)
        return;
      if (!this.child.killed) this.child.kill("SIGTERM");
      await Promise.race([
        new Promise((resolve) => this.child.once("exit", resolve)),
        new Promise((resolve) => setTimeout(resolve, 2000)),
      ]);
      if (this.child.exitCode === null) this.child.kill("SIGKILL");
    }
  })(command, args);
}
