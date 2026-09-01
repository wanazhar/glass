---
id: native-engine-066
scope: glass-browser/native-engine/flex-row-justification
status: complete
depends-on: [native-engine-065]
---

# Native bounded flex-row justification

## Objective

Add bounded `justify-content` distribution to the existing native
`display:flex` single-row layout. The slice must retain the fixed-width item,
margin, gap, and shared layout/paint/hit-test/scroll/capture owners established
by `native-engine-064` and `native-engine-065`.

## Contract

The native CSS grammar accepts the non-inherited `justify-content` values
`flex-start`, `center`, `flex-end`, and `space-between`. The computed default is
`flex-start`; stylesheet and inline declarations use the existing cascade and
inline-style precedence. The property is recognized for diagnostics, but
`normal`, `start`, `end`, `left`, `right`, `space-around`, `space-evenly`,
`stretch`, malformed values, and other alignment grammar remain unsupported.

Only an eligible block-level `display:flex` container from the earlier flex-row
slices consumes the value. Direct visible element items retain source order,
explicit/intrinsic fixed widths, physical margins, their existing y-origins,
and the `gap` minimum between adjacent rendered items. Whitespace-only direct
text remains ignored, while meaningful direct text, `display:contents`, and
visible `<br>` children retain the established normal-flow fallback.

The layout pass preflights each rendered item’s existing outer width and
horizontal margins. The occupied row width is the sum of those extents plus
the existing gaps. When the available content width has positive free space:

- `flex-start` keeps the leading offset at zero and leaves the configured gap
  unchanged;
- `center` places half the free space before the first item, using integer
  division and leaving an odd pixel at the trailing edge;
- `flex-end` places all free space before the first item;
- `space-between` keeps the leading offset at zero and adds free space evenly
  to the inter-item gaps; an odd remainder is assigned one pixel at a time to
  the earliest gaps in source order, and a one-item row has no distributed
  gap.

When the occupied row is wider than the available content width, negative free
space is not distributed: every value uses a zero leading offset and the
existing fixed-width overflow remains reachable through root horizontal
scrolling. No item is shrunk or moved to a negative coordinate.

The final item boxes, nested layout, text artifacts, display-list paint order,
ancestor clips, point hit testing, root overflow, scrolling, semantic
projection, and PNG capture consume the resulting coordinates. The property
has no effect in normal flow or in an ineligible flex container.

This is deterministic justification for one bounded forward row, not
Flexbox conformance. It does not add flex grow/shrink/basis/order, wrapping,
reverse or column direction, anonymous items, `align-items`/`align-content`,
`space-around`, `space-evenly`, nested scrolling, or browser parity.

## Tradeoffs

- Supporting the four common values makes fixed card/control rows useful while
  keeping the algorithm independent of fractional font or flex metrics.
- Integer free-space distribution is reproducible, but differs from browser
  subpixel rounding; the remainder policy is explicit and testable.
- Overflow uses a safe zero offset rather than negative coordinates, preserving
  the existing root-scroll and hit-test invariants at the cost of exact
  `safe`/`unsafe` alignment behavior.
- Preflighting fixed item geometry adds one bounded metadata pass, but avoids
  mutating layout and then trying to reposition already-recorded paint or hit
  artifacts.
- Recognizing unsupported alignment grammar for diagnostics keeps the audit
  truthful even though normal-flow and unsupported flex rows remain unchanged.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, issue, and public
  capability docs after implementation

## Verification

- CSS parsing and cascade accept only the four bounded values and preserve
  non-inheritance and inline precedence;
- `flex-start`, `center`, and `flex-end` place fixed items at deterministic
  leading offsets while preserving the configured gap and margins;
- `space-between` distributes free space across the existing gaps with the
  documented integer remainder rule;
- one-item, zero-item, and overflowing rows remain deterministic;
- meaningful direct text, `display:contents`, visible `<br>`, and normal-flow
  containers do not consume `justify-content`;
- layout boxes, nested descendants, text artifacts, display-list paint,
  clipping, hit testing, root overflow/scroll, semantics, and PNG capture
  share the distributed geometry;
- focused CSS/integration tests, the native suite, strict lint, formatting,
  whitespace, and documentation validators pass;
- implementation and documentation checkpoints are committed locally, issue
  #40 is updated, remote CI status is not claimed until this branch is pushed,
  and exact regenerable Cargo outputs are reclaimed after all validation.

## Completion evidence

- Design checkpoint: `a53b10f`.
- Implementation checkpoint: `3fe5306` (`feat(native-engine): add flex row justification`).
- Focused CSS parser/cascade tests: 2 passed, 0 failed.
- Focused justification integration tests: 2 passed, 0 failed.
- Unsupported alignment diagnostics integration test: 1 passed, 0 failed.
- Supported-declaration parser test: 1 passed, 0 failed.
- Full `native_engine` integration suite: 83 passed, 0 failed.
- Full `glass-browser` library suite: 859 passed, 0 failed, 1 ignored, using
  `RUST_MIN_STACK=4194304` after the default test-thread stack overflowed in
  the environment-sensitive readiness test.
- Strict feature-enabled Clippy passed in 6m10s; strict no-default-feature
  Clippy passed in 6m14s.
- Locked `glass-dev` build passed in 14m13s; all-feature rustdoc with
  `RUSTDOCFLAGS='-D warnings'` passed in 6m33s.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- Documentation validators pass after the current-source capability update.
- Remote CI: pending because this branch remains local-only; no remote-green
  claim is made.
- Exact regenerable Cargo outputs are reclaimed after the final validation
  command; no long-lived Glass process is terminated.
