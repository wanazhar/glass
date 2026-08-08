import { GlassClient } from "../src/index.ts";

const glass = new GlassClient({ command: process.env.GLASS_BINARY ?? "glass" });
const controller = new AbortController();
process.once("SIGINT", () => controller.abort());

try {
  const project = await glass.projectInspect(process.cwd());
  console.log(await glass.projectSessionStatus(project.root));
  await glass.projectCapsuleSave(project.root, { mobileView: "home" });
  await glass.onAttentionRequired(
    (item) => console.log(`needs you: ${item.title} — ${item.detail}`),
    project.root,
    { signal: controller.signal },
  );
} finally {
  glass.close();
}
