---
id: native-engine-browser-812
scope: glass-browser/live-iframe-removal-lifecycle
status: in-progress
depends-on: [native-engine-browser-811]
---

# Glass native-engine browser slice 812: live iframe removal

## Objective

Make iframe/frame removal from a still-live parent Document destroy only the
removed child browsing-context subtree. Recreating or moving a removed frame
owner creates a fresh frame identity, even when the DOM operation batch ends
with the same owner node attached. Preserve unrelated child frames, targets,
and their SharedWorker connections.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) owns native-browser
  completion and closure gates.
- The versioned [Glass Core Web Profile](../native-engine-browser-profile.md)
  defines the browser behavior being implemented.
- [Slice 811](native-engine-browser-811.md) binds SharedWorker ownership to
  committed Document generations and tears down replaced Documents and their
  descendant frames.
- The [HTML iframe removal steps](https://html.spec.whatwg.org/multipage/iframe-embed-object.html#the-iframe-element)
  destroy the child navigable without dispatching `unload`. The
  [Document destruction algorithm](https://html.spec.whatwg.org/multipage/document-lifecycle.html#destroying-documents)
  removes the Document from worker ownership and disentangles its ports.

## Contract

- Observe iframe/frame-owner detach and move operations in the script-batch
  DOM model, including an owner removed and reinserted before the batch ends.
- Reconcile attached frame owners by their parent Document and node identity.
  Keep a child browsing context only when its owner remained continuously
  attached; a removal followed by reinsertion receives a fresh frame ID and
  Document.
- Retire only removed frame subtrees. Preserve sibling frame IDs, live
  Documents, SharedWorker owners/connections, and unrelated top-level targets.
- Destroy the removed Document without firing `beforeunload`, `pagehide`, or
  `unload`. Close its page-side MessagePorts and bridge routes, dispatch the
  corresponding worker-side close effects, remove its SharedWorker Document
  owners, and close its native engine.
- Perform worker and route teardown after releasing target/frame registry
  locks. A removed frame ID must not route any queued or later port delivery to
  a replacement frame.
- Complete frame reconciliation at the operation boundary after script or
  action-generated DOM mutations. A subsequent `listFrames` call is not
  required to trigger destruction.
- Keep iframe `src`/`srcdoc` mutation into an existing owner, retained detached
  `WindowProxy` behavior, and complete iframe/WPT conformance as explicit
  separate issue #40 requirements unless this slice's implementation already
  provides and tests them.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-812.md`

## Verification

Use the scoped browser package check before the exact process-backed HTTP
regression. Cover ordinary removal, same-batch removal/reinsertion, no unload
dispatch, old-port isolation, worker-side close, a surviving sibling frame,
and frame subtree retirement. Run formatting and the maintainer handbook's
release-documentation truth, documentation-depth, TUI-shortcut, and
documentation-coverage gates. Record exact commands, warnings, elapsed test
time, and remaining boundaries. Do not push or run remote CI for this local
checkpoint.

## Results

Implementation and verification are in progress.
