---
id: native-engine-082
scope: glass-browser/native-engine/flex-shorthand
status: active
depends-on: [native-engine-081]
---

# Native bounded flex shorthand expansion

## Objective

Complete the bounded row-flex sizing input by expanding a small, explicit
`flex` shorthand grammar into the already-shipped `flex-grow`, `flex-shrink`,
and `flex-basis` fields. The slice must preserve one computed-style and layout
owner, declaration-order behavior, deterministic integer sizing, and the
existing visual/interaction consumers.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts these bounded shorthand forms:

- `none`, expanding to `0 0 auto`;
- `auto`, expanding to `1 1 auto`;
- one non-negative integer `N`, expanding to `N 1 0px`;
- two tokens `N M`, expanding to `N M 0px` when both are bounded integer
  factors, or to `N 1 B` when the second token is `auto` or a bounded pixel
  basis;
- three tokens `N M B`, where `N` and `M` are bounded integer factors and `B`
  is `auto` or a bounded non-negative integer pixel length.

Grow and shrink factors are bounded to `0..=1024`; pixel bases use the
existing `0px..=MAX_NATIVE_VIEWPORT_DIMENSION` bound. The property is
non-inherited. `initial`, `inherit`, `unset`, `revert`, percentages,
fractional factors, unitless bases, negative values, `calc()`, `content`, and
other token shapes remain unsupported and produce bounded diagnostics.

The shorthand expands at the declaration's existing specificity, source
order, and inline precedence. Within one declaration block, a valid shorthand
sets all three components, a later valid longhand replaces only its component,
and a later valid shorthand replaces all three. Invalid shorthand or longhand
values do not erase an earlier valid component. Across rules, each expanded
component uses the existing per-property cascade winner; inline declarations
remain strongest.

The expanded values feed the completed flex-basis base-size, flex-grow
free-space, and flex-shrink deficit owners. The final item widths continue
through existing wrapping, justification, descendants, content rectangles,
display-list, raster, overflow, viewport projection, hit testing, scrolling,
capture, and semantic/source-order consumers. The shorthand does not change
semantic order or make native-engine selection implicit.

## Tradeoffs

- A deliberately small grammar makes common fixed-row shorthand useful while
  keeping diagnostics and compile cost bounded; full CSS token ambiguity,
  `flex: 1 1 0%` percentage semantics, and browser conformance remain outside
  the claim.
- The one-number form maps its implicit zero basis to `0px`, retaining the
  integer coordinate model rather than introducing a percentage-valued
  intermediate. This is deterministic but not a claim of exact browser used
  values.
- Parsing expands directly into the existing component fields, so later
  longhands in the same block can override individual components without a
  second cascade representation. The tradeoff is that unsupported importance
  or CSS-wide token semantics remain fail-closed rather than partially modeled.
- `none` and `auto` are explicit presets, while `initial` remains unsupported
  under the native engine's existing CSS-wide-value policy. This keeps the
  reset surface consistent with the rest of the bounded parser.
- Since layout already consumes the three computed components, this slice
  adds no new geometry algorithm. It inherits the prior slices' limitations
  around intrinsic sizing, fractional metrics, columns, auto margins, and
  multiple independent flex contexts.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

The implementation, focused tests, full native suites, strict feature and
documentation gates, issue #40 status, and exact regenerable-target cleanup
will be recorded here when the slice closes. Remote CI is not claimed until
the local branch is pushed.
