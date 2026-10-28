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
const actions = [];
const input = [];
const jpg = Buffer.from(
  "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAP//////////////////////////////////////////////////////////////////////////////////////2wBDAf//////////////////////////////////////////////////////////////////////////////////////wAARCAABAAEDASIAAhEBAxEB/8QAFQABAQAAAAAAAAAAAAAAAAAAAAb/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oADAMBAAIQAxAAAAF//8QAFBABAAAAAAAAAAAAAAAAAAAAAP/aAAgBAQABBQJ//8QAFBEBAAAAAAAAAAAAAAAAAAAAAP/aAAgBAwEBPwF//8QAFBEBAAAAAAAAAAAAAAAAAAAAAP/aAAgBAgEBPwF//8QAFBABAAAAAAAAAAAAAAAAAAAAAP/aAAgBAQAGPwJ//8QAFBABAAAAAAAAAAAAAAAAAAAAAP/aAAgBAQABPyF//9oADAMBAAIAAwAAABD/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oACAEDAQE/EB//xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oACAECAQE/EB//xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oACAEBAwE/EB//2Q==",
  "base64",
);
const testModule = `
  import { desktopSurface } from "/src/desktop.js";
  const api = async (path, data, signal) => {
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
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({
      available: Boolean(session), sessionId: session, mode: session ? "managedVirtualSession" : null,
      displayId: session ? ":81" : null, workspaceRoot: "/work/glass", lease,
      width: session ? 1280 : null, height: session ? 800 : null,
      frameSeq: session ? 18 : null, frameAgeMs: session ? 24 : null, error: null,
    }));
    return;
  }
  if (url.pathname === "/api/desktop/stream") {
    if (!session) {
      response.writeHead(409, { "content-type": "application/json" });
      response.end(JSON.stringify({ error: "No managed desktop session", lease }));
      return;
    }
    response.writeHead(200, { "content-type": "multipart/x-mixed-replace; boundary=glassframe" });
    response.write(`--glassframe\r\nContent-Type: image/jpeg\r\nContent-Length: ${jpg.length}\r\nX-Frame-Seq: 18\r\nX-Frame-Time-Ms: 24\r\n\r\n`);
    response.write(jpg);
    response.write(`\r\n--glassframe\r\nContent-Type: image/jpeg\r\nContent-Length: ${jpg.length}\r\nX-Frame-Seq: 19\r\nX-Frame-Time-Ms: 25\r\n\r\n`);
    return;
  }
  if (url.pathname.startsWith("/api/desktop/")) {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    const body = chunks.length ? JSON.parse(Buffer.concat(chunks).toString()) : {};
    response.setHeader("content-type", "application/json");
    if (url.pathname === "/api/desktop/session" && body.action === "start") {
      session = `managed-${nextSession++}`;
      actions.push(body.action);
      response.end(JSON.stringify({ started: true, sessionId: session, mode: "managedVirtualSession", displayId: ":81", width: 1280, height: 800 }));
      return;
    }
    if (url.pathname === "/api/desktop/session" && body.action === "stop") {
      session = null;
      lease = "agent";
      actions.push(body.action);
      response.end(JSON.stringify({ stopped: true }));
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
  await page.locator("#desktop-toggle").click();
  await page.locator("#desktop-viewer").waitFor({ state: "visible" });
  await page.locator("#desktop-start").click();
  await page.getByText("View only · GC may control").waitFor();
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
  await page.locator("#desktop-stream").dispatchEvent("error");
  await page.getByText("Stream disconnected · desktop session continues").waitFor();
  assert.equal(lease, "human", "stream failure must leave the human lease in place");
  assert(!actions.includes("return-to-agent"), "stream failure must not return control implicitly");
  await page.locator("#desktop-more > summary").click();
  await page.locator("#desktop-reconnect").click();
  await page.getByText(/^Live · updated/).waitFor();
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
  await page.waitForTimeout(50);
  assert(input.some((event) => event.action === "text" && event.text === "hello host"));
  assert(input.some((event) => event.action === "key" && event.key === "Enter" && event.pressed));
  assert(input.some((event) => event.action === "scroll" && event.deltaY === 40));
  assert(input.some((event) => event.action === "pointer-button" && event.button === "left" && event.pressed));
  await page.locator("#desktop-touch-mode").click();
  assert.equal(await page.locator("#desktop-touch-mode").getAttribute("aria-pressed"), "true");
  assert.match(await page.locator("#desktop-touch-hint").textContent(), /pinch to zoom/);
  const directPoint = await page.locator("#desktop-viewport").boundingBox();
  assert(directPoint);
  await page.touchscreen.tap(directPoint.x + directPoint.width * 0.65, directPoint.y + directPoint.height * 0.65);
  assert(input.some((event) => event.action === "pointer-move" && event.x > 0.5 && event.y > 0.5));
  const cdp = await page.context().newCDPSession(page);
  const midX = directPoint.x + directPoint.width / 2;
  const midY = directPoint.y + directPoint.height / 2;
  await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [
    { id: 20, x: midX - 20, y: midY + 20, radiusX: 4, radiusY: 4, force: 1 },
  ] });
  await page.waitForTimeout(510);
  assert(input.some((event) => event.action === "pointer-button" && event.button === "right" && event.pressed));
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  await page.waitForTimeout(60);
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
  await page.waitForTimeout(40);
  assert(input.some((event) => event.action === "key" && event.key === "Control" && event.pressed));
  assert(input.some((event) => event.action === "key" && event.key === "A" && event.pressed));
  assert(input.some((event) => event.action === "key" && event.key === "Escape" && event.pressed));
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
  await page.waitForFunction(() => true);
  await page.waitForTimeout(40);
  assert(input.filter((event) => event.action === "scroll").length > beforeTwoFingerScroll);

  for (const width of [390, 768, 1440]) {
    await page.setViewportSize({ width, height: width === 390 ? 844 : 900 });
    const box = await page.locator("#desktop-viewer").boundingBox();
    assert(box && box.x === 0 && box.y === 0 && box.width === width, `viewer geometry failed at ${width}px`);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, `horizontal overflow at ${width}px`);
  }

  await page.locator("#desktop-close").click();
  await page.waitForFunction(() => !document.querySelector("#desktop-viewer").open);
  assert.equal(lease, "human", "closing the viewer must leave the human lease in place");
  assert(!actions.includes("return-to-agent"), "viewer close must not return control implicitly");
  await page.locator("#desktop-toggle").click();
  await page.getByText("You control · GC paused").waitFor();
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
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  assert.equal(input.length, viewOnlyInputCount, "view-only pinch must not send desktop input");
  await page.locator("#desktop-view-reset").click();
  await page.locator("#desktop-trackpad").click();
  await page.locator("#desktop-more > summary").click();
  page.once("dialog", (dialog) => dialog.accept());
  await page.locator("#desktop-stop").click();
  await page.getByText("No managed desktop is running on this host.").waitFor();
  assert.deepEqual(actions.filter((action) => action === "start" || action === "stop"), ["start", "stop"]);
  assert.deepEqual(errors, []);
} finally {
  await browser.close();
  server.closeAllConnections?.();
  await new Promise((resolve) => server.close(resolve));
}
