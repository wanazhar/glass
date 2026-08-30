---
id: native-engine-019
scope: glass-browser/native-engine/non-solid-borders
status: done
depends-on: [native-engine-018]
---

# Native bounded dashed and dotted borders

## Objective

Extend the existing physical side-specific border path with deterministic
integer-pixel `dashed` and `dotted` paint styles:

- preserve `border: Npx solid <color>` and all four physical side declarations;
- accept `Npx dashed <color>` and `Npx dotted <color>` in the same bounded
  declaration grammar;
- carry the style through computed CSS, the immutable display list, and the
  logical software surface; and
- define a bounded, repeatable side pattern that remains correct after clips
  and root viewport scrolling.

This is a renderer-pattern slice, not general CSS border support. It does not
add `double`, `groove`, `ridge`, `inset`, `outset`, border radius, border
images, gradients, standalone `border-style` properties, logical writing-mode
sides, anti-aliased joins, or browser-parity dash metrics.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-018.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`

## Contract

The supported border value grammar is `<non-negative bounded Npx width>` plus
one of `solid`, `dashed`, or `dotted` plus a supported opaque or transparent
color. Style names are ASCII case-insensitive. A valid shorthand still sets
all four physical sides, and a valid physical side declaration still
overrides only that side according to the existing cascade rules. Invalid,
negative, non-pixel, unbounded, unsupported-style, or unsupported-color
declarations are ignored without replacing an earlier valid declaration in
the same block.

The display-list border payload carries the style for each physical side.
Software replay keeps the existing top/right/bottom/left corner precedence,
ancestor clipping, source-over blending, and root-scroll translation. `solid`
paints every pixel in its side region. `dashed` uses a deterministic bounded
integer dash/gap period derived from the side width. `dotted` uses a
deterministic width-sized dot/gap period along the side axis. Pattern phase is
anchored to the outer box's document-space top/left origin, so scrolling does
not change the pattern. A side pattern gap remains empty even when its
corner-overlap region could have been painted by a lower-precedence side.

No stable backend capability, screenshot evidence level, or document revision
semantics change. Layout insets depend only on side widths, as in 018.

## Tradeoffs

- Dashed/dotted patterns cover common static fixtures while avoiding a hidden
  dependency on a full CSS painting engine.
- Integer periods are deterministic and bounded, but they intentionally do not
  match browser dash distribution, corner phase, anti-aliasing, or subpixel
  stroke behavior.
- The side style is typed end-to-end, which makes unsupported styles explicit,
  but changes the experimental public `NativeBorderPaint` shape.
- No new dependency, third crate, runtime fallback, or automatic backend path
  is introduced.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- native-engine unit tests and synchronized capability documentation

## Verification

- `cargo fmt --all -- --check`
- focused CSS, paint, raster, scroll, and backend tests;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- documentation coverage/depth/release-truth validators;
- `git diff --check` and one focused Conventional Commit.

## Completion evidence

Implemented and verified locally. The focused and integration suites cover
case-insensitive `solid`/`dashed`/`dotted` parsing, physical-side cascade,
unsupported-style rejection, deterministic integer dash/dot patterns,
corner precedence, ancestor clipping, document-anchored root-scroll replay,
and the default-feature boundary.

- `cargo fmt --all -- --check` and `git diff --check` pass.
- Native unit tests: 27 passed.
- Native integration tests: 28 passed.
- Strict default-feature and `native-engine` Clippy gates pass.
- Full locked `glass-browser` all-target/all-feature matrix: 809 passed, 1
  ignored.
- Documentation coverage, depth, and release-truth validators pass with zero
  current-claim failures; the exact post-task totals are recorded on issue
  #40.
- Issue #40 records the exact local checkpoint commit and these verification
  results.
