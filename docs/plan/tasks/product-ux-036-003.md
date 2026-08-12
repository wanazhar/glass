id: product-ux-036-003
scope: Glass Dev desktop product UX
status: pending
depends-on: [browser-workspace-036-002]

## Objective

Replace architecture-surface and raw-JSON defaults with direct Agent, Code,
App, Terminal, Tasks, Git, Debug and contextual More work surfaces.

## Context

- `docs/architecture/product-workspace.md`
- `docs/architecture/browser-workspace.md`

## Path

- `crates/glass-dev/src/tui/`
- resident development service adapters and focused tests

## Verification

Desktop/compact interaction tests cover Agent conversation, editor, App,
tasks, Git/runtime/debug projections, focus, local scrolling, context and
confirmation/recovery overlays.
