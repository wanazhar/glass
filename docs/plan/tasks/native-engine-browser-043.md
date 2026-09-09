---
id: native-engine-browser-043
scope: glass-browser/native-engine/hashchange
status: done
depends-on: [native-engine-browser-042]
---

# BE-02t/BE-03u/BE-04y: bounded same-document hashchange

## Objective

Make same-document fragment navigation observable without a network reload,
while retaining the current page realm and providing the standard URL metadata
needed by navigation callbacks.

## Contract

- A GET navigation whose URL differs from the current URL only by its fragment
  updates the current URL/history entry through the existing same-document
  owner and does not fetch or replace the document.
- Local and child-owned realms receive one non-bubbling, non-cancelable
  `hashchange` event on the window with `oldURL`, `newURL`, and the new
  `location.href` visible to the callback.
- The event is delivered after the URL owner has selected the new fragment and
  before the navigation result is returned. Callback document mutations remain
  bounded and use the existing typed owner path.
- The child updates its committed document URL for subsequent script and
  history operations; the parent validates the fragment-only URL relationship.

## Deliberate boundary and tradeoffs

- This does not implement cancelable `beforeunload`, `popstate`, full history
  traversal lifecycle/bfcache semantics, target contexts, or the complete HTML
  navigation task model.
- Non-GET same-document requests remain subject to the existing navigation
  method rules; fragment changes do not bypass method validation.
- The native engine remains default-off inside `glass-browser`; exactly two
  installable crates remain.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/INDEX.md`

## Paths

- `crates/glass-browser/src/browser/native_engine/interaction.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- local hashchange/no-reload test 1/1
- child hashchange/no-reload test 1/1
- existing bounded fragment/history regression 1/1
- `cargo fmt --all -- --check`
- `git diff --check`

Remote CI, source push, release, tag, registry publication, browser-parity,
and issue-closure claims remain pending the wider browser-complete gates.
