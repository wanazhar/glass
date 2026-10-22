# Editor buffers and external file changes

## Current behavior

`ProjectWorkspace` stores open `EditorBuffer` values in memory. Each buffer has
its local text, a content hash from open/save, a dirty bit, cursor/selection,
and an actor. `save_buffer` compares the current file hash with
`original_hash`, then delegates to `write_file`; this correctly rejects a
change that happened before save but does not record external divergence on
the buffer or offer a resolution flow. `open_buffer` returns an already-open
buffer without refreshing it from disk. TUI projection refresh copies the
buffer map into render state but does not reconcile the files.

There is no filesystem watcher or `notify`/inotify dependency in the Glass
workspace. The TUI event loop already wakes every 50 ms and calls periodic
state ticks, so the narrow integration will reconcile open files from that
loop on a slower bounded interval. Project-level unit tests will call the same
reconciliation method directly and stay deterministic.

## Ownership and state

The in-memory buffer remains the authority for unsaved human text. Its base
file version is the content identity observed at open or the last explicit
resolution. The project revision is not a file version: unrelated edits must
not block a save.

Each buffer can expose an external-change snapshot with the current on-disk
content identity and, when readable, bounded text. A missing path is a
detectable deletion (including the source side of a rename); Glass will not
guess the rename destination. External filesystem writes have no trustworthy
actor record, so reconciliation and resolution must retain an explicit
unknown/external attribution instead of assigning the local human or embedded
agent.

## Contract

Reconciliation compares each open buffer's base content identity with disk:

- A clean buffer adopts changed disk text, updates its base identity, and
  stays clean while preserving cursor/selection when they remain valid.
- A dirty buffer never adopts external text automatically. It retains local
  text and records the external snapshot or deletion so the conflict is
  detectable through the editor buffer projection.
- Compare exposes the observed external snapshot and leaves the conflict
  unresolved.
- Reload explicitly replaces local text with the latest disk state and clears
  undo/redo and conflict state. A deleted file cannot be reloaded as text.
- Keep local explicitly rebases the human text onto the latest file identity,
  leaves it dirty, and clears the observed conflict. A later save still checks
  that latest identity, so a subsequent external write is rejected.

Saving uses the buffer's expected file identity, not the workspace-wide
revision. The guarded write validates the file immediately before replacement
and uses the existing sibling-temp/atomic-rename path. A changed or deleted
file returns a conflict rather than silently replacing newer content. This
does not claim cross-process locking; the file is revalidated at the write
boundary because the Core writer does not participate in Glass locks.

The TUI adds a small visible conflict line and explicit compare, reload, and
keep-local actions, routed through the existing editor command/key handling.
The collaboration and ownership surfaces otherwise remain unchanged.

## Module and integration boundaries

| Module | Contract | Integration check |
|---|---|---|
| `development/project.rs` | File identity, clean refresh, dirty conflict snapshot, explicit resolution, expected-version save | Deterministic tests with temporary project files |
| `tui/state.rs` | Periodic reconciliation, focused buffer projection, resolution actions | TUI state test proves displayed dirty text survives and actions resolve conflict |
| `tui/mod.rs` / `tui/render.rs` | Bounded tick scheduling and compact conflict affordance | Existing render/state tests plus focused new assertion |
| `tools.rs` and CLI editor routes | Preserve save conflict result and report external snapshot/resolution consistently | Existing registered editor surface uses same ProjectWorkspace methods |

No watcher library exists. Use the existing periodic TUI tick with a bounded
interval; do not add a new background service or poll files when no buffers
are open. Tests invoke reconciliation directly rather than sleeping.

## Verification targets

Tests must cover clean refresh, dirty-text preservation and detectable
conflict, compare/reload/keep-local semantics, stale/racing save rejection,
save after unrelated file/revision changes, and file deletion (rename source
disappearance). Existing editor undo, proposal, actor attribution, and TUI
buffer tests must remain valid.
