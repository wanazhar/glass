---
id: native-engine-102
scope: glass-browser/native-engine/text-align-last
status: ready
depends-on: [native-engine-101]
---

# Native bounded final-line alignment

## Objective

Add a bounded inherited `text-align-last` property to the native engine's
existing fixed-cell inline-flow owner. Resolve an explicit final-line value
only at the final non-empty line flush of a block, while preserving the 101
soft-wrap justification boundary, source order, and the single geometry path.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-101.md`
- [CSS Text Module Level 3: text-align-last](https://www.w3.org/TR/css-text-3/#text-align-last-property)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the inherited bounded values
`text-align-last:auto|left|center|right|start|end`. The established fallback
remains `auto`; omitted or unsupported declarations retain the current native
line behavior. `justify`, `match-parent`, `justify-all`, and other values are
unsupported and continue to produce the existing typed diagnostic.

For eligible horizontal-tb fixed-cell block flow:

- only the final non-empty line flushed by the owning block's normal completion
  path uses `text-align-last`; soft-wrap, inline-item preflight, explicit
  `<br>`, source-newline, preformatted chunk, break-all, and intermediate
  block-boundary flushes continue using the ordinary `text-align` owner;
- `auto` preserves the current `text-align` result except that
  `text-align:justify` keeps its 101 final-line fallback to physical left
  alignment and does not stretch separators;
- explicit `left|center|right` uses physical alignment, while `start|end`
  resolves through the inherited `direction:ltr|rtl` state already used by
  native-engine-100; no text or semantic/source order is reordered;
- a final empty line is not aligned and does not create a synthetic text run,
  box, or paint command;
- the resolved offset is applied by the existing line-flush owner to every
  line item and text run in the final line. The same geometry reaches boxes,
  display-list commands, raster glyphs, viewport projection, overflow,
  scrolling, capture, hit testing, and semantic/source-order consumers;
- the property remains inherited through the existing DOM style walk, and
  selector, inline, source-order, and unsupported-value precedence remains
  typed and bounded.

The slice remains restricted to the current horizontal-tb, integer-pixel,
fixed-cell implementation. Unicode bidi resolution, glyph shaping, mixed
direction runs, `text-align-last:justify`, `text-align:match-parent`,
`text-align:justify-all`, `text-justify`, logical properties, vertical writing
modes, fractional or font-relative metrics, hyphenation, and browser-wide CSS
conformance remain outside the contract.

## Tradeoffs

- Reusing the final normal block flush keeps final-line alignment numerically
  consistent across all existing artifact consumers, but does not claim
  forced-break and last-line behavior from the full CSS text model.
- Keeping `auto` as a distinct value preserves the native compatibility
  fallback for `text-align:justify` without rewriting authored declarations;
  explicit values opt into final-line placement.
- Limiting the property to the final non-empty block line avoids inventing
  line ownership for nested or forced-break paths, but it leaves those paths
  on ordinary `text-align` until a separate contract supplies evidence.
- No new crate, dependency, renderer, or mutable geometry owner is introduced;
  the native feature stays default-off inside `glass-browser`.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

The implementation gate will include:

- parser, declaration, cascade, inheritance, inline precedence, and typed
  unsupported-value diagnostics for `text-align-last`;
- final-line `auto`, physical, and direction-aware logical alignment,
  inherited/overridden values, no-text and empty-final-line fallback, and
  101 justification remaining limited to eligible soft-wrapped non-final
  lines;
- forced-break, source-newline, preformatted, break-all, intermediate block,
  inline-box, overflow, scroll, capture, display-list, raster, hit-test, and
  semantic/source-order regressions through the shared line-flush owner;
- `cargo fmt --all -- --check`, focused and full native integration tests,
  feature-enabled library tests with the documented explicit stack, strict
  all-feature and no-default-feature Clippy, warning-denied rustdoc, locked
  binaries, paired package/dependency gates, fuzz checking,
  documentation/release validators, and exact isolated-target cleanup.

Remote CI remains pending because `main` is local-only. No browser-parity,
release, registry-publication, or remote-certification claim is part of this
task.
