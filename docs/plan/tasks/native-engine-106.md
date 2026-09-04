---
id: native-engine-106
scope: glass-browser/native-engine/text-decoration-combinations
status: ready
depends-on: [native-engine-105]
---

# Native bounded combined text-decoration lines

## Objective

Extend the existing inherited fixed-cell `text-decoration` owner from one
line keyword to a bounded combination of distinct line keywords in the
`text-decoration` shorthand. Preserve the current `none`, `underline`,
`overline`, and `line-through` behavior while allowing deterministic combined
line artifacts through the existing immutable display-list and software
raster paths.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-105.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts one or more distinct ASCII-whitespace-separated
line keywords in the inherited `text-decoration` shorthand:
`underline|overline|line-through`. `none` is valid only by itself. Matching is
case-insensitive; token order is irrelevant to the resulting line state;
duplicates, `none` combinations, unknown tokens, CSS-wide keywords, colors,
and other shorthand components remain typed unsupported-value diagnostics
without raw stylesheet echo.

The computed value is one bounded immutable three-bit line set. Its default is
the empty set (`none`), and an explicit child `none` clears every inherited
line. Each enabled line reaches the existing display-list command and fixed
cell raster owner:

- `overline` paints at `origin_y - 1`;
- `line-through` paints at `origin_y + 3`;
- `underline` paints at `origin_y + 7`.

Combined lines paint together with their existing fixed positions. The line
set does not alter text width, line formation, wrapping, alignment, overflow
geometry, hit testing, semantic/source order, accessibility projections,
scroll offsets, opacity grouping, capture dimensions, or clipping ownership.
The display command may carry multiple enabled line bits, but it remains one
immutable text artifact consumed by all downstream paths.

The slice remains restricted to the current integer-pixel, fixed-cell,
horizontal-tb implementation. `text-decoration-line` longhand semantics,
decoration propagation parity, colors, thickness, styles, offsets, blink,
font metrics, descender-aware placement, Unicode shaping, bidi, vertical
writing, and browser-wide text conformance remain outside the contract.

## Tradeoffs

- A compact line-set value keeps inheritance, `none` clearing, command
  projection, clipping, scrolling, opacity replay, capture, and raster on one
  owner while allowing the common combined shorthand forms.
- Rejecting duplicate tokens rather than silently deduplicating keeps malformed
  or surprising authored CSS visible through the existing diagnostic channel.
- Order-insensitive parsing gives deterministic computed state without adding
  authored token order to layout or paint artifacts.
- Fixed offsets remain deliberately simple and may be clipped or overlap
  glyph pixels under the existing fixed-cell limitations; font-aware metrics
  are deferred.
- No new crate, dependency, renderer, geometry owner, artifact schema, or
  default feature is introduced; the experiment remains inside
  `glass-browser`.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and plan docs

## Verification

The implementation gate will include:

- parser and declaration tests for all single keywords, every two-line and
  three-line combination, case-insensitive input, token-order invariance,
  duplicate rejection, `none`-combination rejection, and unsupported-value
  diagnostics;
- cascade/inheritance tests proving a combined parent, child `none` clearing,
  inline precedence, and invalid-value fallback;
- display-list assertions proving one text command carries exactly the
  expected combination bits;
- raster golden assertions for every combined line at its fixed position,
  clipping, scroll translation, and alpha replay;
- regressions proving combinations do not change text layout, wrapping,
  overflow, hit testing, or semantic/source order;
- `cargo fmt --all -- --check`, focused and full native integration tests,
  feature-enabled library tests with the documented explicit stack, strict
  all-feature and no-default-feature Clippy, warning-denied rustdoc, locked
  binaries, paired package/dependency gates, fuzz checking,
  documentation/release validators, fresh current-source documentation
  coverage, and exact isolated-target cleanup.

Remote CI remains pending because `main` is local-only. No browser-parity,
release, registry-publication, or remote-certification claim is part of this
task.
