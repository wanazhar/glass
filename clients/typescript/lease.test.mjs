import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { GlassClient } from "./dist/index.js";

test("overlapping lease scopes retain the lease until the last scope exits", async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "glass-client-lease-"));
  const socketPath = path.join(dir, "socket");
  const server = net.createServer((socket) => {
    let buffer = "";
    let held = false;
    socket.on("data", (chunk) => {
      buffer += chunk.toString();
      let end;
      while ((end = buffer.indexOf("\n")) >= 0) {
        const request = JSON.parse(buffer.slice(0, end));
        buffer = buffer.slice(end + 1);
        if (request.id === undefined) continue;
        let result;
        if (request.method === "initialize") result = {glass: {protocolVersion: 1, capabilities: {}, schemas: {}}};
        else if (request.method === "glass/lease/acquire") { held = true; result = {token: "lease"}; }
        else if (request.method === "glass/lease/release") { held = false; result = {}; }
        else if (request.method === "glass/lease/renew" && held) result = {};
        else { socket.write(JSON.stringify({jsonrpc: "2.0", id: request.id, error: {code: -32000, message: "lease is not held"}}) + "\n"); continue; }
        socket.write(JSON.stringify({jsonrpc: "2.0", id: request.id, result}) + "\n");
      }
    });
  });
  await new Promise((resolve) => server.listen(socketPath, resolve));
  const client = new GlassClient({daemonSocket: socketPath});
  try {
    let finishFirst, finishSecond, enteredSecond;
    const firstGate = new Promise((resolve) => { finishFirst = resolve; });
    const secondGate = new Promise((resolve) => { finishSecond = resolve; });
    const entered = new Promise((resolve) => { enteredSecond = resolve; });
    const first = client.withMutationLease(() => firstGate);
    const second = client.withMutationLease(async () => { enteredSecond(); await secondGate; await client.renewMutationLease(); });
    await entered;
    finishFirst();
    await first;
    finishSecond();
    await second;
  } finally {
    client.close();
    await new Promise((resolve) => server.close(resolve));
    fs.rmSync(dir, {recursive: true, force: true});
  }
});
