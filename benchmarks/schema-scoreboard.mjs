#!/usr/bin/env node

// Measure the public MCP schema without starting Chrome. This is a stable,
// reproducible context-budget report for Gate B.4/C.5.
import { spawn } from "node:child_process";

const command = process.env.GLASS_BINARY_PATH ?? "target/debug/glass";
const maxBytes = 4 * 1024 * 1024;
const child = spawn(command, ["--mcp"], { stdio: ["pipe", "pipe", "inherit"] });
let buffer = Buffer.alloc(0);
let nextId = 1;
const pending = new Map();

child.stdout.on("data", (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  if (buffer.length > maxBytes) return fail("MCP output exceeded the schema probe budget");
  consume();
});
child.on("error", fail);
child.on("exit", (code) => { if (code && pending.size) fail(new Error(`Glass exited ${code}`)); });

function consume() {
  while (buffer.length) {
    const newline = buffer.indexOf(10);
    if (newline < 0) return;
    const body = buffer.subarray(0, newline);
    buffer = buffer.subarray(newline + 1);
    let message;
    try { message = JSON.parse(body); } catch (error) { return fail(error); }
    const waiter = pending.get(message.id);
    if (!waiter) continue;
    pending.delete(message.id);
    if (message.error) waiter.reject(new Error(message.error.message));
    else waiter.resolve(message.result);
  }
}

function request(method, params = {}) {
  const id = nextId++;
  child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
  return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
}

function fail(error) {
  for (const waiter of pending.values()) waiter.reject(error instanceof Error ? error : new Error(String(error)));
  pending.clear();
  child.kill();
}

try {
  await request("initialize", {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "schema-scoreboard", version: "1" },
  });
  child.stdin.write(JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized", params: {} }) + "\n");
  const listed = await request("tools/list");
  const tools = listed.tools ?? [];
  const schema = JSON.stringify(tools);
  const report = {
    schema_version: 1,
    tool_count: tools.length,
    schema_bytes: Buffer.byteLength(schema),
    estimated_tokens: Math.ceil(Buffer.byteLength(schema) / 4),
    tools: tools.map((tool) => ({ name: tool.name, schema_bytes: Buffer.byteLength(JSON.stringify(tool.inputSchema ?? {})) })),
    methodology: "UTF-8 JSON bytes divided by four for a conservative token estimate; excludes JSON-RPC framing.",
  };
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  child.kill();
} catch (error) {
  fail(error);
  process.exitCode = 1;
}
