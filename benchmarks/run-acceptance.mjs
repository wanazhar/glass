import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { playwrightMcpProcessDeadlineMs } from "./acceptance-budget.mjs";
import { prepareCheckpointInvocation, retainCheckpointOnTimeout } from "./checkpoint.mjs";
import { comparativeGates } from "./acceptance-gates.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const contract = readJson(path.join(root, "benchmarks/acceptance-v1.json"));
const corpus = readJson(path.join(root, "benchmarks/scenarios/v1.json"));
const outputDir = path.resolve(process.env.GLASS_ACCEPTANCE_OUTPUT_DIR ?? path.join(root, "benchmarks/results/compare-018"));
const rawDir = path.join(outputDir, "raw");
fs.mkdirSync(rawDir, { recursive: true });

let iterations = contract.iterations;
let commandDeadlineMs = 600000;
let playwrightMcpDeadlineMs = playwrightMcpProcessDeadlineMs(iterations);
let configurationError = null;
try {
  iterations = positiveInteger("GLASS_SCORECARD_ITERATIONS", process.env.GLASS_SCORECARD_ITERATIONS ?? String(contract.iterations));
  commandDeadlineMs = positiveInteger("GLASS_ACCEPTANCE_COMMAND_TIMEOUT_MS", process.env.GLASS_ACCEPTANCE_COMMAND_TIMEOUT_MS ?? "600000");
  playwrightMcpDeadlineMs = playwrightMcpProcessDeadlineMs(iterations);
} catch (error) {
  configurationError = String(error.message ?? error);
}
const commands = [];
const adapters = [];
const reports = new Map();
const temp = fs.mkdtempSync(path.join(os.tmpdir(), "glass-acceptance-"));
const npmPrefix = path.join(temp, "npm");
let chromePath = process.env.CHROME_PATH ?? null;
let fatalError = null;
let gitRevision = null;

try {
  if (configurationError) throw new Error(configurationError);
  gitRevision = await commandOutput("git", ["rev-parse", "HEAD"]);
  if (!chromePath) throw new Error("CHROME_PATH is required");
  if (!fs.statSync(chromePath).isFile()) throw new Error(`CHROME_PATH is not a file: ${chromePath}`);
  await run("cargo", ["build", "--release", "--locked"], { stderrFile: path.join(rawDir, "glass-build.stderr.log") });
  await run("npm", ["install", "--prefix", npmPrefix, "--no-save", "playwright@1.61.1", "@playwright/mcp@0.0.78"], {
    env: { PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD: "1" }, stderrFile: path.join(rawDir, "npm-install.stderr.log"),
  });

  await runAdapter("glass", "cargo", ["run", "--release", "--locked", "--example", "scorecard"], {
    GLASS_BINARY_PATH: path.join(root, "target/release/glass"), CHROME_PATH: chromePath,
  });
  await runAdapter("playwright", process.execPath, [path.join(root, "benchmarks/adapters/playwright-scorecard.mjs")], {
    NODE_PATH: path.join(npmPrefix, "node_modules"), CHROME_PATH: chromePath,
  });
  await runAdapter("playwright-mcp", process.execPath, [path.join(root, "benchmarks/adapters/playwright-mcp-scorecard.mjs")], {
    CHROME_PATH: chromePath, PLAYWRIGHT_MCP_COMMAND: path.join(npmPrefix, "node_modules/.bin/playwright-mcp"), PLAYWRIGHT_MCP_VERSION: "0.0.78",
    PLAYWRIGHT_MCP_REQUEST_TIMEOUT_MS: "30000",
  }, playwrightMcpDeadlineMs);

  // agent-browser: install and run if the adapter is listed in the contract
  const agentBrowserContract = contract.adapters.find(({ id }) => id === "agent-browser");
  const agentBrowserCmd = process.env.AGENT_BROWSER_COMMAND;
  if (agentBrowserContract && agentBrowserCmd) {
    const agentBrowserVersion = agentBrowserContract.version ?? "1.3.30";
    await runAdapter("agent-browser", process.execPath, [path.join(root, "benchmarks/adapters/agent-browser-scorecard.mjs")], {
      CHROME_PATH: chromePath,
      AGENT_BROWSER_COMMAND: agentBrowserCmd,
      AGENT_BROWSER_VERSION: agentBrowserVersion,
      AGENT_BROWSER_REQUEST_TIMEOUT_MS: "30000",
    }, commandDeadlineMs);
  } else if (agentBrowserContract) {
    adapters.push({ id: "agent-browser", status: "not_run", report: null,
      reason: "AGENT_BROWSER_COMMAND is not set; install agent-browser globally and set this env var." });
  }
} catch (error) {
  fatalError = String(error.message ?? error);
} finally {
  fs.rmSync(temp, { recursive: true, force: true });
}

