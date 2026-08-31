---
id: native-engine-035
scope: glass-browser/native-engine/pre-wrap-whitespace
status: done
depends-on: [native-engine-034]
---

# Native bounded `white-space: pre-wrap`

## Objective

Extend the existing bounded preformatted inline-flow path with inherited
`white-space: pre-wrap`: retain authored source whitespace and hard line
boundaries while allowing deterministic fixed-cell soft wrapping at the
containing content width.

## Contract

The supported `white-space: pre-wrap` value is inherited through the existing
DOM style walk. Spaces, tabs, and other non-line-break characters remain
literal fixed-cell text. ASCII line-feed and carriage-return boundaries reuse
the existing `<br>` hard-break cursor transition; CRLF is one break. A
pre-wrap segment may soft-wrap only at the bounded fixed-cell capacity of the
current content line, preserving source order and every source character.

The implementation may split a source segment into multiple display-list text
runs at deterministic cell boundaries, but it does not synthesize semantic,
layout, or paint nodes. `white-space: normal` retains collapsed word-aware
wrapping, `pre-line` retains collapsed whitespace with source newline breaks,
and `pre` retains literal whitespace without soft wrapping. Cross-mode inline
whitespace joining remains bounded by the existing flow-item behavior.

The fixed-cell path is deliberately not a browser text engine: tabs do not
use tab stops, unsupported glyphs remain subject to the existing fallback
surface, and Unicode line-breaking opportunities, font metrics, shaping,
baselines, bidi, and justification are not modeled. Every output remains
bounded by the existing document, text, display-list, surface, viewport, and
ancestor-clip limits.

## Tradeoffs

- `pre-wrap` makes bounded source fixtures and authored code blocks usable in
  narrow content boxes while keeping the no-dependency renderer deterministic.
- Character-capacity wrapping can differ from browser break opportunities,
  especially for words, grapheme clusters, CJK text, tabs, and unsupported
  glyphs; those differences are part of the explicit experimental boundary.
- Splitting text runs at fixed-cell boundaries keeps paint order and existing
  revision contracts intact, at the cost of more display-list entries for
  long pre-wrap text.
- Wide or invalidly narrow content remains bounded by existing integer layout
  and clip limits; no horizontal scrolling, font measurement, or overflow
  policy is added.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

- CSS parsing, inheritance, supported-mode cascade, and unsupported-value
  diagnostics;
- native integration coverage for inherited/inline `pre-wrap`, literal
  spaces and tabs, fixed-cell soft wrapping, leading/trailing whitespace,
  LF/CR/CRLF breaks, and the existing `normal`/`pre-line`/`pre` distinction;
- native integration and unit suites;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- native-feature doctests and documentation coverage, depth, release-truth,
  version-sync, and feature-parity validators; and
- `cargo fmt --all -- --check`, `git diff --check`, and one focused
  Conventional Commit before the next slice.

## Completion evidence

Implemented in the existing CSS cascade, DOM style walk, and inline-flow
layout path. `white-space: pre-wrap` is inherited; literal bounded source
spaces, tabs, and other non-line-break characters remain fixed-cell text;
LF, CR, and CRLF reuse the existing hard-break cursor; and source segments
soft-wrap at deterministic fixed-cell capacity. Text runs may split at those
boundaries without synthesizing semantic, layout, or paint nodes, and
semantic visible text remains the existing collapsed projection.

Verified locally on 2026-08-31:

- `cargo fmt --all -- --check` and `git diff --check` passed;
- focused pre-wrap test: 1 passed;
- native integration suite: 48 passed;
- native unit suite: 36 passed, including supported `pre-wrap`, inherited
  whitespace behavior, retained unsupported `break-spaces` diagnostics, and
  existing CSS cascade coverage;
- strict Clippy for default/no-default and `native-engine` feature targets
  passed with warnings denied;
- locked all-target/all-feature `glass-browser` matrix: 819 library tests ran,
  with 818 passed and 1 ignored, and all integration and example targets green;
- native-feature Rust doctests: 4 passed; and
- documentation coverage, depth, release-truth, version-sync, and feature
  parity validators passed: 449 Markdown documents, 83 current documents,
  345 full-product MCP tools, 17 examples, 22 public modules, 19 substantive
  contracts, 536 semantic-audit hits, and 0 current-claim failures.
