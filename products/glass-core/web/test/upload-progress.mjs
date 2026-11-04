import assert from "node:assert/strict";
globalThis.location = new URL("https://workspace.example:8443/");
const stored = new Map();
globalThis.localStorage = { getItem: key => stored.get(key), setItem: (key, value) => stored.set(key, value), removeItem: key => stored.delete(key) };
let mode = "success", last;
class Upload extends EventTarget {
  constructor() { super(); this.upload = new EventTarget(); this.headers = new Map(); last = this; }
  open(method, url) { this.method = method; this.url = url; }
  setRequestHeader(name, value) { this.headers.set(name, value); }
  send(body) {
    this.body = body;
    if (mode === "pending") return;
    queueMicrotask(() => {
      this.upload.dispatchEvent(Object.assign(new Event("progress"), { loaded: 25, total: 100, lengthComputable: true }));
      this.upload.dispatchEvent(Object.assign(new Event("progress"), { loaded: 40, total: 0, lengthComputable: false }));
      this.status = mode === "expired" ? 403 : 200;
      this.responseURL = this.url;
      this.responseText = JSON.stringify(mode === "expired" ? { error: "Access expired", ownerAuthRequired: true } : { token: "fixture" });
      this.onload();
    });
  }
  abort() { this.onabort?.(); }
}
globalThis.XMLHttpRequest = Upload;
const { ownerUpload, setOwnerCapability } = await import("../src/owner-auth.js");
const capability = "a".repeat(64); setOwnerCapability(capability);
const progress = [];
assert.deepEqual(await ownerUpload("/api/upload?id=fixture", new Blob(["fixture"]), { onProgress: (...value) => progress.push(value) }), { token: "fixture" });
assert.deepEqual(progress, [[25, 100], [40, 0]]);
assert.equal(last.headers.get("X-Glass-Owner-Capability"), capability);
assert.equal(last.method, "POST");
await assert.rejects(ownerUpload("https://other.example/api/upload", new Blob()), /workspace upload endpoint/);
await assert.rejects(ownerUpload("/api/secret", new Blob()), /workspace upload endpoint/);
mode = "pending";
const controller = new AbortController(), cancelled = ownerUpload("/api/upload", new Blob(), { signal: controller.signal });
controller.abort(); await assert.rejects(cancelled, error => error.name === "AbortError");
await assert.rejects(ownerUpload("/api/upload", new Blob(), { signal: controller.signal }), error => error.name === "AbortError");
mode = "expired";
await assert.rejects(ownerUpload("/api/upload", new Blob()), /Access expired/);
assert.equal(stored.size, 0);
console.log("PASS actual upload progress, indeterminate fallback, abort, endpoint scope and expired-owner handling");
