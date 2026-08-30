---
id: native-engine-020
scope: glass-browser/native-engine/border-radius
status: done
depends-on: [native-engine-019]
---

# Native bounded border radius

## Objective

Extend the physical border and background paint path with a bounded, circular
corner-radius model:

- parse one-to-four non-negative integer-pixel values from the
  `border-radius` shorthand;
- cascade the physical top-left, top-right, bottom-right, and bottom-left
  radii with the existing specificity/source-order/inline rules;
- carry normalized corner data through layout metadata and immutable paint
  commands;
- clip backgrounds and solid/dashed/dotted border replay to a deterministic
  rounded outer box and inner border ring; and
- make point hit testing respect the same rounded outer geometry.

This is a bounded circular-radius slice, not general CSS border-radius. It does
not add percentage or elliptical radii, corner longhands, CSS-wide keywords,
rounded overflow clips for descendants, transforms, anti-aliasing, or browser
corner-join parity.

## Contract

`border-radius` accepts one, two, three, or four non-negative `<N>px` tokens.
The shorthand expands to physical corners using the CSS 1/2/3/4-value
mapping. Values are bounded by the existing native dimension limit and are
normalized to the concrete outer box so paired corners fit its width and
height. A slash-separated elliptical form, percentages, negative/non-pixel
values, empty values, and unsupported longhand properties are ignored without
replacing an earlier valid declaration.

`NativeLayoutBox` carries the computed physical radii. Its rectangle remains
the integer document-space outer border box, and hit testing rejects pixels in
the rounded-away corner regions before depth/order resolution. Content-box and
border-box dimensions, side-specific border insets, margins, root scrolling,
and revision behavior remain unchanged.

Fill and border display commands carry the same radius data. Software replay
uses integer pixel-center circle tests for the rounded outer shape and a
bounded inner rounded shape derived from the side border widths. Existing
side-pattern precedence, ancestor rectangular clips, source-over blending,
viewport clipping, and document-anchored dash/dot phase remain in force.
Rounded backgrounds and borders are read-only paint artifacts; descendant
commands under `overflow:hidden` continue to use rectangular ancestor clips
until a future clip-shape task owns that expansion.

No stable backend capability, screenshot evidence level, document revision
semantics, dependency, third crate, or automatic backend path changes.

## Tradeoffs

- Physical integer corners make cards and controls visibly rounded while
  keeping the renderer dependency-free and deterministic.
- Circular radii intentionally omit CSS's percentage/elliptical behavior and
  browser normalization details; normalization is conservative and integer
  bounded rather than subpixel.
- A shared rounded-mask primitive keeps fill, border, and hit testing aligned,
  but descendant overflow clips remain rectangular and therefore do not claim
  full CSS clipping semantics.
- Pixel-center inclusion has hard edges with no anti-aliasing, so visual output
  is predictable but not browser-fidelity output.
- The display-command and layout-box shapes grow by experimental fields; no
  stable transport field or backend capability is added.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- native-engine unit tests and synchronized capability documentation

## Verification

- `cargo fmt --all -- --check`
- focused CSS, layout, display-list, raster, scroll, and backend tests;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- documentation coverage/depth/release-truth validators;
- `git diff --check` and one focused Conventional Commit.

## Completion evidence

Implemented and verified locally. The focused and integration suites cover
one-to-four-value shorthand expansion, selector/inline cascade precedence,
unsupported radius rejection, concrete-box normalization, rounded fill and
border masks, rounded hit testing, rectangular ancestor clipping, and
document-space root-scroll replay.

- `cargo fmt --all -- --check` and `git diff --check` pass.
- Native unit tests: 31 passed.
- Native integration tests: 29 passed.
- Strict default-feature and `native-engine` Clippy gates pass.
- Full locked `glass-browser` all-target/all-feature matrix: 813 passed, 1
  ignored.
- Documentation coverage: 434 Markdown files, 345 full-product MCP tools,
  100 browser-only tools, 17 examples, and 22 public modules.
- Documentation depth: 93 current guides and 19 substantive contracts.
- Release-truth audit: 0 current-claim failures.
- Issue #40 records the exact local checkpoint commit and these verification
  results.
