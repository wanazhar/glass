id: runtime-033-004
scope: Glass v0.3.3 development runtime hardening
status: completed
depends-on: [remote-agent-033-003]

## objective

Harden owned process jobs, explicit project-tree/cache semantics, conflict-safe
files, persistent revision-aware language services and a real `nvim --embed`
Msgpack-RPC proof without weakening the one-way product boundary.

## context

- `docs/plan/analysis/release-033.md`
- `docs/architecture/development-tui.md`
- `docs/development-runtime.md`

## path

- development process/project/editor/language/Neovim runtime
- `glass-dev` ownership facade
- runtime tests, tool integration CI and docs

## verification

- descendant cleanup and explicit poll-error tests
- ignore/truncation/cache invalidation tests
- full LSP document lifecycle/revision and operation tests
- installed Neovim embedded buffer/edit/state round trip

## result

Completed locally on 2026-08-08. Owned process trees, explicit bounded project
snapshots, conflict-safe saves, persistent revision-aware LSP, and real
`nvim --embed` Msgpack-RPC exchange all have integrated regression evidence.
