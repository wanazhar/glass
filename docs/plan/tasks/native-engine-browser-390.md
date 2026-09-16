# Native OffscreenCanvas transfer (390)

```yaml
id: native-engine-browser-390
scope: native-engine/offscreencanvas-transfer
status: done
depends-on:
  - native-engine-browser-389
```

## Objective

Make the existing native canvas surface transferable as an
`OffscreenCanvas`. A DOM canvas transferred with
`transferControlToOffscreen()` must be usable by a worker, and the returned
surface must remain a real pixel-backed canvas rather than a JSON object.

## Contract

- `MessagePort.postMessage()` and `structuredClone(value, { transfer })` accept
  native OffscreenCanvas members and preserve dimensions, pixels, and the
  origin-clean bit.
- Both DOM-controlled offscreen surfaces and standalone OffscreenCanvas
  instances use the same bounded RGBA descriptor and existing message/canvas
  limits.
- A receiving realm gets a fresh OffscreenCanvas object with an independent
  ownership marker. A source is detached only after successful clone
  admission; a transferred DOM placeholder remains control-detached.
- The worker receiver exposes bounded 2D operations needed to paint and read
  the transferred surface. Page/worker routes retain the existing JSON-framed
  transport and do not introduce a CDP dependency.
- Detached sources and OffscreenCanvas values omitted from the transfer list
  fail with `DataCloneError` or `InvalidStateError` at the corresponding
  Web-IDL boundary.

## Delivered behavior

- Added page-owned OffscreenCanvas transfer descriptor, source-detach, and
  receiver-factory hooks. Linked DOM surfaces and standalone surfaces are
  both supported.
- Extended the generic native transfer-value graph to carry validated
  OffscreenCanvas pixel descriptors alongside ImageBitmap values, with
  receiver-side validation before object construction.
- Added a worker-realm OffscreenCanvas surface and bounded 2D context for
  fill/clear, image-data read/write, and image-source drawing, allowing the
  worker to modify and return the same transferred resource.
- Added a deterministic page-to-worker-to-page witness that checks identity,
  worker paint bytes, page redraw bytes, source detachment, and placeholder
  invalidation.

## Tradeoffs and explicit follow-up

The current JSON-framed bridge copies RGBA pixels and therefore prioritizes
ownership safety and deterministic cross-realm behavior over zero-copy GPU
sharing. The existing 256 KiB message budget limits practical transferred
surface size until a bounded binary transport exists. Worker 2D parity is
currently the transfer-critical bounded subset; full worker canvas/Web IDL
parity, other transferable platform objects, task-source ordering, broader
Core Web Profile parity, and final production certification remain issue #40
gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/README.md`
- `docs/plan/tasks/native-engine-browser-390.md`

## Verification

Validation completed locally against this checkpoint:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_offscreen_canvas_transfer_preserves_worker_raster_and_detaches_source --locked` — 1 passed
- native-engine message and structured-clone regression groups remain green
- release-documentation, documentation-depth, TUI-shortcut, and documentation-coverage validators
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
