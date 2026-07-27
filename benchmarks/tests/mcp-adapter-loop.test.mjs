import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const adapter = path.resolve("benchmarks/adapters/playwright-mcp-scorecard.mjs");

test("scenario tool errors are recorded through every requested iteration", () => {
  const fixture = setupFakeServer(false);
  try {
    const result = runAdapter(fixture, 2);
    assert.equal(result.status, 0, result.stderr);
    const report = JSON.parse(result.stdout);
    assert.equal(report.scenarios.length, 22);
    assert.equal(report.summary.failures, 16);
    assert.equal(report.summary.unsupported, 6);
    assert.equal(report.scenarios.filter(({ status }) => status === "failure")
      .every(({ error }) => error.includes("fixture tool error")), true);
    assert.equal(JSON.parse(fs.readFileSync(fixture.checkpoint, "utf8")).progress.completed_iterations, 2);
  } finally { fs.rmSync(fixture.directory, { recursive: true, force: true }); }
});

test("transport loss aborts instead of becoming a scenario row", () => {
  const fixture = setupFakeServer(true);
  try {
    const result = runAdapter(fixture, 2);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /MCP server exited/);
    assert.equal(result.stdout, "");
    assert.equal(fs.existsSync(fixture.checkpoint), false);
  } finally { fs.rmSync(fixture.directory, { recursive: true, force: true }); }
});

function runAdapter(fixture, iterations) {
  return spawnSync(process.execPath, [adapter], { encoding: "utf8", timeout: 10_000, env: { ...process.env,
    GLASS_SCORECARD_ITERATIONS: String(iterations), CHROME_PATH: process.execPath, PLAYWRIGHT_MCP_COMMAND: fixture.server,
    PLAYWRIGHT_MCP_VERSION: "0.0.78", GLASS_SCORECARD_GIT_REVISION: "test-revision",
    GLASS_SCORECARD_CHECKPOINT_PATH: fixture.checkpoint, GLASS_SCORECARD_RUN_ID: "93a03835-9006-48c0-b98d-26cd58f886f1",
    GLASS_SCORECARD_STARTED_AT: "2026-07-14T00:00:00.000Z", PLAYWRIGHT_MCP_REQUEST_TIMEOUT_MS: "1000" } });
}

function setupFakeServer(dropTransport) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "glass-mcp-loop-test-"));
  const server = path.join(directory, "fake-mcp.mjs");
  const checkpoint = path.join(directory, "checkpoint.json");
  fs.writeFileSync(server, `#!/usr/bin/env node
import readline from "node:readline";
let calls = 0;
const tools = ["browser_navigate","browser_click","browser_evaluate","browser_fill_form","browser_handle_dialog","browser_resize","browser_snapshot"];
readline.createInterface({ input: process.stdin }).on("line", line => {
  const request = JSON.parse(line);
  if (!request.id) return;
  let result;
  if (request.method === "initialize") result = { serverInfo: { name: "Playwright" } };
  else if (request.method === "tools/list") result = { tools: tools.map(name => ({ name })) };
  else {
    calls++;
    if (${dropTransport} && calls === 3) process.exit(3);
    const fn = request.params?.arguments?.function ?? "";
    const setup = calls <= 2 || fn.includes("resetFixture");
    result = setup ? { content: [{ type: "text", text: "### Result\\ntrue" }] } :
      { isError: true, content: [{ type: "text", text: "fixture tool error" }] };
  }
  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, result }) + "\\n");
});
`);
  fs.chmodSync(server, 0o755);
  return { directory, server, checkpoint };
}
