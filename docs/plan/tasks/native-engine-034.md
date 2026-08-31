---
id: native-engine-034
scope: glass-browser/native-engine/preformatted-whitespace
status: done
depends-on: [native-engine-033]
---

# Native bounded `white-space: pre`

## Objective

Add the next bounded CSS whitespace mode to the existing native inline-flow
path: inherited `white-space: pre` with deterministic preservation of literal
source whitespace and hard source line boundaries.

## Contract

The supported `white-space: pre` value is inherited through the existing DOM
style walk. Text owned by a preformatted element retains its source spaces,
tabs, and other non-line-break characters as fixed one-cell advances in the
existing deterministic text path. ASCII line-feed and carriage-return
boundaries become hard line breaks through the existing `<br>` transition;
CRLF is one break. Preformatted segments do not soft-wrap at the containing
content width. A leading, consecutive, or trailing source newline therefore
creates the same bounded empty-line behavior as `<br>`.

`white-space: normal` remains the default and keeps collapsed word-aware
wrapping. `white-space: pre-line` retains its existing behavior: source
newlines break, other whitespace collapses, and words may wrap. The `pre` mode
does not create semantic, layout, or paint nodes, and it does not alter
semantic visible-text, revision, root-scroll, ownership, clipping, or
display-list contracts. The existing fixed pixel line-height floor determines
each source break.

The fixed-cell text path is deliberately not a browser text engine: tabs do
not use tab stops, unsupported glyphs remain subject to the existing fallback
surface, and a line wider than its content box is not reflowed or horizontally
scrolled. Its output is bounded by the existing document, text, display-list,
logical-surface, viewport, and ancestor-clip limits. `pre-wrap`, `break-spaces`,
`nowrap`, text alignment, word spacing, Unicode line breaking, font metrics,
bidi, and full CSS white-space/overflow conformance remain unsupported.

## Tradeoffs

- `pre` fixes deterministic fixture/code-block layout where authored spaces
  and line boundaries matter, but the fixed-cell glyph model cannot claim
  browser tab-stop, font, or baseline fidelity.
- No-wrap preservation avoids inventing a browser line-breaking policy, but
  wide lines can extend beyond their content box; existing viewport and
  ancestor clipping are the only bounds and no horizontal scrolling is added.
- The mode reuses the current hard-break cursor and text display-list path, so
  it adds no dependency or second layout owner, at the cost of keeping the
  current bounded style recomputation and fixed metrics.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

- CSS parsing, inheritance, and unsupported-value unit coverage;
- native integration coverage for inherited/inline `pre`, preserved spaces and
  tabs, leading/consecutive/trailing/CRLF breaks, no soft wrapping, and the
  existing `normal`/`pre-line` distinction;
- native integration and unit suites;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- documentation coverage, depth, and release-truth validators;
- `cargo fmt --all -- --check` and `git diff --check`; and
- one focused Conventional Commit before the next slice.

## Completion evidence

Implemented in the existing CSS cascade, DOM style walk, and inline-flow
layout path. `white-space: pre` is inherited; literal bounded source spaces,
tabs, and other non-line-break characters remain fixed-cell text runs; LF, CR,
and CRLF reuse the existing hard-break cursor; and preformatted segments do
not soft-wrap. The flow cursor carries the no-wrap policy through supported
inline descendants. Semantic visible text remains the existing collapsed
projection, and no semantic, layout, or paint node is synthesized.

Verified locally on 2026-08-31:

- `cargo fmt --all -- --check` and `git diff --check` passed;
- focused pre test: 1 passed;
- native integration suite: 47 passed;
- native unit suite: 36 passed, including supported `pre`, retained
  unsupported `pre-wrap` diagnostics, and existing CSS cascade coverage;
- strict Clippy for default/no-default and `native-engine` feature targets
  passed with warnings denied;
- locked all-target/all-feature `glass-browser` matrix: 819 library tests ran,
  with 818 passed and 1 ignored, and all integration and example targets green;
- native-feature Rust doctests: 4 passed; and
- documentation coverage, depth, release-truth, version-sync, and feature
  parity validators passed. Final counts and report paths are recorded in the
  checkpoint commit and issue #40.
