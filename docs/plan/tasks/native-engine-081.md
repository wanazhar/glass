---
id: native-engine-081
scope: glass-browser/native-engine/flex-basis
status: active
depends-on: [native-engine-080]
---

# Native bounded flex-basis sizing

## Objective

Extend the existing bounded row-flex geometry owner with a deterministic
`flex-basis` input. Explicit integer-pixel bases must feed the shipped
grow/shrink, wrapping, justification, paint, overflow, and hit-test paths
without adding a second sizing owner, fractional metrics, or a workspace
crate/dependency.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts `flex-basis: auto` and a non-negative integer
pixel length from `0px` through `MAX_NATIVE_VIEWPORT_DIMENSION`. The property
is non-inherited and defaults to `auto`; it uses the existing selector
specificity, source order, and inline precedence. Invalid, negative,
unitless, fractional, percentage, `calc()`, `content`, CSS-wide, and
out-of-range values remain unsupported and do not replace an earlier valid
declaration. The `flex` shorthand is not part of this boundary.

For an eligible block-level `display:flex` row or `row-reverse` container:

- `auto` delegates to the existing width/intrinsic item path, preserving the
  current compatibility behavior for items that do not opt into an explicit
  basis;
- an explicit pixel basis overrides the item's `width` as its initial main
  size, converts through the existing content-box or border-box inset helper,
  and applies the existing effective `min-width`/`max-width` constraints;
- explicit bases are not clamped to the container before line formation, so a
  basis wider than the available line can participate in the existing
  shrink-or-overflow decision; margins and resolved gaps remain outside the
  basis and are never resized;
- wrapping and line formation use the constrained explicit or auto base width
  plus margins and column gap; a later grow or shrink pass changes item width
  only after the line is formed;
- the original base width drives the existing bounded `flex-grow` and
  base-width-weighted `flex-shrink` allocations, including max/min freezing;
  `flex-basis:0px` therefore provides a zero base for positive growth while
  still respecting a declared minimum;
- the final width is passed through the existing descendants, content
  rectangles, display-list, raster, overflow, viewport projection, hit
  testing, scrolling, capture, and semantic/source-order consumers.

The property affects only eligible flex-item sizing. It does not change
non-flex width resolution, normal-flow fallback, semantic/source order,
cross-axis sizing, or the explicit-only native-engine selection boundary.

## Tradeoffs

- Integer pixels and a small `auto`/length grammar keep the sizing owner
  deterministic and cheap to build, but percentages, fractional lengths, and
  the `flex` shorthand remain visibly unsupported.
- Explicit bases bypass the old unwrapped available-width clamp so the
  already-shipped shrink path can resolve an oversized basis. This can expose
  overflow when shrink is zero, and it intentionally differs from legacy
  width-only fixtures only when `flex-basis` is present.
- `auto` delegates to the existing width/intrinsic path, including its current
  compatibility clamp, rather than reimplementing the full browser flex base
  size algorithm. That preserves old layouts but is not a browser-conformance
  claim.
- Existing box-sizing and min/max helpers are reused, so the basis remains an
  outer coordinate in the current integer model. The slice does not add
  intrinsic-content measurement, percentage resolution, aspect ratio, or
  automatic minimum-size behavior.
- Basis-driven wrapping remains a formed-line decision; subsequent growth or
  shrink never moves an item between already-formed lines. This keeps line
  records and artifact translation stable at the cost of full Flexbox parity.

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
