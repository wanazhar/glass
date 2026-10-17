// Real browser/event transport checks for conversation activity and rich content.
import { chromium } from "playwright-core";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { resolve, extname, sep } from "node:path";
import { fileURLToPath } from "node:url";
const root = fileURLToPath(new URL("../dist/", import.meta.url));
const streams = new Set(), answers = [], requests = [];
const { activitySnapshot } = await import("../src/activity.js");
const bounded = activitySnapshot([{ state: "completed", name: "bash", time: { textContent: "" }, command: "build", output: { textContent: "x".repeat(200000) + "FINAL_BUILD_ERROR" } }]);
assert(bounded.length < 132000 && bounded.endsWith("FINAL_BUILD_ERROR"), "Activity snapshots must stay bounded and retain the latest output");
let answerRelease;
const records = [
  { k: "user", t: "Inspect the project and prepare a guide." },
  { k: "reasoning", t: "I will inspect the entry point and prepare a short guide." },
  { k: "call", id: "saved-read", name: "read", args: JSON.stringify({ path: "src/main.rs", offset: 1, limit: 80 }) },
  { k: "tool", id: "saved-read", name: "read", t: "1|fn main() { println!(\"hello\"); }" },
  { k: "call", id: "saved-failure", name: "bash", args: JSON.stringify({ command: "false" }) },
  { k: "bash", id: "saved-failure", cmd: "false", out: "Intended fixture error", code: 1 },
  { k: "tool", id: "saved-failure", name: "bash", t: "[exit 1]\nIntended fixture error" },
  { k: "assistant", t: "## Your project guide\n\nA readable response with **clear emphasis**, a short list, and a diagram.\n\n- Review the source\n- Run the checks\n\n[Project guide](docs/guide.md)\n\n```mermaid\nflowchart TD\n  A[Source code] --> B[Build]\n  B --> C[Browser]\n```\n\n[External PDF](https://example.com/remote.pdf)\n\n[Unsafe](javascript:alert(1))\n" },
  { k: "call", id: "resumed", name: "bash", args: JSON.stringify({ command: "sleep 1" }) },
];
const emit = events => { for (const response of streams) response.write(`data: ${JSON.stringify({ events })}\n\n`); };
const server = createServer(async (request, response) => {
  const url = new URL(request.url, "http://localhost"), path = url.pathname;
  requests.push(path);
  if (path === "/api/events") {
    response.writeHead(200, { "Content-Type": "text/event-stream", "Cache-Control": "no-cache" });
    streams.add(response);
    response.write(`data: ${JSON.stringify({ reset: true, control: { can_write: true, web_workflows: 1, commands: [] }, live: [{ t: "tool", call_id: "resumed", name: "bash", command: "sleep 1" }], active: true })}\n\n`);
    request.on("close", () => streams.delete(response));
    return;
  }
  if (path === "/api/file") {
    response.writeHead(200, { "Content-Type": "text/markdown" });
    response.end("# Project guide\n\nDownloadable source from the host.");
    return;
  }
  if (path.startsWith("/api/")) {
    let data = {};
    if (path === "/api/status") data = { profile: "workspace-edit", workspace: "/tmp", web_workflows: 1 };
    if (path === "/api/threads") data = { threads: ["fixture", "other"].map(session => ({ session, title: session === "fixture" ? "Activity reference" : "Other thread", workspace: "/tmp", workspace_assigned: true, workspace_available: true })), projects: [] };
    if (path === "/api/history") data = url.searchParams.has("before")
      ? { records: [
          { index: 0, record: { k: "user", t: "Earlier task" } },
          { index: 1, record: { k: "reasoning", t: "Earlier reasoning fixture" } },
          { index: 2, record: { k: "custom", c: "provider/chat-reasoning", d: "PRIVATE_PROVIDER_SENTINEL" } },
          { index: 3, record: { k: "assistant", t: "Earlier answer" } },
        ], before: 0 }
      : { records: records.map((record, index) => ({ index: 100 + index, record })), before: 100 };
    if (path === "/api/send") {
      let source = "";
      for await (const chunk of request) source += chunk;
      const body = JSON.parse(source);
      if (body.message.t === "answer") {
        answers.push(body.message);
        await new Promise(resolve => { answerRelease = resolve; });
      }
    }
    response.writeHead(200, { "Content-Type": "application/json" });
    response.end(JSON.stringify(data));
    return;
  }
  try {
    const file = resolve(root, "." + (path === "/" ? "/index.html" : path));
    if (!file.startsWith(root.endsWith(sep) ? root : root + sep)) throw Error("Invalid path");
    const body = await readFile(file);
    response.writeHead(200, { "Content-Type": ({ ".js": "text/javascript", ".html": "text/html", ".css": "text/css", ".svg": "image/svg+xml", ".woff2": "font/woff2", ".webmanifest": "application/manifest+json" })[extname(file)] || "application/octet-stream" });
    response.end(body);
  } catch { response.writeHead(404).end(); }
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
const browser = await chromium.launch({ executablePath: process.env.GC_BROWSER_BINARY || (existsSync("/snap/chromium/current/usr/lib/chromium-browser/chrome") ? "/snap/chromium/current/usr/lib/chromium-browser/chrome" : undefined), headless: true, args: ["--no-sandbox", "--disable-dev-shm-usage"] });
try {
  for (const width of [390, 768, 1440]) for (const colorScheme of ["light", "dark"]) {
    const context = await browser.newContext({ viewport: { width, height: 1000 }, colorScheme, serviceWorkers: "block", reducedMotion: "reduce" });
    const page = await context.newPage(), errors = [];
    page.on("pageerror", error => errors.push(String(error)));
    await page.goto(`http://127.0.0.1:${server.address().port}/?thread=fixture`);
    await page.locator(".app").waitFor({ state: "visible" });
    await page.waitForFunction(() => document.querySelector("#ownership").textContent === "Release control");
    await page.locator('.tool-row[data-call-id="resumed"][data-state="running"]').waitFor();
    const savedThought = page.locator(".thought-row").first();
    assert.equal(await savedThought.locator(".thought-body").textContent(), "", "Collapsed reasoning must stay lazily rendered");
    await savedThought.locator("summary").click();
    await savedThought.locator(".thought-body").getByText("I will inspect the entry point and prepare a short guide.").waitFor();
    await savedThought.locator("summary").click();
    assert(await page.locator('.tool-row[data-call-id="saved-failure"]').evaluate(element => element.classList.contains("failed")));
    const saved = page.locator(".tool-row").first();
    assert.equal(await saved.locator(".tool-time").textContent(), "", "History must not invent an elapsed duration");
    await saved.locator("summary").click();
    assert.match(await saved.locator(".tool-input").textContent(), /"offset": 1/);
    assert.match(await saved.locator(".tool-output").textContent(), /fn main/);
    const card = page.locator(".document-card");
    assert.equal(await card.count(), 1, "Only a standalone local document link becomes a card");
    assert.match(await card.textContent(), /Project guide.*MD.*Open file/);
    assert.equal(await page.locator('.prose a[href^="javascript:"]').count(), 0);
    await card.click();
    await page.locator("#preview-body").getByText("Downloadable source from the host.").waitFor();
    await page.locator("#preview-close").click();
    await page.locator(".diagram-render svg").waitFor();
    const svg = page.locator(".diagram-render svg");
    assert(await svg.isVisible());
    await saved.scrollIntoViewIfNeeded();
    await page.screenshot({ path: `/tmp/gc-rich-tools-${width}-${colorScheme}.png` });
    emit([{ t: "reasoning", delta: "I am checking the **command result** before continuing." }]);
    const thinking = page.locator(".thought-row").last();
    await thinking.locator(".thought-label").getByText("Thinking", { exact: true }).waitFor();
    assert.equal(await thinking.locator(".thought-body").textContent(), "");
    await thinking.locator("summary").click();
    await thinking.locator("strong").getByText("command result").waitFor();
    emit([{ t: "tool", call_id: "live", name: "bash", command: "printf '<script>literal</script>'" }]);
    const live = page.locator('.tool-row[data-call-id="live"]');
    await live.waitFor();
    assert.equal(await thinking.locator(".thought-label").textContent(), "Thoughts");
    await live.locator("summary").click();
    assert.match(await live.locator(".tool-input").textContent(), /<script>literal<\/script>/);
    assert.equal(await live.locator("script").count(), 0);
    await live.locator(".tool-time").filter({ hasText: /s/ }).waitFor();
    // Nest the console above another source preview, then restore it safely.
    await saved.getByRole("button", { name: "Expand output", exact: true }).click();
    await page.evaluate(() => document.querySelector("#agent-activity").click());
    await page.locator("#preview-body.agent-console").waitFor();
    assert.match(await page.locator("#preview-body").textContent(), /Running · bash/);
    emit([{ t: "tool_end", call_id: "live", output: "[exit 1]\nA useful failure message", summary: "exit 1" }]);
    await page.locator("#preview-body").getByText(/Useful failure message/i).waitFor();
    assert.match(await page.locator("#preview-body").textContent(), /× Failed · bash/);
    await page.locator("#preview-close").click();
    assert(await live.evaluate(element => element.classList.contains("failed")));
    emit([{ t: "tool_end", call_id: "resumed", output: "Finished background check", summary: "exit 0" }]);
    await page.locator('.tool-row[data-call-id="resumed"][data-state="completed"]').waitFor({ state: "attached" });
    assert.match(await page.locator("#preview-body").textContent(), /fn main/);
    assert.doesNotMatch(await page.locator("#preview-body").textContent(), /Finished background check/);
    await page.locator("#preview-close").click();
    emit([{ t: "interaction", id: "question-1", request: { kind: "question", request: { questions: [
      { question: "Which output?", options: [{ label: "Compact", description: "Less space" }, { label: "Detailed", description: "More context" }] },
      { question: "Which checks?", multiSelect: true, options: [{ label: "Build" }, { label: "Tests" }] },
    ] } } }]);
    await page.locator("#interaction label").first().waitFor();
    assert(await page.locator("#interaction").evaluate(element => element.closest(".conversation") !== null));
    await page.getByRole("radio", { name: "Detailed More context" }).check();
    await page.getByRole("checkbox", { name: "Build", exact: true }).check();
    await page.getByRole("checkbox", { name: "Tests", exact: true }).check();
    await page.locator("#interaction").scrollIntoViewIfNeeded();
    await page.screenshot({ path: `/tmp/gc-rich-questions-${width}-${colorScheme}.png` });
    const previous = answers.length;
    await page.locator("#interaction form").evaluate(form => { form.requestSubmit(); form.requestSubmit(); });
    await page.waitForFunction(() => document.querySelector("#interaction").getAttribute("aria-busy") === "true");
    await new Promise(resolve => setTimeout(resolve, 50));
    assert.equal(answers.length, previous + 1, "Only one answer should be sent while pending");
    assert.deepEqual(answers.at(-1).answer.answers, [{ answers: ["Detailed"] }, { answers: ["Build", "Tests"] }]);
    assert(await page.locator('#interaction button[type="submit"]').isDisabled());
    answerRelease();
    await page.waitForFunction(() => !document.querySelector("#interaction").hasAttribute("aria-busy"));
    await page.locator("#interaction").scrollIntoViewIfNeeded();
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    await page.screenshot({ path: `/tmp/gc-rich-chat-${width}-${colorScheme}.png` });
    emit([{ t: "interaction_done" }]);
    await page.locator("#interaction").waitFor({ state: "hidden" });
    await page.locator("#agent-activity").click();
    const download = page.waitForEvent("download");
    await page.locator("#download").click();
    const artifact = await download;
    assert.match(artifact.suggestedFilename(), /Agent.activity/);
    assert.match(await readFile(await artifact.path(), "utf8"), /A useful failure message/);
    await page.locator("#preview-close").click();
    // A completed turn refreshes history without reopening collapsed work.
    const firstGroup = page.locator(".work-group").first();
    await firstGroup.evaluate(element => { element.open = false; window.previousGroup = element; });
    emit([{ t: "done" }]);
    await page.waitForFunction(() => !window.previousGroup.isConnected);
    assert.equal(await page.locator(".work-group").first().evaluate(element => element.open), false);
    await page.locator("#older").click();
    const earlier = page.locator(".thought-row").filter({ hasText: "Earlier reasoning fixture" });
    await earlier.locator("summary").click();
    await earlier.locator(".thought-body").getByText("Earlier reasoning fixture", { exact: true }).waitFor();
    assert.equal(await page.getByText("PRIVATE_PROVIDER_SENTINEL").count(), 0);
    assert.deepEqual(errors, []);
    await context.close();
    console.log(`PASS activity rows, safe source, event console, documents, diagrams and questions ${width}px ${colorScheme}`);
  }
} finally {
  answerRelease?.();
  await browser.close();
  for (const response of streams) response.end();
  server.closeAllConnections();
  await new Promise(resolve => server.close(resolve));
}
