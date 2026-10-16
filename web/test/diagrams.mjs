import { chromium } from "playwright-core";
import { readFileSync, existsSync } from "node:fs";
const build = JSON.parse(
  readFileSync(new URL("../dist/build.json", import.meta.url)),
);
const modulePath = Object.keys(build.outputs)
  .find((p) => /chunks\/mermaid-/.test(p))
  .replace(/^dist\//, "");
const browser = await chromium.launch({
  executablePath:
    process.env.GC_BROWSER_BINARY ||
    (existsSync("/snap/chromium/current/usr/lib/chromium-browser/chrome")
      ? "/snap/chromium/current/usr/lib/chromium-browser/chrome"
      : undefined),
  headless: true,
  args: ["--no-sandbox", "--disable-dev-shm-usage"],
});
const page = await browser.newPage();
const charts = {
  flow: "flowchart TD\n A[Start] --> B{Ready?}\n B -->|yes| C[Done]",
  sequence: "sequenceDiagram\n Alice->>Bob: Hello\n Bob-->>Alice: Hi",
  class: "classDiagram\n Animal <|-- Duck\n Animal : +int age",
  state: "stateDiagram-v2\n [*] --> Ready\n Ready --> Done\n Done --> [*]",
  er: "erDiagram\n CUSTOMER ||--o{ ORDER : places",
  gantt:
    "gantt\n title Work\n dateFormat YYYY-MM-DD\n section Build\n Code :a, 2026-10-16, 2d",
  pie: 'pie title Results\n "Pass" : 90\n "Fail" : 10',
  journey: "journey\n title Day\n section Work\n Read: 5: Me\n Write: 4: Me",
  mindmap: "mindmap\n root((Glass))\n  Runtime\n  Browser",
  timeline: "timeline\n title Releases\n 2025 : Core\n 2026 : Browser",
  git: "gitGraph\n commit\n branch dev\n checkout dev\n commit\n checkout main\n merge dev",
  quadrant:
    "quadrantChart\n title Work\n x-axis Low --> High\n y-axis Low --> High\n Task: [0.3, 0.6]",
  xy: 'xychart-beta\n x-axis [Mon, Tue, Wed]\n y-axis "Count" 0 --> 10\n bar [2, 5, 8]',
  sankey: "sankey-beta\n A,B,10\n B,C,8",
  block: "block-beta\n columns 2\n A B\n A --> B",
  architecture:
    "architecture-beta\n service db(database)[Database]\n service app(server)[App]\n app:R -- L:db",
  packet: 'packet-beta\n 0-7: "Header"\n 8-15: "Payload"',
  c4: 'C4Context\n Person(user, "User")\n System(core, "Glass")\n Rel(user, core, "Uses")',
  kanban: "kanban\n Todo\n  task[Read]\n Done\n  finished[Write]",
};
try {
  await page.goto(process.argv[2]);
  const result = await page.evaluate(
    async ({ modulePath, charts }) => {
      const { diagram } = await import("/" + modulePath);
      const results = [];
      for (const light of [true, false]) {
        document.documentElement.classList.toggle("light", light);
        for (const [name, source] of Object.entries(charts)) {
          const start = performance.now();
          try {
            const svg = await diagram(source);
            if (!svg.includes("<svg")) throw Error("No SVG");
            results.push({
              name,
              light,
              ok: true,
              ms: Math.round(performance.now() - start),
            });
          } catch (e) {
            results.push({ name, light, ok: false, error: e.message });
          }
        }
      }
      const copies = await Promise.all([
        diagram(charts.flow),
        diagram(charts.flow),
        diagram(charts.flow),
      ]);
      const ids = copies.flatMap((svg) =>
        [
          ...new DOMParser()
            .parseFromString(svg, "image/svg+xml")
            .querySelectorAll("[id]"),
        ].map((n) => n.id),
      );
      results.push({
        name: "duplicate-ids",
        ok: ids.length === new Set(ids).size,
      });
      try {
        await diagram("flowchart TD\n A[broken");
        results.push({ name: "invalid-source", ok: false });
      } catch {
        results.push({ name: "invalid-source", ok: true });
      }
      return results;
    },
    { modulePath, charts },
  );
  console.log(JSON.stringify(result, null, 2));
  if (result.some((r) => !r.ok)) process.exitCode = 1;
} finally {
  await browser.close();
}