for (const adapter of contract.adapters.filter(({ required }) => required)) {
  if (!adapters.some(({ id }) => id === adapter.id)) adapters.push({ id: adapter.id, status: "not_run", report: null, reason: fatalError ?? "A prerequisite failed." });
}
adapters.push({ id: "codex-browser", status: "unsupported", report: null,
  reason: contract.adapters.find(({ id }) => id === "codex-browser").unsupported_reason });

const prerequisites = {
  ratified_gates: retainEvidence("ratified_gates", "GLASS_RATIFIED_GATES_REPORT"),
  release_validation: retainEvidence("release_validation", "GLASS_RELEASE_VALIDATION_REPORT"),
  real_browser_platform_matrix: retainEvidence("real_browser_platform_matrix", "GLASS_PLATFORM_MATRIX_REPORT"),
};
const controls = controlGates([...reports.values()].map(({ report }) => report));
const glass = reports.get("glass");
const external = prerequisiteGates(prerequisites);
const requiredAdapterIds = contract.adapters.filter(({ required }) => required).map(({ id }) => id);
const comparative = comparativeGates({ reports,
  adapterStatuses: new Map(adapters.map(({ id, status }) => [id, status])), requiredAdapterIds });
const gates = {
  ...external,
  ...comparative.gates,
  controlled_comparison_environment: controls.ok,
  glass_peak_default_workflow_rss: budget(glass?.report?.resources?.runner?.peak_rss_bytes, contract.glass_budgets.peak_runner_rss_bytes),
  glass_compact_context: budget(glass?.report?.resources?.compact_context_bytes, contract.glass_budgets.compact_context_bytes),
  glass_release_binary_size: budget(glass?.report?.resources?.binary_size_bytes, contract.glass_budgets.binary_size_bytes),
};
const bestInClassEligible = Object.values(gates).every((passed) => passed === true);
const environment = {
  schema_version: 1, generated_at: new Date().toISOString(), git_revision: gitRevision,
  chrome_path: chromePath, chrome_version: chromePath ? await commandOutput(chromePath, ["--version"]) : null,
  os: process.platform, architecture: process.arch, machine: `${os.hostname()} ${os.release()}`,
  node: process.version, rust: await commandOutput("rustc", ["--version"]), cargo: await commandOutput("cargo", ["--version"]),
  corpus: contract.corpus, iterations, viewport: contract.viewport, profile_semantics: contract.profile_semantics,
  temperature: contract.temperature, adapter_versions: expectedTools(), commands, fatal_error: fatalError,
};
writeJson(path.join(outputDir, "environment.json"), environment);
const acceptance = {
  schema_version: 1, contract: "benchmarks/acceptance-v1.json", environment: "environment.json",
  adapters, prerequisites, controls: controls.details, comparison: comparative.comparison, gates, best_in_class_eligible: bestInClassEligible,
  claim: bestInClassEligible
    ? "All declared hard gates passed; comparative leadership still requires interpreting the published efficiency evidence."
    : "Glass is not eligible for a best-in-class claim because one or more hard gates failed or lacks revision-bound evidence.",
};
writeJson(path.join(outputDir, "acceptance.json"), acceptance);
process.stdout.write(`${JSON.stringify(acceptance, null, 2)}\n`);
if (!bestInClassEligible && process.env.GLASS_ACCEPTANCE_ALLOW_FAILURE !== "1") process.exitCode = 2;

