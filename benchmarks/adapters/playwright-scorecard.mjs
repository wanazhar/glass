import fs from "node:fs";
import os from "node:os";
import process from "node:process";
import { createRequire } from "node:module";
import { performance } from "node:perf_hooks";

const require = createRequire(import.meta.url);
const { chromium } = require("playwright");

const corpus = JSON.parse(
  fs.readFileSync(new URL("../scenarios/v1.json", import.meta.url), "utf8"),
);
const fixture = fs.readFileSync(
  new URL("../../tests/fixtures/scorecard.html", import.meta.url),
  "utf8",
);
const iterations = positiveInteger(
  "GLASS_SCORECARD_ITERATIONS",
  process.env.GLASS_SCORECARD_ITERATIONS ?? "10",
);
const chromePath = process.env.CHROME_PATH;
if (!chromePath) {
  throw new Error("CHROME_PATH is required for a controlled comparison");
}

const runnerRssStart = process.memoryUsage.rss();
let runnerPeakRss = runnerRssStart;
const sample = setInterval(() => {
  runnerPeakRss = Math.max(runnerPeakRss, process.memoryUsage.rss());
}, 10);
const startupStarted = performance.now();
const browser = await chromium.launch({ executablePath: chromePath, headless: true });
const startupMs = performance.now() - startupStarted;
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
await page.goto(`data:text/html;base64,${Buffer.from(fixture).toString("base64")}`);

const outcomes = [];
for (let iteration = 1; iteration <= iterations; iteration += 1) {
  for (const scenario of corpus.scenarios) {
    await reset(page);
    const started = performance.now();
    let actual = null;
    let error = null;
    try {
      actual = await runScenario(page, scenario.id);
    } catch (caught) {
      error = String(caught?.message ?? caught);
    }
    const status =
      actual === scenario.expected
        ? "success"
        : scenario.forbidden.includes(actual)
          ? "wrong_action"
          : "failure";
    outcomes.push({
      id: scenario.id,
      category: scenario.category,
      iteration,
      expected: scenario.expected,
      actual,
      status,
      error,
      latency_ms: performance.now() - started,
      cdp_requests: null,
    });
  }
}

clearInterval(sample);
const runnerRssEnd = process.memoryUsage.rss();
runnerPeakRss = Math.max(runnerPeakRss, runnerRssEnd);
await browser.close();
const successes = outcomes.filter(({ status }) => status === "success").length;
const wrongActions = outcomes.filter(
  ({ status }) => status === "wrong_action",
).length;
const unsupported = outcomes.filter(
  ({ status }) => status === "unsupported",
).length;
const failures = outcomes.filter(({ status }) => status === "failure").length;

const report = {
  schema_version: 1,
  tool: { name: "playwright", version: require("playwright/package.json").version },
  run: {
    corpus: corpus.corpus,
    corpus_fixture: corpus.fixture,
    iterations,
    temperature: "warm",
    profile: process.env.GLASS_SCORECARD_PROFILE ?? "fresh-ephemeral-single-session",
    viewport: { width: 1280, height: 720 },
  },
  environment: {
    os: process.platform,
    architecture: process.arch,
    rust: null,
    chrome: commandVersion(chromePath),
    machine: `${os.hostname()} ${os.release()}`,
  },
  resources: {
    scope:
      "primary-non-browser-runner-process-rss-v1",
    runner: {
      pid: process.pid,
      rss_start_bytes: runnerRssStart,
      rss_end_bytes: runnerRssEnd,
      peak_rss_bytes: runnerPeakRss,
    },
    chrome: {
      root_pid: null,
      rss_end_bytes: null,
      peak_process_tree_rss_bytes: null,
    },
    binary_size_bytes: fs.statSync(process.execPath).size,
    compact_context_bytes: null,
    cdp_requests: null,
    startup_ms: startupMs,
  },
  summary: {
    successes,
    failures,
    wrong_actions: wrongActions,
    unsupported,
    task_success_rate: successes / outcomes.length,
    hard_gate_passed: successes === outcomes.length,
  },
  scenarios: outcomes,
};

process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);

async function reset(targetPage) {
  await targetPage.evaluate(() => {
    window.resetFixture();
    document.querySelector("#name").value = "";
  });
}

async function result(targetPage) {
  return targetPage.locator("#result").evaluate((node) => node.value);
}

async function runScenario(targetPage, id) {
  switch (id) {
    case "duplicate-label": {
      if (process.env.GLASS_SCORECARD_TARGET_MODE === "wrong") {
        await targetPage.locator("#duplicate-wrong").click();
      } else {
        await targetPage
          .getByRole("button", { name: "Delete", exact: true })
          .click();
      }
      return result(targetPage);
    }
    case "overlay":
      await targetPage.locator("#overlay").evaluate((node) => {
        node.style.display = "block";
      });
      try {
        await targetPage
          .getByRole("button", { name: "Overlay target", exact: true })
          .click({ timeout: 250 });
      } catch {
        return "blocked";
      }
      return result(targetPage);
    case "reflow":
      await targetPage.getByRole("button", { name: "Moving target" }).click();
      return (await result(targetPage)) === "idle"
        ? "blocked"
        : result(targetPage);
    case "delayed-content":
      await targetPage.evaluate(() => window.scheduleDelayed());
      await targetPage.locator("#delayed").waitFor();
      return targetPage.locator("#delayed").textContent();
    case "spa-navigation":
      await targetPage.getByRole("button", { name: "SPA navigation" }).click();
      return result(targetPage);
    case "form":
      await targetPage.getByLabel("Name").fill("Glass");
      await targetPage.getByRole("button", { name: "Submit" }).click();
      return result(targetPage);
    case "popup": {
      const popup = targetPage.waitForEvent("popup");
      await targetPage.getByRole("button", { name: "Popup" }).click();
      const opened = await popup;
      await opened.waitForLoadState();
      await opened.close();
      return "popup-controlled";
    }
    case "frame":
      await targetPage
        .frameLocator("#frame")
        .getByRole("button", { name: "Frame action" })
        .click();
      await targetPage.locator("#result").evaluate((node) => {
        node.value = "frame-clicked";
      });
      return "frame-clicked";
    case "dialog":
      targetPage.once("dialog", (dialog) => dialog.accept());
      await targetPage.getByRole("button", { name: "Dialog" }).click();
      return result(targetPage);
    case "download": {
      const download = targetPage.waitForEvent("download");
      await targetPage.getByRole("link", { name: "Download" }).click();
      const completed = await download;
      await completed.createReadStream();
      return "download-complete";
    }
    case "failure-recovery":
      try {
        await targetPage.getByText("Definitely missing", { exact: true }).click({ timeout: 100 });
        return "unexpected-action";
      } catch {
        await targetPage.locator("#result").evaluate((node) => {
          node.value = "recovered";
        });
        return "recovered";
      }
    default:
      throw new Error(`unknown scenario ${id}`);
  }
}

function positiveInteger(name, value) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }
  return parsed;
}

function commandVersion(command) {
  try {
    return require("node:child_process")
      .execFileSync(command, ["--version"], { encoding: "utf8" })
      .trim();
  } catch {
    return null;
  }
}
