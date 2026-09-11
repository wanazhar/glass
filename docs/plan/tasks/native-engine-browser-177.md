# Native engine browser slice 177: History API across event boundaries

Status: completed locally.

## Objective

Make same-document History API mutations observable and durable regardless of
which native execution boundary produced them: direct page evaluation, local
user input/lifecycle dispatch, or the isolated HTTP(S) content worker.

## Contract

- `history.state` is structured-cloned, bounded, and exposed with the active
  entry; invalid or oversized state fails with a typed error.
- `pushState()` creates a same-document entry, `replaceState()` replaces the
  active entry, and relative same-origin URLs resolve against the current
  document URL without a resource reload.
- `history.length`, `back()`, `forward()`, and bounded `go(delta)` remain
  coherent after page evaluation and after event-handler mutations.
- Popstate delivery observes the activated entry's state and URL; fragment
  traversal retains the established popstate-before-hashchange ordering.
- Local click, type, form, key, beforeunload, page lifecycle, popstate, and
  hashchange handlers do not lose History API commands while their DOM
  mutations are committed.
- Process-backed click/type/form/key/lifecycle callbacks return a validated
  typed history envelope. The worker updates its own realm before evaluating
  the next callback in the same action and persists the final document URL.
- Direct HTTP(S) evaluation keeps initial History API commands separate from
  history commands emitted by validation, submit, and fetch-resolved callback
  turns; the parent applies both in source order exactly once.
- History traversal cannot be silently combined with a competing default or
  script navigation. Bounds and malformed command payloads fail closed.

## Implementation

- Added persistent JSON history state and document-lifecycle identity to
  `NativeHistory` entries.
- Added typed `HistoryPushState`, `HistoryReplaceState`, and `HistoryGo`
  commands to the native JavaScript host, including same-origin URL
  validation and state-size limits.
- Added local-engine history staging for direct evaluation and every local
  event/lifecycle transaction, including key sequences and re-entrant
  traversal.
- Added a process mutation history envelope, strict decode validation, and
  worker-side history synchronization between callback evaluations.
- Added parent-side asynchronous mutation application and content-worker URL,
  state, length, and traversal synchronization.

## Tradeoffs

The implementation keeps Rust as the history owner and the JavaScript realm
as a typed command producer. It uses the existing bounded navigation and
revision model, so it does not introduce a second asynchronous session-history
store or a bfcache. A `HistoryGo` command is intentionally rejected when the
same dispatch also requests a competing navigation; this preserves failure
atomicity and avoids an ambiguous handoff. Cross-document history still uses
the existing native navigation owner, and full browser session-history,
cross-origin, bfcache, and Web IDL conformance remain issue-40 promotion work.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_`
  (66 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_action_event_preserves_history_api`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_script_exposes_same_document_history_api`
  (1 passed, 0 failed)
- implementation checkpoints: `a561259b`, `19943ae9`, `8f93b162`, `28bf73b9`
