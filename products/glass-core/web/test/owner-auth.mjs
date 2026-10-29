// Offline checks for origin-scoped headers and bounded authenticated SSE.
import assert from "node:assert/strict";
const stored = new Map();
Object.defineProperty(globalThis, "localStorage", { configurable: true, value: {
  getItem: key => stored.get(key) ?? null,
  setItem: (key, value) => stored.set(key, value),
  removeItem: key => stored.delete(key),
} });
globalThis.location = new URL("https://host.example:8443/?thread=fixture");
class NativeEvents {
  static CONNECTING = 0; static OPEN = 1; static CLOSED = 2;
  constructor(url) { this.url = url; }
}
globalThis.EventSource = NativeEvents;
const { ownerFetch, ownerEventSource, setOwnerCapability } = await import("../src/owner-auth.js");
const capability = "a".repeat(64), requests = [];
globalThis.fetch = async (url, options) => {
  requests.push({ url, headers: new Headers(options.headers), redirect: options.redirect });
  return new Response("{}", { status: 200 });
};
setOwnerCapability(capability);
await ownerFetch("/api/status");
await ownerFetch("https://host.example:8444/api/status");
await ownerFetch("https://other.example/api/status");
await ownerFetch("/app.js");
assert.equal(requests[0].headers.get("X-Glass-Owner-Capability"), capability);
assert.equal(requests[0].redirect, "error", "Authenticated redirects must not forward custom capabilities");
for (const request of requests.slice(1)) assert.equal(request.headers.get("X-Glass-Owner-Capability"), null);
assert.equal(stored.size, 1, "Only the origin-scoped ordinary access capability is stored");

let calls = 0, errors = 0;
const encoder = new TextEncoder();
globalThis.fetch = async (_url, options) => {
  calls++;
  assert.equal(new Headers(options.headers).get("X-Glass-Owner-Capability"), capability);
  if (calls === 2) assert.equal(new Headers(options.headers).get("Last-Event-ID"), "1:7");
  const bytes = encoder.encode(calls === 1 ? 'id: 1:7\r\ndata: {"label":"é"}\r\n\r\n' : 'data: {"done":true}\n\n');
  const split = bytes.indexOf(0xc3) + 1;
  return new Response(new ReadableStream({ start(controller) {
    controller.enqueue(bytes.slice(0, Math.max(1, split)));
    controller.enqueue(bytes.slice(Math.max(1, split)));
    controller.close();
  } }), { headers: { "Content-Type": "text/event-stream" } });
};
const received = [];
await new Promise((resolve, reject) => {
  const timeout = setTimeout(() => reject(Error("SSE reconnect timed out")), 4000);
  const source = ownerEventSource("/api/events?id=fixture");
  source.onerror = () => errors++;
  source.onmessage = event => {
    received.push(JSON.parse(event.data));
    if (received.length === 2) {
      source.close();
      assert.equal(source.readyState, NativeEvents.CLOSED);
      clearTimeout(timeout);
      resolve();
    }
  };
});
assert.deepEqual(received, [{ label: "é" }, { done: true }]);
assert.equal(calls, 2);
assert.equal(errors, 1, "Only the ended first stream reports reconnect; explicit close does not");
setOwnerCapability(null);
assert.equal(stored.size, 0);
assert.ok(ownerEventSource("/api/events") instanceof NativeEvents);
console.log("PASS owner origin isolation, reload storage, authenticated SSE, UTF-8, reconnect and close");
