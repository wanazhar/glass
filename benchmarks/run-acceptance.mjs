import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const contract = readJson(path.join(root, "benchmarks/acceptance-v1.json"));
const chromePath = requiredEnv("CHROME_PATH");
if (!fs.statSync(chromePath).isFile()) throw new Error(`CHROME_PATH is not a file: ${chromePath}`);
const iterations = positiveInteger("GLASS_SCORECARD_ITERATIONS", process.env.GLASS_SCORECARD_ITERATIONS ?? String(contract.iterations));
const outputDir = path.resolve(process.env.GLASS_ACCEPTANCE_OUTPUT_DIR ?? path.join(root, "benchmarks/results/compare-018"));
const rawDir = path.join(outputDir, "raw");
fs.mkdirSync(rawDir, { recursive: true });
const temp = fs.mkdtempSync(path.join(os.tmpdir(), "glass-acceptance-"));
const npmPrefix = path.join(temp, "npm");
const commands = [];
const adapters = [];

try {
  await run("cargo", ["build", "--release", "--locked"], { cwd: root, stderrFile: path.join(rawDir, "glass-build.stderr.log") });
  await run("npm", ["install", "--prefix", npmPrefix, "--no-save", "playwright@1.61.1", "@playwright/mcp@0.0.78"], {
    cwd: root,
    env: { PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD: "1" },
    stderrFile: path.join(rawDir, "npm-install.stderr.log"),
  });

  await runAdapter("glass", "cargo", ["run", "--release", "--locked", "--example", "scorecard"], {
    GLASS_BINARY_PATH: path.join(root, "target/release/glass"), CHROME_PATH: chromePath,
  });
  await runAdapter("playwright", process.execPath, [path.join(root, "benchmarks/adapters/playwright-scorecard.mjs")], {
    NODE_PATH: path.join(npmPrefix, "node_modules"), CHROME_PATH: chromePath,
  });
  await runAdapter("playwright-mcp", process.execPath, [path.join(root, "benchmarks/adapters/playwright-mcp-scorecard.mjs")], {
    CHROME_PATH: chromePath,
    PLAYWRIGHT_MCP_COMMAND: path.join(npmPrefix, "node_modules/.bin/playwright-mcp"),
    PLAYWRIGHT_MCP_VERSION: "0.0.78",
  });
} finally {
  fs.rmSync(temp, { recursive: true, force: true });
}

adapters.push({ id: "codex-browser", status: "unsupported", report: null,
  reason: contract.adapters.find(({ id }) => id === "codex-browser").unsupported_reason });

const runnable = adapters.filter(({ status }) => status === "completed");
const reports = runnable.map(({ report }) => readJson(path.join(outputDir, report)));
const controls = controlGates(reports);
const glassReport = reports.find(({ tool }) => tool.name === "glass");
const gates = {
  required_adapters_ran: contract.adapters.filter(({ required }) => required).every(({ id }) => adapters.some((row) => row.id === id && row.status === "completed")),
  controlled_environment: controls.ok,
  zero_wrong_actions: reports.length > 0 && reports.every(({ summary }) => summary.wrong_actions === 0),
  deterministic_task_success: reports.length > 0 && reports.every(({ summary }) => summary.hard_gate_passed),
  glass_runner_memory: budget(glassReport?.resources.runner.peak_rss_bytes, contract.glass_budgets.peak_runner_rss_bytes),
  glass_compact_context: budget(glassReport?.resources.compact_context_bytes, contract.glass_budgets.compact_context_bytes),
  glass_binary_size: budget(glassReport?.resources.binary_size_bytes, contract.glass_budgets.binary_size_bytes),
};
const bestInClassEligible = Object.values(gates).every(Boolean);
const environment = {
  schema_version: 1, generated_at: new Date().toISOString(), git_revision: await commandOutput("git", ["rev-parse", "HEAD"]),
  chrome_path: chromePath, chrome_version: await commandOutput(chromePath, ["--version"]),
  os: process.platform, architecture: process.arch, machine: `${os.hostname()} ${os.release()}`,
  node: process.version, rust: await commandOutput("rustc", ["--version"]), cargo: await commandOutput("cargo", ["--version"]),
  corpus: contract.corpus, iterations, viewport: contract.viewport, temperature: contract.temperature,
  adapter_versions: { glass: readCargoVersion(), playwright: "1.61.1", "playwright-mcp": "0.0.78", "codex-browser": null },
  commands,
};
writeJson(path.join(outputDir, "environment.json"), environment);
const acceptance = {
  schema_version: 1, contract: "benchmarks/acceptance-v1.json", environment: "environment.json",
  adapters, controls: controls.details, gates, best_in_class_eligible: bestInClassEligible,
  claim: bestInClassEligible ? "All declared hard gates passed; comparative leadership still requires interpreting the published efficiency evidence." : "Glass is not eligible for a best-in-class claim because one or more hard gates failed.",
};
writeJson(path.join(outputDir, "acceptance.json"), acceptance);
process.stdout.write(`${JSON.stringify(acceptance, null, 2)}\n`);
if (!bestInClassEligible && process.env.GLASS_ACCEPTANCE_ALLOW_FAILURE !== "1") process.exitCode = 2;

