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
await context.addInitScript(() =>
  Object.defineProperty(crypto, "randomUUID", { value: undefined }),
);
const page = await context.newPage();
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
const paths = [];
page.on("request", (r) => paths.push(new URL(r.url()).pathname));
try {
  await page.goto(url);
  await page.evaluate(() => document.fonts.ready);
  assert(
    await page.evaluate(() => document.fonts.check('14px "Glass Sans"')),
    "Bundled UI font did not load",
  );
  assert.match(
    await page
      .locator("#prompt")
      .evaluate((n) => getComputedStyle(n).fontFamily),
    /Glass Sans/,
  );
  await page.emulateMedia({ colorScheme: "light" });
  await page.waitForFunction(() =>
    document.documentElement.classList.contains("light"),
  );
  await page.emulateMedia({ colorScheme: "dark" });
  await page.waitForFunction(
    () => !document.documentElement.classList.contains("light"),
  );

  await page
    .getByRole("button", { name: "Shared runtime workspace" })
    .waitFor();
  assert(
    !paths.some(
      (p) => p.includes("mermaid") || p.includes("pdf") || p.includes("math"),
    ),
    "Heavy preview loaded on welcome",
  );
  await page.locator("#prompt").fill("/");
  await page.locator("#commands").waitFor({ state: "visible" });
  await page.locator(".command-choice").filter({ hasText: "/work" }).click();
  await page.keyboard.press("Escape");
  assert(
    await page.locator("#commands").evaluate((n) => n.open),
    "Escape closed nested commands",
  );
  await page.locator("#command-search").fill("thinking");
  await page.keyboard.press("Enter");
  assert(
    await page
      .locator("#details")
      .evaluate((n) => n.classList.contains("open")),
  );
  await page.locator("#details-close").click();
  await page.locator("#prompt").fill("/");
  await page.locator("#commands").waitFor({ state: "visible" });
  await page.keyboard.press("Escape");
  assert(
    !(await page.locator("#commands").evaluate((n) => n.open)),
    "Root Escape did not close commands",
  );
  await page.locator("#prompt").fill("");
  await page.getByRole("button", { name: "Shared runtime workspace" }).click();
  await page
    .locator("#timeline")
    .getByText("A quieter place to work")
    .waitFor();
  await page.waitForFunction(
    () => document.querySelector("#send").disabled === false,
  );
  const nativeCommand = async (name) => {
    await page.locator("#open-commands").click();
    await page.locator("#command-search").fill(name);
    await page
      .getByRole("option")
      .filter({ hasText: new RegExp(`^/${name}\\b`) })
      .click();
    await page.locator("#command-view").waitFor({ state: "visible" });
    await page.waitForFunction(
      () =>
        !document.querySelector('#command-form button[type="submit"]').disabled,
    );
  };
  await nativeCommand("todo");
  await page.locator('#command-form [name="action"]').selectOption("add");
  await page
    .locator('#command-form [name="args"]')
    .fill("Browser checklist survives reload");
  await page.locator('#command-form button[type="submit"]').click();
  await page
    .locator("#command-result")
    .getByText("Browser checklist survives reload", { exact: false })
    .waitFor();
  await page.locator('#command-form [name="action"]').selectOption("clear");
  await page.locator('#command-form [name="confirmed"]').check();
  await page.locator('#command-form button[type="submit"]').click();
  await page.waitForFunction(
    () =>
      !document.querySelector('#command-form button[type="submit"]').disabled,
  );
  await page.locator("#command-close").click();
  await nativeCommand("status");
  await page
    .locator("#command-result")
    .getByText("mock", { exact: true })
    .waitFor();
  await page.locator("#command-close").click();
  await nativeCommand("tasks");
  await page.locator('#command-form [name="action"]').selectOption("task-add");
  await page
    .locator('#command-form [name="args"]')
    .fill("Browser task fixture");
  await page
    .locator('#command-form [name="acceptance"]')
    .fill("Browser assertions pass");
  await page.locator('#command-form button[type="submit"]').click();
  await page.waitForFunction(
    () =>
      !document.querySelector('#command-form button[type="submit"]').disabled,
  );
  assert.equal(await page.locator("#command-status").textContent(), "");
  await page.locator("#command-close").click();
  await nativeCommand("name");
  await page
    .locator('#command-form [name="args"]')
    .fill("Shared runtime workspace");
  await page.locator('#command-form button[type="submit"]').click();
  await page.waitForFunction(
    () =>
      !document.querySelector('#command-form button[type="submit"]').disabled,
  );
  assert.equal(
    await page.locator("#thread-name").textContent(),
    "Shared runtime workspace",
  );
  await page.locator("#command-close").click();
  await nativeCommand("login");
  await page.locator('#command-form [name="action"]').selectOption("api");
  assert.equal(
    await page.locator('#command-form [name="secret"]').getAttribute("type"),
    "password",
  );
  assert(await page.locator('#command-form [name="secret"]').isVisible());
  await page
    .locator('#command-form [name="secret"]')
    .fill("fixture-only-secret");
  await page.locator('#command-form [name="action"]').selectOption("list");
  assert(await page.locator('#command-form [name="secret"]').isDisabled());
  assert.equal(
    await page.locator('#command-form [name="secret"]').inputValue(),
    "",
  );
  assert.equal(
    await page
      .locator("#command-form")
      .evaluate((form) => new FormData(form).has("secret")),
    false,
  );
  await page.locator("#command-close").click();
  const exportResponse = await page.request.get(
    new URL("/api/export?id=web-fixture", url).href,
  );
  assert(exportResponse.ok());
  assert((await exportResponse.text()).includes("A quieter place to work"));
  await page.locator(".katex").first().waitFor();
  await page.locator(".hljs-keyword").first().waitFor();
  await page.locator(".diagram-render svg").waitFor({ timeout: 30000 });
  assert(
    await page
      .locator(".diagram-render")
      .textContent()
      .then((s) => s.includes("Glass runtime")),
    "Diagram labels missing",
  );
  await page.getByRole("button", { name: "Source", exact: true }).click();
  assert(await page.locator(".diagram-block pre").isVisible());
  await page.getByRole("button", { name: "Diagram", exact: true }).click();
  assert(await page.locator(".diagram-render svg").isVisible());
  assert((await page.locator(".diff-line.add").count()) > 0);
  assert((await page.locator(".diff-line.remove").count()) > 0);
  assert((await page.locator(".table-wrap table th").count()) >= 3);
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
    () => document.querySelector("#activity-text").textContent === "Ready",
  );
  await page.locator("#prompt").fill("Ask a question");
  await page.locator("#send").click();
  await page.locator("#interaction textarea").waitFor({ timeout: 30000 });
  await page.reload();
  await page.locator("#interaction textarea").waitFor({ timeout: 30000 });
  await page.locator("#interaction textarea").fill("A custom answer");
  await page.getByRole("button", { name: "Send answer", exact: true }).click();
  await page.waitForFunction(
    () => document.querySelector("#activity-text").textContent === "Ready",
    { timeout: 30000 },
  );
  await page.locator("#prompt").fill("Request approval");
  await page.locator("#send").click();
  await page
    .getByRole("button", { name: "Deny", exact: true })
    .waitFor({ timeout: 30000 });
  await page.getByRole("button", { name: "Deny", exact: true }).click();
  await page.waitForFunction(
    () => document.querySelector("#activity-text").textContent === "Ready",
    { timeout: 30000 },
  );
  await page.locator("#prompt").fill("HOLD-WEB-STREAM");
  await page.locator("#send").click();
  await page.waitForFunction(() =>
    document.querySelector("#activity").classList.contains("working"),
  );
  await page.locator("#prompt").fill("SteeringReady correction");
  await page.locator("#send").click();
  await page.waitForFunction(
    () => document.querySelector("#activity-text").textContent === "Ready",
    { timeout: 15000 },
  );
  await page
    .locator("#timeline")
    .getByText(/Follow-up.*SteeringReady correction/)
    .first()
    .waitFor();
  await page.locator("#prompt").fill("Hello streaming");
  await page.locator("#send").click();
  await page.getByText("StreamingReady.").first().waitFor({ timeout: 30000 });
  await page.waitForFunction(
    () => document.querySelector("#activity-text").textContent === "Ready",
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
  await page.locator("#prompt").fill("stability fixture");
  await page.locator("#send").click();
  await page
    .locator(".message.streaming")
    .getByText("StableParagraph", { exact: false })
    .waitFor();
  await page.evaluate(
    () =>
      (window.__stableBlock = document.querySelector(
        ".message.streaming .markdown-block",
      )),
  );
  await page
    .locator(".message.streaming")
    .getByText("Frame 8 remains readable.", { exact: true })
    .waitFor();
  assert(
    await page.evaluate(
      () =>
        window.__stableBlock.isConnected &&
        window.__stableBlock ===
          document.querySelector(".message.streaming .markdown-block"),
    ),
    "Completed streamed block was replaced",
  );
  await page.waitForFunction(
    () => document.querySelector("#activity-text").textContent === "Ready",
  );
  await page.waitForTimeout(100);
  assert(
    await page.evaluate(() => window.__stableBlock.isConnected),
    "Settled history replaced stable streamed content",
  );
  await page.locator("#prompt").fill("interleaved fixture");
  await page.locator("#send").click();
  await page
    .locator("#timeline")
    .getByText("BeforeToolReady.", { exact: true })
    .waitFor();
  await page.waitForFunction(
    () => document.querySelector("#activity-text").textContent === "Ready",
  );
  await page.locator("#prompt").fill("cap fixture");
  await page.locator("#send").click();
  await page.locator(".message .display-cap").waitFor();
  const cap = await page
    .locator(".message:has(.display-cap) .prose")
    .textContent();
  assert(cap.length < 263000, "Streaming display cap exceeded");
  await page.waitForFunction(
    () => document.querySelector("#activity-text").textContent === "Ready",
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
  await page.locator("#details-toggle").click();
  await page.getByRole("button", { name: "Files", exact: true }).click();
  await page.locator(".file-name").filter({ hasText: "notes.md" }).waitFor();
  await page.locator(".file-open").filter({ hasText: "notes.md" }).click();
  await page
    .locator("#preview-body")
    .getByText("PreviewReady notes stay on the host.")
    .waitFor();
  await page.locator("#preview-close").click();
  await page.locator(".file-open").filter({ hasText: "project-two" }).click();
  await page.waitForFunction(() =>
    document.querySelector(".file-path input")?.value.endsWith("/project-two"),
  );
  for (let repeat = 0; repeat < 2; repeat++) {
    await page.locator(".file-open").filter({ hasText: "readme.md" }).click();
    await page.waitForFunction(
      () => document.querySelector("#preview-body img")?.naturalWidth === 1,
    );
    assert(
      (await page.locator("#preview-body img").getAttribute("src")).includes(
        "workspace=",
      ),
    );
    await page.locator("#preview-close").click();
  }
  await page
    .getByRole("button", { name: "New thread here", exact: true })
    .click();
  await page.waitForFunction(
    () =>
      document.querySelector("#workspace-name").textContent === "project-two",
  );
  await page.locator("#details-toggle").click();
  await page.getByRole("button", { name: "Context", exact: true }).click();
  await page
    .locator("#detail-content")
    .getByText(/project-two/)
    .first()
    .waitFor();
  await page.locator("#details-close").click();
  const mobile = await browser.newContext({
    viewport: { width: 390, height: 844 },
    deviceScaleFactor: 3,
    isMobile: true,
    hasTouch: true,
  });
  await mobile.addInitScript(() =>
    document.addEventListener("DOMContentLoaded", () => {
      document.documentElement.style.setProperty("--safe-top", "47px");
      document.documentElement.style.setProperty("--safe-bottom", "34px");
    }),
  );
  const phone = await mobile.newPage();
  phone.on("pageerror", (e) => errors.push(String(e)));
  await phone.goto(url);
  for (const size of [
    { width: 390, height: 844 },
    { width: 375, height: 667 },
    { width: 430, height: 932 },
    { width: 844, height: 390 },
  ]) {
    await phone.setViewportSize(size);
    await phone.evaluate((w) => {
      document.documentElement.style.setProperty(
        "--safe-left",
        w > 700 ? "47px" : "0px",
      );
      document.documentElement.style.setProperty(
        "--safe-right",
        w > 700 ? "47px" : "0px",
      );
    }, size.width);
    await phone.waitForTimeout(100);
    assert(
      await phone.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    );
    assert(
      await phone.evaluate(() => document.body.scrollHeight <= innerHeight + 1),
    );
    if (size.width > 700) {
      await phone.locator("#details-toggle").click();
      const close = await phone.locator("#details-close").boundingBox();
      assert(
        close.x + close.width <= size.width - 47,
        "Landscape drawer close overlaps safe area",
      );
      await phone.locator("#details-close").click();
    }
    if (size.width <= 700) {
      assert.equal(
        await phone
          .locator(".workspace")
          .evaluate((n) => n.getBoundingClientRect().left),
        0,
        "Mobile workspace shifted",
      );
      const menu = phone.locator("#menu");
      assert((await menu.boundingBox()).y >= 47);
      await menu.click();
      await phone.waitForFunction(
        () =>
          document.querySelector("#menu").getAttribute("aria-expanded") ===
          "true",
      );
      await phone.locator("#search").focus();
      assert.equal(
        await phone
          .locator("#search")
          .evaluate((n) => getComputedStyle(n).fontSize),
        "16px",
      );
      await phone.locator("#sidebar-close").click();
      await phone.locator("#details-toggle").click();
      await phone.getByRole("button", { name: "Files", exact: true }).click();
      await phone.locator(".file-list").waitFor();
      await phone.locator("#details-close").click();
    }
  }
  await phone.setViewportSize({ width: 390, height: 844 });
  await phone.evaluate(() => {
    document.documentElement.style.setProperty("--safe-left", "0px");
    document.documentElement.style.setProperty("--safe-right", "0px");
  });
  await phone.waitForTimeout(250);
  await phone.waitForFunction(
    () =>
      document.querySelector("#sidebar").getBoundingClientRect().right <= 0 &&
      document.querySelector(".workspace").getBoundingClientRect().left === 0,
  );
  await phone.screenshot({ path: "/tmp/gc-pwa-standalone-mobile.png" });
  await mobile.close();
  const manifest = await page.evaluate(async () =>
    fetch("/manifest.webmanifest").then((r) => r.json()),
  );
  assert.equal(manifest.display, "standalone");
  assert.equal(manifest.name, "Glass");
  assert.deepEqual(errors, []);
  console.log(
    "PASS: desktop + iPhone layouts, lazy rendering, Mermaid, math, syntax, safe HTML, files, commands, custom answers, denied approval, reasoning, reload and PWA",
  );
} catch (error) {
  console.error(
    "Browser surface:",
    (await page.locator("body").innerText()).slice(-10000),
  );
  await page.screenshot({ path: "/tmp/gc-connect-browser-failure.png" });
  throw error;
} finally {
  await context.close();
  await browser.close();
}
