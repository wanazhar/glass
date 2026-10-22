---
id: editor-buffer-reconcile-001
scope: issue-63-external-core-edits
status: complete
depends-on: [issue63-mixed-tools-001]
---

# Editor buffer reconciliation with external edits

## Objective

Reconcile open Glass editor buffers with files changed by Core while preserving
unsaved human text. Clean buffers refresh from disk. Dirty buffers expose an
external snapshot/deletion and support explicit compare, reload, or keep-local
resolution. Saves validate the target file's actual content identity at the
write boundary and do not use the unrelated project revision as a guard.

## Context

- `docs/plan/analysis/editor-buffer-external-reconcile.md`
- `docs/architecture/development-tui.md`
- `docs/development-runtime.md`
- `AGENTS.md`

## Path

- `crates/glass-dev/src/development/project.rs`
- `crates/glass-dev/src/development/events.rs` only if attribution types need
  a narrowly scoped external/unknown representation
- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/mod.rs`
- `crates/glass-dev/src/tui/render.rs`
- `crates/glass-dev/src/tools.rs` or CLI editor routing only if required to
  expose the same resolution contract
- `docs/glass-core-harness.md` is out of scope; update the current editor and
  development runtime guides instead
- Relevant project and TUI tests in the same modules

## Acceptance

- [x] Clean open buffers adopt external file changes on reconciliation.
- [x] Dirty buffers preserve local text and expose changed/deleted disk state.
- [x] Compare, reload, and keep-local are distinct explicit resolutions.
- [x] A stale or concurrent target-file change rejects save; unrelated
  project revision changes do not reject save.
- [x] Deletion/source-rename conflict is represented and safely resolvable.
- [x] External writes are never attributed to the local human or an agent
  without actor evidence.
- [x] The TUI detects changes through bounded polling only while buffers are
  open and presents compact resolution actions.
- [x] Deterministic tests cover clean refresh, dirty preservation/conflict,
  compare/reload/keep-local, guarded save, unrelated-file changes, and deletion.
- [x] Focused `glass-dev` tests, strict scoped Clippy, formatting, and relevant
  documentation validators pass.
- [x] Commit locally with a Conventional Commit message; do not push, release,
  alter active worktrees, restart services, or touch installed binaries.

## Verification

- `cargo test -p glass-dev --lib development::project::tests --locked` — 22 passed.
- Focused TUI reconciliation and bounded-diff tests — passed.
- `cargo clippy -p glass-dev --all-targets --locked -- -D warnings` — passed.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- Documentation depth, coverage, release-truth, and version-sync validators — passed.
- Implementation commit: `faf65716 feat: reconcile external editor buffer changes`.

## Design constraints

Do not overwrite dirty buffers during automatic refresh. Do not infer an
external actor from filesystem state. Do not reject a save because another
file changed or the workspace's global project revision advanced. Do not add a
watcher daemon or a large editor redesign.
