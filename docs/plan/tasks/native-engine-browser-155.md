---
id: native-engine-browser-155
scope: glass-browser/native-engine/web-idl-foundation
status: completed
depends-on: [native-engine-browser-154]
---

# Native engine browser slice 155: Web IDL foundation

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Give ordinary page scripts the foundational browser object identity they
expect from a native realm. The existing native host projected useful DOM,
location, event, and collection values, but those values were plain objects or
arrays; common feature detection and type checks therefore disagreed with a
browser even when the underlying behavior was available.

## Contract

- `window` is a `Window` instance and exposes stable `self`, `parent`, `top`,
  `frames`, and `length` values for the current top-level realm;
- `document` is a `Document`/`Node` instance with `nodeType`, `nodeName`, URL,
  visibility, compatibility, and `defaultView` metadata;
- native elements expose `Node`/`Element`/`HTMLElement` identity, node-name
  metadata, `ownerDocument`, and the supported concrete HTML element
  constructors for the current tag;
- `Location` identity covers the live page location and WindowProxy location
  values without changing their existing navigation command ownership;
- `NodeList` and `HTMLCollection` results retain array-compatible bounded
  behavior while exposing their corresponding Web IDL identity;
- `Event`, `CustomEvent`, and `StorageEvent` instances preserve their event
  fields and pass the corresponding `instanceof` checks across repeated host
  refreshes;
- local and sandboxed HTTP(S) content-process realms use the same identity
  contract, and refreshes do not leave element `ownerDocument` references
  attached to an obsolete host snapshot;
- constructors remain non-constructible where the browser platform requires
  an existing platform object, and no identity claim changes unsupported DOM
  mutation or layout behavior.

## Deliberate boundary and tradeoffs

The constructors are a bounded compatibility layer over the existing Rust DOM
and QuickJS host objects. It improves feature detection, common library type
guards, and event/collection interoperability without introducing a second DOM
implementation or copying document state into a separate object graph. The
element constructor set covers the native HTML controls and embedding elements
already owned by the host; complete Web IDL descriptors, live tree mutation,
shadow/custom-element constructors, ranges, and the rest of the platform
remain separate browser-completeness work.

The host reuses element objects across refreshes for listener and control
state, so `ownerDocument` resolves through the current global document rather
than closing over a previous bootstrap snapshot. This preserves object
identity while preventing stale-realm observations.

## Implementation surface

- `crates/glass-browser/src/browser/native_engine/javascript.rs`: bounded
  constructors, prototype chains, node metadata, collection projections, and
  current-document identity;
- `crates/glass-browser/tests/native_engine.rs`: local and content-process
  identity witnesses;
- `docs/architecture/native-engine.md`, `docs/plan/README.md`, and
  `docs/plan/analysis/native-engine.md`: synchronized status and tradeoffs;
- GitHub issue #40: exact local evidence and remaining production gates.

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --tests`;
- focused Web IDL identity witnesses: 2 passed;
- full native integration target: 438 passed, 0 failed;
- `cargo fmt --all -- --check` and `git diff --check`.
