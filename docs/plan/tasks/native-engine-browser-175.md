# Native engine browser slice 175: layout geometry and resize observation

Status: completed locally.

## Objective

Expose the native layout engine to page scripts through the standard geometry
surface and deliver resize observations from the same Rust-owned layout
revision in local, content-worker, and projected same-origin frame realms.

## Contract

- `getBoundingClientRect()` returns a DOMRect backed by the current native
  border box, translated by the root scroll offset without viewport clipping.
- `getClientRects()` returns the bounded element client rectangle for rendered
  boxes and an empty collection for zero-sized/unrendered elements.
- `clientWidth`, `clientHeight`, `offsetWidth`, `offsetHeight`, `scrollWidth`,
  and `scrollHeight` expose the current bounded native dimensions.
- `DOMRect` and `DOMRectReadOnly` constructors retain identity across
  same-document evaluations and expose JSON-safe edge accessors.
- `ResizeObserver` registrations, `disconnect()`, `unobserve()`,
  `takeRecords()`, content/border/device-pixel size arrays, and bounded record
  queues are available in the page realm.
- Resize delivery is scheduled at the existing Promise-job checkpoint and is
  driven by dimension changes, not position-only root scrolling.
- Geometry snapshots include projected same-origin frame documents. Frame
  node identity and origin validation remain owned by the existing frame
  bridge.
- The isolated content worker receives parent-owned scroll offsets before
  script and input-event evaluation, so process-backed geometry follows the
  visible viewport.

## Implementation

- Added layout-backed geometry and scroll fields to the typed script document
  snapshot, derived from `NativeLayoutSnapshot` border/content boxes.
- Refreshed local and frame-projected element geometry from persistent host
  state, including frame bindings and detached-node zero geometry.
- Added DOMRect constructors, element geometry accessors, and bounded
  ResizeObserver host delivery.
- Added a fail-closed framed `scroll_sync` content-worker message and wired it
  before process-backed script/input evaluation.

## Tradeoffs

The native layout model intentionally transfers integer CSS-pixel geometry and
uses its existing bounded content-box calculation; it does not create a second
layout owner in JavaScript. Resize notifications are emitted when dimensions
change at a host evaluation checkpoint, while position-only scrolling does not
produce a resize record. Layout/resource observer expansion, full fractional
CSS geometry, and browser-wide conformance remain separate issue-40 gates.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_script_exposes_layout_geometry_and_resize_observer -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_script_exposes_web_idl_identity -- --nocapture`
  (1 passed, 0 failed; includes process-backed geometry and scroll sync)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
  (1 passed, 0 failed)
- implementation commit `586f82ce`
