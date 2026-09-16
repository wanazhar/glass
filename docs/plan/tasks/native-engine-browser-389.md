# Native ImageBitmap transfer (389)

```yaml
id: native-engine-browser-389
scope: native-engine/imagebitmap-transfer
status: done
depends-on:
  - native-engine-browser-388
```

## Objective

Make the already-rendered native `ImageBitmap` resource transferable through
the shared structured-clone owner. A bitmap must remain a pixel-backed platform
object when it crosses a same-realm channel or a native page/worker boundary;
it must not degrade into an empty JavaScript object.

## Contract

- `MessagePort.postMessage()` and `structuredClone(value, { transfer })` accept
  native `ImageBitmap` members and preserve dimensions, pixels, and the
  origin-clean bit.
- The encoded transfer is bounded by the existing native message and canvas
  limits. Pixel descriptors are validated on both encode and decode, including
  exact RGBA length and byte values.
- A receiver gets a fresh realm-local `ImageBitmap` object. References to the
  same transferred bitmap in one graph resolve to the same receiver object.
- A source bitmap closes only after successful clone admission. Closed source
  bitmaps and bitmap values omitted from the transfer list fail with
  `DataCloneError`.
- Page, worker, popup, and HTTP content-process routes continue to use the
  existing JSON-framed transport; no Rust wire-format or CDP dependency is
  added.

## Delivered behavior

- Added page-owned transfer descriptor, detach, and receiver-factory hooks to
  the native canvas owner.
- Added a worker-realm fallback `ImageBitmap` surface so worker messages can
  receive, inspect, and return transferred bitmaps without a DOM canvas.
- Extended the tagged graph with bounded `transfer_values` and an ImageBitmap
  transfer marker. The same path is used by local channels, worker messages,
  WindowProxy routes, Service Worker messaging, and content-process events.
- Added a deterministic page-canvas/worker round trip that verifies local
  `structuredClone`, source closing, closed-source rejection, worker identity
  and dimensions, and pixel-perfect redraw after the bitmap returns.

## Tradeoffs and explicit follow-up

The current JSON-framed bridge copies RGBA pixels, so a transfer is ownership
safe but not zero-copy. The existing 256 KiB message budget intentionally
limits the useful cross-realm bitmap size until a bounded binary transport is
introduced. The worker receiver exposes the transferable ImageBitmap surface;
full worker-side canvas creation and rendering remain a separate platform
slice. OffscreenCanvas and other transferable platform objects, full Message
Port Web IDL semantics, task-source ordering, broader Core Web Profile parity,
and final production certification remain issue #40 gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/README.md`
- `docs/plan/tasks/native-engine-browser-389.md`

## Verification

Validation completed locally against this checkpoint:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_image_bitmap_transfer_preserves_pixels_across_worker_realm --locked` — 1 passed
- the existing native-engine message and structured-clone regression groups
- release-documentation, documentation-depth, TUI-shortcut, and documentation-coverage validators
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
