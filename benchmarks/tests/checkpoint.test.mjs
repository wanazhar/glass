import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { atomicWriteJson, retainCheckpointOnTimeout, validatePartialCheckpoint } from "../checkpoint.mjs";

const scenarios = [
  { id: "one", category: "targeting", expected: "ok", forbidden: ["wrong"] },
  { id: "two", category: "recovery", expected: "recovered", forbidden: [] },
];
const run = { corpus: "fixture-v1", corpus_fixture: "fixture.html", iterations: 3, temperature: "warm",
  profile: "fresh-ephemeral-single-session", viewport: { width: 1280, height: 720 } };
const configuration = { mcp_command: "/tmp/playwright-mcp", chrome_path: "/tmp/chrome", request_timeout_ms: 30000, headless: true, isolated: true };
const expected = { id: "playwright-mcp", version: "0.0.78", gitRevision: "abc123", configuration, run, scenarios };

test("atomic checkpoint survives a simulated runner timeout and stays ineligible", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "glass-checkpoint-test-"));
  try {
    const file = path.join(directory, "raw", "playwright-mcp.checkpoint.json");
    atomicWriteJson(file, checkpoint());
    const retained = JSON.parse(fs.readFileSync(file, "utf8"));
    const derived = validatePartialCheckpoint(retained, expected);
    assert.equal(derived.completed_iterations, 1);
    assert.equal(derived.completed_rows, 2);
    assert.equal(derived.summary.hard_gate_passed, false);
    assert.deepEqual(fs.readdirSync(path.dirname(file)), ["playwright-mcp.checkpoint.json"]);
    assert.equal(retainCheckpointOnTimeout({ timedOut: false, file, expected }), null);
    assert.equal(retainCheckpointOnTimeout({ timedOut: true, file, expected }).derived.summary.hard_gate_passed, false);
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});

test("partial validation rejects revision drift and fabricated passing gates", () => {
  const wrongRevision = checkpoint();
  wrongRevision.git_revision = "different";
  assert.throws(() => validatePartialCheckpoint(wrongRevision, expected), /identity metadata/);
  const fabricatedPass = checkpoint();
  fabricatedPass.summary.hard_gate_passed = true;
  assert.throws(() => validatePartialCheckpoint(fabricatedPass, expected), /summary disagrees/);
});

test("atomic publication rejects oversized checkpoints without residue", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "glass-checkpoint-limit-"));
  try {
    const file = path.join(directory, "checkpoint.json");
    assert.throws(() => atomicWriteJson(file, { payload: "x".repeat(100) }, 32), /exceeds 32 bytes/);
    assert.deepEqual(fs.readdirSync(directory), []);
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});

function checkpoint() {
  const rows = [outcome("one", "targeting", "ok"), outcome("two", "recovery", "recovered")];
  return { schema_version: 1, partial: true, git_revision: "abc123", tool: { name: "playwright-mcp", version: "0.0.78" }, configuration, run,
    progress: { completed_iterations: 1, total_iterations: 3, completed_rows: 2, total_rows: 6 },
    summary: { successes: 2, failures: 0, wrong_actions: 0, unsupported: 0, task_success_rate: 1, hard_gate_passed: false }, scenarios: rows };
}
function outcome(id, category, actual) {
  return { id, category, iteration: 1, expected: actual, actual, status: "success", error: null, latency_ms: 1, cdp_requests: null };
}
