import { GlassClient } from "../src/index.ts";

const glass = new GlassClient({ command: process.env.GLASS_BINARY ?? "glass" });
const controller = new AbortController();
process.once("SIGINT", () => controller.abort());

try {
  const runtime = await glass.call("glass.runtime.inspect") as { project?: { root?: string } };
  const browser = await glass.call("glass.browser.state") as { connected?: boolean };
  console.log({
    root: runtime.project?.root,
    browserConnected: browser.connected,
  });
  while (!controller.signal.aborted) {
    console.log(await glass.call("glass.browser.state"));
    await new Promise((resolve) => {
      const timer = setTimeout(resolve, 2_000);
      controller.signal.addEventListener("abort", () => {
        clearTimeout(timer);
        resolve(undefined);
      }, { once: true });
    });
  }
} finally {
  glass.close();
}
