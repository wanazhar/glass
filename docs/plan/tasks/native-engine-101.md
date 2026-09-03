---
id: native-engine-101
scope: glass-browser/native-engine/text-align-justify
status: ready
depends-on: [native-engine-100]
---

# Native bounded text justification

## Objective

Add a bounded inherited `text-align:justify` value to the native engine's
existing fixed-cell inline-flow owner. Expand only eligible collapsed ASCII
word separators on lines ended by soft wrapping, while keeping the existing
physical/logical alignment values, source order, and no-bidi/shaping boundary
intact.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-100.md`
- [CSS Text Module Level 3: text-align](https://www.w3.org/TR/css-text-3/#text-align-property)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the additional inherited value
`text-align:justify`. The established fixed-cell default remains physical
left alignment when the declaration is omitted or unsupported. Existing
`left`, `center`, `right`, `start`, and `end` values retain their current
behavior.

For eligible horizontal-tb fixed-cell flow:

- only `white-space:normal|pre-line` with `word-break:normal` participates;
  `pre`, `pre-wrap`, `nowrap`, `break-all`, ellipsis, and unsupported text
  modes remain unchanged and fail closed where already required;
- a line is justified only when it ended because a subsequent collapsed word
  could not fit and the line contains at least one emitted ASCII space between
  supported text fragments; the final line, an empty line, a line terminated by
  `<br>` or a source newline, and a line with no positive free space are not
  stretched;
- positive remaining line width is distributed across eligible separators in
  deterministic source order using integer division and a bounded remainder;
  earlier separators receive the remainder pixel, and the sum of added space
  advances never exceeds the line's available width;
- existing `word-spacing` remains part of each separator's base advance and
  `letter-spacing` is unchanged; justification adds a separate bounded
  per-space advance rather than rewriting the authored style;
- direct text, `display:contents` descendants, and already-supported inline
  item subtrees remain in source order. Any shifted later item, box, text run,
  display-list command, raster glyph, viewport projection, overflow result,
  root scroll, capture, and semantic/source-order consumer uses the same
  expanded geometry owner;
- direction is not used to reorder text or inspect strong characters. The
  existing inherited `direction:ltr|rtl` state remains available to
  `start|end`, while justified spacing follows the bounded source-order model.

The slice is intentionally not a general inline-formatting implementation.
Unicode bidi resolution, glyph shaping, mixed-direction runs, tabs, CJK or
language-specific line breaking, `text-align:match-parent|justify-all`,
`text-justify`, logical properties, vertical writing modes, fractional or
font-relative metrics, hyphenation, and browser-wide CSS conformance remain
outside the contract.

## Tradeoffs

- Recording the added separator advance on the immutable text/display path
  keeps layout, paint, raster, overflow, and capture numerically aligned; it
  costs one bounded field per text run and a small raster branch.
- Applying justification only to soft-wrapped non-final lines avoids silently
  stretching short paragraphs or hard-break lines, but it does not claim the
  full CSS last-line and `text-align-last` matrix.
- Source-order remainder allocation is deterministic and easy to audit, but
  it is not visual-order distribution for bidi or shaped text; those features
  remain explicitly excluded rather than being approximated.
- Limiting participation to collapsed normal/pre-line spaces preserves the
  current whitespace model and prevents preformatted literal spacing from
  being rewritten. Wider whitespace and language rules require a separate
  contract with new evidence.
- No new crate, dependency, renderer, or mutable geometry owner is introduced;
  the native feature stays default-off inside `glass-browser`.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

The implementation gate will include:

- parser, declaration, cascade, inheritance, inline precedence, and typed
  unsupported-value diagnostics for `text-align:justify`;
- deterministic spacing for one and multiple soft-wrapped lines, uneven
  integer remainder allocation, word-spacing composition, no-space and
  negative/zero-free-space fallback, final-line and hard-break exclusion,
  preformatted/break-all exclusion, and physical/logical alignment regressions;
- display-list and software-raster assertions proving added space advance is
  represented consistently, with source-order text and supported inline
  subtree translations preserved through overflow, scrolling, capture, and
  semantic projections;
- `cargo fmt --all -- --check`, focused and full native integration tests,
  feature-enabled library tests with the documented explicit stack, strict
  all-feature and no-default-feature Clippy, warning-denied rustdoc, locked
  binaries, paired package/dependency gates, fuzz checking,
  documentation/release validators, and exact isolated-target cleanup.

Remote CI remains pending because `main` is local-only. No browser-parity,
release, registry-publication, or remote-certification claim is part of this
task.
