import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { GlassClient } from "./dist/index.js";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

const client = new GlassClient({
  command: process.env.GLASS_BINARY ?? "glass",
});
try {
  const manifest = await client.initialize();
  if (manifest?.protocolVersion !== 1) throw new Error("unexpected Glass protocol version");
  if (!client.supportsCapability("action")) throw new Error("action capability missing");
  const fixture = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "crates/glass-browser/tests/fixtures/client-conformance-v1.json"), "utf8"));
  for (const [schema, versions] of Object.entries(fixture.requiredSchemas)) {
    for (const version of versions) if (!client.supportsSchema(schema, version)) throw new Error(`schema missing: ${schema}@${version}`);
  }
  for (const capability of fixture.requiredCapabilities) client.requireCapability(capability);
  const toolNames = (await client.listTools()).map((tool) => tool.name).sort();
  if (JSON.stringify(toolNames) !== JSON.stringify(fixture.tools)) throw new Error("MCP tool inventory does not match the conformance fixture");
  const projectRoot = repositoryRoot;
  const project = await client.projectInspect(projectRoot);
  if (project.schemaVersion !== "glass.development.v1") throw new Error("unexpected project schema");
  const events = await client.projectEvents(projectRoot, undefined, 8);
  if (!Array.isArray(events.events) || events.events.length > 8) throw new Error("invalid bounded project event page");
  await client.projectInspect(projectRoot);
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
} finally {
  client.close();
}
