---
id: native-engine-047
scope: glass-browser/native-engine/inherited-line-height
status: done
depends-on: [native-engine-046]
---

# Native bounded inherited fixed line height

## Objective

Extend the existing bounded positive-pixel `line-height` support through the
DOM style walk. A descendant without its own valid declaration should inherit
the nearest computed fixed line-height, so one layout owner controls the
minimum line box for direct text and inline descendants.

## Contract

The existing parser accepts exactly one positive bounded `<N>px` value. When a
node has no valid local `line-height` declaration, its computed style inherits
the value from its parent; an explicit valid stylesheet or inline declaration
still wins through the existing cascade. Unsupported, malformed, zero,
relative, percentage, unitless, and `normal` values do not replace a valid
inherited value.

The inherited value is consumed by the existing layout owner: a flow cursor
uses it as its minimum line height, and an inline element without an explicit
height uses it as the minimum auto content height. Explicit content-box or
border-box height remains authoritative. The value reaches the existing
display-list, viewport, hit-test, root-scroll, and raster projections without a
second geometry owner.

This slice does not add font-relative or unitless line metrics, baselines,
`vertical-align`, font shaping, general computed-style inheritance, CSS-wide
keywords, nested scrolling, or browser line-layout parity. It adds no
dependency, stable transport capability, third crate, automatic backend path,
or screenshot evidence contract.

## Tradeoffs

- Inheriting one fixed pixel floor fixes a common nested-fixture mismatch while
  preserving deterministic integer layout.
- Propagating only the already-supported positive pixel value avoids implying
  that font metrics or CSS-wide keyword resolution exists, but leaves most
  line-height semantics outside the boundary.
- Keeping explicit height authoritative preserves the established box-model
  contract, even where browser line boxes and used heights differ.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and capability docs

## Verification

- CSS cascade/unit tests cover inherited values, explicit child precedence,
  and invalid child declarations retaining the inherited value;
- nested block/inline flow confirms inherited minimum line height, explicit
  height precedence, document-space layout, display-list origins, hit testing,
  and raster behavior;
- existing normal/pre/nowrap whitespace, scrolling, action, and history
  boundaries remain unchanged;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass.

## Completion evidence

Implementation and focused validation are complete locally. The code and
synchronized docs are committed as a focused Conventional Commit; the issue
#40 checkpoint follows before the next slice.

Validation evidence:

- `cargo fmt --all -- --check` and `git diff --check` passed;
- focused inherited line-height unit and integration tests: 1 passed each;
- full native integration suite: 60 passed;
- native-engine module unit suite: 43 passed;
- strict Clippy passed with all features and with no default features;
- existing `pre` and `pre-wrap` fixtures were updated to assert the intended
  inherited child auto-height behavior;
- synchronized README, feature, SDK, architecture, plan, and task docs now
  describe bounded inherited positive-pixel `line-height`;
- documentation depth, release-documentation, version-sync, and
  feature-parity validators passed: 93 current guides, 461 Markdown documents,
  0 current-claim failures, synchronized 0.3.14 versions, and 14 capabilities
  across 4 targets.
