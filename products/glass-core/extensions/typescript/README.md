# TypeScript extension starter

This starter contributes the `ts_preview` tool and `/ts-demo` command. Both return
native Markdown previews with diagnostics. They do not change files.

Build from the repository root:

```sh
npm ci --ignore-scripts
npm run build
```

Configure an absolute path:

```toml
extensions = ["node /absolute/path/to/glass-core/extensions/typescript/dist/index.js"]
```

Start GC and open `/ts-demo`. The command opens GC's native output viewer. Esc
returns to the palette when launched there.

`src/index.ts` contains the small handshake manifest. `src/preview.ts` is imported
on the first `ts_preview` call and retained for the session. Customize these two
files, validate arguments, and declare mutating tools as `serial`.

See the [SDK guide](../../packages/extension-sdk/README.md) for typed hooks, host
calls, state, cancellation, output limits, and a complete example.
