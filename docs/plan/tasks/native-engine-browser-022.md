---
id: native-engine-browser-022
scope: glass-browser/native-engine/javascript-click-preflight
status: done
depends-on: [native-engine-browser-021]
---

# BE-03f/BE-04g: atomic cancelable click preflight

## Objective

Move cancelable scripted click delivery ahead of semantic activation. Page
listeners must be able to cancel a Rust-owned click without allowing a partial
document commit, and external pages must receive the same behavior inside the
content process.

## Contract

- When a local page has a JavaScript realm, a semantic click is evaluated on a
  cloned document: focus transition first, focus callbacks, target click
  callbacks, callback commands, then the control's default activation when the
  click is not canceled.
- `preventDefault()` on the preflight click suppresses the control activation
  while preserving the focus transition and the click effect record. The
  action remains an accepted user action and commits one revision.
- Callback commands and default activation are applied to one clone. The
  resulting state, revision, and privacy-safe effects are committed together;
  a callback failure or invalid command leaves the Rust document unchanged.
- External HTTP(S) clicks use one typed `mutate_click_preflight` request. The
  sandboxed content worker owns the clone, JavaScript dispatch, cancellation,
  default activation, and final snapshot/effects transfer. The parent commits
  one validated revision and never evaluates the network page.
- Pages without an initialized local realm retain the existing Rust-only click
  path. The bridge is activated only after script-capable page code has
  created a realm, avoiding a needless runtime for pure semantic callers.

## Deliberate boundary and tradeoffs

- This slice defines cancellation for click activation only. Type/input/change
  ordering, link navigation, form submission, pointer/keyboard activation,
  and dialog/download default actions remain separate gates.
- A callback can mutate persistent JavaScript globals before a later host
  command fails; the Rust document still remains clone-atomic. A future
  transaction boundary must specify page-global rollback or exception-visible
  DOM behavior.
- The public `NativeActionResult` reports the accepted action revision; event
  listener callbacks are synchronous and bounded by the existing script,
  command, memory, and timeout budgets.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- focused local click preflight/cancelation test
- focused child-process click preflight/cancelation test
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` — 289/289 passed.
- `git diff --check`

The next gate is transactional input/change and focus event ordering for
Rust-owned type actions, followed by link navigation/default actions, timers,
modules, and Fetch/XHR.
