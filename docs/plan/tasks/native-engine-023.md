---
id: native-engine-023
scope: glass-browser/native-engine/direct-text-flow
status: done
depends-on: [native-engine-022]
---

# Native bounded direct-text flow fragments

## Objective

Make direct text paint consume the same bounded flow positions that layout
uses for inline content:

- collapse each direct text node through the existing bounded text projection;
- split long text into deterministic fixed-width line fragments;
- retain the actual document-space origin for every fragment;
- carry element and text paint entries in source order; and
- keep the existing style-owner, clip, scroll, revision, and raster contracts.

This repairs the current `text -> inline element -> text` failure mode, where
layout advances the flow correctly but paint places an element's aggregate
direct text back at its content origin. It remains a fixed-glyph fixture
model, not a general inline formatting implementation.

## Contract

Every non-empty direct text node that participates in visible layout is
collapsed with the existing bounded whitespace/text-byte policy. The result
is represented by one or more `NativeTextLayout` fragments owned by the
containing element for style, clipping, and display-list identity. A fragment
records its document-space origin, text, and source-truncation state.

Fragments consume the current flow cursor from left to right. They use the
existing integer `CHARACTER_WIDTH` model, fill the remaining line when
possible, and flush to the next line when no character fits. A long text node
therefore produces deterministic line fragments rather than one text command
whose glyphs overlap later inline content. Element-box and text-fragment paint
entries retain traversal order so mixed direct text and inline descendants do
not get reordered by a later aggregate-text lookup.

The display list continues to use the containing element's computed color and
ancestor `overflow:hidden` clip. Layout boxes remain the source for hit
testing, content height, and root-scroll translation; text origins are only a
derived paint/layout projection. The native engine remains default-off inside
`glass-browser`, with no new dependency, crate, transport capability, or
browser-parity claim.

## Tradeoffs

- Carrying fragments through the layout snapshot removes a duplicate paint
  positioning rule, but adds bounded layout metadata and an internal paint
  order sequence.
- Collapsing whitespace before width calculation keeps layout and raster text
  consistent, but does not implement CSS whitespace modes, word breaking, or
  preservation of author formatting.
- The fixed integer character width is deterministic and cheap, but it is not
  the rasterizer's 5x7 glyph advance and cannot represent font metrics,
  shaping, baselines, bidi, or Unicode fallback.
- Fragments are retained per direct text node. Whitespace joining across
  adjacent text nodes and inline descendants remains outside this slice.
- A truncated source is reported on the final fragment for that source node;
  callers must treat the fragment set as one bounded projection rather than
  as a complete text stream.

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and capability docs

## Verification

- `cargo fmt --all -- --check`
- focused layout/display-list/raster tests for mixed direct text, wrapping,
  source order, clipping, and root-scroll translation;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- documentation coverage/depth/release-truth validators;
- `git diff --check` and one focused Conventional Commit.

## Completion evidence

Implemented and verified locally. Layout now collapses each visible direct text
node once, places bounded fixed-width fragments through the shared flow cursor,
and records source-order box/text entries. The display list consumes those
entries directly, preserving actual mixed-content origins, style ownership,
ancestor clipping, and root-scroll translation without re-deriving aggregate
text at an element content origin.

- `cargo fmt --all -- --check` and `git diff --check` pass.
- Focused direct-text fragment tests: 2 passed.
- Native integration tests: 33 passed, including mixed text/inline source
  order, collapsed bounded text width, clipping, scrolling, and raster output.
- Native unit tests: 33 passed.
- Strict default-feature and `native-engine` Clippy gates pass.
- Explicit native-feature library check passes.
- Full locked `glass-browser` all-target/all-feature matrix: 815 passed, 1
  ignored; all integration suites passed.
- Documentation coverage: 437 Markdown files, 345 full-product MCP tools,
  100 browser-only tools, 17 examples, and 22 public modules.
- Documentation depth: 93 current guides and 19 substantive contracts.
- Release-truth audit: 0 current-claim failures.
- No new dependency, third crate, stable transport capability, automatic
  backend path, or browser-parity/security claim was introduced.
