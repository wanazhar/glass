---
id: native-engine-063
scope: glass-browser/native-engine/vertical-align
status: complete
depends-on: [native-engine-062]
---

# Native bounded vertical alignment

## Objective

Add a bounded inherited `vertical-align` choice to the existing fixed-cell
inline-flow owner. Explicit fixture inline and inline-block items should be
able to move within their current line box while retaining the existing line
break, width, semantic, paint-order, and hit-test contracts.

## Contract

The native CSS grammar accepts only `baseline`, `top`, `middle`, and `bottom`
for `vertical-align`. The property is inherited through the existing DOM style
walk and has `Baseline` as its initial value. Stylesheet and inline values use
the existing cascade. `text-top`, `text-bottom`, `sub`, `super`, lengths,
percentages, CSS-wide keywords, malformed values, and other alignment syntax
are diagnosed as unsupported and do not replace the inherited value.

The bounded property affects only a rendered inline or inline-block item that
is recorded in a parent's normal-flow line. Block items and direct text in a
block flow keep the established placement. `Baseline` preserves the current
top-origin behavior because this engine does not claim font metrics or a
typographic baseline. For a line whose top is `y` and whose bounded line-box
height is `H`, an item with a margin-box height `h` is shifted as one artifact
range by:

- `baseline`: `0` pixels;
- `top`: `0` pixels;
- `middle`: `floor((H - h) / 2)` pixels; and
- `bottom`: `H - h` pixels.

The line-box height is the existing maximum of the parent minimum line-height
and placed item heights. Offsets are clamped at zero when an item is at least
as tall as the line. The containing inline item's layout boxes, direct text
runs, and paint-owned descendants move together during the existing line
flush; horizontal origins, widths, line breaks, wrapping, fragment text,
semantic source text, hit-test ownership, display-list order, clipping,
scrolling, and capture remain on their current owners.

This is a deterministic fixed-cell line-item rule, not CSS inline formatting
or baseline parity. It does not implement font ascent/descent metrics,
baseline alignment between different fonts, `text-top`/`text-bottom`, subscript
or superscript, percentage/length offsets, logical writing modes, direction or
bidi, ruby, table-cell alignment, replaced-element baselines, line box struts,
or browser conformance. Translated artifacts remain bounded by the same line
and root limits; no second layout tree or renderer dependency is introduced.

## Tradeoffs

- Reusing the existing line-item artifact ranges makes inline boxes and their
  text move together, preserving paint and hit-test ownership, but it cannot
  model real font baselines or mixed-font ascent/descent behavior.
- Treating the current top origin as the `baseline` value keeps existing
  fixtures byte- and geometry-stable, but the name is intentionally a bounded
  compatibility label rather than a typographic claim.
- Supporting only four keywords keeps the offset arithmetic deterministic and
  cheap, but common length/percentage and inline-formatting variants remain
  visibly unsupported.
- Keeping line height and horizontal measurement unchanged avoids reflow and
  root-overflow changes, but a child artifact that intentionally exceeds its
  item box remains governed by the existing bounded overflow behavior.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and public
  capability docs after implementation

## Verification

- parser, initial value, stylesheet/inline cascade, inheritance, child
  override, and invalid-value fallback are covered by unit tests;
- baseline preserves existing coordinates, while top/middle/bottom move
  inline/inline-block boxes and all contained text/display artifacts by the
  documented fixed offsets;
- line height, width, wrapping, source-order paint, overflow clips, hit-test
  ownership, scrolling, capture, and semantic text remain consistent;
- block items and direct block-flow text remain on the baseline path, and
  unsupported keyword/length/percentage forms remain diagnosed;
- focused integration and CSS unit tests, the full native integration suite,
  the native browser library suite, strict native/default lint, the `glass-dev`
  build, rustdoc, formatting, whitespace, and documentation validators pass;
- the implementation and documentation checkpoints are committed locally,
  issue #40 is updated with the evidence, and exact regenerable Cargo outputs
  are reclaimed after validation.

## Completion evidence

The design checkpoint is `7721df2`; implementation is `facd2f6`. The focused
vertical-alignment integration test passed (1/1), the focused CSS unit tests
passed (2/2), and the full native integration suite passed (78/78). With
`RUST_MIN_STACK=4194304`, the native `glass-browser` library suite passed
(855 passed, 1 ignored, 0 failed). The raw unbounded `--nocapture` form hit
the pre-existing `cli::args::tests::agent_readiness_commands_are_explicit`
stack overflow, so the repository's bounded stack setting is required for
that full suite.

Native strict validation passed: feature-enabled clippy (all targets, 8m40s),
default/no-feature clippy (all targets, 4m37s), `glass-dev` locked build
(13m09s), and warnings-denied rustdoc (6m22s). Formatting, whitespace,
release-documentation, documentation-depth, feature/version parity, coverage,
TUI shortcut, reliability-matrix, read-only-adapter, and Web IR validators
also passed. The implementation checkpoint is committed locally before this
documentation closeout; issue #40 records the final local hashes and remote CI
remains pending until the branch is pushed.
