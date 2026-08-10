import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { GlassClient } from "./dist/index.js";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

const client = new GlassClient({
  command: process.env.GLASS_BINARY ?? path.join(repositoryRoot, "target", "debug", "glass"),
});
try {
  const manifest = await client.initialize();
  if (manifest?.protocolVersion !== 1) throw new Error("unexpected Glass protocol version");
  if (!client.supportsCapability("action")) throw new Error("action capability missing");
  const browserFixture = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "crates/glass-browser/tests/fixtures/client-conformance-v1.json"), "utf8"));
  const developmentFixture = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "crates/glass-dev/tests/fixtures/client-conformance-v1.json"), "utf8"));
  for (const [schema, versions] of Object.entries(browserFixture.requiredSchemas)) {
    for (const version of versions) if (!client.supportsSchema(schema, version)) throw new Error(`schema missing: ${schema}@${version}`);
  }
  for (const capability of browserFixture.requiredCapabilities) client.requireCapability(capability);
  const toolNames = (await client.listTools()).map((tool) => tool.name).sort();
  if (JSON.stringify(toolNames) !== JSON.stringify(developmentFixture.tools)) throw new Error("MCP tool inventory does not match the full-product conformance fixture");
  const projectRoot = repositoryRoot;
  const project = await client.projectInspect(projectRoot);
  if (project.schemaVersion !== "glass.development.v1") throw new Error("unexpected project schema");
  const tree = await client.projectFiles(projectRoot);
  if (!Array.isArray(tree.entries) || typeof tree.truncated !== "boolean" || tree.limit < tree.entries.length) {
    throw new Error("invalid bounded project tree result");
  }
  const events = await client.projectEvents(projectRoot, undefined, 8);
  if (!Array.isArray(events.events) || events.events.length > 8) throw new Error("invalid bounded project event page");
  const session = await client.projectSessionStatus(projectRoot);
  if (!session.resident) throw new Error("project session did not remain resident");
  await client.projectAttach("typescript-smoke", projectRoot);
  const controller = new AbortController();
  const subscription = client.watchProjectEvents(projectRoot, {
    afterId: events.cursor,
    limit: 8,
    pollIntervalMs: 50,
    signal: controller.signal,
  });
  const firstPage = await subscription.next();
  controller.abort();
  await subscription.return();
  if (firstPage.done || !Array.isArray(firstPage.value.events)) throw new Error("project event subscription did not yield");
  await client.projectAttach("typescript-smoke-wait", projectRoot);
  const joined = await client.waitForEvent(
    (event) => event.kind === "actorJoined" && event.actor.name === "typescript-smoke-wait",
    projectRoot,
    { afterId: firstPage.value.cursor ?? undefined, timeoutMs: 2_000, pollIntervalMs: 50 },
  );
  if (joined.kind !== "actorJoined") throw new Error("waitForEvent returned the wrong event");
  const healthy = await client.runUntilHealthy("typescript-smoke", "printf 'ready\\n'; sleep 5", {
    root: projectRoot,
    timeoutMs: 2_000,
    pollIntervalMs: 50,
  });
  if (healthy.health !== "healthy") throw new Error("resident process did not become healthy");
  await client.projectProcessStop("typescript-smoke", projectRoot);
  const card = await client.projectVerificationCard("TypeScript smoke", projectRoot);
  if (card.visualStatus !== "not-captured") throw new Error("verification card captured pixels implicitly");
  await client.projectCapsuleSave(projectRoot, {
    eventCursor: firstPage.value.cursor ?? undefined,
    mobileView: "app",
    mobileScroll: 12,
  });
  const capsule = (await client.projectCapsuleShow(projectRoot)).capsule;
  if (capsule === null || capsule.mobileScroll !== 12) throw new Error("reconnect capsule was not saved");
  await client.projectCapsuleClear(true, projectRoot);
  if (!Array.isArray(await client.projectInbox(projectRoot))) throw new Error("attention inbox was not an array");
} finally {
  client.close();
}
