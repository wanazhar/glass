export function comparativeGates({ reports, adapterStatuses, requiredAdapterIds }) {
  const glass = reports.get("glass");
  const comparators = requiredAdapterIds.filter((id) => id !== "glass").map((id) => reports.get(id));
  const requiredAdaptersCompleteExactMatrix = requiredAdapterIds.every((id) => adapterStatuses.get(id) === "completed" && reports.has(id));
  const glassRate = glass?.derived.task_success_rate;
  const taskSuccessNotTrailing = Number.isFinite(glassRate) && comparators.length > 0 && comparators.every((row) =>
    row && Number.isFinite(row.derived.task_success_rate) && glassRate >= row.derived.task_success_rate);
  const efficiency = declaredEfficiencyWin(glass, comparators);
  return {
    gates: {
      required_adapters_complete_exact_matrix: requiredAdaptersCompleteExactMatrix,
      glass_zero_wrong_actions: glass?.derived.wrong_actions === 0,
      glass_deterministic_task_success: glass?.derived.hard_gate_passed === true,
      glass_task_success_not_trailing: taskSuccessNotTrailing,
      glass_declared_efficiency_win: efficiency.passed,
    },
    comparison: { outcomes: Object.fromEntries([...reports].map(([id, row]) => [id, {
      task_success_rate: row.derived.task_success_rate, failures: row.derived.failures,
      wrong_actions: row.derived.wrong_actions, unsupported: row.derived.unsupported,
    }])), efficiency },
  };
}

const PRIMARY_NON_BROWSER_RUNNER_RSS_V1 = "primary-non-browser-runner-process-rss-v1";

function declaredEfficiencyWin(glass, comparators) {
  const metric = "peak_non_browser_runner_rss_bytes";
  const glassScope = comparableRunnerScope(glass?.report);
  const glassValue = glass?.report.resources.runner.peak_rss_bytes;
  const comparisons = comparators.filter(Boolean).map((row) => {
    const scope = comparableRunnerScope(row.report);
    const value = row.report.resources.runner.peak_rss_bytes;
    const comparable = glassScope !== null && scope === glassScope && positiveFinite(glassValue) && positiveFinite(value);
    return { comparator: row.report.tool.name, comparable, glass_value: glassValue ?? null, comparator_value: value ?? null,
      scope: comparable ? glassScope : null, glass_wins: comparable && glassValue < value };
  });
  return { passed: comparisons.some(({ glass_wins }) => glass_wins), metric, comparisons };
}

function comparableRunnerScope(report) {
  if (!report) return null;
  const scope = report.resources.scope;
  if ((report.tool.name === "glass" || report.tool.name === "playwright") && scope === PRIMARY_NON_BROWSER_RUNNER_RSS_V1)
    return PRIMARY_NON_BROWSER_RUNNER_RSS_V1;
  return null;
}
function positiveFinite(value) { return Number.isFinite(value) && value > 0; }
