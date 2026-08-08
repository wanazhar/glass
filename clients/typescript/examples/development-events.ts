import { GlassClient } from "../src/index.ts";

const root = process.argv[2] ?? process.cwd();
const glass = new GlassClient({ command: process.env.GLASS_BINARY ?? "glass" });
const controller = new AbortController();
process.once("SIGINT", () => controller.abort());

try {
  const project = await glass.projectInspect(root);
  console.log(`watching ${project.root} (${project.detection.languages.join(", ")})`);
  for await (const page of glass.watchProjectEvents(project.root, {
    signal: controller.signal,
  })) {
    if (page.cursorExpired) console.error("event cursor expired; resumed at oldest retained event");
    for (const event of page.events) {
      console.log(`${event.id} ${event.kind} ${event.actor.id}`);
    }
  }
} finally {
  glass.close();
}
