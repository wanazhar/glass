import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { performance } from "node:perf_hooks";

const require = createRequire(import.meta.url);
const { chromium } = require("playwright");
const iterations = Number(process.env.GLASS_BENCH_ITERATIONS || 50);
const executablePath = process.env.CHROME_PATH;
const fixture = readFileSync(new URL("../tests/fixtures/basic.html", import.meta.url), "utf8");
const dataUrl = "data:text/html;base64," + Buffer.from(fixture).toString("base64");

const startupStarted = performance.now();
const browser = await chromium.launch({
  headless: true,
  ...(executablePath ? { executablePath } : {}),
});
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
await page.goto(dataUrl, { waitUntil: "load" });
const startup_ms = performance.now() - startupStarted;

async function measure(name, count, operation) {
  const samples = [];
  for (let index = 0; index < count; index += 1) {
    const started = performance.now();
    await operation();
    samples.push(performance.now() - started);
  }
  samples.sort((a, b) => a - b);
  const total = samples.reduce((sum, value) => sum + value, 0);
  const percentile = (ratio) => samples[Math.round((samples.length - 1) * ratio)];
  return {
    operation: name,
    iterations: count,
    total_ms: total,
    average_ms: total / count,
    p50_ms: percentile(0.5),
    p95_ms: percentile(0.95),
  };
}

const results = [
  await measure("evaluate", iterations, () => page.evaluate(() => 1 + 1)),
  await measure("text", iterations, () => page.locator("body").innerText()),
  await measure("dom_snapshot", Math.max(5, Math.floor(iterations / 5)), () =>
    page.locator("body").ariaSnapshot(),
  ),
  await measure("screenshot", Math.max(5, Math.floor(iterations / 5)), () =>
    page.screenshot({ type: "png" }),
  ),
];

console.log(JSON.stringify({
  tool: "playwright",
  version: require("playwright/package.json").version,
  iterations,
  startup_ms,
  browser: executablePath || "Playwright-managed Chromium",
  results,
}, null, 2));
await browser.close();