async function runAdapter(id, command, args, extraEnv, deadlineMs = commandDeadlineMs) {
  const stdoutFile = path.join(rawDir, `${id}.json`);
  const stderrFile = path.join(rawDir, `${id}.stderr.log`);
  const checkpointFile = path.join(rawDir, `${id}.checkpoint.json`);
  const invocation = prepareCheckpointInvocation(checkpointFile);
  try {
    await run(command, args, { deadlineMs, env: { ...extraEnv, GLASS_SCORECARD_ITERATIONS: String(iterations),
      GLASS_SCORECARD_PROFILE: contract.profile_semantics, GLASS_SCORECARD_GIT_REVISION: gitRevision,
      GLASS_SCORECARD_CHECKPOINT_PATH: checkpointFile, GLASS_SCORECARD_RUN_ID: invocation.run_id,
      GLASS_SCORECARD_STARTED_AT: invocation.started_at }, stdoutFile, stderrFile });
    const report = readJson(stdoutFile);
    const derived = validateReport(report, id);
    reports.set(id, { report, derived });
    fs.rmSync(checkpointFile, { force: true });
    adapters.push({ id, status: "completed", report: path.relative(outputDir, stdoutFile), reason: null });
  } catch (error) {
    let checkpoint = null;
    let checkpointReason = null;
    if (error.timedOut && fs.existsSync(checkpointFile)) {
      try {
        const retained = retainCheckpointOnTimeout({ timedOut: error.timedOut, file: checkpointFile,
          expected: checkpointExpectation(id, extraEnv, invocation) });
        checkpoint = path.relative(outputDir, retained.file);
      } catch (checkpointError) {
        checkpoint = path.relative(outputDir, checkpointFile);
        checkpointReason = `; invalid partial checkpoint: ${String(checkpointError.message ?? checkpointError)}`;
      }
    }
    if (!error.timedOut) fs.rmSync(checkpointFile, { force: true });
    adapters.push({ id, status: "failed", report: fs.existsSync(stdoutFile) ? path.relative(outputDir, stdoutFile) : null,
      checkpoint, reason: `${String(error.message ?? error)}${checkpointReason ?? ""}` });
  }
}

async function run(command, args, options = {}) {
  const deadlineMs = options.deadlineMs ?? commandDeadlineMs;
  commands.push({ command: path.basename(command), args: sanitizeArgs(args), timeout_ms: deadlineMs });
  const child = spawn(command, args, { cwd: root, env: { ...process.env, ...options.env }, detached: process.platform !== "win32", stdio: ["ignore", "pipe", "pipe"] });
  const stdout = boundedSink(child.stdout, options.stdoutFile, 16 * 1024 * 1024);
  const stderr = boundedSink(child.stderr, options.stderrFile, 8 * 1024 * 1024);
  let timedOut = false;
  const timer = setTimeout(() => { timedOut = true; terminate(child); }, deadlineMs);
  const code = await new Promise((resolve, reject) => { child.once("error", reject); child.once("close", resolve); });
  clearTimeout(timer);
  const [out, err] = await Promise.all([stdout.done, stderr.done]);
  if (timedOut) {
    const error = new Error(`${path.basename(command)} exceeded ${deadlineMs} ms`);
    error.timedOut = true;
    throw error;
  }
  if (out.truncated || err.truncated) throw new Error(`${path.basename(command)} output exceeded its capture budget`);
  if (code !== 0) throw new Error(`${path.basename(command)} exited ${code}; see ${options.stderrFile ?? "captured stderr"}`);
  return out.text;
}

function boundedSink(stream, file, limit) {
  const writer = file ? fs.createWriteStream(file) : null;
  const chunks = [];
  let bytes = 0;
  let truncated = false;
  stream.on("data", (chunk) => {
    const remaining = Math.max(0, limit - bytes);
    const kept = chunk.subarray(0, remaining);
    bytes += kept.length;
    if (kept.length < chunk.length) truncated = true;
    if (writer && kept.length) writer.write(kept);
    else if (!writer && kept.length) chunks.push(kept);
  });
  const done = new Promise((resolve) => stream.on("end", () => {
    if (writer) writer.end(() => resolve({ text: "", truncated }));
    else resolve({ text: Buffer.concat(chunks).toString("utf8"), truncated });
  }));
  return { done };
}

function terminate(child) {
  try { if (process.platform === "win32") child.kill("SIGKILL"); else process.kill(-child.pid, "SIGKILL"); } catch {}
}

