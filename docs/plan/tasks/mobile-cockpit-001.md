id: mobile-cockpit-001
scope: development runtime, MCP, TUI, clients
status: done
depends-on: []

## objective

Deliver resident development sessions, reconnect capsules, a bounded attention
inbox, semantic tap mode, verification cards, adaptive live quality, and
higher-level TypeScript/Python workflows without weakening Glass privacy or
structured-first contracts.

## context

- `docs/architecture/mobile-cockpit.md`
- `docs/development-runtime.md`
- `docs/architecture/development-tui.md`
- `docs/mobile-remote.md`

## path

- `crates/glass-browser/src/development/`
- `crates/glass-browser/src/mcp/server.rs`
- `crates/glass-browser/src/tui/`
- `clients/typescript/`
- `clients/python/`
- `docs/`

## verification

- Rust unit, MCP integration, and TUI reducer tests
- TypeScript typecheck/build/smoke
- Python compile/smoke
- full workspace fmt, Clippy, tests, rustdoc, fuzz, docs, and package gates
