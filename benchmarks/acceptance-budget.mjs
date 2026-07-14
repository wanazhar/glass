const MCP_PROCESS_HEADROOM_MS = 120_000;
const MCP_PROCESS_BUDGET_PER_ITERATION_MS = 30_000;

export function playwrightMcpProcessDeadlineMs(iterations) {
  if (!Number.isSafeInteger(iterations) || iterations <= 0)
    throw new Error("Playwright MCP process budget requires a positive safe iteration count");
  const deadline = MCP_PROCESS_HEADROOM_MS + iterations * MCP_PROCESS_BUDGET_PER_ITERATION_MS;
  if (!Number.isSafeInteger(deadline))
    throw new Error("Playwright MCP process budget exceeds the safe integer range");
  return deadline;
}