function validateReport(report, id) {
  exactKeys(report, ["schema_version", "tool", "run", "environment", "resources", "summary", "scenarios"], `${id} report`);
  if (report.schema_version !== 1 || !Array.isArray(report.scenarios)) throw new Error(`${id} report has an invalid schema`);
  exactKeys(report.tool, ["name", "version"], `${id} tool`);
  exactKeys(report.run, ["corpus", "corpus_fixture", "iterations", "temperature", "profile", "viewport"], `${id} run`);
  exactKeys(report.environment, ["os", "architecture", "rust", "chrome", "machine"], `${id} environment`);
  exactKeys(report.resources, ["scope", "runner", "chrome", "binary_size_bytes", "compact_context_bytes", "cdp_requests", "startup_ms"], `${id} resources`);
  exactKeys(report.resources.runner, ["pid", "rss_start_bytes", "rss_end_bytes", "peak_rss_bytes"], `${id} runner resources`);
  exactKeys(report.resources.chrome, ["root_pid", "rss_end_bytes", "peak_process_tree_rss_bytes"], `${id} Chrome resources`);
  exactKeys(report.summary, ["successes", "failures", "wrong_actions", "unsupported", "task_success_rate", "hard_gate_passed"], `${id} summary`);
  const expectedTool = expectedTools()[id];
  if (report.tool?.name !== id || report.tool?.version !== expectedTool) throw new Error(`${id} reported an unexpected tool identity/version`);
  if (!nonEmpty(report.resources.scope) || !positiveIntegerValue(report.resources.runner.pid) ||
      !nullableNonNegativeInteger(report.resources.runner.rss_start_bytes) || !nullableNonNegativeInteger(report.resources.runner.rss_end_bytes) ||
      !nullableNonNegativeInteger(report.resources.runner.peak_rss_bytes) || !nullablePositiveInteger(report.resources.chrome.root_pid) ||
      !nullableNonNegativeInteger(report.resources.chrome.rss_end_bytes) || !nullableNonNegativeInteger(report.resources.chrome.peak_process_tree_rss_bytes) ||
      !nullableNonNegativeInteger(report.resources.binary_size_bytes) || !nullableNonNegativeInteger(report.resources.compact_context_bytes) ||
      !nullableNonNegativeInteger(report.resources.cdp_requests) || !Number.isFinite(report.resources.startup_ms) || report.resources.startup_ms < 0) {
    throw new Error(`${id} contains invalid resource metrics`);
  }
  if (report.run?.corpus !== contract.corpus || report.run?.corpus_fixture !== corpus.fixture || report.run?.iterations !== iterations ||
      report.run?.temperature !== contract.temperature || report.run?.profile !== contract.profile_semantics ||
      !sameViewport(report.run?.viewport, contract.viewport)) throw new Error(`${id} report violates controlled run metadata`);
  const seen = new Set();
  const counts = { success: 0, failure: 0, wrong_action: 0, unsupported: 0 };
  const definitions = new Map(corpus.scenarios.map((scenario) => [scenario.id, scenario]));
  for (const row of report.scenarios) {
    exactKeys(row, ["id", "category", "iteration", "expected", "actual", "status", "error", "latency_ms", "cdp_requests"], `${id} scenario`);
    const definition = definitions.get(row.id);
    if (!definition || row.category !== definition.category || row.expected !== definition.expected || !Number.isInteger(row.iteration) || row.iteration < 1 || row.iteration > iterations) throw new Error(`${id} contains invalid scenario metadata`);
    const key = `${row.id}:${row.iteration}`;
    if (seen.has(key)) throw new Error(`${id} duplicates ${key}`);
    seen.add(key);
    if (!Object.hasOwn(counts, row.status) || typeof row.latency_ms !== "number" || row.latency_ms < 0 ||
        !(row.actual === null || typeof row.actual === "string") || !(row.error === null || typeof row.error === "string") ||
        !(row.cdp_requests === null || Number.isInteger(row.cdp_requests) && row.cdp_requests >= 0)) throw new Error(`${id} contains an invalid outcome`);
    const classified = row.actual === definition.expected ? "success" : definition.forbidden.includes(row.actual) ? "wrong_action" : row.status === "unsupported" && row.actual === null && typeof row.error === "string" ? "unsupported" : "failure";
    if (row.status !== classified) throw new Error(`${id} misclassified ${key}`);
    counts[classified]++;
  }
  if (seen.size !== definitions.size * iterations) throw new Error(`${id} has an incomplete scenario matrix`);
  const total = seen.size;
  const derived = { successes: counts.success, failures: counts.failure, wrong_actions: counts.wrong_action,
    unsupported: counts.unsupported, task_success_rate: counts.success / total, hard_gate_passed: counts.success === total };
  for (const [key, value] of Object.entries(derived)) if (report.summary?.[key] !== value) throw new Error(`${id} summary disagrees with raw scenarios for ${key}`);
  return derived;
}

