---
id: native-engine-085
scope: glass-browser/native-engine/place-content
status: complete
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

The implementation checkpoint is `02f866e6`, following design checkpoint
`adc61a1f`. Focused CSS parser/cascade coverage passed 2/2 tests in 6m57s
after rebuilding from a clean target. Focused shared-layout integration
coverage passed 1/1 test in 20.68s after that rebuild, covering wrapped-line
distribution, main-axis distribution, nested subtree coordinates, paint, and
hit testing.

The full native integration suite passed 111/111 tests in 2.49s. The full
native library suite passed 887 tests with 1 ignored in 7.28s under
`RUST_MIN_STACK=8388608`. Strict all-feature Clippy passed with warnings
denied in 14m19s; the no-default-feature matrix passed with warnings denied in
7m19s. Rustdoc with `RUSTDOCFLAGS='-D warnings'` passed in 3m23s, and the
locked `glass-dev` build passed in 11m15s. Formatting and `git diff --check`
passed.

Repository audits passed at version `0.3.14`: feature parity 14 capabilities
across 4 targets; release documentation 499 Markdown files with 0
current-claim failures; TUI 15 implementation help keys and 63 documentation
markers; documentation depth 93 guides and 19 substantive contracts;
documentation coverage 499 Markdown files, 345 full-product MCP tools (100
browser-only), 17 examples, and 22 public modules; reliability 6 scenarios
across 4 targets; 5 public read-only adapters; and Web IR 8 fixtures, 8
scenarios, and 11 categories with runtime goldens verified.

Issue #40 records the implementation checkpoint and remains open for the next
dependency-ordered slice. Remote CI is not claimed because the branch is
local-only; no push, release, tag, registry publication, or browser-parity
certification is implied. After the final documentation validation, exact
regenerable Glass `target` output fell from 5.3G to 4.0K. No active
Cargo/Rust writer, open target handle, or Git lock remained; `/dev/sda1` moved
from 134G used/60G available/70% to 129G used/65G available/67%. Shared
registries/toolchains and long-lived Glass processes were retained.
