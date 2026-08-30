id: native-engine-026
scope: glass-browser/native-engine/whitespace-boundaries
status: done
depends-on: [native-engine-025]
---

# Native bounded whitespace boundaries

## Objective

Close the next inline-flow correctness gap without importing a CSS text or
layout engine:

- retain whether a direct text node begins or ends with author whitespace;
- join sibling direct-text nodes and `display:contents` descendants using at
  most one bounded separator;
- keep supported inline element boxes separated when a source whitespace
  boundary exists; and
- preserve the existing word-aware wrapping, source-order paint, line-height,
  clip, scroll, hit-test, and revision contracts.

This is a focused continuation of the 023/024 text-fragment work. It stays
inside `glass-browser`'s default-off native feature and keeps the workspace at
exactly two installable crates.

## Contract

Each direct text node is collapsed to the existing bounded ASCII-space word
projection, while its source-level leading/trailing whitespace boundary is
retained for the containing flow. One separator may be consumed between
non-empty inline-flow items when either side supplies a whitespace boundary.
Leading whitespace at a fresh line and trailing whitespace at a block-flow
boundary are discarded. If a separator cannot fit, the line is flushed and
the separator is dropped rather than painted at the beginning of the new line.

The same bounded separator policy applies around supported inline element boxes
and through `display:contents` children. A rendered separator is represented
by the existing fixed-width text fragment/display path; it does not create a
new DOM node or transport field. Direct text without a source whitespace
boundary remains adjacent to the preceding inline-flow item.

This slice does not implement CSS `white-space`, `word-spacing`, `text-indent`,
preserved tabs/newlines, Unicode line breaking, bidi, font metrics/shaping,
anonymous inline boxes, whitespace joining across independent nested flow
owners, margin collapsing, or browser inline-formatting parity. Unsupported
values remain ignored and all geometry remains bounded integer pixels.

## Tradeoffs

- Retaining boundary bits fixes accidental spaces between separate text nodes
  while preserving intentional spaces around simple inline fixtures, but it is
  not the CSS whitespace processing model.
- Painting a consumed separator through the existing containing-element text
  fragment keeps source order and style ownership observable, but the fragment
  is a derived layout artifact rather than an independent DOM text node.
- Dropping a separator when it would start a fresh line matches the bounded
  line policy and avoids leading whitespace, but differs from configurable CSS
  wrapping and typography.
- `display:contents` can share its parent flow cursor, while an inline
  element's nested flow remains an independent bounded owner; cross-owner
  whitespace is intentionally limited to the outer boundary.
- The implementation remains dependency-free and cheap to reason about, at
  the cost of leaving Unicode shaping, font metrics, and full inline parity to
  later capability waves.

## Path

- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and capability docs

## Verification

- focused whitespace-boundary, inline, `display:contents`, wrapping, and
  source-order tests;
- full native integration and native unit suites;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- documentation coverage, depth, and release-truth validators;
- `cargo fmt --all -- --check` and `git diff --check`; and
- one focused Conventional Commit before the next slice.

## Completion evidence

Implemented and verified locally in the focused
`feat(native-engine): preserve whitespace boundaries` checkpoint. The issue
#40 comment/body records the resulting commit identity.

- Focused whitespace-boundary tests: 2 passed.
- Native integration tests: 37 passed.
- Native unit tests: 35 passed.
- Strict default-feature and `native-engine` Clippy gates pass with warnings
  denied.
- Full locked `glass-browser` all-target/all-feature matrix: 817 passed, 1
  ignored; all integration suites passed, including 37 native integration
  tests.
- `cargo fmt --all -- --check` and `git diff --check` pass.
- Documentation coverage: 440 Markdown files, 345 full-product MCP tools,
  100 browser-only tools, 17 examples, and 22 public modules.
- Documentation depth: 93 current guides and 19 substantive contracts.
- Release-truth audit: 0 current-claim failures.
- No new dependency, third crate, stable transport capability, automatic
  backend path, or browser-parity/security claim was introduced.
