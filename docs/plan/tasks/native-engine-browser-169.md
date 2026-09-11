# Native engine browser slice 169: frame event target projection

Status: completed locally.

## Objective

Make same-origin frame projections usable as event targets from the parent
realm while preserving the ownership and propagation boundaries of each
document and window.

## Contract

- Projected frame elements, detached projected elements, projected frame
  documents, and frame/window proxies expose addEventListener(),
  removeEventListener(), and dispatchEvent().
- Listener keys are unique to the projected frame node, document, or window,
  so a child event cannot invoke top-level listeners that happen to use the
  same native node index or event type.
- Bubbling events walk the projected parent-node tree and terminate at the
  projected frame document and its default window; detached nodes do not
  bubble into an owner document.
- Projected click(), focus(), and blur() dispatch the corresponding
  parent-side event before retaining their existing typed command handoff to
  the child native owner. Cancelled projected clicks do not queue the child
  click command.
- Existing local/content-worker event behavior, frame-origin checks, and
  child-side command processing remain unchanged.

## Implementation

- Added private frame-qualified event-owner keys and taught the shared
  dispatcher to derive propagation from ownerDocument, parentNode, and
  defaultView.
- Added EventTarget methods to same-origin frame element/document projections,
  detached frame elements, and all window proxies.
- Routed projected focus, blur, and click through the shared event dispatcher
  while preserving the existing frame command queue for committed browser
  behavior.
- Added an HTTP same-origin frame integration witness for capture/bubble
  phases, target/currentTarget identity, listener removal, frame-local
  document/window delivery, and top-level listener isolation.

## Tradeoffs

The projection keeps event callbacks in the evaluating parent realm and uses
the existing bounded listener table; it does not duplicate JavaScript objects
or native arena pointers across content boundaries. Child-owned host input and
child-realm event dispatch continue through the typed frame command path, so a
future event-observation bridge must be added before claiming complete
cross-process event-loop or Web IDL parity.

## Verification

- cargo fmt --all -- --check
- cargo check --quiet -p glass-browser --features native-engine --tests
- cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture
  (441 passed, 0 failed, 0 ignored, 132.23s)
- git diff --check
