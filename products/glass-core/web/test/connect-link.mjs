// Opt-in check of the real CLI link, without injected authorization headers.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
const { webkit } = await import(process.env.GC_PLAYWRIGHT_MODULE || "playwright-core");
assert.equal(process.env.GC_E2E_CONNECT_LINK, "1", "Set GC_E2E_CONNECT_LINK=1 to check the running host's Connect endpoint");
const binary = process.env.GC_CONNECT_BINARY || "gc";
assert.match(execFileSync(binary, ["connect", "status"], { encoding: "utf8" }), /running/, "Start Connect before running this opt-in check");
const output = execFileSync(binary, ["connect", "--http"], { encoding: "utf8" });
const link = output.match(/https?:[^\s\x1b]+#owner=[A-Za-z0-9_-]+/)?.[0];
assert(link, "CLI did not print a private owner link");
const parsed = new URL(link), bootstrap = parsed.hash.slice("#owner=".length);
assert.equal(bootstrap.length, 22);
const browser = await webkit.launch(process.env.GC_WEBKIT_BINARY ? { executablePath: process.env.GC_WEBKIT_BINARY } : {});
try {
  for (const scenario of ["fresh", "stale-saved-capability", "cut-off-then-paste", "invalid-then-paste"]) {
    const context = await browser.newContext({ viewport: { width: 390, height: 844 }, hasTouch: true, serviceWorkers: "block" });
    try {
      if (scenario === "stale-saved-capability") await context.addInitScript(() => localStorage.setItem("gc-owner-capability-v1", "f".repeat(64)));
      const page = await context.newPage();
      let exchanges = 0;
      page.on("request", request => { if (new URL(request.url()).pathname === "/api/local") exchanges++; });
      if (scenario === "cut-off-then-paste" || scenario === "invalid-then-paste") {
        const invalid = (bootstrap[0] === "A" ? "B" : "A") + bootstrap.slice(1);
        await page.goto(parsed.origin + "/#owner=" + (scenario === "cut-off-then-paste" ? bootstrap.slice(0, 10) : invalid));
        await page.getByRole("textbox", { name: "Private Connect link" }).waitFor();
        assert.match(await page.locator("#launch-detail").textContent(), scenario === "cut-off-then-paste" ? /link was cut off/ : /invalid or expired/);
        assert.equal(exchanges, scenario === "cut-off-then-paste" ? 0 : 1);
        await page.getByRole("textbox", { name: "Private Connect link" }).fill(link);
        await page.getByRole("button", { name: "Unlock workspace", exact: true }).click();
      } else await page.goto(link);
      await page.waitForFunction(() => document.querySelector("#launch")?.hidden, null, { timeout: 20000 });
      assert.equal(new URL(page.url()).hash, "");
      assert.equal(exchanges, scenario === "invalid-then-paste" ? 2 : 1);
      const retained = await page.evaluate(value => Object.values(localStorage).includes(value) || document.body.innerText.includes(value), bootstrap);
      assert.equal(retained, false, "Bootstrap must not remain in storage or visible content");
      if (scenario !== "stale-saved-capability") {
        await page.reload();
        await page.waitForFunction(() => document.querySelector("#launch")?.hidden, null, { timeout: 20000 });
      }
      console.log(`Real CLI link: ${scenario} passed without authorization injection`);
    } finally { await context.close(); }
  }
  console.log(JSON.stringify({ linkCharacters: link.length, bootstrapCharacters: bootstrap.length }));
} catch (error) {
  console.error(String(error).replace(/((?:owner|secret)=)[A-Za-z0-9_-]+/g, "$1[redacted]"));
  process.exitCode = 1;
} finally { await browser.close(); }
