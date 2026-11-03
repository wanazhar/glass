import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { chromium } from "playwright-core";

const [firstUrl, secondUrl, sessionId, glassRef, gcRef] = process.argv.slice(2);
assert(firstUrl && secondUrl && sessionId && glassRef && gcRef,
  "usage: node test/shared-chat-live.mjs FIRST_URL SECOND_URL ID GLASS_REF GC_REF");

const browser = await chromium.launch({
  executablePath:
    process.env.GC_BROWSER_BINARY ||
    (existsSync("/snap/chromium/current/usr/lib/chromium-browser/chrome")
      ? "/snap/chromium/current/usr/lib/chromium-browser/chrome"
      : undefined),
  headless: true,
  args: ["--no-sandbox", "--disable-dev-shm-usage"],
});
const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
const first = await context.newPage();
const secondContext = await browser.newContext({ viewport: { width: 1280, height: 900 } });
const second = await secondContext.newPage();
const errors = [];
for (const page of [first, second]) page.on("pageerror", error => errors.push(String(error)));

const openRow = async (page, title) => {
  await page.locator("#search").fill(title);
  const row = page.locator("#threads .thread").filter({ hasText: title });
  await row.waitFor({ state: "visible" });
  await row.click();
  await page.locator("#timeline .loading-surface").waitFor({ state: "detached" }).catch(() => {});
};
const loadHistory = async (page, base, token, sourceRef) => {
  const url = new URL(`/api/history?id=${encodeURIComponent(sessionId)}&sourceRef=${encodeURIComponent(sourceRef)}`, base);
  const response = await page.request.get(url.href, {
    headers: { Authorization: `Bearer ${token}`, Origin: base },
  });
  assert(response.ok(), `history request failed: ${response.status()} ${await response.text()}`);
  return (await response.json()).records.map(item => item.record);
};
const auth = url => {
  const parsed = new URL(url);
  return { base: parsed.origin, token: parsed.hash.slice(1) };
};
const firstAuth = auth(firstUrl), secondAuth = auth(secondUrl);

