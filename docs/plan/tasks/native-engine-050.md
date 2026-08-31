---
id: native-engine-050
scope: glass-browser/native-engine/min-max-dimensions
status: done
depends-on: [native-engine-049]
---

# Native bounded minimum and maximum dimensions

## Objective

Add bounded physical `min-width`, `max-width`, `min-height`, and `max-height`
constraints to the existing integer-pixel box-model owner. Common fixture
layouts should be able to constrain auto-sized and explicitly sized boxes
without introducing a second geometry system.

## Contract

The native CSS grammar accepts each min/max dimension only as a non-negative,
bounded pixel value already accepted by the `width`/`height` parser. The
declarations participate in the existing selector, specificity, source-order,
and inline-style cascade independently for each physical dimension.

Used outer dimensions apply the constraints after converting content-box
values through the existing border/padding insets; `box-sizing:border-box`
values constrain the outer box directly. A minimum is retained even when it
exceeds the parent's available width, so measured root overflow can expose the
resulting fixture geometry. A maximum limits auto and explicit dimensions;
contradictory min/max declarations resolve deterministically with the minimum
as the lower bound.

The resulting outer/content rectangles continue to feed the existing normal
flow, text origins, viewport projection, point hit testing, display-list,
raster, root-scroll, and history owners. No new layout owner is introduced.

Negative, percentage, `auto`, logical, aspect-ratio, intrinsic, flex/grid,
margin-collapsing, and browser CSS sizing semantics remain outside the slice.

## Tradeoffs

- Min/max constraints cover useful card and fixture bounds while retaining the
  current dependency-free integer geometry.
- Applying constraints before parent-width clamping preserves minimum-size
  overflow, but can intentionally create a root scroll extent in a narrow
  viewport.
- Content-box conversion reuses existing insets and is predictable, while
  intrinsic and replaced-element sizing remain explicitly unsupported.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and public
  capability docs

## Verification

- min/max parsing and independent cascade cover content-box and border-box;
- auto and explicit widths/heights honor lower and upper bounds;
- minimum width can produce bounded root overflow while maximum width prevents
  unintended expansion;
- existing box-model, line-height, overflow, scroll, hit-test, paint, raster,
  and history boundaries remain green;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass.

## Completion evidence

Implementation and focused validation are complete locally. The code and
synchronized docs are committed as a focused Conventional Commit; the issue
#40 checkpoint follows before the next slice.

Validation evidence:

- focused minimum/maximum dimensions integration test: 1 passed;
- full native integration suite: 63 passed;
- native-engine module unit suite: 45 passed;
- strict Clippy passed with all features and with no default features;
- `cargo fmt --all -- --check` and `git diff --check` passed;
- documentation depth, release-documentation, version-sync, and
  feature-parity validators passed: 93 current guides, 464 Markdown
  documents, 0 current-claim failures, synchronized 0.3.14 versions, and 14
  capabilities across 4 targets;
- no CLI/MCP inventory changed, so binary documentation coverage remains
  covered by the existing release gates.
