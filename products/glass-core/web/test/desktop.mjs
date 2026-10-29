import { chromium } from "playwright-core";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { existsSync } from "node:fs";

const webRoot = new URL("../", import.meta.url);
const source = (path) => readFile(new URL(path, webRoot), "utf8");
let session = null;
let lease = "agent";
let nextSession = 1;
let nextFrameSeq = 18;
let hostReachable = true;
let gnomeCapabilityAvailable = false;
let savedProfileExists = true;
let sessionEnvironment = null;
let sessionLifecycle = null;
let inputDelayMs = 0;
let streamResponse = null;
let streamTimer = 0;
let streamFrameDelayMs = 0;
const streamWidths = [];
const actions = [];
const sessionStartEnvironments = [];
const input = [];
async function waitForInput(predicate, timeoutMs = 2000) {
  const deadline = Date.now() + timeoutMs;
  while (!input.some(predicate)) {
    assert(Date.now() < deadline, "expected desktop input was not sent");
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}
const jpg = Buffer.from(
  "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/2wBDAQkJCQwLDBgNDRgyIRwhMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjL/wAARCAAKABADASIAAhEBAxEB/8QAFgABAQEAAAAAAAAAAAAAAAAABwIE/8QAIxAAAQQCAQMFAAAAAAAAAAAAAQIDBAUREgBBUWEGExQxgf/EABQBAQAAAAAAAAAAAAAAAAAAAAP/xAAYEQEBAAMAAAAAAAAAAAAAAAABAAISIv/aAAwDAQACEQMRAD8AMrAOxrKQwlwpS2UgAJT1SD1HnmqkjOT3ZCVq3DYQRkAYzt2Hjjc96epJDqnXqevccV9rXFQSf0jlx6SpibfGq4TO2NvbjoTnHfA4S8xOJrf/2Q==",
  "base64",
);
const testModule = `
  import { desktopSurface } from "/src/desktop.js";
  const api = async (path, data, signal, options) => {
    if (path === "desktop/session" && data?.action === "start") window.__desktopStartTimeout = options?.timeoutMs;
    const response = await fetch("/api/" + path, {
      method: data === undefined ? "GET" : "POST",
      headers: data === undefined ? {} : { "Content-Type": "application/json" },
      body: data === undefined ? undefined : JSON.stringify(data), signal,
    });
    const value = await response.json();
    if (!response.ok) throw Error(value.error || "Request rejected");
    return value;
  };
  const surface = desktopSurface({ api });
  document.querySelector("#desktop-toggle").addEventListener("click", () => surface.open());
  window.setDesktopHostReachable = (reachable) => fetch("/__test/host?reachable=" + reachable);
  window.setDesktopInputDelay = (ms) => fetch("/__test/input-delay?ms=" + ms);
  window.disconnectDesktopStream = () => fetch("/__test/stream-close");
  window.setStaleDesktopStream = (delayMs) => fetch("/__test/stream-stale?delayMs=" + delayMs);
  window.__inputTimings = [];
  window.__frameTimings = [];
  window.addEventListener("glass-desktop-input-timing", (event) => window.__inputTimings.push(event.detail));
  window.addEventListener("glass-desktop-frame-timing", (event) => window.__frameTimings.push(event.detail));
`;

const server = createServer(async (request, response) => {
  const url = new URL(request.url, "http://localhost");
  if (url.pathname === "/") {
    const html = (await source("src/index.html"))
      .replace("__LAUNCH_CSS__", "")
      .replaceAll("__SHELL_VERSION__", "desktop-test")
      .replace('<script type="module" src="/boot.js?version=desktop-test"></script>', "<script type=\"module\" src=\"/desktop-test.js\"></script>")
      .replace("<div class=\"app\" hidden inert>", "<div class=\"app\">")
      .replace('<section id="launch" class="launch" aria-label="Opening Glass">', '<section id="launch" class="launch" aria-label="Opening Glass" hidden>');
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(html);
    return;
  }
  if (url.pathname === "/desktop-test.js") {
    response.writeHead(200, { "content-type": "text/javascript; charset=utf-8" });
    response.end(testModule);
    return;
  }
  if (url.pathname === "/__test/host") {
    hostReachable = url.searchParams.get("reachable") === "true";
    response.writeHead(204).end();
    return;
  }
  if (url.pathname === "/__test/gnome") {
    gnomeCapabilityAvailable = url.searchParams.get("available") === "true";
    response.writeHead(204).end();
    return;
  }
  if (url.pathname === "/__test/input-delay") {
    inputDelayMs = Math.max(0, Math.min(1000, Number(url.searchParams.get("ms")) || 0));
    response.writeHead(204).end();
    return;
  }
  if (url.pathname === "/__test/stream-close") {
    clearInterval(streamTimer);
    streamTimer = 0;
    streamResponse?.end();
    streamResponse = null;
    response.writeHead(204).end();
    return;
  }
  if (url.pathname === "/__test/stream-stale") {
    streamFrameDelayMs = Math.max(0, Math.min(10_000, Number(url.searchParams.get("delayMs")) || 0));
    response.writeHead(204).end();
    return;
  }
  if (url.pathname === "/__test/stream-widths") {
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify(streamWidths));
    return;
  }
  if (url.pathname === "/src/desktop.js") {
    response.writeHead(200, { "content-type": "text/javascript; charset=utf-8" });
    response.end(await source("src/desktop.js"));
    return;
  }
  if (url.pathname === "/style.css") {
    response.writeHead(200, { "content-type": "text/css; charset=utf-8" });
    response.end(await source("dist/style.css"));
    return;
  }
  if (url.pathname.startsWith("/fonts/")) {
    try {
      const body = await readFile(new URL("dist" + url.pathname, webRoot));
      response.writeHead(200, { "content-type": "font/woff2" });
      response.end(body);
    } catch {
      response.writeHead(404).end();
    }
    return;
  }
  if (url.pathname === "/api/desktop/status") {
    if (!hostReachable) {
      response.writeHead(503, { "content-type": "application/json" });
      response.end(JSON.stringify({ error: "Host timed out. Tap the connection indicator to reconnect." }));
      return;
    }
    response.writeHead(200, { "content-type": "application/json" });
    const frameSeq = session ? nextFrameSeq++ : null;
    response.end(JSON.stringify({
      available: Boolean(session), sessionId: session, mode: session ? "managedVirtualSession" : null,
      environment: session ? sessionEnvironment : null,
      lifecycle: session ? sessionLifecycle : null,
      capabilities: [
        { environment: "xfce", lifecycle: "disposable", label: "Quick desktop", available: true, reason: null, profileExists: false },
        { environment: "gnome", lifecycle: "reusable", label: "Reusable workspace", available: gnomeCapabilityAvailable, reason: gnomeCapabilityAvailable ? null : "This host has no verified isolated GNOME session.", profileExists: savedProfileExists },
      ],
      displayId: session ? ":81" : null, workspaceRoot: "/work/glass", lease,
      width: session ? 1280 : null, height: session ? 800 : null,
      frameSeq, frameAgeMs: session ? 24 : null, frameTimeMs: session ? frameSeq * 100 : null, error: null,
    }));
    return;
  }
  if (url.pathname === "/api/desktop/stream") {
    streamWidths.push(url.searchParams.get("width"));
    if (!session) {
      response.writeHead(409, { "content-type": "application/json" });
      response.end(JSON.stringify({ error: "No managed desktop session", lease }));
      return;
    }
    response.writeHead(200, { "content-type": "multipart/x-mixed-replace; boundary=glassframe" });
    streamResponse = response;
    const writeFrame = () => {
      if (!streamResponse || streamResponse.destroyed) return;
      const seq = nextFrameSeq++;
      const frameTimeMs = Math.max(0, seq * 100 - streamFrameDelayMs);
      response.write(`--glassframe\r\nContent-Type: image/jpeg\r\nContent-Length: ${jpg.length}\r\nX-Frame-Seq: ${seq}\r\nX-Frame-Time-Ms: ${frameTimeMs}\r\n\r\n`);
      response.write(jpg);
      response.write("\r\n");
    };
    writeFrame();
    streamTimer = setInterval(writeFrame, 100);
    response.on("close", () => {
      clearInterval(streamTimer);
      streamTimer = 0;
      if (streamResponse === response) streamResponse = null;
    });
    return;
  }
  if (url.pathname.startsWith("/api/desktop/")) {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    const body = chunks.length ? JSON.parse(Buffer.concat(chunks).toString()) : {};
    response.setHeader("content-type", "application/json");
    if (url.pathname === "/api/desktop/session" && body.action === "start") {
      session = `managed-${nextSession++}`;
      sessionEnvironment = body.environment;
      sessionLifecycle = body.lifecycle;
      sessionStartEnvironments.push({ environment: body.environment, lifecycle: body.lifecycle });
      actions.push(body.action);
      response.end(JSON.stringify({ started: true, sessionId: session, mode: "managedVirtualSession", environment: body.environment, lifecycle: body.lifecycle, displayId: ":81", width: 1280, height: 800 }));
      return;
    }
    if (url.pathname === "/api/desktop/session" && body.action === "stop") {
      session = null;
      sessionEnvironment = null;
      sessionLifecycle = null;
      lease = "agent";
      actions.push(body.action);
      response.end(JSON.stringify({ stopped: true }));
      return;
    }
    if (url.pathname === "/api/desktop/profile" && body.action === "delete" && body.profileId === "default") {
      if (session) {
        response.writeHead(409);
        response.end(JSON.stringify({ error: "Stop the reusable desktop before deleting its saved profile" }));
        return;
      }
      savedProfileExists = false;
      actions.push("delete-profile");
      response.end(JSON.stringify({ deleted: true, profileId: "default" }));
      return;
    }
    if (url.pathname === "/api/desktop/lease") {
      lease = body.action === "take-control" ? "human" : "agent";
      actions.push(body.action);
      response.end(JSON.stringify({ lease }));
      return;
    }
    if (url.pathname === "/api/desktop/input") {
      if (lease !== "human") {
        response.writeHead(409);
        response.end(JSON.stringify({ error: "Input belongs to the other participant", lease }));
        return;
      }
      if (inputDelayMs) await new Promise((resolve) => setTimeout(resolve, inputDelayMs));
      input.push(body);
      response.end(JSON.stringify({ sent: true, lease }));
      return;
    }
  }
  response.writeHead(404).end();
});

