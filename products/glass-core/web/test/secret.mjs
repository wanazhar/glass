// Isolated browser checks for HTTPS-only, out-of-transcript secret answers.
import { chromium } from "playwright-core";
import assert from "node:assert/strict";
import { createServer as createHttpServer } from "node:http";
import { createServer as createHttpsServer } from "node:https";
import { readFile, mkdtemp, rm } from "node:fs/promises";
import { existsSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { resolve, extname, sep, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../dist/", import.meta.url));
const secretRequests = [], chatRequests = [], streams = new Set();
const firstRequest = {
  kind: "secret",
  request: {
    purpose: "Read the approved account status for this task.",
    destination: "https://service.example/account/status",
    tool: "web_secret_request",
    operation: "GET /account/status",
    secureSubmission: true,
  },
};
const emit = (events) => {
  for (const response of streams) response.write(`data: ${JSON.stringify({ events })}\n\n`);
};
const handler = async (request, response) => {
  const url = new URL(request.url, "http://localhost"), path = url.pathname;
  if (path === "/api/events") {
    response.writeHead(200, { "Content-Type": "text/event-stream", "Cache-Control": "no-cache" });
    streams.add(response);
    response.write(`data: ${JSON.stringify({ reset: true, control: { can_write: true }, interaction: { id: "secret-1", request: firstRequest } })}\n\n`);
    request.on("close", () => streams.delete(response));
    return;
  }
  if (path === "/api/secret" || path === "/api/send") {
    let source = "";
    for await (const chunk of request) source += chunk;
    const body = JSON.parse(source);
    (path === "/api/secret" ? secretRequests : chatRequests).push(body);
    response.writeHead(200, { "Content-Type": "application/json" });
    response.end(JSON.stringify(path === "/api/secret" ? { sent: true } : {}));
    return;
  }
  if (path.startsWith("/api/")) {
    let data = {};
    if (path === "/api/status") data = { profile: "workspace-edit", workspace: "/tmp" };
    if (path === "/api/threads") data = { threads: [{ session: "fixture", title: "Secret request", workspace: "/tmp", workspace_assigned: true, workspace_available: true }], projects: [] };
    if (path === "/api/history") data = { records: [], before: 0 };
    response.writeHead(200, { "Content-Type": "application/json" });
    response.end(JSON.stringify(data));
    return;
  }
  try {
    const file = resolve(root, "." + (path === "/" ? "/index.html" : path));
    if (!file.startsWith(root.endsWith(sep) ? root : root + sep)) throw Error("Invalid path");
    const body = await readFile(file);
    response.writeHead(200, { "Content-Type": ({ ".js": "text/javascript", ".css": "text/css", ".html": "text/html", ".svg": "image/svg+xml", ".woff2": "font/woff2", ".webmanifest": "application/manifest+json" })[extname(file)] || "application/octet-stream" });
    response.end(body);
  } catch { response.writeHead(404).end(); }
};

const certificateDir = await mkdtemp(join(tmpdir(), "glass-secret-web-test-"));
const keyPath = join(certificateDir, "key.pem"), certPath = join(certificateDir, "cert.pem");
const cert = spawnSync("openssl", ["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", keyPath, "-out", certPath, "-days", "1", "-subj", "/CN=localhost", "-addext", "subjectAltName=DNS:localhost,IP:127.0.0.1"], { encoding: "utf8", stdio: "ignore" });
assert.equal(cert.status, 0, "openssl must create a temporary test certificate");
const tls = { key: await readFile(keyPath), cert: await readFile(certPath) };
const httpServer = createHttpServer(handler), httpsServer = createHttpsServer(tls, handler);
await Promise.all([
  new Promise(resolve => httpServer.listen(0, "127.0.0.1", resolve)),
  new Promise(resolve => httpsServer.listen(0, "127.0.0.1", resolve)),
]);
const browser = await chromium.launch({ executablePath: process.env.GC_BROWSER_BINARY || (existsSync("/snap/chromium/current/usr/lib/chromium-browser/chrome") ? "/snap/chromium/current/usr/lib/chromium-browser/chrome" : undefined), headless: true, args: ["--no-sandbox", "--disable-dev-shm-usage"] });
try {
  const context = await browser.newContext({ ignoreHTTPSErrors: true, serviceWorkers: "block" });
  const page = await context.newPage(), errors = [];
  page.on("pageerror", error => errors.push(String(error)));
  await page.goto(`https://localhost:${httpsServer.address().port}/?thread=fixture`);
  const panel = page.locator("#interaction");
  await page.getByRole("heading", { name: "Secure information requested" }).waitFor();
  assert.equal(await page.getByRole("heading", { name: "Secure information requested" }).count(), 1);
  assert.match(await panel.textContent(), /Read the approved account status/);
  assert.match(await panel.textContent(), /https:\/\/service\.example\/account\/status/);
  emit([{ t: "control", can_write: false }]);
  await page.getByText("Take control to enter or decline this request.").waitFor();
  assert.equal(await page.locator('#interaction input[type="password"]').count(), 0);
  emit([{ t: "control", can_write: true }]);
  const input = page.getByRole("textbox", { name: "Secret value" });
  assert.equal(await input.getAttribute("type"), "password");
  assert.equal(await page.getByRole("button", { name: "Send securely" }).isEnabled(), true);
  const firstSecret = "never-show-this-secret-value";
  await input.fill(firstSecret);
  const secretResponse = page.waitForResponse(response => new URL(response.url()).pathname === "/api/secret");
  await page.getByRole("button", { name: "Send securely" }).click();
  assert.deepEqual(await (await secretResponse).json(), { sent: true }, "The HTTP response must not echo the secret");
  await page.getByText("Secure response sent. Waiting for the request to finish.").waitFor();
  assert.equal(await input.inputValue(), "", "The submitted value is cleared immediately");
  assert.deepEqual(secretRequests[0], { id: "fixture", interactionId: "secret-1", secret: firstSecret });
  assert.equal(chatRequests.length, 0, "Secret answers must never use the ordinary chat endpoint");
  assert.doesNotMatch(await page.locator("body").innerText(), new RegExp(firstSecret));
  emit([{ t: "interaction_done", id: "secret-1" }]);
  await panel.waitFor({ state: "hidden" });

  emit([{ t: "interaction", id: "secret-2", request: { ...firstRequest, request: { ...firstRequest.request, secureSubmission: false } } }]);
  await page.getByRole("heading", { name: "Secure information requested" }).waitFor();
  assert.equal(await page.getByRole("textbox", { name: "Secret value" }).isDisabled(), true);
  assert.equal(await page.getByRole("button", { name: "Send securely" }).isDisabled(), true);
  assert.match(await panel.textContent(), /Remote secure submission is unavailable for this connection\. Complete this request in the local GC terminal\./);
  const cancelResponse = page.waitForResponse(response => new URL(response.url()).pathname === "/api/send");
  await page.getByRole("button", { name: "Cancel request" }).click();
  await cancelResponse;
  assert.deepEqual(chatRequests.at(-1), { id: "fixture", message: { t: "cancel" } });
  await page.getByText("Request declined.").waitFor();
  assert.equal(await page.getByRole("textbox", { name: "Secret value" }).isDisabled(), true);

  emit([{ t: "interaction", id: "secret-3", request: firstRequest }]);
  await page.getByRole("heading", { name: "Secure information requested" }).waitFor();
  const replacementValue = page.getByRole("textbox", { name: "Secret value" });
  await replacementValue.fill("erase-on-replay");
  emit([{ t: "replay", can_write: true }]);
  await page.waitForFunction(() => document.querySelector('#interaction input[type="password"]')?.value === "");
  await replacementValue.fill("erase-on-disconnect");
  emit([{ t: "disconnected", message: "Temporary connection loss" }]);
  await page.getByText(/Connection lost\. Reconnect to continue/).waitFor();
  assert.equal(await replacementValue.inputValue(), "");
  emit([{ t: "error", settled: true, message: "Secret request expired." }]);
  await panel.waitFor({ state: "hidden" });
  await page.getByText("Secret request expired.").waitFor();
  assert.deepEqual(errors, []);
  await context.close();

  const httpContext = await browser.newContext({ serviceWorkers: "block" });
  const httpPage = await httpContext.newPage();
  await httpPage.goto(`http://127.0.0.1:${httpServer.address().port}/?thread=fixture`);
  await httpPage.getByRole("heading", { name: "Secure information requested" }).waitFor();
  assert.equal(await httpPage.getByRole("textbox", { name: "Secret value" }).isDisabled(), true);
  assert.equal(await httpPage.getByRole("button", { name: "Send securely" }).isDisabled(), true);
  assert.match(await httpPage.locator(".secret-status").textContent(), /HTTPS link/);
  assert.equal(secretRequests.length, 1, "HTTP pages must not submit secrets");
  await httpContext.close();
  await browser.close();
  console.log("PASS HTTPS-only secret card, dedicated submission, cancellation, clearing, and HTTP/unsupported gating");
} finally {
  await browser.close().catch(() => {});
  for (const response of streams) response.end();
  httpServer.closeAllConnections();
  httpsServer.closeAllConnections();
  await Promise.all([new Promise(resolve => httpServer.close(resolve)), new Promise(resolve => httpsServer.close(resolve))]);
  await rm(certificateDir, { recursive: true, force: true });
}
