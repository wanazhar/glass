# Development product boundary completion

Status: Core extraction complete; CLI and compatibility retirement pending

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

## Evidence

The exact package checks pass:

```console
cargo check -p glass-browser --no-default-features
cargo check -p glass-browser
cargo check -p glass-dev
```

All 117 `glass-dev` library tests and strict all-target/all-feature Clippy pass.
Gate 3 remains open until `glass-dev::dispatch` owns its complete command model
and the browser compatibility module can be deleted.
