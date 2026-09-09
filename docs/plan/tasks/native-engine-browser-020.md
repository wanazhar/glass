---
id: native-engine-browser-020
scope: glass-browser/native-engine/javascript-event-graph
status: done
depends-on: [native-engine-browser-019]
---

# BE-03d/BE-04e: bounded document event graph

## Objective

Replace target-only event delivery with a bounded document event graph while
keeping the current snapshot/command boundary. JavaScript must be able to
observe parent relationships and use capture, target, and bubble listener
phases without giving page code a mutable Rust DOM reference.

## Contract

- Each projected element carries its current nearest element parent index.
  `parentElement` and `parentNode` are non-enumerable host links, so ordinary
  JSON results remain bounded and cannot recurse through the graph.
- Listener records retain callback, capture, and once state. Registration is
  deduplicated by owner, normalized event type, callback, and capture flag;
  removal uses the same identity and capture matching rules.
- Dispatch builds a current target-to-window path from the snapshot and runs
  window/document/ancestor capture listeners, target capture and bubble
  listeners, and ancestor bubble listeners when `bubbles` is true. A listener
  snapshot gives registration/removal deterministic behavior during dispatch.
- `stopPropagation()` prevents later nodes and
  `stopImmediatePropagation()` also prevents later listeners in the current
  dispatch. `once` records are removed after invocation. `preventDefault()`
  retains the existing cancelable/default-action result contract.
- The graph is rebuilt on every host refresh, while listener callbacks remain
  in the persistent realm and are keyed by current document generation/index.
  Full navigation discards both the realm and its listener registry.

## Deliberate boundary and tradeoffs

- Parent links are a projection of the current parsed tree, not live DOM
  identity. Insertion/removal, text-node targets, shadow-root retargeting,
  slots, composed paths, and iframe boundaries remain unimplemented.
- Rust semantic actions still produce Rust-only effects and do not re-enter
  the JavaScript realm. The next owner-action slice must define how listener
  exceptions, cancelation, default actions, and one-revision commits interact
  across local and child IPC paths.
- Event listener objects, `handleEvent`, passive listeners, abort signals,
  pointer/keyboard/IME event families, and queued task ordering remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- focused nested propagation/cancellation test
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` — 289/289 passed.
- `git diff --check`

The next gate is the Rust-action owner bridge: dispatching semantic click,
input, change, focus, and blur events into the correct page realm with
cancelation and default-action ordering, then committing callback mutations
without bypassing child ownership.
