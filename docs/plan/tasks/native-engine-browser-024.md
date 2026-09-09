---
id: native-engine-browser-024
scope: glass-browser/native-engine/javascript-navigation
status: done
depends-on: [native-engine-browser-023]
---

# BE-02f/BE-03h/BE-04i: script-driven link navigation

## Objective

Connect a page's bounded `element.click()` link command to the existing
navigation/history owner. A script click must not be reported as complete when
it only changes focus or emits a click effect.

## Contract

- Top-level local script batches validate at most one non-empty link target,
  apply the click command to a clone, commit its effects, and pass the typed
  link request to the existing local resource/history navigation path.
- Top-level external script batches use the child-owned document to validate
  and apply the link click, transfer a bounded `{node_index, href}` navigation
  request with the final snapshot/effects, and let the parent resolve the link
  through the existing asynchronous navigation path. The parent never parses
  or executes the external page itself.
- Full navigation resets the JavaScript realm; same-document navigation keeps
  it. The existing history/revision owner remains authoritative. Navigation
  failures do not invent a successful target or silently fall back to CDP.
- Script event/callback transactions still reject link activation inside a
  callback preflight; only the top-level script mutation path can emit the
  explicit navigation handoff in this slice.

## Deliberate boundary and tradeoffs

- A script link click and its navigation currently consume separate document
  revisions: one for the click command/effects and one for the navigation
  commit. A future task can merge them after navigation fetch/history and
  callback cancellation ordering are fully transactional.
- Relative navigation remains limited by the existing fixture resolver;
  external relative URL resolution, target contexts, downloads, opener
  relationships, redirects from script, and popup/window creation remain open.
- This is link activation, not a complete HTML default-action model. Form
  submission, keyboard activation, `beforeunload`, unload/pagehide/pageshow,
  and navigation task ordering remain separate work.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- focused local fixture-navigation test
- focused external child-navigation test
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` — 291/291 passed.
- `git diff --check`

The next gate is navigation task ordering and remaining link/default-action
coverage, followed by timers, modules, page-script loading, and Fetch/XHR.
