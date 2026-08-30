---
id: native-engine-015
scope: glass-browser/native-engine/png-capture
status: done
depends-on: [native-engine-014]
---

# Native bounded PNG capture

## Objective

Expose the current native renderer artifact through the existing backend
capture contract without widening the engine into a general screenshot
implementation:

- encode the bounded logical RGBA surface as deterministic PNG bytes;
- enforce the existing bounded capture payload limit;
- make explicit `CaptureFormat::Png` available through the native backend;
- reject JPEG and PDF capture explicitly; and
- preserve the rule that screenshot-containing evidence levels remain denied
  until the stable evidence schema can carry the required visual payload.

This slice does not add image decoding, fonts, physical-pixel/device-scale
rendering, viewport/element/full-page capture modes, JPEG/PDF encoders,
screenshots through the native CLI, or new dependencies.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/browser-host-rfc.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-014.md`
- `crates/glass-browser/src/browser_backend.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

`NativeSurface::to_png` encodes the existing logical RGBA pixels with the
configured surface width and height, using the already-present `png` crate.
The encoded payload is validated against `MAX_CAPTURE_BYTES`; exceeding the
limit returns a typed native limit error and does not mutate the document or
revision.

The native engine exposes `capture_png` only while running. The backend
profile declares `BrowserCapability::Capture` available with a bounded PNG
limitation. Its dispatcher accepts `CaptureFormat::Png` and returns the
current surface as `CaptureResult`; `Jpeg` and `Pdf` return an explicit
unsupported-operation error. Capture is read-only and does not advance the
document revision.

`EvidenceLevel::Screenshot` and `EvidenceLevel::Combined` remain explicitly
denied because `EvidenceResult` has no image field and the native CLI does not
gain a screenshot command in this slice. No operation falls back to CDP or
another renderer.

## Tradeoffs

- Reusing the existing PNG dependency keeps the Cargo graph unchanged and
  avoids an encoder implementation, but the native path inherits that
  encoder's output behavior rather than defining a new byte-level codec.
- Logical-surface capture is deterministic and bounded, but it is not a
  physical-device screenshot: device scale, font fidelity, image decoding,
  viewport clipping modes, and browser compositing remain absent.
- The stable capture contract is exposed before screenshot evidence levels so
  callers can request explicit PNG bytes without silently changing compact or
  deep observation semantics.

## Path

- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- native-engine design/plan/public capability documentation

## Verification

- `cargo fmt --all -- --check`
- `cargo test -p glass-browser --features native-engine --test native_engine --locked`
- native raster unit tests decode the generated PNG and verify dimensions,
  pixels, determinism, read-only revision behavior, and format denials;
- `cargo check -p glass-browser --no-default-features --locked`
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` test matrix;
- documentation coverage/depth/release-truth validators;
- `git diff --check` and a clean focused Conventional Commit.

## Completion evidence

Completed locally in the current checkpoint. The real dispatcher capture path
returns decodable PNG bytes, preserves the document revision, and denies
JPEG/PDF explicitly. Native integration tests (23), native unit tests (23),
the locked all-targets/all-features matrix (805 passed, 1 ignored), default
feature checks/Clippy, formatting, and documentation gates are required
evidence before the final commit and issue update.
