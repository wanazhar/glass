---
id: native-engine-111
scope: glass-browser/native-engine/text-underline-offset
status: complete
depends-on: [native-engine-110]
---

# Native bounded text-underline-offset

## Objective

Expose a bounded inherited signed-pixel `text-underline-offset` value for the
existing fixed-cell underline band. The value must travel through the current
immutable text command and adjust only the underline's raster y origin without
adding a second decoration geometry or layout path.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-110.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts `text-underline-offset` with one
case-insensitive signed fixed-pixel value from `-4px` through `4px`, inclusive.
The computed value is inherited through the existing fixed-cell text
decoration owner and defaults to `0px`. A declaration on a descendant replaces
the inherited offset for that descendant's emitted text runs; omission
preserves the inherited value. Existing specificity, stylesheet order, and
inline precedence remain unchanged. `-0px` computes to zero. An explicit
positive offset moves the underline toward increasing y; an explicit negative
offset moves it toward decreasing y.

Only underline bands use the offset. Overline and line-through retain their
existing line origins. The underline's existing line origin is translated by
the signed offset before the 110 thickness band is painted; thickness still
extends each row toward increasing y. The existing solid/dashed/dotted helper
and its 110 thickness-scaled periods remain the only horizontal pattern owner,
anchored at each immutable text command's x origin. The signed offset is
carried beside the existing text colors, style, thickness, and line flags in
`TextRun`.

The slice does not change text width, line formation, wrapping, alignment,
overflow geometry, line-height, baseline or font metrics, glyph origin,
thickness, overline or line-through geometry, decoration color/style, font,
shaping, bidi, writing mode, accessibility projections, hit testing, capture,
opacity grouping, or source/semantic order. Negative offsets may overlap glyph
rows or other decorations; that is an intentional fixed-cell pixel rule, not
typographic offset negotiation. The existing integer-pixel, fixed-cell,
horizontal-tb, fixture-first, default-off `native-engine` boundary remains in
force.

`auto`, percentages, non-pixel, fractional, out-of-range, positive-sign,
CSS-wide, unknown, empty, and other unsupported syntax remains bounded typed
diagnostics without raw stylesheet echo. The `text-decoration` shorthand is
not extended with underline-offset components by this slice.

## Tradeoffs

- A signed `-4px..=4px` bound exposes both directions while bounding raster
  work and avoiding arbitrary or fractional metrics. It omits `auto`,
  percentages, and font-derived behavior.
- Applying the offset only to underline follows the property's bounded intent
  and leaves overline/line-through consumers stable, but does not model a
  general decoration-origin or all-line offset system.
- Reusing the existing line origin, thickness loop, style helper, clipping,
  scroll translation, opacity, and capture keeps one geometry/raster owner;
  negative values can intentionally overlap glyph pixels in this experiment.
- Carrying the offset in `TextRun` preserves immutable per-fragment state and
  restart semantics, but does not provide CSS fragment continuity across
  multiple commands.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and plan docs

## Verification

Implementation commit: `215b02a8`. Current-claim documentation checkpoint:
`e76a732a`.

The parser/cascade unit tests pass for signed bounds, negative zero,
inheritance, explicit zero, inline precedence, omission, and invalid values.
The focused raster and diagnostics tests pass, and the native integration
fixture passes negative, zero, and positive underline placement, inherited and
inline override behavior, unchanged overline/line-through origins,
thickness/style composition, x-origin anchoring, immutable display-command
propagation, scroll translation, and unchanged layout geometry.

Local gate evidence from the isolated `/tmp/glass-111-target` includes:

- focused CSS units: 2 passed; clean compile `13:54.78`, peak RSS
  `2,475,600 KiB`;
- focused native integration: 1 passed; diagnostics: 1 passed; raster clamp:
  1 passed;
- full native integration: 148 passed in `4.03s`, peak RSS `83,200 KiB`;
- all-feature browser package tests: 907 passed, 1 expected ignored, native
  integration 148 passed, all other integration/doctest groups passed;
- `glass-dev`: 365 unit, 4 integration, and 15 PTY tests passed in `20:24.66`,
  peak RSS `1,995,964 KiB`;
- browser all-target/all-feature Clippy: `8:46.33`, peak RSS `1,899,228 KiB`;
  workspace Clippy: `10:18.86`, peak RSS `1,913,252 KiB`; no-default browser
  Clippy: `6:15.65`, peak RSS `1,801,936 KiB`;
- warning-denied workspace rustdoc: `3:07.41`, peak RSS `1,643,284 KiB`;
- locked browser package verification: 196 files, 984,968-byte archive,
  verified in `18:01.15`, peak RSS `2,186,932 KiB`; locked dev package:
  69 files, 524,453-byte archive, packaged in `2.40s` without re-verification;
- packaged dev dependency check resolves `glass-browser` exactly at `0.3.14`;
  both non-uploading `cargo publish --dry-run` checks pass and confirm the
  already-indexed 0.3.14 versions;
- offline fuzz all-target check: 290 targets in `9:57.46`, peak RSS
  `1,568,600 KiB`; `cargo deny check` passes with existing duplicate warnings;
  `cargo audit` exits 0 with the four existing allowed findings;
- static docs/feature/release/TUI/depth/coverage/reliability/adapter/Web IR
  validators all pass: 525 Markdown documents, 0 current-claim failures,
  14 capabilities across 4 targets, 15 TUI implementation keys, 93 guides,
  345 full-product MCP tools, 6 reliability scenarios, 5 adapters, and 8
  Web IR fixtures;
- explicit binary refreshes pass: `glass-dev` in `3.10s` and native-feature
  `glass-browser` in `2.07s`.

Remote CI, browser parity, release, registry publication, and a third crate
remain outside this local task. Remote CI is not claimed because this branch
has not been pushed.

## Cleanup

Cleanup is recorded after certification below. Do not remove shared Cargo
registries, toolchains, source, durable user data, or other projects'
non-regenerable artifacts.
