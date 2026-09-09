---
id: native-engine-browser-019
scope: glass-browser/native-engine/javascript-events
status: done
depends-on: [native-engine-browser-018]
---

# BE-03c/BE-04d: bounded JavaScript event and focus owner

## Objective

Give the persistent JavaScript realm a bounded event owner that can retain
listeners across evaluations, dispatch host-created events, and express
focus changes through the same validated owner-command boundary as DOM
mutation. Event callbacks must run in the realm that owns the page document;
they must not be reconstructed or executed by the parent for an external page.

## Contract

- Elements, `document`, and `window` support bounded
  `addEventListener`/`removeEventListener`/`dispatchEvent` registration. The
  registry is persistent for a document generation and is discarded on full
  navigation with the JavaScript realm.
- Listener registration is deduplicated by owner, event type, and callback;
  event types are normalized and bounded, callbacks must be callable, and the
  total listener count is capped by the native effect budget.
- The host supplies bounded `Event` and `CustomEvent` constructors with
  `type`, `target`, `currentTarget`, `eventPhase`, `bubbles`, `cancelable`,
  `defaultPrevented`, `detail` for custom events, `preventDefault`, and bounded
  propagation-method stubs. Dispatch invokes a snapshot of target listeners
  synchronously and returns the inverse of `defaultPrevented`.
- `element.focus()` and `element.blur()` update the local host projection and
  emit typed `focus`/`blur` commands. Glass validates focusability, hidden and
  disabled state, applies focus transitions on a document clone, and records
  privacy-safe focus/blur effects in the committed revision.
- `element.click()` performs the bounded focus step, dispatches the target
  click listeners, and only emits its control activation command when the
  click event was not canceled. Local and child-owned realms use the same
  projection and command vocabulary.
- A script batch remains one transaction and one document revision. Listener
  callback exceptions fail the evaluation before the command batch is
  returned; owner validation still prevents partial document publication.

## Deliberate boundary and tradeoffs

- Dispatch is target-local in this slice. There is no ancestor bubbling or
  capture path because the current script snapshot does not yet expose a
  live parent/child Web IDL graph.
- The event registry is an engine-owned JavaScript map, not a Rust callback
  ABI. It enables persistent same-realm callbacks without sending executable
  code through IPC, but Rust semantic actions do not yet re-enter the page
  realm to invoke listeners.
- `preventDefault()` cancels the bounded scripted click activation, but the
  existing Rust action path and navigation/default-action model remain
  separate until event-loop ordering and script navigation are integrated.
- Focus/blur callbacks run synchronously in the script realm. Timers,
  queued tasks, microtasks beyond the existing promise evaluation, mutation
  observers, insertion/removal, pointer/keyboard event families, and full
  Web IDL identity remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- focused local listener/focus test and focused child-process listener test
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` — 289/289 passed.
- `git diff --check`

The next gate is a document event graph and owner-action bridge: ancestor
propagation/capture, Rust-action listener dispatch, default-action ordering,
and mutation invalidation. Timers, modules, page-script loading, Fetch/XHR,
frames, remaining resource classes, and browser parity remain open.
