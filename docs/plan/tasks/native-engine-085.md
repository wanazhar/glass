---
id: native-engine-085
scope: glass-browser/native-engine/place-content
status: active
depends-on: [native-engine-084]
---

# Native bounded place-content shorthand

## Objective

Expand a deliberately bounded `place-content` shorthand into the existing
`align-content` and `justify-content` computed components. The slice must keep
one cascade representation and one flex geometry owner while preserving the
existing wrapped-line and main-axis distribution behavior.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts one or two ASCII-whitespace-separated,
case-insensitive `place-content` keywords:

- one shared keyword `flex-start`, `center`, `flex-end`, or `space-between`,
  expanding to the same value for `align-content` and `justify-content`;
- two keywords in the explicit order `align-content justify-content`, where
  the first token is one of `flex-start`, `center`, `flex-end`,
  `space-between`, `space-around`, `space-evenly`, `stretch`, or `normal`, and
  the second token is one of `flex-start`, `center`, `flex-end`, or
  `space-between`.

The property is non-inherited. It expands into the existing two computed
components and has no standalone layout state. CSS-wide keywords, logical
start/end, safe/unsafe combinations, duplicate or reversed ambiguous forms,
three or more tokens, unsupported justify values, and other unsupported forms
remain typed diagnostics and do not erase an earlier valid declaration.

Valid shorthand declarations use the existing specificity, source-order, and
inline precedence independently for both expanded components. Within one
declaration block, a valid shorthand sets both components, including the
explicit first/second values; a later valid longhand replaces only its
component; a later valid shorthand replaces both; and an invalid shorthand or
longhand leaves earlier valid winners unchanged.

The expanded values reuse the completed main-axis `justify-content` owner and
wrapped-row `align-content` owner, including gaps, flex sizing, wrapping,
wrap-reverse, complete artifact translation, overflow, projection, hit
testing, scrolling, capture, and semantic/source order. The shorthand does
not add grid behavior, change item alignment, alter line formation, or make
native-engine selection implicit.

## Tradeoffs

- The one-token form is limited to values already meaningful in both bounded
  axes. This gives a useful compact form without inventing a justify fallback
  for `stretch`, `normal`, `space-around`, or `space-evenly`.
- The two-token form follows the CSS shorthand axis order explicitly: the
  first token owns cross-line distribution and the second owns main-axis
  distribution. That keeps the expansion deterministic and makes unsupported
  token order fail closed.
- Expansion writes directly into the existing component fields, so later
  longhands retain the established per-component precedence without another
  cascade pass. The tradeoff is rejection of full CSS grammar and logical or
  writing-mode semantics.
- No new geometry, display-list, raster, or dependency algorithm is needed;
  the slice inherits the current integer-pixel, fixed-width flex limitations
  and does not claim browser conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

The implementation, focused parser/cascade and shared-layout tests, full native
suites, strict feature/documentation gates, issue #40 status, and exact
regenerable-target cleanup will be recorded here when the slice closes. Remote
CI is not claimed until the local branch is pushed.
