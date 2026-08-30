---
id: native-engine-017
scope: glass-browser/native-engine/viewport-scrolling
status: done
depends-on: [native-engine-016]
---

# Native bounded viewport scrolling

## Objective

Make the existing native layout, hit-test, paint, raster, and capture paths
consume one explicit, bounded vertical viewport scroll state:

- derive document content height and the maximum vertical scroll offset;
- accept the existing `SemanticAction::Scroll` contract for vertical deltas;
- clamp scrolling to the document's viewport bounds and expose the resulting
  offset in the Rust layout/display artifacts;
- map viewport points back into document coordinates for hit testing;
- translate display-list geometry during logical software replay and PNG
  capture; and
- revision and effect-report accepted scroll changes without changing the
  stable evidence schema.

This is scrolling for the single native viewport, not general CSS overflow or
browser scrolling. It does not add horizontal, nested, smooth, keyboard,
scroll anchoring, scroll snapping, or platform-window behavior.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-016.md`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

`NativeEngine` owns one unsigned viewport scroll offset. Successful navigation
resets it to the origin. The bounded layout snapshot retains document-space
outer/content rectangles and reports content height, maximum scroll, and the
current offset. A viewport point is translated by the current offset before
deepest-box hit testing; no action implicitly scrolls a target into view.

The native backend accepts `SemanticAction::Scroll` only when `delta_x` is
zero. A non-zero vertical delta clamps to `[0, max_scroll_y]`. A delta that
does not move the viewport is an accepted no-op result with `accepted: false`
and no revision/effect change; a moved viewport advances the document revision
once and records bounded internal scroll effect metadata. Horizontal scrolling
is an explicit typed denial.

Display-list commands and clips remain in document coordinates. The list carries
the matching offset, and software replay translates rectangles/points before
viewport clipping. PNG capture therefore reflects the current logical viewport
after scrolling, while screenshot-containing evidence remains unavailable.

## Tradeoffs

- Keeping layout boxes in document coordinates avoids signed public rectangles
  and preserves the 016 outer/content geometry contract, but callers must use
  the snapshot's viewport projection when they need on-screen coordinates.
- Translating during replay keeps the display-list derivation immutable and
  makes capture read-only, but adds one bounded transform to every raster
  primitive.
- Vertical root scrolling provides useful interaction and capture coverage
  without pretending to implement CSS scroll containers; horizontal and nested
  scroll behavior remain future work.
- Revisioning viewport movement makes stale references and effects observable,
  but scroll-only work invalidates revision-bound references just like other
  accepted native actions.
- No dependency, stable evidence field, or third crate is introduced.

## Path

- `crates/glass-browser/src/browser/native_engine/interaction.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- native-engine integration/unit tests and capability documentation

## Verification

- `cargo fmt --all -- --check`
- focused native integration and layout/raster unit tests;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` test matrix;
- documentation coverage/depth/release-truth validators;
- `git diff --check` and a clean focused Conventional Commit.

## Completion evidence

Verified locally before checkpointing: 24 native unit tests and 26 native
integration tests cover vertical dispatch, clamping/no-op behavior,
document/viewport coordinate mapping, post-scroll raster/capture output,
revision/effect semantics, and direct semantic actions after an explicit
scroll. Strict native and default-feature lint gates, the locked all-target
all-feature matrix (806 passed, 1 ignored), and documentation coverage,
depth, and release-truth validators pass. Issue #40 records the exact local
checkpoint; remote CI remains unrun until the commit is pushed.
