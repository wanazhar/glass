import assert from "node:assert/strict";
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { GlassClient } from "./dist/index.js";

test("individually bounded responses may share one socket chunk", async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "glass-client-frames-"));
  const socketPath = path.join(dir, "socket");
  const server = net.createServer((socket) => {
    let buffered = "";
    const requests = [];
    socket.on("data", (chunk) => {
      buffered += chunk.toString();
      let end;
      while ((end = buffered.indexOf("\n")) >= 0) {
        const request = JSON.parse(buffered.slice(0, end));
        buffered = buffered.slice(end + 1);
        if (request.method === "initialize") {
          socket.write(JSON.stringify({jsonrpc: "2.0", id: request.id, result: {glass: {protocolVersion: 1, capabilities: {}, schemas: {}}}}) + "\n");
        } else if (request.method === "tools/list") {
          requests.push(request);
          if (requests.length === 2) {
            socket.write(requests.map(({id}) => JSON.stringify({jsonrpc: "2.0", id, result: {tools: [{name: "x".repeat(40)}]}}) + "\n").join(""));
          }
        }
      }
    });
  });
  await new Promise((resolve) => server.listen(socketPath, resolve));
  const client = new GlassClient({daemonSocket: socketPath, maxFrameBytes: 150});
  try {
    const pages = await Promise.all([client.listTools(), client.listTools()]);
    assert.deepEqual(pages.map((page) => page[0].name), ["x".repeat(40), "x".repeat(40)]);
  } finally {
    client.close();
    await new Promise((resolve) => server.close(resolve));
    fs.rmSync(dir, {recursive: true, force: true});
  }
});
