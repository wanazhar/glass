---
id: native-engine-093
scope: glass-browser/native-engine/justify-content-stretch
status: active
depends-on: [native-engine-092]
---

# Native bounded justify-content stretch

## Objective

Accept explicit non-inherited `justify-content:stretch` in the bounded native
flex-row grammar while retaining a distinct computed keyword and routing its
used geometry through the already-proven `flex-start` owner. This closes the
remaining standard content-distribution alias after the completed explicit
`normal` boundary without changing omitted-value behavior or adding another
layout owner.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [CSS Box Alignment Level 3: flex-container stretch fallback](https://drafts.csswg.org/css-align/#content-distribution-flex)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts explicit non-inherited
`justify-content:stretch` alongside the completed bounded positional,
distributed, and `normal` values. Parsing remains case-insensitive and
accepts one token only. The computed value remains a distinct `Stretch`
keyword. Omitted `justify-content` continues to use Glass native's
established `FlexStart` fallback and does not start inheriting.

For an eligible fixed-width `row` or `row-reverse` flex line, after existing
visibility, source/order sorting, explicit gap, flex grow/shrink, bounded
width, and margin calculations have formed the item list, explicit `stretch`
uses the same bounded used placement as `flex-start`:

- no positive main-axis free-space offset is inserted before or between items;
- explicit gaps and item margins remain authoritative;
- row-reverse retains its existing physical reverse walk and mirrors no new
  distribution, without changing source/order identity;
- wrapped lines remain independently formed and use the same per-line path;
- empty and overflowing lines retain the established bounded behavior.

The existing shared one-token `place-content` parser consequently accepts
`place-content:stretch`, producing the distinct
`AlignContentValue::Stretch` cross-axis component and
`JustifyContentValue::Stretch` main-axis component. A two-token form such as
`place-content:center stretch` is valid through the same axis parser
composition; the cross-axis component continues to use its existing owner.

Cascade specificity, source order, inline precedence, non-inheritance, and
invalid-declaration retention remain unchanged. A valid explicit `stretch`
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

- Keeping `Stretch` distinct in computed style preserves specified-value
  observability while using the existing `FlexStart` used geometry, avoiding a
  second main-axis algorithm.
- Mapping `stretch` to flex-start only in the supported flex context captures
  the standards-defined flex fallback without pretending that the bounded
  native backend implements every layout mode. Other layout-mode semantics
  remain explicitly outside the contract.
- Reusing the reverse walk, margins, gaps, sizing, wrapping, and artifact
  consumers keeps the change small and deterministic, at the cost of not
  modeling the complete CSS alignment matrix.
- Expanding the shared one-token and two-token `place-content` forms keeps
  shorthand components consistent, but makes previously unsupported
  `place-content:stretch` declarations valid and therefore requires explicit
  cascade-regression coverage.
- No new dependency, crate, runtime, or artifact pipeline is added; the
  native backend remains default-off inside `glass-browser`, and Chromium/CDP
  remains the production runtime.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

Validation evidence will be appended after implementation. The required gate
set is focused parser/cascade and shared-layout coverage, the full native
integration and feature-enabled library suites, strict all-feature and
no-default-feature Clippy, warning-denied workspace rustdoc, locked
`glass-dev` binary compilation, static documentation/release validators, and
an exact isolated-target cleanup audit. Remote CI remains pending because the
branch is local-only.