async function runAdapter(id, command, args, extraEnv) {
  const stdoutFile = path.join(rawDir, `${id}.json`);
  const stderrFile = path.join(rawDir, `${id}.stderr.log`);
  try {
    await run(command, args, { cwd: root, env: { ...extraEnv, GLASS_SCORECARD_ITERATIONS: String(iterations), GLASS_SCORECARD_PROFILE: "acceptance-warm-v1" }, stdoutFile, stderrFile });
    const report = readJson(stdoutFile);
    validateReport(report, id);
    adapters.push({ id, status: "completed", report: path.relative(outputDir, stdoutFile), reason: null });
  } catch (error) {
    adapters.push({ id, status: "failed", report: fs.existsSync(stdoutFile) ? path.relative(outputDir, stdoutFile) : null, reason: String(error.message ?? error) });
  }
}

async function run(command, args, options = {}) {
  commands.push({ command: path.basename(command), args: sanitizeArgs(args) });
  const stdout = [];
  const stderr = [];
  const child = spawn(command, args, { cwd: options.cwd, env: { ...process.env, ...options.env }, stdio: ["ignore", "pipe", "pipe"] });
  child.stdout.on("data", (chunk) => stdout.push(chunk));
  child.stderr.on("data", (chunk) => stderr.push(chunk));
  const code = await new Promise((resolve, reject) => { child.once("error", reject); child.once("close", resolve); });
  const out = Buffer.concat(stdout);
  const err = Buffer.concat(stderr);
  if (options.stdoutFile) fs.writeFileSync(options.stdoutFile, out);
  if (options.stderrFile) fs.writeFileSync(options.stderrFile, err);
  if (code !== 0) throw new Error(`${path.basename(command)} exited ${code}; see ${options.stderrFile ?? "stderr"}`);
  return out.toString("utf8");
}

function controlGates(reports) {
  const details = reports.map((report) => ({ tool: report.tool.name, corpus: report.run.corpus,
    iterations: report.run.iterations, temperature: report.run.temperature, viewport: report.run.viewport,
    chrome: report.environment.chrome }));
  const ok = reports.length === 3 && details.every((row) => row.corpus === contract.corpus && row.iterations === iterations &&
    row.temperature === contract.temperature && JSON.stringify(row.viewport) === JSON.stringify(contract.viewport) && row.chrome === details[0].chrome);
  return { ok, details };
}
function budget(value, maximum) { return Number.isFinite(value) && value <= maximum; }
function validateReport(report, id) {
  for (const key of ["schema_version", "tool", "run", "environment", "resources", "summary", "scenarios"]) if (!(key in report)) throw new Error(`${id} report missing ${key}`);
  if (report.schema_version !== 1 || report.run.corpus !== contract.corpus || report.run.iterations !== iterations) throw new Error(`${id} report violates acceptance controls`);
  if (report.scenarios.length !== iterations * 11) throw new Error(`${id} report has an incomplete scenario matrix`);
}
function sanitizeArgs(args) { return args.map((arg) => arg.startsWith(os.tmpdir()) ? `<temporary>/${path.basename(arg)}` : arg); }
function readCargoVersion() { return fs.readFileSync(path.join(root, "Cargo.toml"), "utf8").match(/^version\s*=\s*"([^"]+)"/m)?.[1] ?? null; }
function readJson(file) { return JSON.parse(fs.readFileSync(file, "utf8")); }
function writeJson(file, value) { fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`); }
function requiredEnv(name) { const value = process.env[name]; if (!value) throw new Error(`${name} is required`); return value; }
function positiveInteger(name, value) { const parsed = Number(value); if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${name} must be a positive integer`); return parsed; }
async function commandOutput(command, args) { try { return (await run(command, args, { cwd: root })).trim(); } catch { return null; } }
