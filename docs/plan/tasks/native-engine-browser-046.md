---
id: native-engine-browser-046
scope: glass-browser/native-engine/navigation-cancellation-history
status: done
depends-on: [native-engine-browser-045]
---

# BE-01j/BE-02v/BE-03x/BE-04ab: bounded navigation cancellation and traversal events

## Objective

Close the next navigation edge without pretending that the native history
model is already a full bfcache/session-history implementation.

## Contract

- Replacement navigation dispatches a non-bubbling, cancelable window
  `beforeunload` before `pagehide`/`unload` or resource loading.
- `preventDefault()` or a non-empty `beforeunload` `returnValue` cancels the
  navigation, leaves the current document/URL intact, and does not issue a
  replacement request.
- Accepted replacement navigation preserves the existing order: `pagehide`,
  `unload`, resource replacement, then `pageshow`.
- Local and child-owned navigation paths use the same typed cancellation and
  callback-mutation boundary.
- Same-document history traversal dispatches non-bubbling, non-cancelable
  window `popstate` before `hashchange`; push-style fragment navigation keeps
  its existing hashchange-only behavior.

## Deliberate boundary and tradeoffs

This slice does not add prompts, persisted history state, bfcache entries,
cross-document traversal restoration, popup/opener contexts, or full HTML
navigation task-source semantics. A canceled child `beforeunload` still
commits bounded callback mutations in the child owner; navigation is the only
operation canceled. The process request is serialized synchronously at the
navigation boundary, so no network request is started after cancellation.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/interaction.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `native_local_beforeunload_can_cancel_replacement_navigation`
- `native_content_process_beforeunload_can_cancel_replacement_navigation`
- `native_local_history_traversal_orders_popstate_before_hashchange`
- `cargo fmt --all -- --check`
- `git diff --check`

Remote CI, source push, release, tag, registry publication, browser-parity,
and issue-closure claims remain pending the wider browser-complete gates.
