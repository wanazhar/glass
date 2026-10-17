// Isolated browser checks for startup, recovery and read-only thread attachment.
import { chromium } from "playwright-core";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { resolve, sep, extname } from "node:path";
const root = fileURLToPath(new URL("../dist/", import.meta.url));
const counts = new Map();
const server = createServer(async (req, res) => {
  const path = new URL(req.url, "http://localhost").pathname;
  counts.set(path, (counts.get(path) || 0) + 1);
  if (path.startsWith("/api/")) {
    res.setHeader("Content-Type", "application/json");
    res.end(JSON.stringify(path === "/api/status" ? { profile: "workspace-edit", workspace: "/tmp" } : { threads: [], projects: [] }));
    return;
  }
  try {
    const file = resolve(root, "." + (path === "/" ? "/index.html" : path));
    if (!file.startsWith(root.endsWith(sep) ? root : root + sep)) throw Error("Invalid path");
    const body = await readFile(file);
    res.setHeader("Content-Type", ({ ".js": "text/javascript", ".css": "text/css", ".html": "text/html", ".svg": "image/svg+xml", ".woff2": "font/woff2", ".webmanifest": "application/manifest+json" })[extname(file)] || "application/octet-stream");
    res.setHeader("Cache-Control", "no-store");
    res.end(body);
  } catch { res.writeHead(404).end(); }
});
await new Promise(r => server.listen(0, "127.0.0.1", r));
const url = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({ executablePath: process.env.GC_BROWSER_BINARY || (existsSync("/snap/chromium/current/usr/lib/chromium-browser/chrome") ? "/snap/chromium/current/usr/lib/chromium-browser/chrome" : undefined), headless: true, args: ["--no-sandbox", "--disable-dev-shm-usage"] });
const deferred = () => { let resolve; const promise = new Promise(r => resolve = r); return { promise, resolve }; };
const geometry = page => page.evaluate(() => {
  const box = s => { const b = document.querySelector(s).getBoundingClientRect(); return { x: b.x, y: b.y, width: b.width, height: b.height }; };
  return { lockup: box(".launch-lockup"), cube: box(".launch-cube"), wordmark: box(".launch-wordmark"), viewport: innerWidth };
});
const stable = (a, b) => {
  assert(Math.abs(a.lockup.y - b.lockup.y) < 1, "Startup status shifted branding vertically");
  assert(Math.abs((b.cube.x + b.wordmark.x + b.wordmark.width) / 2 - b.viewport / 2) < 1, "Logo group is skewed horizontally");
};
try {
  for (const viewport of [{ width: 1440, height: 900 }, { width: 390, height: 844 }, { width: 1024, height: 768 }, { width: 844, height: 390 }]) {
    const context = await browser.newContext({ viewport, serviceWorkers: "block", reducedMotion: "reduce" });
    const page = await context.newPage();
    const boot = deferred(), status = deferred(), threadRequest = deferred();
    const errors = [], paths = [];
    page.on("pageerror", e => errors.push(String(e)));
    page.on("request", r => paths.push(new URL(r.url()).pathname));
    await page.route("**/boot.js?*", async route => { await boot.promise; await route.continue(); });
    await page.route("**/api/status", async route => { await status.promise; await route.continue(); });
    await page.route("**/api/threads?*", async route => { threadRequest.resolve(); await route.continue(); });
    await page.clock.install();
    await page.goto(url, { waitUntil: "commit" });
    await page.waitForFunction(() => document.querySelector('link[rel="stylesheet"]')?.sheet);
    await page.evaluate(() => document.fonts.ready);
    const before = await geometry(page);
    boot.resolve();
    await page.waitForFunction(() => document.querySelector("#launch").dataset.booted);
    stable(before, await geometry(page));
    // Threads must start without waiting for the independent status response.
    await Promise.race([threadRequest.promise, new Promise((_, reject) => setTimeout(() => reject(Error("Startup reads are serialized")), 3000))]);
    status.resolve();
    await page.locator(".app").waitFor({ state: "visible" });
    assert(await page.locator("#launch").evaluate(n => n.hidden));
    assert(!paths.some(p => /\/(?:pdf|mermaid|math|syntax|workflows)[^/]*\.js$/.test(p)), "Optional renderer loaded on startup");
    assert.equal(paths.filter(p => p === "/app.js").length, 1);
    assert.equal(paths.filter(p => p === "/boot.js").length, 1);
    assert.equal(paths.filter(p => p === "/fonts/inter-latin.woff2").length, 1);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    assert.deepEqual(errors, []);
    await page.screenshot({ path: `/tmp/gc-web-ready-${viewport.width}.png` });
    await context.close();
  }
  console.log("PASS stable centered startup, parallel reads, slow recovery, lazy assets and four viewport sizes");

  const slowContext = await browser.newContext({ serviceWorkers: "block", reducedMotion: "reduce" });
  const slowPage = await slowContext.newPage();
  const appGate = deferred();
  await slowPage.route("**/app.js?*", async route => { await appGate.promise; await route.continue(); });
  await slowPage.clock.install();
  await slowPage.goto(url, { waitUntil: "commit" });
  await slowPage.waitForFunction(() => document.querySelector("#launch").dataset.booted);
  await slowPage.evaluate(() => document.fonts.ready);
  const slowBefore = await geometry(slowPage);
  await slowPage.clock.fastForward(15100);
  await slowPage.waitForFunction(() => document.querySelector("#launch").dataset.state === "slow");
  stable(slowBefore, await geometry(slowPage));
  appGate.resolve();
  await slowPage.locator(".app").waitFor({ state: "visible" });
  await slowContext.close();

  for (const asset of ["style.css", "app.js"]) {
    const context = await browser.newContext({ viewport: { width: 390, height: 844 }, serviceWorkers: "block" });
    const page = await context.newPage();
    const matcher = url => url.pathname === "/" + asset;
    await page.route(matcher, route => route.abort());
    await page.goto(url);
    await page.waitForFunction(() => document.querySelector("#launch").dataset.state === "error");
    const g = await geometry(page);
    stable(g, g);
    assert.equal(await page.locator("#launch").evaluate(n => getComputedStyle(n).position), "fixed");
    assert(await page.locator(".app").evaluate(n => n.hidden && n.inert));
    assert(await page.locator("#launch-retry").isVisible());
    await page.screenshot({ path: `/tmp/gc-web-failed-${asset}.png` });
    await page.unroute(matcher);
    await Promise.all([page.waitForEvent("domcontentloaded"), page.locator("#launch-retry").click()]);
    await page.locator(".app").waitFor({ state: "visible" });
    await context.close();
  }
  console.log("PASS styled recovery with missing CSS/module and successful retry");

  const context = await browser.newContext({ viewport: { width: 1024, height: 1365 }, colorScheme: "light", serviceWorkers: "block" });
  const page = await context.newPage();
  const thread = { session: "tui-fixture", title: "TUI saved conversation", workspace: "/tmp", workspace_assigned: true, workspace_available: true };
  const calls = [];
  await page.route("**/api/**", async route => {
    const path = new URL(route.request().url()).pathname;
    calls.push(path);
    let json = {};
    if (path === "/api/status") json = { profile: "workspace-edit", workspace: "/tmp" };
    if (path === "/api/threads") json = { threads: [thread], projects: [] };
    if (path === "/api/history") json = { records: [{ index: 0, record: { k: "user", t: "Saved TUI question" } }, { index: 1, record: { k: "assistant", t: "Saved TUI answer" } }], before: 0 };
    if (path === "/api/open") return route.fulfill({ status: 400, json: { error: "No such file or directory (os error 2)" } });
    await route.fulfill({ json });
  });
  await page.goto(url + "/?thread=tui-fixture");
  await page.locator(".app").waitFor({ state: "visible" });
  await page.getByText("Saved TUI answer", { exact: true }).waitFor();
  assert(calls.indexOf("/api/history") < calls.indexOf("/api/open"));
  assert.equal(await page.locator("#connection").textContent(), "History only");
  assert(await page.locator("#send").isDisabled());
  assert.match(await page.locator("#notice").textContent(), /Saved conversation loaded/);
  assert.equal(calls.filter(p => p === "/api/history").length, 1);
  const contrast = await page.locator("#notice").evaluate(n => {
    const luminance = color => {
      const channels = color.match(/[\d.]+/g).slice(0, 3).map(Number).map(v => {
        v /= 255; return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
      });
      return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
    };
    const style = getComputedStyle(n), a = luminance(style.color), b = luminance(style.backgroundColor);
    return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
  });
  assert(contrast >= 4.5, "Light-mode error banner is unreadable");
  await page.screenshot({ path: "/tmp/gc-web-tui-history-only.png" });
  await context.close();
  console.log("PASS TUI history remains readable when live attachment fails, without Load earlier");

  // Exercise the actual service worker, including a damaged cached application.
  const cachedContext = await browser.newContext();
  const cachedPage = await cachedContext.newPage();
  await cachedPage.goto(url);
  await cachedPage.locator(".app").waitFor({ state: "visible" });
  await cachedPage.evaluate(() => navigator.serviceWorker.ready);
  await cachedPage.waitForFunction(() => navigator.serviceWorker.controller);
  const fontRequests = counts.get("/fonts/inter-latin.woff2");
  await cachedPage.evaluate(async () => {
    await fetch("/fonts/inter-latin.woff2", { cache: "no-store" });
    await fetch("/fonts/inter-latin.woff2", { cache: "no-store" });
    const name = (await caches.keys()).find(k => k.startsWith("glass-shell-"));
    const cache = await caches.open(name);
    const app = document.querySelector('link[rel="modulepreload"]').href;
    await cache.put(app, new Response("throw new Error('damaged cached application')", { headers: { "Content-Type": "text/javascript" } }));
  });
  assert.equal(counts.get("/fonts/inter-latin.woff2"), fontRequests, "Cached fonts downloaded again");
  await cachedPage.goto(url + "/?damaged=1");
  await cachedPage.waitForFunction(() => document.querySelector("#launch").dataset.state === "error");
  await Promise.all([cachedPage.waitForEvent("domcontentloaded"), cachedPage.locator("#launch-retry").click()]);
  await cachedPage.locator(".app").waitFor({ state: "visible" });
  await cachedContext.setOffline(true);
  await cachedPage.reload();
  await cachedPage.waitForFunction(() => document.querySelector("#launch").dataset.state === "error");
  assert(await cachedPage.locator("#launch-retry").isVisible());
  const healthyCache = await cachedPage.evaluate(async () => {
    const app = document.querySelector('link[rel="modulepreload"]').href;
    return (await caches.match(app)).text();
  });
  await Promise.all([cachedPage.waitForEvent("domcontentloaded"), cachedPage.locator("#launch-retry").click()]);
  await cachedPage.waitForFunction(() => document.querySelector("#launch").dataset.state === "error");
  assert.equal(await cachedPage.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--glass-ui-ready").trim()), "1");
  assert.equal(await cachedPage.evaluate(async () => (await caches.match(document.querySelector('link[rel="modulepreload"]').href)).text()), healthyCache, "Offline retry discarded the healthy cached application");
  await cachedContext.setOffline(false);
  await Promise.all([cachedPage.waitForEvent("domcontentloaded"), cachedPage.locator("#launch-retry").click()]);
  await cachedPage.locator(".app").waitFor({ state: "visible" });
  await cachedContext.close();
  console.log("PASS service-worker retry escapes damaged cache and reuses cached fonts");
} finally {
  await browser.close();
  await new Promise(r => server.close(r));
}
