---
id: native-engine-080
scope: glass-browser/native-engine/flex-shrink
status: active
depends-on: [native-engine-079]
---

# Native bounded flex-shrink allocation

## Objective

Extend the existing bounded row-flex geometry owner with deterministic
negative-free-space allocation. The slice makes common fixed-width rows honor
the CSS `flex-shrink` contract without adding a second layout engine,
fractional metrics, or a new workspace crate/dependency.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts `flex-shrink` as a non-negative decimal integer
from `0` through `1024`. The property is non-inherited, defaults to `1`, and
uses the existing selector specificity, source order, and inline precedence.
Invalid, negative, fractional, unit-bearing, CSS-wide, and out-of-range values
remain unsupported and do not replace an earlier valid declaration. A factor
of `0` is the explicit opt-out from shrinking.

For an eligible block-level `display:flex` row or `row-reverse` container, the
existing computed outer width after padding, borders, min/max constraints, and
margins remains each item's flex base width. Wrapping and line formation still
use those base widths and the resolved column gap. After a line is formed, if
its base items, margins, and gaps exceed the available width, only item widths
are reduced; margins and gaps are not reduced or redistributed.

Each positive shrink factor receives a weight equal to its integer factor times
its original flex base width. The deficit is allocated with bounded `u64`
intermediates and deterministic prefix-floor integer shares in visual/source
order. An item cannot shrink below its effective outer `min-width`; reaching
that floor freezes the item and redistributes its remaining deficit among the
remaining positive weighted items. If all eligible items reach their minimum,
or all factors are zero, the line remains intentionally overflowing and the
existing reverse/scroll/viewport behavior handles it.

Shrink occurs before `justify-content`, after base-size line formation, and is
mutually exclusive with the 079 positive-growth pass. If the line has no
negative free space, the prior growth/justification behavior remains. The final
width is passed through the existing descendants, content rectangles,
display-list, raster, overflow, viewport projection, hit testing, scrolling,
capture, and semantic/source-order consumers.

This slice explicitly excludes `flex-basis`, the `flex` shorthand, fractional
shrink factors, reflow based on shrunk widths, auto margins, column directions,
percentage/intrinsic sizing changes, multiple independent flex formatting
contexts, and browser Flexbox conformance. It does not alter semantic order or
make native-engine selection implicit.

## Tradeoffs

- Defaulting to the CSS initial factor `1` makes negative free space useful, but
  existing overflow fixtures must opt out with `flex-shrink:0` when they are
  specifically testing legacy overflow reachability.
- Integer factors and prefix-floor shares preserve the current bounded,
  deterministic coordinate model, but decimal CSS shrink values remain
  visibly unsupported.
- Weighting by the original base width matches the important proportional
  behavior while keeping the algorithm small; it does not implement the full
  CSS flexing freeze algorithm or intrinsic sizing.
- Shrinking after line formation keeps the current line owner stable and makes
  wrapped rows predictable, but a grown or shrunk item never changes which
  already-formed line contains it.
- Minimum freezing protects the existing min/max box contract, but a line can
  still overflow when its floors plus margins and gaps cannot fit.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

The implementation, focused tests, full native suites, strict feature and
documentation gates, issue #40 status, and exact regenerable-target cleanup
will be recorded here when the slice closes. Remote CI is not claimed until
the local branch is pushed.
