---
id: native-engine-033
scope: glass-browser/native-engine/pre-line-source-breaks
status: done
depends-on: [native-engine-032]
---

# Native bounded `white-space: pre-line` breaks

## Objective

Preserve author source line boundaries in the existing bounded inline flow for
the one useful newline mode that can be implemented without a font or CSS
layout engine: `white-space: pre-line`.

## Contract

The supported `white-space: pre-line` value is inherited through the existing
DOM style walk. In text owned by a pre-line element, ASCII line-feed and
carriage-return boundaries become hard line breaks through the same bounded
flow-cursor transition used by visible `<br>`; CRLF is one break. Spaces and
other whitespace remain collapsed by the existing bounded text path. A
leading, consecutive, or trailing source newline therefore creates the same
bounded empty-line behavior as `<br>`.

`white-space: normal` remains the default and keeps the current collapsing
behavior, including treating source newlines as whitespace. `pre-line` does
not create semantic, layout, or paint nodes, and it does not alter visible
semantic text, revision, root scroll, ownership, clipping, or display-list
contracts. The existing containing element owns text style and the existing
fixed pixel line-height floor determines each break.

The engine continues to reject or diagnose `pre`, `pre-wrap`, `break-spaces`,
`nowrap`, tabs/newline preservation beyond this bounded mode, CSS text
alignment, Unicode line breaking, font metrics, bidi, and full CSS
white-space conformance. The behavior remains bounded by the existing DOM,
text, flow, and viewport limits.

## Tradeoffs

- `pre-line` fixes a common fixture authoring need while reusing the proven
  `<br>` cursor transition, but it intentionally does not preserve tabs or
  arbitrary whitespace.
- Inheritance is resolved through the existing per-node style walk, keeping a
  single style owner, at the cost of the current bounded recomputation rather
  than a style cache.
- CRLF normalization is deterministic and bounded, but no browser-grade line
  breaking or font metric behavior is implied.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

- CSS parsing, inheritance, and unsupported-value unit coverage;
- native integration coverage for inherited and inline `pre-line`, leading,
  consecutive, trailing, CRLF, and default-normal behavior;
- native integration and unit suites;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- documentation coverage, depth, and release-truth validators;
- `cargo fmt --all -- --check` and `git diff --check`; and
- one focused Conventional Commit before the next slice.

## Completion evidence

Implemented in the existing CSS cascade, DOM style walk, and inline-flow
layout path. `white-space: normal` remains the default; inherited or inline
`pre-line` style causes LF, CR, and one CRLF sequence to reuse the 032 hard
break transition, while each segment continues through the existing collapsed
word/text fragmenter. No semantic, layout, or paint node is synthesized.

Verified locally on 2026-08-31:

- `cargo fmt --all -- --check` and `git diff --check` passed;
- focused pre-line integration test: 1 passed;
- native integration suite: 46 passed;
- native unit suite: 36 passed, including supported/unsupported
  `white-space` parsing;
- strict Clippy for default and `native-engine` feature targets passed with
  warnings denied;
- locked all-target/all-feature `glass-browser` matrix: 818 passed, 1
  ignored, 0 failed in the 819-test library target, with all integration and
  example targets green;
- native-feature Rust doctests: 4 passed; and
- issue #40 and the architecture, analysis, and plan indexes were synchronized
  with this contract. Documentation and release-truth validator results are
  recorded in the final checkpoint commit.
