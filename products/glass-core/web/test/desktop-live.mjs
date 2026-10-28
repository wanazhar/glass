import { chromium } from "playwright-core";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { createConnection } from "node:net";
import { dirname, join } from "node:path";

const baseUrl = process.env.DESKTOP_LIVE_URL;
const tokenPath = process.env.DESKTOP_TOKEN_FILE;
assert(baseUrl, "Set DESKTOP_LIVE_URL to an authenticated local Connect URL");
assert(tokenPath, "Set DESKTOP_TOKEN_FILE to the local Connect state file");
const { token } = JSON.parse(await readFile(tokenPath, "utf8"));
assert(token, "Connect state has no local token");
const agentSocket = join(dirname(tokenPath), "desktop-agent.sock");
function agentRequest(request) {
  return new Promise((resolve, reject) => {
    const socket = createConnection(agentSocket);
    let response = "";
    socket.setTimeout(5000, () => socket.destroy(new Error("Desktop agent socket timed out")));
    socket.on("connect", () => socket.write(`${JSON.stringify(request)}\n`));
    socket.on("data", (chunk) => {
      response += chunk;
      const newline = response.indexOf("\n");
      if (newline < 0) return;
      socket.end();
      try { resolve(JSON.parse(response.slice(0, newline))); }
      catch (error) { reject(error); }
    });
    socket.on("error", reject);
  });
}

const browser = await chromium.launch({
  executablePath: process.env.GC_BROWSER_BINARY ||
    (existsSync("/snap/chromium/current/usr/lib/chromium-browser/chrome")
      ? "/snap/chromium/current/usr/lib/chromium-browser/chrome" : undefined),
  headless: true,
  args: ["--no-sandbox", "--disable-dev-shm-usage"],
});
const viewportWidth = Number(process.env.DESKTOP_VIEWPORT_WIDTH) || 1280;
const viewportHeight = Number(process.env.DESKTOP_VIEWPORT_HEIGHT) || 900;
const context = await browser.newContext({
  viewport: { width: viewportWidth, height: viewportHeight },
  isMobile: viewportWidth <= 500,
  hasTouch: viewportWidth <= 500,
});
const page = await context.newPage();
const errors = [];
const responses = [];
page.on("pageerror", (error) => errors.push(error.message));
page.on("response", (response) => {
  const path = new URL(response.url()).pathname;
  responses.push({ path, status: response.status() });
});
try {
  await page.goto(`${baseUrl.replace(/\/$/, "")}/#${encodeURIComponent(token)}`);
  try {
    await page.waitForFunction(() => document.querySelector("#launch")?.hidden === true, { timeout: 30000 });
  } catch (error) {
    const state = await page.evaluate(() => ({
      launchHidden: document.querySelector("#launch")?.hidden,
      launchStatus: document.querySelector("#launch-status")?.textContent,
      launchDetail: document.querySelector("#launch-detail")?.textContent,
      appHidden: document.querySelector(".app")?.hidden,
      desktopTogglePresent: Boolean(document.querySelector("#desktop-toggle")),
      desktopDialogPresent: Boolean(document.querySelector("#desktop-viewer")),
      appScripts: [...document.scripts].map((script) => new URL(script.src).pathname),
    }));
    throw Error(`Connect shell did not open: ${JSON.stringify(state)}; responses=${JSON.stringify(responses)}; pageErrors=${JSON.stringify(errors)}; ${error.message}`);
  }
  await page.locator("#desktop-toggle").click();
  try {
    await page.locator("#desktop-viewer").waitFor({ state: "visible", timeout: 10000 });
  } catch (error) {
    const notice = await page.locator("#notice").textContent().catch(() => "");
    throw Error(`Desktop button did not open: notice=${notice}; responses=${JSON.stringify(responses)}; pageErrors=${JSON.stringify(errors)}; ${error.message}`);
  }
  await page.getByText("View only · GC may control").waitFor();
  if (viewportWidth <= 500) {
    assert.equal(await page.locator("#desktop-trackpad").isVisible(), true);
    assert.equal(await page.locator("#desktop-touch-mode").isVisible(), true);
  }
  await page.getByText(/^Live · updated/).waitFor({ timeout: 30000 });
  await page.waitForFunction(() => document.querySelector("#desktop-stream").naturalWidth > 0);
  const before = await page.evaluate(async () => (await fetch("/api/desktop/status")).json());
  assert.equal(before.available, true);
  assert.equal(before.lease, "agent");
  assert.equal(before.mode, "managedVirtualSession");
  assert.equal(await page.locator("#desktop-display").textContent(), before.displayId);
  assert.equal(await page.locator("#desktop-workspace").textContent(), before.workspaceRoot);
  const agentStatus = await agentRequest({ action: "status" });
  assert.equal(agentStatus.sessionId, before.sessionId, "GC tools and viewer must share the same session");
  const agentFrame = await agentRequest({ action: "frame" });
  assert.equal(agentFrame.sessionId, before.sessionId);
  assert.equal(Buffer.from(agentFrame.jpeg, "base64").subarray(0, 2).toString("hex"), "ffd8");
  if (process.env.DESKTOP_LIVE_PREVIEW)
    await page.screenshot({ path: process.env.DESKTOP_LIVE_PREVIEW, fullPage: true });

  await page.locator("#desktop-take").click();
  await page.getByText("You control · GC paused").waitFor();
  await page.locator("#desktop-keyboard-open").click();
  await page.locator("#desktop-text").fill("glass-live-viewer-check");
  await page.locator("#desktop-send-text").click();
  await page.waitForFunction(() => document.querySelector("#desktop-text").value === "");
  const held = await page.evaluate(async () => (await fetch("/api/desktop/status")).json());
  assert.equal(held.lease, "human");
  const agentConflict = await agentRequest({
    action: "input",
    value: { action: "key", key: "Enter", pressed: true },
  });
  assert.match(agentConflict.error, /lease is held by the other participant/i);
  await page.locator("#desktop-close").click();
  await page.waitForFunction(() => !document.querySelector("#desktop-viewer").open);
  const afterClose = await page.evaluate(async () => (await fetch("/api/desktop/status")).json());
  assert.equal(afterClose.lease, "human", "closing the viewer must retain the human lease");
  assert.equal(afterClose.sessionId, before.sessionId, "closing the viewer must leave the desktop running");

  await page.locator("#desktop-toggle").click();
  await page.getByText("You control · GC paused").waitFor();
  await page.locator("#desktop-more > summary").click();
  await page.locator("#desktop-reconnect").click();
  await page.getByText(/^Live · updated/).waitFor({ timeout: 15000 });
  await page.locator("#desktop-return").click();
  await page.getByText("View only · GC may control").waitFor();
  const returned = await page.evaluate(async () => (await fetch("/api/desktop/status")).json());
  assert.equal(returned.lease, "agent");
  assert.equal(returned.sessionId, before.sessionId);
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({
    live: true,
    displayId: before.displayId,
    resolution: `${before.width}x${before.height}`,
    leaseAfterClose: afterClose.lease,
    leaseAfterReturn: returned.lease,
    sessionRetained: returned.sessionId === before.sessionId,
    agentSharesSession: agentStatus.sessionId === before.sessionId,
    agentFrameBytes: Buffer.from(agentFrame.jpeg, "base64").length,
    agentInputBlockedDuringHumanLease: /lease is held/.test(agentConflict.error),
  }));
} finally {
  await browser.close();
}
