---
id: native-engine-092
scope: glass-browser/native-engine/justify-content-normal
status: complete
depends-on: [native-engine-091]
---

# Native bounded justify-content normal

## Objective

Accept explicit non-inherited `justify-content:normal` in the bounded native
flex-row grammar while retaining a distinct computed keyword and routing its
used geometry through the already-proven `flex-start` owner. This closes the
normal-value gap left after the completed positional and distributed
main-axis values without changing omitted-value behavior or adding another
layout owner.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [CSS Box Alignment Level 3: flex-container normal behavior](https://drafts.csswg.org/css-align/#content-distribution-flex)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts explicit non-inherited
`justify-content:normal` alongside the completed bounded positional and
distributed values. Parsing remains case-insensitive and accepts one token
only. The computed value remains a distinct `Normal` keyword. Omitted
`justify-content` continues to use Glass native’s established
`FlexStart` fallback and does not start inheriting.

For an eligible fixed-width `row` or `row-reverse` flex line, after existing
visibility, source/order sorting, explicit gap, flex grow/shrink, bounded
width, and margin calculations have formed the item list, explicit `normal`
uses the same bounded used placement as `flex-start`:

- no positive main-axis free-space offset is inserted before or between items;
- explicit gaps and item margins remain authoritative;
- row-reverse retains its existing physical reverse walk and mirrors no new
  distribution, without changing source/order identity;
- wrapped lines remain independently formed and use the same per-line path;
- empty and overflowing lines retain the established bounded behavior.

The existing shared one-token `place-content` parser consequently accepts
`place-content:normal`, producing the distinct `AlignContentValue::Normal`
cross-axis component and `JustifyContentValue::Normal` main-axis component.
The cross-axis component continues to use its existing wrapped-line normal
owner; this slice adds no shorthand or cross-axis geometry path.

Cascade specificity, source order, inline precedence, non-inheritance, and
invalid-declaration retention remain unchanged. A valid explicit `normal`
declaration wins at its existing cascade position; an invalid later
declaration does not erase an earlier valid winner. Unsupported CSS-wide,
logical, safe/unsafe, percentage, fractional, auto-margin, and other
unsupported forms remain typed diagnostics or out of scope.

Outside the bounded fixed-width row/row-reverse flex line, this slice does not
claim block, grid, absolute-positioned, column-direction, writing-mode,
negative-free-space, intrinsic-sizing, or browser-wide conformance semantics.
The resulting box and complete descendant artifact range must remain
consistent across layout, display-list paint, software rasterization,
viewport projection, hit testing, scrolling, capture, and semantic/source
order.

## Tradeoffs

- Keeping `Normal` distinct in computed style preserves specified-value
  observability while using the existing `FlexStart` used geometry, avoiding a
  second main-axis algorithm.
- Preserving the omitted-value `FlexStart` fallback avoids a broad default
  semantics change in this bounded engine; only an explicit `normal` value is
  newly observable.
- Reusing the reverse walk, margins, gaps, sizing, wrapping, and artifact
  consumers keeps the change small and deterministic, at the cost of not
  modeling other layout modes or the complete CSS alignment matrix.
- Accepting one-token `place-content:normal` follows the existing shared
  parser expansion; its cross-axis normal behavior remains owned by
  `align-content`.
- No new dependency, crate, runtime, or artifact pipeline is added; the
  native backend remains default-off inside `glass-browser`, and Chromium/CDP
  remains the production runtime.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

The design checkpoint is `4cbaf338`; the implementation checkpoint is
`ce19db39`; the documentation closeout is this follow-up checkpoint. Focused
CSS parser/cascade coverage passed 73/73 in 13m39.54s with 2,467,944 KiB peak
RSS. Focused shared-layout integration passed 1/1 in 5m57.92s with
2,108,756 KiB peak RSS. The full native integration suite passed 119/119 in
4.31s with 82,500 KiB peak RSS. The feature-enabled library suite passed 887
with 1 ignored and 0 failures in 7.01s with 82,360 KiB peak RSS when run with
the repository-established `RUST_MIN_STACK=8388608`; an unconfigured probe
reproduced the known existing large-Clap parser-test stack overflow and was
not treated as a code failure.

Strict all-feature Clippy passed with warnings denied in 8m36.24s with
1,879,584 KiB peak RSS. Strict no-default-feature Clippy passed with warnings
denied in 5m02.28s with 1,800,092 KiB peak RSS. Workspace rustdoc passed with
`RUSTDOCFLAGS="-D warnings"` in 7m04.33s with 1,635,152 KiB peak RSS. The
locked `glass-dev --bins` build passed in 11m25.61s with 1,989,968 KiB peak
RSS. The exact generated AArch64 ELF binaries were 140,088,688 bytes for
`glass` and 90,867,448 bytes for `glass-browser`.

Static validators passed at version `0.3.14`: feature parity 14 capabilities
across 4 targets; release documentation 506 Markdown documents, 83 current
documents, 57 previous-version hits, 566 semantic audit hits, and 0 current
claim failures; TUI 15 implementation keys and 63 documentation markers;
depth 93 guides and 19 contracts; live documentation coverage 506 Markdown
files, 345 full-product MCP tools (100 browser-only), 17 examples, and 22
public modules; reliability 6 scenarios across 4 targets; 5 public read-only
adapters; and Web IR 8 fixtures, 8 scenarios, and 11 categories. Formatting
and `git diff --check` passed.

The exact `/tmp/glass-092-target` tree reached 5.0G. After all commands
exited, an exact process and open-file audit passed and the tree was removed
with bounded same-filesystem deletion. Only the 4.0K project `target/`
placeholder remains; the separate regenerable `fuzz/target` tree is absent.
Shared registries, toolchains, source, and long-lived Glass processes were
retained. Remote CI remains pending because the branch is local-only.
