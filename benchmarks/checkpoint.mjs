import fs from "node:fs";
import path from "node:path";

export function atomicWriteJson(file, value, maximumBytes = 8 * 1024 * 1024) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const temporary = `${file}.tmp-${process.pid}-${Date.now()}`;
  try {
    const payload = `${JSON.stringify(value, null, 2)}\n`;
    if (Buffer.byteLength(payload) > maximumBytes) throw new Error(`checkpoint exceeds ${maximumBytes} bytes`);
    const fd = fs.openSync(temporary, "wx", 0o600);
    try {
      fs.writeFileSync(fd, payload);
      fs.fsyncSync(fd);
    } finally { fs.closeSync(fd); }
    fs.renameSync(temporary, file);
  } finally { fs.rmSync(temporary, { force: true }); }
}

export function validatePartialCheckpoint(checkpoint, expected) {
  exactKeys(checkpoint, ["schema_version", "partial", "git_revision", "tool", "configuration", "run", "progress", "summary", "scenarios"], `${expected.id} checkpoint`);
  if (checkpoint.schema_version !== 1 || checkpoint.partial !== true || checkpoint.git_revision !== expected.gitRevision || !Array.isArray(checkpoint.scenarios))
    throw new Error(`${expected.id} checkpoint has invalid identity metadata`);
  exactKeys(checkpoint.tool, ["name", "version"], `${expected.id} checkpoint tool`);
  exactKeys(checkpoint.configuration, ["mcp_command", "chrome_path", "request_timeout_ms", "headless", "isolated"], `${expected.id} checkpoint configuration`);
  exactKeys(checkpoint.run, ["corpus", "corpus_fixture", "iterations", "temperature", "profile", "viewport"], `${expected.id} checkpoint run`);
  exactKeys(checkpoint.progress, ["completed_iterations", "total_iterations", "completed_rows", "total_rows"], `${expected.id} checkpoint progress`);
  exactKeys(checkpoint.summary, ["successes", "failures", "wrong_actions", "unsupported", "task_success_rate", "hard_gate_passed"], `${expected.id} checkpoint summary`);
  if (checkpoint.tool.name !== expected.id || checkpoint.tool.version !== expected.version ||
      !sameConfiguration(checkpoint.configuration, expected.configuration) || !sameRun(checkpoint.run, expected.run))
    throw new Error(`${expected.id} checkpoint violates controlled run metadata`);
  const completed = checkpoint.progress.completed_iterations;
  const totalRows = expected.scenarios.length * expected.run.iterations;
  if (!Number.isInteger(completed) || completed < 1 || completed > expected.run.iterations ||
      checkpoint.progress.total_iterations !== expected.run.iterations || checkpoint.progress.completed_rows !== expected.scenarios.length * completed ||
      checkpoint.progress.total_rows !== totalRows || checkpoint.scenarios.length !== checkpoint.progress.completed_rows)
    throw new Error(`${expected.id} checkpoint has inconsistent progress`);
  const definitions = new Map(expected.scenarios.map((scenario) => [scenario.id, scenario]));
  const seen = new Set();
  const counts = { success: 0, failure: 0, wrong_action: 0, unsupported: 0 };
  for (const row of checkpoint.scenarios) {
    exactKeys(row, ["id", "category", "iteration", "expected", "actual", "status", "error", "latency_ms", "cdp_requests"], `${expected.id} checkpoint scenario`);
    const definition = definitions.get(row.id);
    const key = `${row.id}:${row.iteration}`;
    if (!definition || row.category !== definition.category || row.expected !== definition.expected || !Number.isInteger(row.iteration) ||
        row.iteration < 1 || row.iteration > completed || seen.has(key) || !Object.hasOwn(counts, row.status) ||
        !Number.isFinite(row.latency_ms) || row.latency_ms < 0 || !(row.actual === null || typeof row.actual === "string") ||
        !(row.error === null || typeof row.error === "string" && Buffer.byteLength(row.error) <= 4096) ||
        !(row.actual === null || typeof row.actual === "string" && Buffer.byteLength(row.actual) <= 4096) ||
        !(row.cdp_requests === null || Number.isInteger(row.cdp_requests) && row.cdp_requests >= 0))
      throw new Error(`${expected.id} checkpoint contains an invalid outcome`);
    const classified = row.actual === definition.expected ? "success" : definition.forbidden.includes(row.actual) ? "wrong_action" :
      row.status === "unsupported" && row.actual === null && typeof row.error === "string" ? "unsupported" : "failure";
    if (row.status !== classified) throw new Error(`${expected.id} checkpoint misclassified ${key}`);
    seen.add(key);
    counts[classified]++;
  }
  const total = checkpoint.scenarios.length;
  const summary = { successes: counts.success, failures: counts.failure, wrong_actions: counts.wrong_action,
    unsupported: counts.unsupported, task_success_rate: counts.success / total, hard_gate_passed: false };
  for (const [key, value] of Object.entries(summary)) if (checkpoint.summary[key] !== value) throw new Error(`${expected.id} checkpoint summary disagrees for ${key}`);
  return { completed_iterations: completed, completed_rows: total, summary };
}

export function retainCheckpointOnTimeout({ timedOut, file, expected }) {
  if (!timedOut || !fs.existsSync(file)) return null;
  const checkpoint = JSON.parse(fs.readFileSync(file, "utf8"));
  return { file, derived: validatePartialCheckpoint(checkpoint, expected) };
}

function sameRun(actual, expected) {
  return actual.corpus === expected.corpus && actual.corpus_fixture === expected.corpus_fixture && actual.iterations === expected.iterations &&
    actual.temperature === expected.temperature && actual.profile === expected.profile && actual.viewport?.width === expected.viewport.width &&
    actual.viewport?.height === expected.viewport.height;
}
function sameConfiguration(actual, expected) {
  return actual.mcp_command === expected.mcp_command && actual.chrome_path === expected.chrome_path &&
    actual.request_timeout_ms === expected.request_timeout_ms && actual.headless === expected.headless && actual.isolated === expected.isolated;
}
function exactKeys(value, expected, label) {
  if (!value || typeof value !== "object" || JSON.stringify(Object.keys(value).sort()) !== JSON.stringify([...expected].sort()))
    throw new Error(`${label} has unexpected or missing keys`);
}
