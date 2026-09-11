---
id: native-engine-browser-157
scope: glass-browser/native-engine/nested-frame-script-bridge
status: completed
depends-on: [native-engine-browser-156]
---

# Native engine browser slice 157: nested frame script bridge

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Make the parent-owned frame-script bridge recursively usable from an embedding
page. Slice 156 exposed direct child frame objects, but a projected child
document could not yet expose its own `iframe.contentWindow`,
`contentDocument`, or `window.frames` chain.

## Contract

- frame bindings transfer their bounded direct-child tree, with each child
  carrying its own document snapshot and nested bindings;
- same-origin projected child documents expose nested frame elements,
  `contentWindow`, `contentDocument`, `frames`, `length`, and numeric child
  windows with stable `Window`/`Document` identity;
- nested WindowProxy objects preserve `window`, `self`, `frames`, `parent`,
  `top`, `frameElement`, and `document` relationships within the embedding
  realm;
- nested frame `postMessage()` and `location.assign()`/`replace()` effects
  route through the existing parent-owned frame resolver, including parked
  descendants;
- cached nested documents and windows refresh when a child revision,
  navigation URL, origin, or descendant topology changes;
- the recursive transfer remains bounded and does not share engine pointers or
  expose a foreign document through a cross-origin binding.

## Deliberate boundary and tradeoffs

The recursive projection is a bounded snapshot tree, not a live multi-realm
DOM. It improves normal embedding-page traversal and effect routing while
keeping each child engine isolated behind the parent queue. A fixed numeric
Window index surface and the existing frame-count/JSON limits bound memory and
script work. Complete selected-child realm `parent` metadata, live cross-realm
object identity, cross-origin Window Web IDL behavior, and full browser frame
lifecycle/topology remain separate issue #40 gates.

## Implementation surface

- `crates/glass-browser/src/browser/native_backend.rs`: recursive binding
  assembly for the parent-owned flat frame registry;
- `crates/glass-browser/src/browser/native_engine/javascript.rs`: nested
  WindowProxy/document projections, dynamic cache refresh, and bounded child
  windows;
- `crates/glass-browser/tests/native_engine.rs`: same-origin nested HTTP
  traversal, identity, message delivery, and navigation witness;
- native-engine architecture/analysis and plan index: checkpoint evidence and
  remaining production gates.

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --tests`;
- focused nested frame witness: 1 passed;
- full native integration target after the checkpoint;
- `cargo fmt --all -- --check` and `git diff --check`.
