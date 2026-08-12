id: browser-workspace-036-002
scope: shared browser workspace
status: complete
depends-on: [correctness-036-001]

## Objective

Implement one canonical BrowserWorkspace controller/view and adapt both
standalone Glass Browser and Glass Dev App to it with equivalent operations.

## Context

- `docs/architecture/browser-workspace.md`
- `docs/architecture/connection-presentation.md`
- `docs/architecture/browser-connection.md`

## Path

- `crates/glass-browser/src/browser_workspace/`
- `crates/glass-browser/src/tui/`
- `crates/glass-dev/src/browser.rs`
- `crates/glass-dev/src/tui/`

## Verification

One behavior contract runs against both adapters and covers revisions,
selection, input lease, capability parity, recovery, workflow and bounded state.
