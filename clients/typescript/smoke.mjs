import fs from "node:fs";
import path from "node:path";
import { GlassClient } from "./dist/index.js";

const client = new GlassClient({
  command: process.env.GLASS_BINARY ?? "glass",
});
try {
  const manifest = await client.initialize();
  if (manifest?.protocolVersion !== 1) throw new Error("unexpected Glass protocol version");
  if (!client.supportsCapability("action")) throw new Error("action capability missing");
  const fixture = JSON.parse(fs.readFileSync(path.resolve("../../tests/fixtures/client-conformance-v1.json"), "utf8"));
  for (const [schema, versions] of Object.entries(fixture.requiredSchemas)) {
    for (const version of versions) if (!client.supportsSchema(schema, version)) throw new Error(`schema missing: ${schema}@${version}`);
  }
  for (const capability of fixture.requiredCapabilities) client.requireCapability(capability);
  const toolNames = (await client.listTools()).map((tool) => tool.name).sort();
  if (JSON.stringify(toolNames) !== JSON.stringify(fixture.tools)) throw new Error("MCP tool inventory does not match the conformance fixture");
} finally {
  client.close();
}
