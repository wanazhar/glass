import { GlassClient } from "./dist/index.js";

const client = new GlassClient({
  command: process.env.GLASS_BINARY ?? "glass",
});
try {
  const manifest = await client.initialize();
  if (manifest?.protocolVersion !== 1) throw new Error("unexpected Glass protocol version");
  if (!client.supportsCapability("action")) throw new Error("action capability missing");
  if (!client.supportsSchema("workflow", 1)) throw new Error("workflow schema missing");
  client.requireCapability("action");
} finally {
  client.close();
}
