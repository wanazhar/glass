---
id: native-engine-browser-158
scope: glass-browser/native-engine/selected-frame-script-context
status: completed
depends-on: [native-engine-browser-157]
---

# Native engine browser slice 158: selected-frame script context

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Make a selected child browsing context observe the same parent/topology
relationships that the embedding page observes. The recursive snapshot bridge
from slice 157 was usable from the parent realm, but a realm selected through
`selectFrame` still had a self-contained `window` because its parent/top
metadata was not transferred with the real frame identity.

## Contract

- every native JavaScript realm carries its actual frame ID separately from
  its storage/target context ID;
- a selected child realm receives bounded parent and top Window/document
  descriptors, including the active selected child in the parent descriptor's
  recursive child tree;
- selected child scripts observe identity-stable `window.parent`,
  `window.top`, `window.frameElement`, `window.parent.document`,
  `window.parent.frames[n]`, and nested child windows;
- parent-document `defaultView` and the embedding frame's `contentWindow`
  resolve to the selected realm rather than a duplicate proxy;
- local and sandboxed HTTP(S) realms use the same context payload, and the
  content worker validates the current frame identity before installing it;
- the content-worker wire advances to protocol 7 so a mismatched helper cannot
  silently construct a realm with the wrong frame identity;
- frame-target routes accept frame IDs, while popup and target-owner message,
  close, and navigation effects retain the target context ID; storage
  ownership remains target-owned as well.

## Deliberate boundary and tradeoffs

The context is a bounded snapshot assembled by the parent frame registry. It
does not share engine pointers or turn a selected realm into a live remote DOM;
the parent remains authoritative for frame ownership, effects, navigation, and
cross-origin checks. The extra descriptor transfer and active-frame merge cost
some serialization and projection work, but prevents duplicate WindowProxy
identity and makes frame selection behave like a real browsing-context switch.

Parent/top descriptors accept their own `contextId` wire shape while embedded
bindings use `frameId`; the JavaScript bridge normalizes both into one cache
identity. Cross-origin document access, complete Window Web IDL descriptors,
full frame lifecycle/load ordering, and complete browser parity remain issue
#40 gates.

## Implementation surface

- `crates/glass-browser/src/browser/native_engine/javascript.rs`: frame ID
  transport, descriptor normalization, relationship installation, and stable
  document/window caches;
- `crates/glass-browser/src/browser/native_engine/{engine,content_process}.rs`:
  frame identity/context propagation through local and sandboxed realms;
- `crates/glass-browser/src/browser/native_backend.rs`: active-frame inclusion
  in parent/top recursive descriptors;
- `crates/glass-browser/tests/native_engine.rs`: selected-child identity,
  nested traversal, message, and descendant-navigation witness;
- native-engine plan, architecture, and analysis docs: synchronized evidence.

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --tests`;
- selected nested frame identity/effect witness: 1 passed;
- full native integration target after the checkpoint;
- `cargo fmt --all -- --check` and `git diff --check`.
