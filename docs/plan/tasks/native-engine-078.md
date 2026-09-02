---
id: native-engine-078
scope: glass-browser/native-engine/flex-gap-family
status: complete
depends-on: [native-engine-077]
---

# Native engine 078: bounded flex gap family

## Objective

Complete the bounded gap family for the already-owned fixed-width flex-row
layout path. The slice adds the CSS one- or two-value `gap` form and the
`column-gap` longhand, then resolves `gap`, `row-gap`, and `column-gap` with
declaration-order-aware cascade metadata so one shorthand does not silently
override a later longhand (or vice versa). It remains inside `glass-browser`,
behind the default-off `native-engine` feature, and preserves the two-crate
workspace boundary.

## Context

Read `docs/INDEX.md`, `docs/architecture/native-engine.md`,
`docs/plan/README.md`, and `docs/plan/analysis/native-engine.md` before
implementation. This slice composes with the completed
`native-engine-064` through `native-engine-077` flex-row, main-axis spacing,
justification, visual order, item alignment, direction, wrapping, cross-line
distribution, wrap-reverse, stretch, normal, and explicit row-gap owners.

## Contract

The bounded native CSS grammar accepts these non-inherited integer-pixel
values in the existing `0..=MAX_NATIVE_VIEWPORT_DIMENSION` range:

- `gap: <length>` resolves the same value to both row and column axes;
- `gap: <row-length> <column-length>` resolves the first value to
  cross-line `row-gap` and the second to main-axis `column-gap`;
- `row-gap: <length>` and `column-gap: <length>` resolve one axis each.

Only one or two whitespace-separated non-negative integer `px` values are
accepted. Fractions, percentages, negative values, additional tokens, CSS
wide keywords, and other units remain invalid and produce the existing
bounded diagnostic/fallback behavior.

For each eligible block-level `display:flex` container using row direction and
`nowrap`, `wrap`, or `wrap-reverse`:

- the resolved `column-gap` is the minimum main-axis spacing between visible
  direct element items; line formation, `justify-content`, row reverse, item
  margins, overflow, and stable visual `(order, source_index)` sorting consume
  that same value;
- the resolved `row-gap` is inserted between adjacent formed lines before
  `align-content` computes positive explicit cross-axis free space; the
  occupied line size and every completed cross-line distribution mode consume
  it exactly once;
- one-value `gap` now intentionally supplies both axes, correcting the
  previous 077 compatibility boundary where `gap` supplied only the main
  axis. Existing fixtures that want cross-line spacing can use `row-gap` or a
  two-value shorthand explicitly;
- `row-gap` is a no-op for `nowrap` and for a single formed line, while
  `column-gap` remains useful for both wrapped and unwrapped eligible rows;
- valid declarations participate in the same bounded cascade as other CSS:
  inline declarations outrank stylesheet declarations, specificity outranks
  stylesheet order, and within a matching rule the later valid declaration
  wins. A shorthand writes both axes at its own declaration position; an
  axis longhand only replaces that axis when its own cascade position wins.
  Invalid declarations do not erase an earlier valid declaration;
- gap properties do not inherit. Descendants without their own declaration
  retain zero gap values, and normal-flow fallback containers do not apply
  flex spacing merely because their computed style contains a gap value;
- the final document-space boxes, descendant/text ranges, display-list
  commands, software replay, viewport projection, root overflow, scrolling,
  capture, point hit testing, and action effects all consume the final shared
  coordinates. Semantic traversal, source text, node references, and keyboard
  order remain document order.

## Explicit exclusions

This slice does not add percentage, fractional, `calc()`, CSS-wide, or
negative gap values; `column-gap` for column-direction flex; grid; flex
growth/shrink/basis; auto margins; intrinsic or percentage sizing; logical
direction/RTL mapping; nested scrolling; positioned or stacking layout;
JavaScript; network access; or browser Flexbox parity. It does not add a
public backend capability, change the default Chromium/CDP runtime, or imply
that the native engine is safe for arbitrary remote content.

## Tradeoffs and risks

Changing one-value `gap` to its CSS two-axis meaning changes the cross-line
geometry of existing wrapped fixtures that used `gap` without `row-gap`.
That is an intentional compatibility correction, so affected goldens must be
updated in the same implementation checkpoint rather than preserving a
misleading legacy behavior. The parser still accepts only fixed integer
pixels, which keeps deterministic arithmetic and fast tests but misses real
CSS length resolution, fractional distribution, and responsive layouts.

