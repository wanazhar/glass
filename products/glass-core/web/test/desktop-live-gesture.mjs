import { chromium } from "playwright-core";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { createConnection } from "node:net";
import { dirname, join } from "node:path";

const baseUrl = process.env.DESKTOP_LIVE_URL;
const tokenPath = process.env.DESKTOP_TOKEN_FILE;
assert(baseUrl && tokenPath, "Set DESKTOP_LIVE_URL and DESKTOP_TOKEN_FILE for an isolated desktop");
const { token } = JSON.parse(await readFile(tokenPath, "utf8"));
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
const page = await browser.newPage({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });
try {
  await page.goto(`${baseUrl.replace(/\/$/, "")}/#${encodeURIComponent(token)}`);
  await page.waitForFunction(() => document.querySelector("#launch")?.hidden === true, { timeout: 30000 });
  await page.locator("#desktop-toggle").click();
  try {
    await page.getByText("View only · GC may control").waitFor({ timeout: 10000 });
  } catch (error) {
    const diagnostics = await page.evaluate(async () => ({
      owner: document.querySelector("#desktop-owner")?.textContent,
      status: document.querySelector("#desktop-status")?.textContent,
      placeholder: document.querySelector("#desktop-placeholder")?.textContent,
      host: await fetch("/api/desktop/status").then(async (response) => ({ status: response.status, body: await response.json() })),
    }));
    throw Error(`Desktop did not become viewable: ${JSON.stringify(diagnostics)}; ${error.message}`);
  }
  await page.getByText(/^Live · updated/).waitFor({ timeout: 30000 });
  const sessionBeforeTap = await agentRequest({ action: "status" });
  const hasFileManagerWindow = (snapshot) => snapshot.nodes?.some((node) => node.depth === 1
    && node.application === "Thunar"
    && /Thunar/i.test(node.name)
    && node.states?.includes("visible"));
  const initialObservation = await agentRequest({ action: "observe", value: { sessionId: sessionBeforeTap.sessionId } });
  if (hasFileManagerWindow(initialObservation)) {
    // The isolated display may be reused after an earlier run. Close its observed test window.
    await page.locator("#desktop-take").click();
    await page.getByText("You control · GC paused").waitFor();
    await page.locator("#desktop-keyboard-open").click();
    await page.locator("#desktop-mod-control").click();
    await page.locator("#desktop-text").focus();
    await page.keyboard.press("q");
    await page.waitForTimeout(250);
    await page.locator("#desktop-keyboard-open").click();
    await page.locator("#desktop-return").click();
    await page.getByText("View only · GC may control").waitFor();
    const cleared = await agentRequest({ action: "observe", value: { sessionId: sessionBeforeTap.sessionId } });
    assert(!hasFileManagerWindow(cleared), "close the prior isolated file-manager window before testing the new double tap");
  }

  await page.locator("#desktop-take").click();
  await page.getByText("You control · GC paused").waitFor();
  await page.locator("#desktop-touch-mode").click();

  const viewport = await page.locator("#desktop-viewport").boundingBox();
  assert(viewport, "mobile desktop stage is visible");
  await page.evaluate(() => {
    window.__desktopInputTimings = [];
    window.__desktopFrameTimings = [];
    window.addEventListener("glass-desktop-input-timing", (event) => window.__desktopInputTimings.push(event.detail));
    window.addEventListener("glass-desktop-frame-timing", (event) => window.__desktopFrameTimings.push(event.detail));
  });
  const iconX = viewport.x + viewport.width * 0.08;
  const iconY = viewport.y + viewport.height * 0.25;
  await page.touchscreen.tap(iconX, iconY);
  await page.waitForTimeout(120);
  await page.touchscreen.tap(iconX, iconY);
  await page.waitForTimeout(700);
  const observedWindow = await agentRequest({ action: "observe", value: { sessionId: sessionBeforeTap.sessionId } });
  assert(hasFileManagerWindow(observedWindow), "direct touch double tap should open the file manager on the desktop");

  let activeStage = await page.locator("#desktop-viewport").boundingBox();
  assert(activeStage);
  await page.touchscreen.tap(activeStage.x + activeStage.width * 0.30, activeStage.y + activeStage.height * 0.13);
  await page.locator("#desktop-keyboard-open").click();
  await page.locator("#desktop-mod-control").click();
  await page.locator("#desktop-text").focus();
  await page.keyboard.press("a");
  await page.locator("#desktop-text").fill("/tmp");
  await page.locator("#desktop-send-text").click();
  await page.waitForFunction(() => document.querySelector("#desktop-text").value === "");
  await page.locator("#desktop-viewport").focus();
  await page.keyboard.press("Enter");
  await page.waitForTimeout(400);
  const typedObservation = await agentRequest({ action: "observe", value: { sessionId: sessionBeforeTap.sessionId, query: "tmp" } });
  assert(typedObservation.nodes?.some((node) => node.application === "Thunar" && /tmp - Thunar/i.test(node.name)),
    "typing /tmp and pressing Enter should navigate the real file-manager window");
  const afterDoubleTap = await agentRequest({ action: "frame" });
  assert(afterDoubleTap.jpeg, "fresh desktop frame should follow the direct-touch double tap");
  await page.waitForFunction(() => document.querySelector("#desktop-status").textContent.startsWith("Live · updated")
    && document.querySelector("#desktop-placeholder").hidden
    && !document.querySelector("#desktop-stream").hidden, { timeout: 5000 });
  if (process.env.DESKTOP_GESTURE_PREVIEW) await page.screenshot({ path: process.env.DESKTOP_GESTURE_PREVIEW, fullPage: true });

  await page.locator("#desktop-keyboard-open").click();
  activeStage = await page.locator("#desktop-viewport").boundingBox();
  assert(activeStage);
  const cdp = await page.context().newCDPSession(page);
  const dragX = activeStage.x + activeStage.width * 0.20;
  const dragY = activeStage.y + activeStage.height * 0.06;
  await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [
    { id: 61, x: dragX, y: dragY, radiusX: 4, radiusY: 4, force: 1 },
  ] });
  await cdp.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: [
    { id: 61, x: dragX + 40, y: dragY + 20, radiusX: 4, radiusY: 4, force: 1 },
  ] });
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  await page.waitForTimeout(250);
  await page.waitForFunction(() => document.querySelector("#desktop-status").textContent.startsWith("Live · updated")
    && document.querySelector("#desktop-placeholder").hidden
    && !document.querySelector("#desktop-stream").hidden, { timeout: 5000 });
  await page.locator("#desktop-return").click();
  await page.getByText("View only · GC may control").waitFor();
  const returned = await agentRequest({ action: "status" });
  assert.equal(returned.lease, "agent");
  const finalFrame = await agentRequest({ action: "frame" });
  assert(finalFrame.jpeg, "fresh desktop frame should follow drag release");
  const timing = await page.evaluate(() => ({
    input: window.__desktopInputTimings.map((event) => event.totalMs).sort((a, b) => a - b),
    eventToFrame: window.__desktopFrameTimings.map((event) => event.eventToFrameMs).sort((a, b) => a - b),
    requestToFrame: window.__desktopFrameTimings.map((event) => event.requestToFrameMs).sort((a, b) => a - b),
  }));
  const percentile = (values, p) => values.length ? Math.round(values[Math.min(values.length - 1, Math.floor(values.length * p))]) : null;
  console.log(JSON.stringify({
    live: true,
    directTouchDoubleTapSent: true,
    openedApplication: "File System - Thunar",
    typedTextAndNavigated: "/tmp",
    draggedApplicationWindow: true,
    dragReleasedBeforeReturn: true,
    leaseAfterReturn: returned.lease,
    freshFrameSequence: finalFrame.seq,
    inputActions: await page.evaluate(() => window.__desktopInputTimings.map(({ action, queuedMs, requestMs, totalMs, accepted }) => ({
      action, queuedMs: Math.round(queuedMs), requestMs: Math.round(requestMs), totalMs: Math.round(totalMs), accepted,
    }))),
    inputMs: { count: timing.input.length, p50: percentile(timing.input, 0.5), p95: percentile(timing.input, 0.95) },
    eventToFrameMs: { count: timing.eventToFrame.length, p50: percentile(timing.eventToFrame, 0.5), p95: percentile(timing.eventToFrame, 0.95) },
    requestToFrameMs: { count: timing.requestToFrame.length, p50: percentile(timing.requestToFrame, 0.5), p95: percentile(timing.requestToFrame, 0.95) },
  }));
} finally {
  await browser.close();
}