try {
  await first.goto(firstUrl);
  await first.locator(".app").waitFor({ state: "visible" });
  const rows = first.locator("#threads .thread");
  await first.locator("#search").fill("Shared");
  await first.getByText("Shared Glass resume fixture", { exact: true }).waitFor();
  await first.getByText("Shared GC duplicate fixture", { exact: true }).waitFor();
  assert.equal(await rows.count(), 2, "the same logical ID from two stores must render as two distinct rows");
  assert.equal(await rows.filter({ hasText: "Created in Glass" }).count(), 1);
  assert.equal(await rows.filter({ hasText: "Created in GC" }).count(), 1);

  await openRow(first, "Shared Glass resume fixture");
  await first.locator("#timeline").getByText("Glass saved question", { exact: true }).waitFor();
  assert.equal(await first.locator("#thread-origin").textContent(), "Created in Glass");
  const deepLink = new URL(first.url());
  assert.equal(deepLink.searchParams.get("thread"), sessionId);
  assert.equal(deepLink.searchParams.get("sourceRef"), glassRef);

  await first.locator("#prompt").fill("Append from the Web to the Glass log");
  await first.locator("#send").click();
  await first.locator("#stop").waitFor({ state: "visible" });
  assert.equal(await first.locator("#handoff").isDisabled(), true,
    "handoff must stay unavailable while a turn is working");
  await first.locator("#timeline").getByText("Browser resumption response.", { exact: true }).waitFor({ timeout: 30000 });
  const firstHistory = await loadHistory(first, firstAuth.base, firstAuth.token, glassRef);
  assert.equal(firstHistory.filter(row => row.t === "Append from the Web to the Glass log").length, 1);
  assert.equal(firstHistory.filter(row => row.t === "Browser resumption response.").length, 1);

  await first.reload();
  await first.locator("#timeline").getByText("Browser resumption response.", { exact: true }).waitFor();
  assert.equal(await first.locator("#thread-origin").textContent(), "Created in Glass");
  assert.equal(new URL(first.url()).searchParams.get("sourceRef"), glassRef,
    "the source reference must survive reload");
  for (const width of [390, 768, 1280]) {
    await first.setViewportSize({ width, height: 844 });
    const badge = await first.locator("#thread-origin").boundingBox();
    assert(badge && badge.width >= 80 && badge.height > 0,
      `origin label must remain visible at ${width}px`);
    assert(badge.x >= 0 && badge.x + badge.width <= width,
      `origin label must fit the toolbar at ${width}px`);
    assert(await first.locator("#thread-origin").evaluate(node => {
      const box = node.getBoundingClientRect();
      return document.elementFromPoint(box.x + box.width/2, box.y + box.height/2)?.closest("#thread-origin") === node;
    }), `origin label must not be clipped or covered at ${width}px`);
  }
  await first.setViewportSize({ width: 390, height: 844 });
  await first.screenshot({path:"/tmp/gc-shared-chat-mobile.png"});
  await first.locator("#menu").click();
  await first.locator("#sidebar.open").waitFor();
  assert.equal(await first.locator("#threads .thread").filter({ hasText: "Created in Glass" }).count(), 1,
    "the mobile conversation picker must show the origin label: " + JSON.stringify(await first.locator("#threads .thread").allTextContents()));
  await first.keyboard.press("Escape");
  await first.locator("#sidebar.open").waitFor({ state: "detached" });
  await first.setViewportSize({ width: 1280, height: 900 });

  await first.locator("#handoff").waitFor({ state: "visible" });
  assert.equal(await first.locator("#handoff").isDisabled(), false,
    "idle owned conversations can be released");
  await first.locator("#handoff").click();
  await first.getByText("Conversation released. Open it in another app to continue.").waitFor();

  await second.goto(secondUrl);
  await second.locator(".app").waitFor({ state: "visible" });
  await openRow(second, "Shared Glass resume fixture");
  await second.locator("#timeline").getByText("Browser resumption response.", { exact: true }).waitFor();
  assert.equal(await second.locator("#thread-origin").textContent(), "Created in Glass");

  await openRow(first, "Shared GC duplicate fixture");
  try {
    await first.locator("#timeline").getByText("GC saved question", { exact: true }).waitFor();
  } catch (error) { throw new Error(String(error) + " Current notice: " + await first.locator("#notice").textContent()); }
  assert.equal(await first.locator("#thread-origin").textContent(), "Created in GC");
  await first.locator("#prompt").fill("Draft stays with the selected GC chat");
  await openRow(first, "Shared Glass resume fixture");
  await first.locator("#notice").getByText(/active in another app/i).waitFor();
  assert.equal(await first.locator("#thread-name").textContent(), "Shared GC duplicate fixture");
  assert.equal(await first.locator("#thread-origin").textContent(), "Created in GC");
  assert.equal(await first.locator("#prompt").inputValue(), "Draft stays with the selected GC chat");
  assert.equal(new URL(first.url()).searchParams.get("sourceRef"), gcRef,
    "failed writer acquisition must leave the prior selection intact");
  await first.locator("#search").fill("Shared");
  await rows.filter({ hasText: "Shared Glass resume fixture" }).waitFor();
  await rows.filter({ hasText: "Shared GC duplicate fixture" }).waitFor();
  assert.equal(await rows.count(), 2, "reloading or opening must not duplicate sidebar rows");

  await second.locator("#handoff").waitFor({ state: "visible" });
  await second.locator("#handoff").click();
  assert.deepEqual(errors, [], `browser errors: ${errors.join(" | ")}`);
  console.log("PASS real Connect browser journey: same-ID GC/Glass routing, append/resume, reload origin, release, competing-client rejection, draft retention, unique sidebar rows");
} finally {
  await context.close();
  await secondContext.close();
  await browser.close();
}
