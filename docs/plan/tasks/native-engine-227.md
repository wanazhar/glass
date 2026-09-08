---
id: native-engine-227
scope: glass-browser/native-engine/order-inherit
status: planned
depends-on: [native-engine-226]
---

# Native explicit `order: inherit`

## Objective

Expose the bounded CSS-wide inheritance path for the direct flex-item
`order` property. A standalone, case-insensitive `inherit` must copy the
computed parent order through the existing private ancestor-style chain while
preserving finite signed item ordering, reset/rollback semantics, and the
separation between visual order and semantic/source order.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-226.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- The bounded `order` property accepts standalone, case-insensitive `inherit`
  in addition to its existing signed finite range, local reset, and
  `revert-layer` forms.
- A winning `order: inherit` resolves from the already computed parent order
  component carried by the private inherited-style chain. At the root, or when
  a direct unit computation has no supplied parent value, the existing bounded
  `0` fallback remains the result.
- Omitted `order` remains local and resolves to `0`; finite signed values remain
  explicit. Mixed or token-bearing forms such as `inherit 1`, `1 inherit`, or
  `inherit !important` after terminal marker extraction remain invalid and do
  not replace a preceding valid candidate. Existing source order, important
  priority, reset semantics, and `revert-layer` rollback remain unchanged.
- The resolved order continues through stable visual `(order, source_index)`
  sorting, row/column and wrapped placement, gap/margin mapping, complete
  subtree movement, display-list, fixed-cell raster, PNG, point-hit, semantic
  source order, and diagnostics. No public computed-style field, layout schema,
  dependency, feature default, or crate boundary is added.

## Boundary and tradeoffs

- Reuse the existing private order resolver and ancestor traversal. Do not add a
  public visual-order field, reorder semantic/source traversal, or create a
  second inherited-style object.
- Explicit `inherit` copies the computed parent order value, not the parent's
  declaration. Omitted `order` remains non-inherited. The bounded root fallback
  is deterministic but is not a claim of browser-wide CSS conformance.
- Percentages, additional origins, transitions, animations, generic CSS-wide
  machinery, grid ordering, browser parity, security boundaries, and remote CI
  remain outside this slice.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on standalone case-insensitive `inherit`,
  supplied parent signed order, root/default fallback, omitted local fallback,
  finite signed values, invalid token-bearing forms, `!important`, source
  order, reset handling, and `revert-layer` preservation.
- Run one public fixture through visual order sorting, complete subtree
  movement, row/column or wrapped placement, display-list, fixed-cell
  raster/PNG, point-hit, semantic/source order preservation, and typed
  diagnostics for excluded forms.
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

Pending implementation and local certification. Remote CI, push, release, tag,
registry publication, browser-parity, security-boundary certification, and
promotion remain outside this local task.
