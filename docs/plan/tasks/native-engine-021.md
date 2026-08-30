---
id: native-engine-021
scope: glass-browser/native-engine/inline-flow
status: done
depends-on: [native-engine-020]
---

# Native bounded inline line placement

## Objective

Make the existing integer normal-flow layout place inline element boxes on the
correct line before their geometry is materialized:

- estimate each bounded inline box's actual outer width, including supported
  border/padding and margins;
- flush the current line before laying out an inline box that cannot fit in the
  remaining content width;
- preserve deterministic line height, document-space coordinates, paint
  origins, hit testing, and root scroll behavior; and
- prove the real layout/display-list/input path with adjacent inline fixtures.

This is a bounded inline-box flow correction, not full inline formatting. It
does not add font shaping, word-boundary whitespace, anonymous inline boxes,
baseline alignment, bidi, floats, replaced-element metrics, inline text
fragments, flex/grid layout, or browser line-layout parity.

## Contract

For supported `display:inline`/default-inline elements, the layout builder
computes the same integer outer width that `layout_element` will use, including
the existing side-specific border and uniform padding insets. If that width
plus the supported uniform margins exceeds the remaining line width, the
builder flushes the current line before invoking `layout_element`. The element
is then placed at the line start, clamped to the available content width, and
its line height contributes to the parent flow height.

The existing block flow, character-width text measurement, deterministic
line-height fallback, outer/content rectangles, rounded hit testing, display
list generation, root scrolling, and revision behavior remain unchanged.
Inline text remains represented through the current direct-text paint model;
this checkpoint fixes inline element placement, not general text fragmentation
or typography.

No stable backend capability, dependency, third crate, automatic backend path,
or screenshot evidence contract changes.

## Tradeoffs

- Preflight width measurement prevents an inline child from being committed at
  an overflowing x-coordinate and then retroactively moving only the cursor.
- Reusing the integer width calculation keeps layout and placement consistent,
  but at this checkpoint it still missed font metrics, whitespace collapsing,
  baseline behavior, and real word-aware wrapping. The later native-engine-023
  and native-engine-024 checkpoints own the bounded collapsed direct-text
  fragments, word-aware wrapping, and their paint origins.
- Flushing whole inline boxes preserves atomic element ownership and simple
  hit testing, but a long inline box is not split into fragments.
- The two-crate boundary and dependency-free renderer remain intact; source
  edits still invalidate the `glass-browser` compilation unit.

## Path

- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- native-engine unit/integration tests and synchronized capability documentation

## Verification

- `cargo fmt --all -- --check`
- focused layout, display-list, hit-test, scroll, and backend tests;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- documentation coverage/depth/release-truth validators;
- `git diff --check` and one focused Conventional Commit.

## Completion evidence

Implemented and verified locally. The layout builder now preflights each
supported inline element's clamped integer outer width plus uniform margins
before materializing its box, flushing the current line when the box cannot
fit. The shared width calculation keeps content-box/border-box, side-specific
border, padding, and intrinsic inline sizing consistent between preflight and
final layout. The integration fixture verifies deterministic fixed line
heights, document-space boxes, display-list text origins, fill geometry, and
depth-aware hit testing across the wrapped line.

- `cargo fmt --all -- --check` and `git diff --check` pass.
- Native unit tests: 31 passed.
- Native integration tests: 30 passed, including the inline line-placement
  fixture.
- Strict default-feature and `native-engine` Clippy gates pass.
- Full locked `glass-browser` all-target/all-feature matrix: 813 passed, 1
  ignored.
- Documentation coverage: 435 Markdown files, 345 full-product MCP tools,
  100 browser-only tools, 17 examples, and 22 public modules.
- Documentation depth: 93 current guides and 19 substantive contracts.
- Release-truth audit: 0 current-claim failures.
- The unsupported typography and general inline-formatting boundary remains
  explicit; no stable transport capability, dependency, third crate, or
  automatic backend path changed.
