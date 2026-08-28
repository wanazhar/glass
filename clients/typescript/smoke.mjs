import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { GlassClient, GlassStructuredError } from "./dist/index.js";

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
  const trust = await client.call("glass.workspace.trust.status");
  if (!trust || typeof trust !== "object" || !["untrusted", "trustedOnce", "trustedProject"].includes(trust.trust)) {
    throw new Error("invalid workspace trust status");
  }
  const authority = await client.call("glass.workspace.trust.inspect");
  if (!authority || typeof authority !== "object" || !Array.isArray(authority.items)) {
    throw new Error("invalid workspace authority inspection");
  }
  const tree = await client.call("glass.file.list");
  if (!tree || typeof tree !== "object" || !Array.isArray(tree.entries)) {
    throw new Error("invalid bounded file list");
  }
  const browser = await client.call("glass.browser.state");
  if (!browser || typeof browser !== "object" || typeof browser.connected !== "boolean") {
    throw new Error("invalid resident browser state");
  }
  if (!Array.isArray(await client.call("glass.task.list"))) throw new Error("task list was not an array");
  if (!Array.isArray(await client.call("glass.replay.list"))) throw new Error("replay list was not an array");
  if (trust.trust === "untrusted") {
    try {
      await client.call("glass.test.discover");
      throw new Error("untrusted executable project discovery was not denied");
    } catch (error) {
      if (!(error instanceof GlassStructuredError)) throw error;
    }
  }
} finally {
  client.close();
}
