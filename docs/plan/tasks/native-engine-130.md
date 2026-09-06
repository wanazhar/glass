---
id: native-engine-130
scope: glass-browser/native-engine/cascade-layers-white-space-revert-layer
status: planned
depends-on: [native-engine-129]
---

# Native bounded `white-space: revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven by native-engine-129 to add
the explicit CSS-wide `revert-layer` keyword to the existing inherited
`white-space` owner. Preserve the five finite whitespace modes, source-boundary
handling, hard-break transitions, fixed-cell soft wrapping, and root overflow
behavior already implemented by the native line-flow path.

## Context

The native engine currently resolves `white-space: normal|pre-line|pre|pre-wrap|nowrap`
through the bounded DOM parent-style walk. Those modes already feed the shared
text collapse, hard-break, soft-wrap, line-origin, overflow, display-list,
capture, raster, and semantic consumers. The stylesheet and inline declaration
paths still store a concrete whitespace value in one cascade slot. This slice
adds private declaration-only rollback state beside that slot and reuses the
existing 15 named-layer registry plus unlayered/inline bucket.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-text-3/#white-space-property>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-129.md`

## Contract

### Declaration and cascade state

- `white-space` accepts one standalone, case-insensitive `revert-layer`
  token. The token exists only in private declaration/candidate state;
  `WhiteSpaceValue` remains the complete public computed-value set.
- Stylesheet candidates use the existing bounded first-appearance named-layer
  order. Selector specificity and source order decide ties within a layer;
  unlayered and inline declarations remain above named layers.
- A winning rollback blocks only its current layer and selects the highest
  priority remaining concrete candidate. Repeated rollback continues through
  lower layers and then the inherited value; unlayered/inline rollback can
  expose the highest named candidate.
- With no candidate remaining, the root default remains
  `white-space: normal`. A descendant with no local candidate retains the
  existing computed parent mode.
- The existing mode semantics remain unchanged: `normal` and `nowrap` keep
  collapsed whitespace, `pre-line` keeps the bounded newline break behavior,
  `pre` keeps literal fixed-cell whitespace without soft wrapping, and
  `pre-wrap` keeps literal whitespace with deterministic fixed-cell wrapping.
- Supported rollback declarations produce no false unsupported-value
  diagnostic. Other CSS-wide keywords, `all`, mixed values, multiple origins,
  `!important` inversion, layer statements, nested/anonymous/comma layers,
  animation, script, and browser-wide whitespace conformance remain outside
  the contract.

### Existing owners preserved

Resolved values continue through the existing inherited style, whitespace
collapse, hard-break, soft-wrap, line-height, fixed-cell text, overflow,
scrolling, hit-testing, display-list, capture, and raster owners. No public
layout/display-list schema, raster geometry, dependency, feature default, or
crate boundary changes are permitted.

The slice remains a horizontal-tb, fixed-cell, bounded local-content contract;
it does not claim browser parity, full CSS cascade semantics, or a complete
browser engine.

## Tradeoffs

- A private declaration enum and per-layer candidate array add a small amount
  of property-local code. This preserves the public finite mode type and avoids
  introducing generic CSS-wide origin/importance machinery prematurely.
- The slice exercises rollback across all currently supported whitespace modes
  in one flow-owner test. That makes line behavior easier to audit, while
  keeping the implementation limited to declaration selection rather than new
  whitespace semantics.
- The existing omitted-value defaults and parent walk remain unchanged, so
  explicit rollback remains distinguishable from omission and cannot silently
  change current fixtures.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The focused gate must cover:

- case-insensitive standalone parsing and typed rejection of other CSS-wide,
  mixed, and malformed forms;
- named-layer ordering before specificity, repeated rollback,
  unlayered/inline precedence, inherited descendants, and root fallback;
- preservation of `normal`, `pre-line`, `pre`, `pre-wrap`, and `nowrap` line
  behavior, including hard breaks, literal whitespace, and fixed-cell wrapping;
- no false unsupported-value diagnostics and unchanged layout/display-list,
  overflow, capture, and decoded-raster evidence.

Targeted checks follow the completed behavioral unit. Full native,
two-crate, strict, package, fuzz/security, and static documentation gates are
final validation only; remote CI remains unclaimed until an explicitly
authorized push.
