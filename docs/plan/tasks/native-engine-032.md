---
id: native-engine-032
scope: glass-browser/native-engine/hard-line-breaks
status: done
depends-on: [native-engine-031]
---

# Native bounded hard line breaks

## Objective

Make visible `<br>` elements participate in the existing bounded inline flow
as hard line breaks. The current text path can wrap words, but treating a void
line-break element as an ordinary empty inline box loses an author-visible
line boundary and can offset the following fragment.

## Contract

In supported normal flow, a visible `<br>` advances the containing flow to the
next fixed line, resets the inline cursor to its line start, and contributes no
layout box or paint command of its own. Consecutive and leading breaks create
bounded empty lines; a trailing break advances the flow height once. The
existing containing element owns the following text fragment's style and
clip, and the existing line-height floor determines the break height.

`display:none`, hidden, and aria-hidden `<br>` elements do not break flow.
Explicit styles, normal block/inline layout, word wrapping, source-whitespace
boundaries, root scrolling, display-list ordering, hit-testing, and revision
behavior otherwise remain unchanged. `<wbr>`, preserved source newlines,
CSS `white-space` modes, font metrics, Unicode line breaking, bidi, and full
HTML/CSS inline-formatting conformance remain unsupported.

The behavior is bounded by the existing DOM depth, node, text, and viewport
limits. A break must not create a semantic target, mutable owner, or hidden
side effect.

## Tradeoffs

- A real hard break fixes common fixture structure while reusing the existing
  integer flow cursor, but it does not claim browser line-box or font-metric
  parity.
- Empty lines are represented only by cursor movement, avoiding synthetic
  display-list or semantic nodes, but callers inspecting layout see only the
  text fragments around the break.
- Hidden breaks are ignored consistently with hidden text/layout, but more
  detailed CSS display/visibility interactions remain outside the bounded
  model.

## Path

- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

- flow-unit or integration coverage for leading, consecutive, trailing, and
  hidden `<br>` behavior;
- native integration coverage proving fragment y-origins, content height,
  display-list ordering, and raster output follow hard breaks;
- full native integration and native unit suites;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- documentation coverage, depth, and release-truth validators;
- `cargo fmt --all -- --check` and `git diff --check`; and
- one focused Conventional Commit before the next slice.

## Completion evidence

Implemented in `layout.rs` with one focused integration fixture. The bounded
flow cursor now has an explicit hard-break transition: it clears pending
whitespace, advances by the current line-height floor, reserves the new empty
line, resets to the containing flow's x-origin, and leaves the line empty.
`process_children` recognizes only `<br>` after the existing non-rendered and
hidden checks, so visible breaks create no layout box or display command while
hidden, `aria-hidden`, and `display:none` breaks remain inert.

Verified locally on 2026-08-31:

- `cargo fmt --all -- --check` passed;
- focused hard-break integration test: 1 passed;
- native integration suite: 45 passed;
- native unit suite: 36 passed;
- strict Clippy for default and `native-engine` feature targets passed with
  warnings denied;
- locked all-target/all-feature `glass-browser` matrix: 818 passed, 1
  ignored, 0 failed in the 819-test library target, with all integration and
  example targets green;
- display-list and software-surface assertions verified the two text origins,
  content height, omission of `<br>` nodes, and painted pixels; and
- documentation coverage: 446 Markdown files, 345 full-product MCP tools,
  17 examples, and 22 public modules;
- documentation depth: 93 current guides and 19 substantive contracts;
- release-truth: 446 Markdown documents, 83 current documents, 57
  previous-version hits, 536 semantic-audit hits, and 0 current-claim
  failures;
- feature parity: 14 capabilities across 4 targets with checkout 0.3.14; and
- issue #40 and the architecture, analysis, and plan indexes were synchronized
  with this contract.
