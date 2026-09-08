---
id: native-engine-217
scope: glass-browser/native-engine/local-align-items-inherit
status: planned
depends-on: [native-engine-216]
---

# Native explicit `align-items: inherit`

## Objective

Add one bounded explicit inheritance path for the local `align-items` owner,
reusing the established private parent-style propagation without changing
omitted-value behavior, the two-crate boundary, or the existing `align-self`
override path.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-216.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- The bounded local `align-items` owner accepts standalone,
  case-insensitive `inherit` in addition to its existing finite values,
  CSS-wide reset forms, and `revert-layer`.
- A winning `align-items: inherit` resolves to the computed `align-items`
  value supplied by the parent-style chain. At the root, or when a direct unit
  computation has no supplied parent value, the existing bounded `FlexStart`
  initial fallback remains the result.
- Omitted `align-items` remains non-inherited and therefore resolves to
  `FlexStart` on a child even when its parent uses `center`, `flex-end`,
  `stretch`, or `normal`; only the explicit `inherit` keyword opts into
  propagation. Invalid mixed forms and invalid later declarations preserve the
  preceding valid candidate. Existing important/source-order, finite
  cross-axis placement, `align-self` overrides, and `revert-layer` behavior
  remain unchanged.
- The inherited value continues through nested flex cross-axis placement,
  explicit/auto line sizing, wrapping, flex sizing, display-list, fixed-cell
  raster, PNG, point-hit, semantic, and diagnostic consumers. No public
  computed-style field or layout schema is added.

## Boundary and tradeoffs

- Add only the `align-items` value to `NativeInheritedStyle` and thread it
  through the existing document ancestor walk. `align-self` remains a local
  item override and is not made inherited; `align-content` and
  `justify-content` inheritance remain their separate completed owners.
- Explicit `inherit` copies the already computed parent value; it does not
  copy declarations, bypass the cascade, or make finite values implicitly
  inherited. The bounded root fallback is deterministic but is not a claim of
  browser-wide CSS conformance.
- Percentages, baseline/safe/unsafe forms, additional origins, transitions,
  animations, generic CSS-wide machinery, `place-items`, grid, browser parity,
  security boundaries, and remote CI remain outside the slice.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on standalone case-insensitive `inherit`, the
  supplied-parent value, root/default fallback, omitted-property
  non-inheritance, invalid mixed/later forms, `!important`, source order,
  finite values, and `revert-layer`.
- Run one public nested-flex fixture through parent-to-child computed-style
  propagation, cross-axis placement, explicit/auto line sizing, `align-self`
  overrides, flex sizing, display-list, fixed-cell raster/PNG, point-hit,
  semantic consumers, and typed diagnostics for excluded mixed/unsupported
  forms.
- Near issue completion retain the full native integration, feature-library,
  strict lint, rustdoc, paired two-crate, package, security/fuzz, static
  documentation, workspace all-target/all-feature, clean-install, issue-sync,
  and bounded cleanup gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Cleanup

Only exact task-specific regenerable targets and reports may be removed after
Cargo/Rust process and open-handle checks. Source, durable data, repository
targets, issue snapshots, and unrelated workloads remain untouched.

## Completion evidence

This design checkpoint is intentionally incomplete until implementation,
focused tests, full native integration, and the issue-level documentation
closeout are recorded. Remote CI, push, release, tag, registry publication,
browser-parity, security-boundary certification, and promotion remain outside
this local task.
