# `@glass-browser/client`

This dependency-free TypeScript client starts `glass --mcp` and exposes typed
helpers for guarded navigation, clicks, typing, form filling, scrolling,
verification, batches, and waits. The action helpers accept optional revision
guards. It has no Playwright or browser
runtime dependency. Build it with `npm run build` before publishing;
the package exports compiled JavaScript and declaration files.

```ts
import { GlassClient } from "@glass-browser/client";

const glass = new GlassClient({ command: "/absolute/path/to/glass" });
await glass.navigate("https://example.com");
const context = await glass.observe();
await glass.click("name=More information");
glass.close();
```

The client accepts both newline-delimited MCP responses and `Content-Length`
frames, and enforces a 4 MiB frame budget by default.
