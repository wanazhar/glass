---
id: native-engine-browser-018
scope: glass-browser/native-engine/javascript-dom-mutation
status: done
depends-on: [native-engine-browser-017]
---

# BE-03b/BE-04c: bounded JavaScript DOM mutation commands

## Objective

Connect a small, explicit JavaScript mutation surface to the native document
owner. JavaScript must not mutate a detached snapshot and leave Rust state
behind; it emits validated commands, Glass applies the complete batch to a
document clone, and the committed result is re-projected on the next
evaluation.

## Contract

- Projected elements support `click()`, `value`, `checked`, and `selected`
  setters, `setAttribute()`, and `removeAttribute()`. Each setter updates the
  current evaluation's local projection and appends a typed host command.
- The command vocabulary is bounded and typed: click, text-control value,
  checkbox/radio checkedness, single-select option selectedness, attribute set,
  and attribute removal. Node identity is a current document generation and
  arena index; invalid or detached indices fail closed.
- A script's command list is capped at `MAX_NATIVE_EFFECTS` entries and its
  serialized size is capped by the existing script-result budget. Text values
  and attribute names/values retain the existing native limits; sensitive value
  contents are not placed in events, diagnostics, or logs.
- Commands are applied in order to a clone. If validation fails, neither the
  clone nor the live document is committed. If one or more commands succeed,
  the engine commits the resulting document exactly once with one new revision.
  Programmatic value/checked/selected/attribute changes do not synthesize
  browser input/change events; `click()` reuses the existing semantic click
  owner and emits its bounded focus/click/change effects where applicable.
- Local documents apply commands in the engine owner. External HTTP(S)
  documents apply the same command vocabulary inside the content process and
  transfer the resulting document snapshot/effects over the existing typed
  IPC channel. The parent commits the child result and revision; page code is
  never evaluated in the parent for network documents.
- Clicked links are rejected as script-driven navigation until navigation can
  be coordinated with asynchronous resource loading and history. Unsupported
  element kinds, multi-selects, hidden/disabled action paths, and malformed
  commands remain typed failures.

## Deliberate boundary and tradeoffs

- The command bridge is a useful owner seam, not a standards DOM. It has no
  live object identity, child insertion/removal, text-node replacement,
  attributes as Web IDL reflection, event listener registration/dispatch,
  default actions, mutation observers, focus APIs, forms submission, shadow
  DOM, custom elements, or script-generated navigation.
- One revision per script batch keeps evidence and effects coherent, but it is
  coarser than the browser's internal mutation/event loop. Later event-loop
  work must define task ordering and exception/rollback behavior before
  widening this surface.
- Applying a clone protects the Rust document from partial host commands, but
  JavaScript global variables still persist if a later command fails. A future
  Web IDL transaction model must specify how page-observable JS state and DOM
  state interact on exceptions.
- Attribute changes feed the existing next-snapshot parser/semantic/CSS owners;
  they do not yet trigger a full style invalidation, layout, paint, accessibility
  update, or resource load pipeline.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` — 288/288 passed.
- Coverage includes local and child-owned command application, one-revision
  batching, click effects, empty form-value assignment, checkbox/radio and
  option state, attribute set/remove, fresh host re-projection, and the
  pre-existing native navigation/IPC suite.
- `git diff --check`

The next gate is a real event/Web IDL owner: listener registration, event
dispatch/default-action ordering, focus/blur, mutation invalidation, and
script-visible event effects. Fetch/XHR, timers, modules, page-script
loading, frames, remaining resource classes, and browser parity remain open.
