import assert from "node:assert/strict";
import test from "node:test";
import { playwrightMcpProcessDeadlineMs } from "../acceptance-budget.mjs";

test("MCP process deadline scales with the controlled iteration count", () => {
  assert.equal(playwrightMcpProcessDeadlineMs(1), 150_000);
  assert.equal(playwrightMcpProcessDeadlineMs(100), 3_120_000);
});

test("MCP process deadline rejects invalid and overflowing iteration counts", () => {
  for (const iterations of [0, -1, 1.5, Number.NaN, Number.MAX_SAFE_INTEGER])
    assert.throws(() => playwrightMcpProcessDeadlineMs(iterations));
});
