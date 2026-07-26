# `@glass-browser/client`

This dependency-free TypeScript client starts `glass --mcp` and exposes typed
helpers for `navigate`, `observe`, `click`, and `wait`. It has no Playwright or
browser runtime dependency. Use it with a TypeScript runner such as `tsx`, or
copy the small source file into an agent project.

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
