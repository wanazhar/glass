---
id: native-engine-049
scope: glass-browser/native-engine/axis-specific-overflow
status: done
depends-on: [native-engine-048]
---

# Native bounded axis-specific overflow clips

## Objective

Extend the existing rectangular `overflow:hidden`/`overflow:clip` clip owner
with bounded physical `overflow-x` and `overflow-y` declarations. A fixture
can clip one axis while leaving the other axis visible, and all current layout
consumers must observe the same axis-specific result.

## Contract

The native CSS grammar accepts `overflow-x` and `overflow-y` only with the
already-supported `hidden` and `clip` values. The existing `overflow` shorthand
continues to set both axes. Longhands cascade independently per axis, so a
later or more-specific axis declaration can override the corresponding
shorthand axis without changing the other axis.

The computed style exposes two bounded clip bits. The existing document-space
rectangular clip representation constrains only the selected axis and leaves
the other axis unconstrained. Paint replay, viewport rectangle projection,
point hit testing, and the 048 root-overflow text measurement all consume that
same clip result; no second geometry or clipping owner is introduced.

`visible`, `auto`, and `scroll` remain unsupported and continue to produce
bounded diagnostics. This slice does not add nested scroll offsets, scrollbars,
axis-specific scrolling, scroll anchoring, visible-overflow propagation,
rounded descendant clips, or browser CSS overflow parity.

## Tradeoffs

- Per-axis clip bits make common fixture layouts expressible while preserving
  the current dependency-free rectangular representation.
- Leaving the non-selected axis unconstrained avoids falsely hiding content,
  but is not a full used-value implementation for mixed `visible` and
  non-visible overflow.
- Independent longhand cascade is more precise than treating the two
  declarations as one boolean, while unsupported scrolling values remain
  explicit instead of acquiring accidental behavior.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and public
  capability docs

## Verification

- shorthand and longhand parsing/cascade cover independent x/y precedence;
- x-only and y-only clips preserve visibility, hit testing, paint, viewport
  projection, and root overflow on the unselected axis;
- existing combined hidden/clip, scrolling, history, display-list, raster,
  and diagnostics boundaries remain green;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass.

## Completion evidence

Implementation and focused validation are complete locally. The code and
synchronized docs are committed as a focused Conventional Commit; the issue
#40 checkpoint follows before the next slice.

Validation evidence:

- focused axis-specific overflow integration test: 1 passed;
- full native integration suite: 62 passed;
- native-engine module unit suite: 44 passed;
- strict Clippy passed with all features and with no default features;
- `cargo fmt --all -- --check` and `git diff --check` passed;
- documentation depth, release-documentation, version-sync, and
  feature-parity validators passed: 93 current guides, 463 Markdown
  documents, 0 current-claim failures, synchronized 0.3.14 versions, and 14
  capabilities across 4 targets;
- no CLI/MCP inventory changed, so binary documentation coverage remains
  covered by the existing release gates.