function checkpointExpectation(id, extraEnv, invocation) {
  return { id, version: expectedTools()[id], gitRevision, invocation, configuration: {
      mcp_command: extraEnv.PLAYWRIGHT_MCP_COMMAND, chrome_path: extraEnv.CHROME_PATH,
      request_timeout_ms: Number(extraEnv.PLAYWRIGHT_MCP_REQUEST_TIMEOUT_MS),
      headless: true, isolated: true },
    run: { corpus: contract.corpus, corpus_fixture: corpus.fixture, iterations, temperature: contract.temperature,
      profile: contract.profile_semantics, viewport: contract.viewport }, scenarios: corpus.scenarios };
}

function controlGates(rows) {
  const details = rows.map((report) => ({ tool: report.tool.name, corpus: report.run.corpus, iterations: report.run.iterations,
    temperature: report.run.temperature, profile: report.run.profile, viewport: report.run.viewport, chrome: report.environment.chrome }));
  const chrome = details[0]?.chrome;
  const ok = details.length === 3 && typeof chrome === "string" && details.every((row) => row.corpus === contract.corpus && row.iterations === iterations &&
    row.temperature === contract.temperature && row.profile === contract.profile_semantics && sameViewport(row.viewport, contract.viewport) && row.chrome === chrome);
  return { ok, details };
}

function retainEvidence(id, variable) {
  const source = process.env[variable];
  if (!source) return { status: "missing", passed: false, report: null, reason: `Set ${variable} to revision-bound evidence.` };
  try {
    const evidence = readJson(source);
    if (evidence.schema_version !== 1 || evidence.git_revision !== gitRevision) throw new Error("Evidence must use schema 1 and match the tested git revision.");
    const derived = validateEvidence(id, evidence);
    const destination = path.join(rawDir, `${id}.json`);
    fs.copyFileSync(source, destination);
    return { status: "completed", passed: derived.passed, report: path.relative(outputDir, destination), reason: derived.passed ? null : "Validated evidence reports a failed check.", derived };
  } catch (error) {
    return { status: "invalid", passed: false, report: null, reason: String(error.message ?? error) };
  }
}

function prerequisiteGates(evidence) {
  const metrics = evidence.ratified_gates.derived?.metrics ?? {};
  const limits = contract.ratified_gates;
  return {
    representative_task_success: evidence.ratified_gates.passed && atLeast(metrics.representative_task_success_rate, limits.representative_task_success_rate_min),
    fresh_compact_observe_latency: evidence.ratified_gates.passed && budget(metrics.fresh_compact_observe_p95_ms, limits.fresh_compact_observe_p95_ms_max),
    cached_compact_observe_latency: evidence.ratified_gates.passed && budget(metrics.cached_compact_observe_p95_ms, limits.cached_compact_observe_p95_ms_max),
    fast_action_client_overhead: evidence.ratified_gates.passed && budget(metrics.fast_action_client_overhead_p95_ms, limits.fast_action_client_overhead_p95_ms_max),
    idle_glass_rss: evidence.ratified_gates.passed && budget(metrics.idle_glass_rss_bytes, limits.idle_glass_rss_bytes_max),
    mcp_malformed_input_survival: evidence.ratified_gates.passed && atLeast(metrics.mcp_malformed_input_survival_rate, limits.mcp_malformed_input_survival_rate_min),
    release_validation: evidence.release_validation.passed,
    real_browser_platform_matrix: evidence.real_browser_platform_matrix.passed,
  };
}

