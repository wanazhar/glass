import { chromium } from "playwright-core";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
const url = process.argv[2];
const browser = await chromium.launch({
  executablePath:
    process.env.GC_BROWSER_BINARY ||
    (existsSync("/snap/chromium/current/usr/lib/chromium-browser/chrome")
      ? "/snap/chromium/current/usr/lib/chromium-browser/chrome"
      : undefined),
  headless: true,
  args: ["--no-sandbox", "--disable-dev-shm-usage"],
});
const context = await browser.newContext({
  viewport: { width: 1440, height: 1000 },
});
// Plain HTTP has getRandomValues but does not expose randomUUID.
await context.addInitScript(() => Object.defineProperty(crypto, "randomUUID", { value: undefined }));
const page = await context.newPage();
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
const paths = [];
page.on("request", (r) => paths.push(new URL(r.url()).pathname));
try {
  await page.goto(url);
  await page
    .getByRole("button", { name: "Shared runtime workspace" })
    .waitFor();
  assert(
    !paths.some(
      (p) => p.includes("mermaid") || p.includes("pdf") || p.includes("math"),
    ),
    "Heavy preview loaded on welcome",
  );
  await page.getByRole("button", { name: "Shared runtime workspace" }).click();
  await page
    .locator("#timeline")
    .getByText("A quieter place to work")
    .waitFor();
  await page.waitForFunction(
    () => document.querySelector("#send").disabled === false,
  );
  await page.locator(".katex").first().waitFor();
  await page.locator(".hljs-keyword").first().waitFor();
  await page
    .getByRole("button", { name: "Render diagram", exact: true })
    .click();
  await page.locator(".diagram-render svg").waitFor({ timeout: 30000 });
  assert(
    await page
      .locator(".diagram-render")
      .textContent()
      .then((s) => s.includes("Glass runtime")),
    "Diagram labels missing",
  );
  await page.screenshot({
    path: "/tmp/gc-connect-desktop.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "Preview", exact: true }).click();
  await page.locator("#preview iframe").waitFor();
  await page.waitForTimeout(100);
  assert.equal(await page.evaluate(() => window.__unsafe), undefined);
  await page.locator("#preview-close").click();
  await page.getByRole("link", { name: "the project notes" }).click();
  await page
    .locator("#preview")
    .getByText("PreviewReady notes stay on the host.")
    .waitFor();
  await page.locator("#preview-close").click();
  await page.getByRole("link", { name: "a small JSON artifact" }).click();
  await page.locator("#preview .json-node").waitFor();
  await page.locator("#preview-close").click();
  await page.getByRole("link", { name: "a PDF artifact" }).click();
  await page.locator("#preview canvas").waitFor({ timeout: 30000 });
  await page.locator("#preview").getByText("Page 1 of 1").waitFor();
  await page.locator("#preview-close").click();
  await page.locator("#prompt").fill("Run a command");
  await page.locator("#send").click();
  await page
    .getByText("Decision received. Work continues.")
    .first()
    .waitFor({ timeout: 30000 });
  await page.waitForFunction(
    () =>
      document.querySelector("#activity-text").textContent ===
      "Ready when you are",
  );
  await page.locator("#prompt").fill("Ask a question");
  await page.locator("#send").click();
  await page.locator("#interaction textarea").waitFor({ timeout: 30000 });
  await page.reload();
  await page.locator("#interaction textarea").waitFor({ timeout: 30000 });
  await page.locator("#interaction textarea").fill("A custom answer");
  await page.getByRole("button", { name: "Send answer", exact: true }).click();
  await page.waitForFunction(
    () =>
      document.querySelector("#activity-text").textContent ===
      "Ready when you are",
    { timeout: 30000 },
  );
  await page.locator("#prompt").fill("Request approval");
  await page.locator("#send").click();
  await page
    .getByRole("button", { name: "Deny", exact: true })
    .waitFor({ timeout: 30000 });
  await page.getByRole("button", { name: "Deny", exact: true }).click();
  await page.waitForFunction(
    () =>
      document.querySelector("#activity-text").textContent ===
      "Ready when you are",
    { timeout: 30000 },
  );
  await page.locator('#prompt').fill('HOLD-WEB-STREAM');
  await page.locator('#send').click();
  await page.waitForFunction(()=>document.querySelector('#activity').classList.contains('working'));
  await page.locator('#prompt').fill('SteeringReady correction');
  await page.locator('#send').click();
  await page.waitForFunction(()=>document.querySelector('#activity-text').textContent==='Ready when you are',{timeout:15000});
  await page.locator('#timeline').getByText(/Follow-up.*SteeringReady correction/).first().waitFor();
  await page.locator("#prompt").fill("Hello streaming");
  await page.locator("#send").click();
  await page.getByText("StreamingReady.").first().waitFor({ timeout: 30000 });
  await page.waitForFunction(
    () =>
      document.querySelector("#activity-text").textContent ===
      "Ready when you are",
    { timeout: 30000 },
  );
  await page.locator("#details-toggle").click();
  await page.getByRole("button", { name: "Reasoning", exact: true }).click();
  await page
    .locator("#detail-content")
    .getByText("Considering the workspace and preserving context.")
    .waitFor();
  await page.locator("#details-close").click();
  await page.reload();
  await page
    .locator("#timeline")
    .getByText("StreamingReady.")
    .first()
    .waitFor();
  assert.equal(
    await page
      .locator("#timeline .message.user")
      .filter({ hasText: "Hello streaming" })
      .count(),
    1,
  );
  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForTimeout(200);
  await page.screenshot({ path: "/tmp/gc-connect-mobile.png", fullPage: true });
  assert(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
    "Horizontal mobile overflow",
  );
  await page.locator("#menu").click();
  await page.waitForFunction(
    () => document.querySelector("#sidebar").getBoundingClientRect().left === 0,
  );
  await page.locator("#scrim").click({ position: { x: 370, y: 400 } });
  const manifest = await page.evaluate(async () =>
    fetch("/manifest.webmanifest").then((r) => r.json()),
  );
  assert.equal(manifest.display, "standalone");
  assert.equal(manifest.name, "Glass");
  assert.deepEqual(errors, []);
  console.log(
    "PASS: desktop + iPhone layouts, lazy rendering, Mermaid, math, syntax, safe HTML, files, commands, custom answers, denied approval, reasoning, reload and PWA",
  );
} catch(error) {
  console.error('Browser surface:',(await page.locator('body').innerText()).slice(-10000));
  await page.screenshot({path:'/tmp/gc-connect-browser-failure.png'});
  throw error;
} finally {
  await context.close();
  await browser.close();
}
