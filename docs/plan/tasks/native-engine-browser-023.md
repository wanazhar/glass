---
id: native-engine-browser-023
scope: glass-browser/native-engine/javascript-type-events
status: done
depends-on: [native-engine-browser-022]
---

# BE-03g/BE-04h: transactional type/input/change event bridge

## Objective

Make Rust-owned text typing observable to page code with browser-shaped event
ordering and one owner revision. A listener that captures an element across
host refreshes must still be able to send a bounded mutation command to the
current Rust document.

## Contract

- Local type actions apply the bounded value change to a document clone, then
  dispatch the generated focus/blur, input, and change events in order against
  the updated projection. Callback commands are applied to that same clone.
- External type actions use one `mutate_type_events` request. The content
  worker owns the value update, event sequence, callback commands, and final
  document/effect transfer; the parent commits one validated revision.
- Input and change events remain non-cancelable in this slice. Programmatic
  callback setters do not synthesize a second input/change sequence, avoiding
  recursive event amplification.
- The host bootstrap publishes the current command vector as the active sink.
  Persistent callback closures and previously projected element methods route
  setters through that current bounded buffer, so captured callback elements do
  not silently write to a detached evaluation buffer.
- Value, command, event, memory, stack, source, result, and timeout limits
  remain enforced. Raw form values stay out of effects and diagnostics.

## Deliberate boundary and tradeoffs

- This is command-sink continuity, not full live Web IDL identity. Captured
  snapshot properties and tree links can still be stale until the next host
  refresh; insertion/removal, selection/range APIs, mutation observers,
  `beforeinput`, composition, keyboard input, and form submission remain open.
- Callback exceptions fail the transactional operation before publication, but
  persistent JavaScript globals and listener closures are not rolled back.
- Focus/input/change dispatch for Rust type actions is synchronous and bounded;
  task queues, timers, microtasks beyond the existing promise drain, and
  script navigation remain separate workstreams.

## Paths

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- focused local type/input/change callback test
- focused child-process type/input/change callback test
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` — 289/289 passed.
- `git diff --check`

The next gate is link navigation/default-action ownership and script-driven
navigation, followed by timers, modules, page-script loading, and Fetch/XHR.
