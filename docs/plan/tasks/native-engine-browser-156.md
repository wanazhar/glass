---
id: native-engine-browser-156
scope: glass-browser/native-engine/frame-script-bridge
status: completed
depends-on: [native-engine-browser-155]
---

# Native engine browser slice 156: frame script bridge

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Connect the already parent-owned native frame tree to the embedding page's
JavaScript realm. Before this slice, Rust callers could list and select child
frames, but ordinary page scripts saw no usable `iframe.contentWindow`,
`contentDocument`, or `window.frames` relationship.

## Contract

- directly embedded `iframe` and `frame` elements expose stable
  `contentWindow` proxies keyed to their parent-owned frame IDs;
- same-origin HTTP(S) child documents expose bounded `contentDocument`,
  `Window.document`, `Document.defaultView`, document metadata, element
  identity, selectors, collections, and `frameElement` projections;
- cross-origin child documents retain a WindowProxy surface while
  `contentDocument` is null and direct `Window.document` access fails closed;
- the embedding realm exposes direct child windows through `window.frames[n]`
  and `window.length`, with stable `parent` and `top` relationships;
- direct frame WindowProxy `postMessage()` and `location.assign()`/
  `replace()` effects are routed through the parent target owner, origin
  checked, and applied to the actual child engine;
- child navigation replaces the child document projection without reloading
  the frame from the stale embedding `src`, while descendant owners are
  drained before later topology discovery;
- local and sandboxed HTTP(S) runtime paths use the same bounded projection
  and transfer contract.

## Deliberate boundary and tradeoffs

Frame bindings are snapshots assembled by the parent registry; they do not
share a Rust engine pointer or let a child JavaScript realm bypass the parent
effect queue. Same-origin access is limited to the existing snapshot DOM and
collection model, so it improves real page feature detection and common
cross-frame reads without claiming live cross-realm object identity, complete
Web IDL descriptors, shadow/custom elements, ranges, nested child-window
projections in every event path, or full browser topology. Cross-origin
WindowProxy-safe properties remain available, while document access is denied
instead of exposing a foreign snapshot.

The frame owner remains authoritative for child navigation, postMessage
target-origin matching, child lifecycle, and descendant cleanup. This keeps
the content worker isolated and makes frame effects observable and
recoverable, at the cost of an explicit parent synchronization step before
each embedding script evaluation.

## Implementation surface

- `crates/glass-browser/src/browser/native_engine/javascript.rs`: frame
  binding transfer, WindowProxy/document projections, and frame collection
  relationships;
- `crates/glass-browser/src/browser/native_engine/{engine,content_process,dom}.rs`:
  bounded binding storage and sandboxed IPC transfer;
- `crates/glass-browser/src/browser/native_backend.rs`: frame ownership,
  same-origin snapshot assembly, postMessage delivery, and child navigation;
- `crates/glass-browser/tests/native_engine.rs`: same-origin HTTP identity,
  mutation delivery, and child navigation witness;
- issue #40 and the native-engine architecture/analysis docs: checkpoint
  evidence and remaining production gates.

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --tests`;
- same-origin frame identity/effect witness: 1 passed;
- full native integration target after the checkpoint;
- `cargo fmt --all -- --check` and `git diff --check`.
