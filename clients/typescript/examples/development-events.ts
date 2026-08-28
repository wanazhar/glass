import { GlassClient } from "../src/index.ts";

const root = process.argv[2] ?? process.cwd();
const glass = new GlassClient({ command: process.env.GLASS_BINARY ?? "glass", cwd: root });
const controller = new AbortController();
process.once("SIGINT", () => controller.abort());

try {
  const trust = await glass.call("glass.workspace.trust.status") as { trust?: string };
  const runtime = await glass.call("glass.runtime.inspect") as { project?: { root?: string } };
  console.log(`watching ${runtime.project?.root ?? root} trust=${trust.trust ?? "unknown"}`);
  const seen = new Set<string>();
  while (!controller.signal.aborted) {
    const entries = await glass.call("glass.replay.list", { since: 0, limit: 128 }) as unknown;
    const list = Array.isArray(entries) ? entries : [];
    for (const entry of list) {
      const record = entry && typeof entry === "object" ? entry as Record<string, unknown> : {};
      const identity = String(record.sequence ?? record.id ?? entry);
      if (seen.has(identity)) continue;
      seen.add(identity);
      console.log(`${identity} ${record.kind ?? "replay"} ${record.actor ?? ""}`);
    }
    await new Promise((resolve) => {
      const timer = setTimeout(resolve, 500);
      controller.signal.addEventListener("abort", () => {
        clearTimeout(timer);
        resolve(undefined);
      }, { once: true });
    });
  }
} finally {
  glass.close();
}
