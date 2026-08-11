# Development product boundary completion

Status: Core and development dispatch complete; compatibility retirement pending

## Completed checkpoint

- `glass-dev` directly owns the project, buffer, PTY, language, event, graph,
  replay, Neovim, remote-view, and governed tool contracts under
  `glass_dev::development`.
- `DevelopmentWorkspace` and every resident service import those local types;
  no `glass_browser::development` import remains in `glass-dev`.
- Removed the `development-runtime` Cargo feature and its optional dependency
  bridge from `glass-browser`; `glass-dev` uses the ordinary browser package.
- Moved the Pi tool/system assets into the package that owns the runtime.
- Browser-only builds retain a deprecated non-executable compatibility module
  while their legacy CLI/TUI/MCP references are retired. They no longer expose
  PTY or `glass.toml` execution.
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
Gate 3 remains open until the deprecated browser compatibility module and its
legacy browser CLI/TUI/MCP type references can be deleted.
