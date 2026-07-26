import { GlassClient } from "../src/index.ts";

const glass = new GlassClient({ command: process.env.GLASS_BINARY ?? "glass" });
try {
  await glass.navigate("https://example.com");
  const page = await glass.observe<{ page: { url: string }; accessibility: unknown }>();
  console.log(page.page.url);
} finally {
  glass.close();
}