function validateEvidence(id, evidence) {
  validateProducer(evidence.producer, id);
  if (evidence.type !== id) throw new Error(`${id} evidence has the wrong type`);
  if (id === "ratified_gates") {
    exactKeys(evidence, ["schema_version", "type", "git_revision", "producer", "passed", "metrics", "raw_reports"], id);
    exactKeys(evidence.metrics, ["representative_task_success_rate", "fresh_compact_observe_p95_ms", "cached_compact_observe_p95_ms", "fast_action_client_overhead_p95_ms", "idle_glass_rss_bytes", "mcp_malformed_input_survival_rate"], `${id} metrics`);
    exactKeys(evidence.raw_reports, Object.keys(evidence.metrics), `${id} raw reports`);
    if (typeof evidence.passed !== "boolean" || Object.values(evidence.metrics).some((value) => !Number.isFinite(value) || value < 0) || Object.values(evidence.raw_reports).some((value) => !nonEmpty(value))) throw new Error(`${id} evidence has invalid metrics or raw references`);
    return { passed: evidence.passed, metrics: evidence.metrics };
  }
  if (id === "release_validation") {
    exactKeys(evidence, ["schema_version", "type", "git_revision", "producer", "checks"], id);
    return validateRows(evidence.checks, contract.release_checks, "check", id);
  }
  if (id === "real_browser_platform_matrix") {
    exactKeys(evidence, ["schema_version", "type", "git_revision", "producer", "platforms"], id);
    return validateRows(evidence.platforms, contract.platform_targets, "platform", id);
  }
  throw new Error(`unknown evidence type ${id}`);
}

function validateProducer(producer, label) {
  exactKeys(producer, ["name", "version", "command", "run_url"], `${label} producer`);
  if (![producer.name, producer.version, producer.command, producer.run_url].every(nonEmpty)) throw new Error(`${label} producer is not auditable`);
}

function validateRows(rows, expectedIds, kind, label) {
  if (!Array.isArray(rows) || rows.length !== expectedIds.length) throw new Error(`${label} has an incomplete ${kind} matrix`);
  const ids = new Set();
  for (const row of rows) {
    if (kind === "check") {
      exactKeys(row, ["id", "status", "raw_report"], `${label} check`);
      ids.add(row.id);
      if (row.status !== "passed" || !nonEmpty(row.raw_report)) throw new Error(`${label} contains a failed or unauditable check`);
    } else {
      exactKeys(row, ["target", "os", "architecture", "chrome", "status", "raw_report"], `${label} platform`);
      ids.add(row.target);
      if (![row.os, row.architecture, row.chrome, row.raw_report].every(nonEmpty) || row.status !== "passed") throw new Error(`${label} contains a failed or unauditable platform`);
    }
  }
  if (ids.size !== expectedIds.length || expectedIds.some((id) => !ids.has(id))) throw new Error(`${label} has duplicate or unexpected ${kind} rows`);
  return { passed: true };
}

function expectedTools() { return { glass: readCargoVersion(), playwright: "1.61.1", "playwright-mcp": "0.0.78", "agent-browser": "1.3.30", "codex-browser": null }; }
function exactKeys(value, expected, label) { if (!value || typeof value !== "object" || JSON.stringify(Object.keys(value).sort()) !== JSON.stringify([...expected].sort())) throw new Error(`${label} has unexpected or missing keys`); }
function budget(value, maximum) { return Number.isFinite(value) && value <= maximum; }
function atLeast(value, minimum) { return Number.isFinite(value) && value >= minimum; }
function nonEmpty(value) { return typeof value === "string" && value.length > 0; }
function positiveIntegerValue(value) { return Number.isInteger(value) && value >= 1; }
function nullablePositiveInteger(value) { return value === null || positiveIntegerValue(value); }
function nullableNonNegativeInteger(value) { return value === null || Number.isInteger(value) && value >= 0; }
function sameViewport(actual, expected) { return actual?.width === expected.width && actual?.height === expected.height; }
function sanitizeArgs(args) { return args.map((arg) => arg.startsWith(os.tmpdir()) ? `<temporary>/${path.basename(arg)}` : arg); }
function readCargoVersion() { return fs.readFileSync(path.join(root, "Cargo.toml"), "utf8").match(/^version\s*=\s*"([^"]+)"/m)?.[1] ?? null; }
function readJson(file) { return JSON.parse(fs.readFileSync(file, "utf8")); }
function writeJson(file, value) { fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`); }
function positiveInteger(name, value) { const parsed = Number(value); if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${name} must be a positive integer`); return parsed; }
async function commandOutput(command, args) { try { return (await run(command, args)).trim(); } catch { return null; } }