The existing declaration model stores one parsed value per property. Gap
shorthand/longhand precedence requires preserving each valid declaration's
position within its rule, while retaining the existing specificity, rule
order, and inline precedence across rules. A small gap-specific cascade record
is preferable to changing every existing property because it limits risk to
the interacting gap family and makes the source-order contract executable.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`: add typed bounded
  two-axis gap parsing, `column-gap` recognition/diagnostics, per-rule valid
  declaration positions, gap-family cascade resolution, and non-inheritance
  unit coverage;
- `crates/glass-browser/src/browser/native_engine/layout.rs`: use the
  resolved column gap for main-axis item spacing and line formation while
  retaining the existing resolved row-gap cross-line owner;
- `crates/glass-browser/tests/native_engine.rs`: add parser/cascade,
  shorthand/longhand source-order, invalid fallback, non-inheritance,
  one-line/nowrap no-op, wrapped two-axis geometry, wrap-reverse,
  align-content, overflow, paint, hit-test, scrolling, and semantic-order
  coverage; update intentional wrapped-row goldens;
- architecture/analysis/plan records and issue #40 after implementation
  evidence is complete.

## Verification

- [x] CSS unit tests cover one- and two-value shorthand, longhands, bounds,
  invalid fallback, source-order cascade, selector specificity, inline
  precedence, and non-inheritance;
- [x] integration tests cover main-axis placement/line formation, all
  completed cross-line alignment values, wrap-reverse, nowrap/single-line
  fallback, overflow/scrolling, paint, hit testing, and source/semantic order;
- [x] full native integration/library, strict feature matrices, formatting,
  rustdoc, and repository documentation validators pass;
- [x] implementation, documentation, and issue #40 checkpoints are committed
  locally; remote CI is not claimed until this branch is pushed;
- [x] exact regenerable Cargo outputs are reclaimed after validation without
  terminating long-lived Glass processes.

## Completion evidence

Completed locally on 2026-09-02. The design checkpoint is `c6ebecd` and the
implementation checkpoint is `1bca33f`. The implementation stays inside
`glass-browser`; `native-engine` remains default-off and the workspace still
has exactly two installable crates.

Focused and full native validation passed:

```text
cargo test -p glass-browser --features native-engine --test native_engine native_flex --locked -- --nocapture
23 passed; 0 failed; 0 ignored

cargo test -p glass-browser --features native-engine --lib gap --locked -- --nocapture
6 passed; 0 failed; 0 ignored

cargo test -p glass-browser --features native-engine --test native_engine --locked -- --nocapture
101 passed; 0 failed; 0 ignored

RUST_MIN_STACK=8388608 cargo test -p glass-browser --features native-engine --lib --locked -- --nocapture
873 passed; 0 failed; 1 ignored

cargo clippy --all-targets --all-features --locked -- -D warnings
cargo clippy --all-targets --no-default-features --locked -- -D warnings
both passed

env RUSTDOCFLAGS='-D warnings' cargo doc --all-features --locked --no-deps
cargo build -p glass-dev --bin glass --locked
both passed
```

The final focused flex run completed in `21.11s` wall time with a `0.70s`
test phase and peak RSS of `608,300 KiB`. The focused CSS gap run completed in
`7m21.69s` with peak RSS `2,486,660 KiB`; the full native integration completed
in `1m25.49s` with peak RSS `1,476,560 KiB`; and the full native library
completed in `1m36.61s` with peak RSS `1,687,464 KiB`. Strict all-feature
Clippy passed in `13m47.64s` with peak RSS `1,879,380 KiB`; no-default-feature
Clippy passed in `7m31.70s` with peak RSS `1,792,468 KiB`; rustdoc passed in
`3m27.58s` with peak RSS `1,631,772 KiB`; and the locked `glass-dev` binary
build passed in `11m24.52s` with peak RSS `2,000,744 KiB`. Formatting and
`git diff --check` passed.

Repository validators also passed after the 078 docs update: version sync at
`0.3.14`; feature parity across 14 capabilities and 4 targets;
release-documentation audit over 492 Markdown documents with 0 current-claim
failures; TUI shortcut inventory with 15 implementation keys and 63
documentation markers; documentation depth with 93 current guides and 19
substantive contracts; documentation coverage with 492 Markdown files, 345
full-product MCP tools (100 browser-only), 17 examples, and 22 public
modules; reliability matrix with 6 scenarios across 4 targets; 5 public
read-only adapters; and the Web IR corpus with 8 fixtures, 8 scenarios, and 11
categories with runtime goldens verified. Runtime certification remains
explicitly unclaimed.

The complete integration and library runs verify that one-value `gap` now
feeds both axes, two-value shorthand uses row/column order, longhands obey
declaration order and specificity/inline precedence, and all prior row,
wrap, alignment, reverse, overflow, paint, hit-test, scroll, capture, and
semantic/source-order consumers use the same final coordinates. Remote CI
remains pending because the checkout is local-only; no push, release, tag, or
registry publication is claimed.
