import assert from "node:assert/strict";
import test from "node:test";
import { comparativeGates } from "../acceptance-gates.mjs";

const requiredAdapterIds = ["glass", "playwright", "playwright-mcp"];

test("competitor at ninety percent does not fail perfect Glass correctness", () => {
  const result = derive({ playwrightRate: 0.9, playwrightWrong: 10 });
  assert.equal(result.gates.glass_zero_wrong_actions, true);
  assert.equal(result.gates.glass_deterministic_task_success, true);
  assert.equal(result.gates.glass_task_success_not_trailing, true);
  assert.equal(result.gates.glass_declared_efficiency_win, true);
  assert.equal(result.gates.required_adapters_complete_exact_matrix, true);
  assert.equal(result.comparison.outcomes.playwright.task_success_rate, 0.9);
});

test("incomplete comparator fails required exact matrix", () => {
  const reports = baselineReports();
  reports.delete("playwright-mcp");
  const result = comparativeGates({ reports, adapterStatuses: statuses(), requiredAdapterIds });
  assert.equal(result.gates.required_adapters_complete_exact_matrix, false);
});

test("competitor wrong action is published without becoming a Glass safety failure", () => {
  const result = derive({ playwrightRate: 0.9, playwrightWrong: 1 });
  assert.equal(result.gates.glass_zero_wrong_actions, true);
  assert.equal(result.comparison.outcomes.playwright.wrong_actions, 1);
});

test("Glass trailing a comparator fails comparison", () => {
  const result = derive({ glassRate: 0.9, glassPerfect: false });
  assert.equal(result.gates.glass_task_success_not_trailing, false);
});

test("incomparable resource scopes cannot create an efficiency win", () => {
  const reports = baselineReports();
  reports.get("playwright").report.resources.scope = "complete Node and browser process tree";
  const result = comparativeGates({ reports, adapterStatuses: statuses(), requiredAdapterIds });
  assert.equal(result.gates.glass_declared_efficiency_win, false);
});

function derive(options = {}) {
  return comparativeGates({ reports: baselineReports(options), adapterStatuses: statuses(), requiredAdapterIds });
}
function statuses() { return new Map(requiredAdapterIds.map((id) => [id, "completed"])); }
function baselineReports({ glassRate = 1, glassPerfect = true, playwrightRate = 1, playwrightWrong = 0 } = {}) {
  return new Map([
    ["glass", row("glass", glassRate, glassPerfect, 0, 8_000_000, "Runner and owned Chrome process trees are disjoint; bytes are RSS")],
    ["playwright", row("playwright", playwrightRate, playwrightRate === 1, playwrightWrong, 100_000_000,
      "Runner RSS is Node only; Chrome process-tree RSS and raw CDP request count are unavailable through the public Playwright adapter and reported as null")],
    ["playwright-mcp", row("playwright-mcp", 0.9, false, 1, 80_000_000,
      "Runner RSS is the released MCP server process only; client and Chrome process-tree metrics are unavailable and reported as null")],
  ]);
}
function row(name, rate, perfect, wrongActions, rss, scope) {
  return { derived: { task_success_rate: rate, hard_gate_passed: perfect, wrong_actions: wrongActions,
      failures: perfect ? 0 : 1, unsupported: 0 },
    report: { tool: { name }, resources: { scope, runner: { peak_rss_bytes: rss } } } };
}