await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const { port } = server.address();
const browser = await chromium.launch({
  executablePath: process.env.GC_BROWSER_BINARY ||
    (existsSync("/snap/chromium/current/usr/lib/chromium-browser/chrome")
      ? "/snap/chromium/current/usr/lib/chromium-browser/chrome" : undefined),
  headless: true,
  args: ["--no-sandbox", "--disable-dev-shm-usage"],
});
const page = await browser.newPage({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });
const errors = [];
page.on("pageerror", (error) => errors.push(String(error)));
try {
  await page.goto(`http://127.0.0.1:${port}/`);
  await page.evaluate(() => window.setDesktopHostReachable(false));
  await page.locator("#desktop-toggle").click();
  await page.locator("#desktop-viewer").waitFor({ state: "visible" });
  assert.equal(await page.locator("#desktop-close").evaluate((button) => button === document.activeElement), true);
  await page.locator("#desktop-retry").waitFor({ state: "visible" });
  assert.equal(await page.locator("#desktop-status").textContent(), "Host connection unavailable", await page.locator("#desktop-placeholder").textContent());
  assert.equal(await page.locator("#desktop-start").isDisabled(), true);
  assert.equal(await page.locator("#desktop-keyboard-open").isVisible(), false);
  assert.equal(await page.locator(".desktop-mobile-tools").isVisible(), false);
  assert.match(await page.locator("#desktop-placeholder").textContent(), /Reconnect when the host is available/);
  const offlineViewport = await page.locator("#desktop-viewport").boundingBox();
  assert(offlineViewport && offlineViewport.height < 360, "offline viewer should keep the empty stage compact");
  if (process.env.DESKTOP_OFFLINE_PREVIEW)
    await page.screenshot({ path: process.env.DESKTOP_OFFLINE_PREVIEW, fullPage: true });
  await page.evaluate(() => window.setDesktopHostReachable(true));
  await page.locator("#desktop-retry").click();
  await page.getByText("Ready to start").waitFor();
  assert.equal(await page.locator("#desktop-retry").isVisible(), false);
  assert.equal(await page.locator("#desktop-preset-quick").isChecked(), true);
  assert.equal(await page.locator("#desktop-preset-reusable").evaluate((radio) => radio.disabled), true);
  assert.match(await page.locator("#desktop-reusable-note").textContent(), /no verified isolated GNOME session/);
  await page.locator("#desktop-start").click();
  await page.getByText("View only · GC may control").waitFor();
  assert.deepEqual(sessionStartEnvironments, [{ environment: "xfce", lifecycle: "disposable" }]);
  await page.locator("#desktop-details > summary").click();
  assert.equal(await page.locator("#desktop-environment-active").textContent(), "XFCE");
  assert.equal(await page.locator("#desktop-lifecycle-active").textContent(), "Quick desktop");
  await page.locator("#desktop-details > summary").click();
  await page.locator("#desktop-more > summary").click();
  page.once("dialog", (dialog) => {
    assert.match(dialog.message(), /temporary workspace will be removed/);
    dialog.accept();
  });
  await page.locator("#desktop-stop").click();
  await page.getByText("Choose a desktop workspace above to open it here.").waitFor();
  await page.evaluate(() => fetch("/__test/gnome?available=true"));
  await page.evaluate(() => window.setDesktopHostReachable(false));
  await page.waitForFunction(() => !document.querySelector("#desktop-retry").hidden);
  await page.evaluate(() => window.setDesktopHostReachable(true));
  await page.locator("#desktop-retry").click();
  await page.getByText("Ready to start").waitFor();
  assert.equal(await page.locator("#desktop-preset-reusable").evaluate((radio) => radio.disabled), false);
  await page.locator("#desktop-preset-reusable").check();
  assert.equal(await page.locator("#desktop-start").textContent(), "Resume workspace");
  await page.locator("#desktop-start").click();
  await page.getByText("View only · GC may control").waitFor();
  assert.deepEqual(sessionStartEnvironments, [
    { environment: "xfce", lifecycle: "disposable" },
    { environment: "gnome", lifecycle: "reusable" },
  ]);
  await page.locator("#desktop-details > summary").click();
  assert.equal(await page.locator("#desktop-environment-active").textContent(), "GNOME");
  assert.equal(await page.locator("#desktop-lifecycle-active").textContent(), "Reusable workspace");
  await page.locator("#desktop-details > summary").click();
  assert.equal(await page.evaluate(() => window.__desktopStartTimeout), 60_000);
  await page.locator("#desktop-take").click();
  await page.getByText("You control · GC paused").waitFor();
  assert.equal(await page.locator("#desktop-return").isVisible(), true);
  assert.equal(await page.locator("#desktop-lease-note").isVisible(), true);
  assert.equal(await page.locator("#desktop-text").isEnabled(), true);
  assert.equal(await page.locator("#desktop-keyboard-row").isVisible(), false);
  assert.equal(await page.locator("#desktop-touch-mode").isVisible(), true);
  await page.locator("#desktop-details > summary").click();
  assert.equal(await page.locator("#desktop-workspace-full").textContent(), "/work/glass");
  await page.locator("#desktop-details > summary").click();
  if (process.env.DESKTOP_PREVIEW)
    await page.screenshot({ path: process.env.DESKTOP_PREVIEW, fullPage: true });
  await page.evaluate(() => window.disconnectDesktopStream());
  await page.getByText(/^Stream disconnected/).waitFor();
  assert.equal(await page.locator("#desktop-stream").isHidden(), true, "broken image alt text should stay hidden");
  assert.equal(await page.locator("#desktop-retry").isVisible(), true);
  assert.equal(lease, "human", "stream failure must leave the human lease in place");
  assert(!actions.includes("return-to-agent"), "stream failure must not return control implicitly");
  await page.locator("#desktop-more > summary").click();
  assert.equal(await page.locator("#desktop-more").evaluate((details) => details.open), true);
  await page.locator("#desktop-reconnect").click();
  assert.equal(await page.locator("#desktop-more").evaluate((details) => details.open), false);
  assert.equal(await page.locator("#desktop-more > summary").evaluate((summary) => summary === document.activeElement), true);
  await page.waitForFunction(() => {
    const status = document.querySelector("#desktop-status").textContent;
    return status.startsWith("Live · updated")
      && document.querySelector("#desktop-placeholder").hidden
      && !document.querySelector("#desktop-stream").hidden;
  }, { timeout: 5000 });
  assert.equal(await page.locator("#desktop-stream").evaluate((image) => image.naturalWidth / image.naturalHeight), 1.6);
  assert((await page.evaluate(async () => await (await fetch("/__test/stream-widths")).json())).includes("480"),
    "phone viewer should request the bandwidth-friendly 480px stream");
  assert.equal(await page.locator("#desktop-placeholder").isHidden(), true);
  await page.evaluate(() => window.setStaleDesktopStream(1500));
  await page.getByText(/^Delayed ·/).waitFor({ timeout: 3000 });
  assert.equal(await page.locator("#desktop-stream").isVisible(), true, "a decoded frame within the delay bound should remain visible");
  assert.equal(await page.locator("#desktop-placeholder").isHidden(), true, "a decoded delayed frame should not black-screen the stage");
  await page.evaluate(() => window.setStaleDesktopStream(0));
  await page.getByText(/^Live · updated/).waitFor({ timeout: 5000 });
  await page.locator("#desktop-placeholder").waitFor({ state: "hidden" });
  await page.evaluate(() => window.setStaleDesktopStream(5000));
  await page.waitForFunction(() => document.querySelector("#desktop-stream").hidden, { timeout: 5000 });
  assert.match(await page.locator("#desktop-status").textContent(), /Waiting for a recent frame/);
  await page.evaluate(() => window.setStaleDesktopStream(0));
  await page.getByText(/^Live · updated/).waitFor({ timeout: 5000 });
  await page.locator("#desktop-more > summary").click();
  await page.locator("#desktop-title").click();
  assert.equal(await page.locator("#desktop-more").evaluate((details) => details.open), false, "outside pointer should close More");
  const moreSummary = page.locator("#desktop-more > summary");
  const moreBox = await moreSummary.boundingBox();
  assert(moreBox);
  await page.touchscreen.tap(moreBox.x + moreBox.width / 2, moreBox.y + moreBox.height / 2);
  assert.equal(await page.locator("#desktop-more").evaluate((details) => details.open), true, "touch should open More");
  const mobileMenu = await page.locator(".desktop-more-menu").boundingBox();
  const mobileStage = await page.locator("#desktop-viewport").boundingBox();
  assert(mobileMenu && mobileStage && mobileMenu.y >= mobileStage.y + mobileStage.height,
    "mobile More menu should sit below the desktop image instead of covering touch controls or the stage");
  if (process.env.DESKTOP_MORE_PREVIEW)
    await page.screenshot({ path: process.env.DESKTOP_MORE_PREVIEW, fullPage: true });
  await page.keyboard.press("Escape");
  assert.equal(await page.locator("#desktop-more").evaluate((details) => details.open), false, "Escape should close More");
  assert.equal(await page.locator("#desktop-viewer").evaluate((dialog) => dialog.open), true, "Escape should keep the viewer open when More was open");
  assert.equal(await moreSummary.evaluate((summary) => summary === document.activeElement), true, "Escape should restore focus to More");
  await page.locator("#desktop-more > summary").click();
  await page.locator("#desktop-keyboard-open").click();
  assert.equal(await page.locator("#desktop-keyboard-row").isVisible(), true);
  if (process.env.DESKTOP_KEYBOARD_PREVIEW)
    await page.screenshot({ path: process.env.DESKTOP_KEYBOARD_PREVIEW, fullPage: true });
  await page.locator("#desktop-text").fill("hello host");
  await page.locator("#desktop-send-text").click();
  await page.waitForFunction(() => document.querySelector("#desktop-text").value === "");
  await page.locator("#desktop-viewport").focus();
  await page.keyboard.press("Enter");
  const viewport = await page.locator("#desktop-viewport").boundingBox();
  assert(viewport);
  await page.mouse.move(viewport.x + viewport.width / 2, viewport.y + viewport.height / 2);
  await page.mouse.down();
  await page.mouse.up();
  await page.touchscreen.tap(viewport.x + viewport.width * 0.65, viewport.y + viewport.height * 0.65);
  await page.mouse.wheel(2, 40);
  assert(input.some((event) => event.action === "text" && event.text === "hello host"));
  assert(input.some((event) => event.action === "key" && event.key === "Enter" && event.pressed));
  await waitForInput((event) => event.action === "scroll" && event.deltaY === 40);
  assert(input.some((event) => event.action === "pointer-button" && event.button === "left" && event.pressed));
  await page.locator("#desktop-touch-mode").click();
  assert.equal(await page.locator("#desktop-touch-mode").getAttribute("aria-pressed"), "true");
  assert.match(await page.locator("#desktop-touch-hint").textContent(), /pinch to zoom/);
  const directPoint = await page.locator("#desktop-viewport").boundingBox();
  assert(directPoint);
  await page.touchscreen.tap(directPoint.x + directPoint.width * 0.65, directPoint.y + directPoint.height * 0.65);
  await waitForInput((event) => event.action === "pointer-click" && event.button === "left" && event.x > 0.5 && event.y > 0.5);
  const cdp = await page.context().newCDPSession(page);
  const midX = directPoint.x + directPoint.width / 2;
  const midY = directPoint.y + directPoint.height / 2;
  const dragX = midX - 70;
  const dragY = midY - 20;
  const beforeDrag = input.length;
  await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [
    { id: 19, x: dragX, y: dragY, radiusX: 4, radiusY: 4, force: 1 },
  ] });
  await cdp.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: [
    { id: 19, x: dragX + 36, y: dragY + 18, radiusX: 4, radiusY: 4, force: 1 },
  ] });
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  await page.waitForTimeout(80);
  const directDragActions = input.slice(beforeDrag);
  assert(directDragActions.some((event) => event.action === "pointer-button" && event.button === "left" && event.pressed));
  const directDragRelease = directDragActions.find((event) => event.action === "pointer-button" && event.button === "left" && !event.pressed);
  assert(directDragRelease, "direct touch drag must always release its left button");
  assert(Math.abs(directDragRelease.x - (dragX + 36 - directPoint.x) / directPoint.width) < 0.03,
    "direct touch release must carry the latest drag coordinate even when pending movement is bypassed");

  // A slow endpoint must not turn a phone drag into a long queue of stale coordinates.
  await page.evaluate(() => window.setDesktopInputDelay(120));
  const beforeSlowDrag = input.length;
  const slowDragStartX = midX - 65;
  const slowDragStartY = midY + 8;
  await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [
    { id: 23, x: slowDragStartX, y: slowDragStartY, radiusX: 4, radiusY: 4, force: 1 },
  ] });
  await Promise.all(Array.from({ length: 18 }, (_, index) => {
    const step = index + 1;
    return cdp.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: [
      { id: 23, x: slowDragStartX + step * 3, y: slowDragStartY + step, radiusX: 4, radiusY: 4, force: 1 },
    ] });
  }));
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  await page.waitForFunction(() => document.querySelectorAll("#desktop-viewer").length === 1);
  await page.waitForTimeout(500);
  await page.evaluate(() => window.setDesktopInputDelay(0));
  const slowDragActions = input.slice(beforeSlowDrag);
  const slowMoves = slowDragActions.filter((event) => event.action === "pointer-move");
  const slowButtons = slowDragActions.filter((event) => event.action === "pointer-button");
  assert(slowMoves.length <= 4, `slow drag sent too many stale moves: ${slowMoves.length}`);
  assert.deepEqual(slowButtons.map((event) => event.pressed), [true, false], "drag button down/up order must be preserved");
  const slowStage = await page.locator("#desktop-viewport").boundingBox();
  const expectedSlowX = (slowDragStartX + 54 - slowStage.x) / slowStage.width;
  const expectedSlowY = (slowDragStartY + 18 - slowStage.y) / slowStage.height;
  const finalCoordinates = slowMoves.at(-1) || slowButtons.at(-1);
  assert(Math.abs(finalCoordinates.x - expectedSlowX) < 0.03 && Math.abs(finalCoordinates.y - expectedSlowY) < 0.03,
    `last coalesced move or release should reach the latest touch coordinate (got ${JSON.stringify(finalCoordinates)}, expected ${expectedSlowX},${expectedSlowY})`);
  assert((await page.evaluate(() => window.__inputTimings)).some((timing) => timing.requestMs >= 100), "latency instrumentation should report the injected API delay");
  await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [
    { id: 20, x: midX - 20, y: midY + 20, radiusX: 4, radiusY: 4, force: 1 },
  ] });
  await page.waitForTimeout(560);
  assert(input.some((event) => event.action === "pointer-button" && event.button === "right" && event.pressed));
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  await page.waitForTimeout(100);
  assert(input.some((event) => event.action === "pointer-button" && event.button === "right" && !event.pressed));
  await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [
    { id: 21, x: midX - 28, y: midY, radiusX: 4, radiusY: 4, force: 1 },
    { id: 22, x: midX + 28, y: midY, radiusX: 4, radiusY: 4, force: 1 },
  ] });
  await cdp.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: [
    { id: 21, x: midX - 55, y: midY, radiusX: 4, radiusY: 4, force: 1 },
    { id: 22, x: midX + 55, y: midY, radiusX: 4, radiusY: 4, force: 1 },
  ] });
  await page.waitForFunction(() => Number(document.querySelector("#desktop-stream").style.getPropertyValue("--desktop-zoom")) > 1);
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  assert.equal(await page.locator("#desktop-view-reset").isVisible(), true);
  await page.locator("#desktop-view-reset").click();
  assert.equal(await page.locator("#desktop-stream").evaluate((image) => image.style.getPropertyValue("--desktop-zoom")), "1");
  assert.equal(await page.locator("#desktop-shortcuts").isVisible(), true);
  await page.locator("#desktop-mod-control").click();
  await page.locator("#desktop-text").focus();
  await page.keyboard.press("a");
  await page.locator('[data-desktop-key="Escape"]').click();
  await page.waitForTimeout(120);
  assert(input.some((event) => event.action === "key" && event.key === "Control" && event.pressed));
  assert(input.some((event) => event.action === "key" && event.key === "A" && event.pressed));
  assert(input.some((event) => event.action === "key" && event.key === "Escape" && event.pressed), JSON.stringify(input.slice(-12)));
  await page.locator("#desktop-trackpad").click();
  const beforeTwoFingerScroll = input.filter((event) => event.action === "scroll").length;
  await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [
    { id: 31, x: midX - 12, y: midY - 10, radiusX: 4, radiusY: 4, force: 1 },
    { id: 32, x: midX + 12, y: midY + 10, radiusX: 4, radiusY: 4, force: 1 },
  ] });
  await cdp.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: [
    { id: 31, x: midX - 12, y: midY + 12, radiusX: 4, radiusY: 4, force: 1 },
    { id: 32, x: midX + 12, y: midY + 32, radiusX: 4, radiusY: 4, force: 1 },
  ] });
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  await page.waitForFunction(() => window.__inputTimings.some((timing) => timing.action === "scroll" && timing.accepted));
  assert(input.filter((event) => event.action === "scroll").length > beforeTwoFingerScroll);
  assert(input.some((event) => event.action === "scroll" && event.deltaY !== 0), "two-finger trackpad movement must send a vertical scroll delta");

  for (const width of [390, 768, 1440]) {
    await page.setViewportSize({ width, height: width === 390 ? 844 : 900 });
    const box = await page.locator("#desktop-viewer").boundingBox();
    assert(box && box.x === 0 && box.y === 0 && box.width === width, `viewer geometry failed at ${width}px`);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, `horizontal overflow at ${width}px`);
    if (width === 390) {
      const stage = await page.locator("#desktop-viewport").boundingBox();
      assert(stage && Math.abs(stage.width / stage.height - 1.6) < 0.15, "mobile desktop image should use a landscape stage, not a tall portrait viewport");
    }
  }

  const humanSessionBeforeReconnect = await page.evaluate(async () => await (await fetch("/api/desktop/status")).json());
  await page.locator("#desktop-close").click();
  await page.waitForFunction(() => !document.querySelector("#desktop-viewer").open);
  assert.equal(await page.locator("#desktop-toggle").evaluate((button) => button === document.activeElement), true);
  assert.equal(lease, "human", "closing the viewer must leave the human lease in place");
  assert(!actions.includes("return-to-agent"), "viewer close must not return control implicitly");
  await page.locator("#desktop-toggle").click();
  await page.getByText("You control · GC paused").waitFor();
  const humanSessionAfterReconnect = await page.evaluate(async () => await (await fetch("/api/desktop/status")).json());
  assert.equal(humanSessionAfterReconnect.sessionId, humanSessionBeforeReconnect.sessionId, "viewer reconnect must preserve the desktop session");
  assert.equal(humanSessionAfterReconnect.displayId, humanSessionBeforeReconnect.displayId, "viewer reconnect must preserve the display");
  assert.equal(humanSessionAfterReconnect.lease, "human", "viewer reconnect must preserve human input ownership");
  await page.locator("#desktop-return").click();
  await page.getByText("View only · GC may control").waitFor();
  const viewOnlyInputCount = input.length;
  await page.locator("#desktop-touch-mode").click();
  const viewOnlyBox = await page.locator("#desktop-viewport").boundingBox();
  assert(viewOnlyBox);
  const viewMidX = viewOnlyBox.x + viewOnlyBox.width / 2;
  const viewMidY = viewOnlyBox.y + viewOnlyBox.height / 2;
  await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [
    { id: 41, x: viewMidX - 20, y: viewMidY, radiusX: 4, radiusY: 4, force: 1 },
    { id: 42, x: viewMidX + 20, y: viewMidY, radiusX: 4, radiusY: 4, force: 1 },
  ] });
  await cdp.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: [
    { id: 41, x: viewMidX - 36, y: viewMidY, radiusX: 4, radiusY: 4, force: 1 },
    { id: 42, x: viewMidX + 36, y: viewMidY, radiusX: 4, radiusY: 4, force: 1 },
  ] });
  await page.waitForFunction(() => Number(document.querySelector("#desktop-stream").style.getPropertyValue("--desktop-zoom")) > 1);
  const panBefore = await page.locator("#desktop-stream").evaluate((image) => image.style.getPropertyValue("--desktop-pan-y"));
  await cdp.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: [
    { id: 41, x: viewMidX - 36, y: viewMidY + 24, radiusX: 4, radiusY: 4, force: 1 },
    { id: 42, x: viewMidX + 36, y: viewMidY + 24, radiusX: 4, radiusY: 4, force: 1 },
  ] });
  await page.waitForFunction((previous) => document.querySelector("#desktop-stream").style.getPropertyValue("--desktop-pan-y") !== previous, panBefore);
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  assert.equal(input.length, viewOnlyInputCount, "view-only pinch must not send desktop input");
  await page.locator("#desktop-view-reset").click();
  await page.locator("#desktop-trackpad").click();
  await page.locator("#desktop-more > summary").click();
  page.once("dialog", (dialog) => {
    assert.match(dialog.message(), /saved GNOME workspace and files will be kept/);
    dialog.accept();
  });
  await page.locator("#desktop-stop").click();
  await page.getByText("Choose a desktop workspace above to open it here.").waitFor();
  assert.deepEqual(actions.filter((action) => action === "start" || action === "stop"), ["start", "stop", "start", "stop"]);
  await page.locator("#desktop-details > summary").click();
  assert.equal(await page.locator("#desktop-profile-delete").isVisible(), true);
  page.once("dialog", (dialog) => {
    assert.match(dialog.message(), /cannot be undone/);
    dialog.accept();
  });
  await page.locator("#desktop-profile-delete").click();
  await page.waitForFunction(() => document.querySelector("#desktop-profile-delete").hidden);
  await page.locator("#desktop-details > summary").click();
  assert(actions.includes("delete-profile"));
  assert.deepEqual(errors, []);
} finally {
  await browser.close();
  server.closeAllConnections?.();
  await new Promise((resolve) => server.close(resolve));
}
