---
id: native-engine-browser-007
scope: glass-browser/native-engine/content-process-mutation
status: done
depends-on: [native-engine-browser-006]
---

# BE-01f: child-owned DOM mutation and cancellation

## Objective

Move the bounded semantic click/type mutation state for external documents into
the native content worker. The worker now retains the parsed document after a
successful load, applies one transaction against a cloned child document, and
returns a fresh typed snapshot plus privacy-safe event effects. The parent
remains the single revision/history/effect publisher and commits the returned
state only after validating every wire entry.

This is a process-ownership and failure-atomicity slice, not script or browser
event-loop completion. Only the existing bounded click/type control model is
covered; link navigation remains an explicit parent-owned navigation handoff.

## Contract

- A successful child load installs the parsed `NativeDocument` in the worker;
  failed loads leave the previous child document untouched.
- External click/type actions send a node index and typed action payload over
  the versioned request-ID-correlated IPC channel. The child revalidates the
  node and actionability contract instead of trusting parent-only locators.
- Mutations run on a cloned child document. An action that fails validation or
  exceeds the bounded effect quota cannot partially change the worker state.
- The child returns one bounded DOM/computed-style snapshot and typed effects
  (`blur`, `focus`, `click`, `input`, or `change`). The parent validates node
  indices, reconstructs the arena with the current generation, advances its
  revision exactly once, and records effects without transferring raw values.
- Mutation IPC has a five-second deadline. A timeout, broken pipe, malformed
  snapshot, malformed effect, or child rejection poisons and terminates the
  content process. The parent does not claim success or silently fall back to
  CDP; a later external navigation creates a fresh worker.
- Scroll remains parent-owned because it changes viewport/history state rather
  than child DOM state. Non-empty link activation also remains parent-owned so
  navigation and history cannot be reported as a local mutation.

## Tradeoffs and missed behavior

- Full snapshots on every control action are intentionally simple and preserve
  failure atomicity, but they copy the bounded DOM and style cache repeatedly.
  A later binary delta/shared-memory protocol can reduce copies after the
  ownership and recovery contract stabilizes.
- Node-index actions avoid sending locators or raw DOM text back into the
  worker, but they require parent and child to remain on the same committed
  document. A navigation or poisoned worker invalidates the session path and
  must be recovered by a fresh load.
- The current mutation set changes control state and focus only; it does not
  run JavaScript handlers, dispatch a standards event loop, mutate attributes
  or structure, invalidate styles, or implement selection/forms parity.
- The parent and child still use the same bounded Rust document owner. This
  improves process ownership and recovery behavior but does not yet provide an
  OS sandbox, site isolation, supervisor restart, or hostile-script boundary.

## Paths

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

The mutation-ownership slice was checked and tested as one batch:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine --locked`
- `cargo test -p glass-browser --features native-engine --test native_engine native_content_process_ --locked -- --nocapture`

The affected check passed without warnings. The child-owned external checkbox
and text-input mutation/effect test passed 1/1, and the existing child load,
limit, malformed-document, and computed-style process tests passed 4/4.
The wider runtime cancellation, supervisor recovery, diagnostics transfer,
script execution, standards events, OS sandboxing, WPT, security, and browser
promotion gates remain open.
