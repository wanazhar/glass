# Development product boundary completion

Status: Complete and verified

## Completed checkpoint

- `glass-dev` directly owns the project, buffer, PTY, language, event, graph,
  replay, Neovim, remote-view, and governed tool contracts under
  `glass_dev::development`.
- `DevelopmentWorkspace` and every resident service import those local types;
  no `glass_browser::development` import remains in `glass-dev`.
- Removed the `development-runtime` Cargo feature and its optional dependency
  bridge from `glass-browser`; `glass-dev` uses the ordinary browser package.
- Moved the Pi tool/system assets into the package that owns the runtime.
- Deleted the browser-owned development compatibility module and its legacy
  CLI/MCP dispatch, schemas, session registry, and mixed development TUI.
- The focused browser TUI remains structured-first and responsive at phone,
  compact, and desktop widths. Complete development surfaces remain owned by
  the decomposed `glass-dev` TUI.
- `glass-dev::dispatch` now owns project, agent, daemon, MCP, and development
  TUI behavior. Its fallback into the browser runner is limited to browser
  commands; project and agent operations execute the Glass Dev-owned types.

## Evidence

The exact package checks pass:

```console
cargo check -p glass-browser --no-default-features
cargo check -p glass-browser
cargo check -p glass-dev
```

All 117 `glass-dev` library tests and strict all-target/all-feature Clippy pass.
The browser minimal check and strict all-target/all-feature Clippy also pass,
and repository searches find no browser development module, feature bridge, or
`glass_browser::development` import.
