---
id: native-engine-228
scope: glass-browser/native-engine/gap-inherit
status: complete
depends-on: [native-engine-227]
---

# Native explicit `gap: inherit`

## Objective

Expose the bounded CSS-wide inheritance path for the flex `gap` shorthand.
A standalone, case-insensitive `inherit` must copy the computed parent row and
column gap components through the existing private ancestor-style chain while
preserving local shorthand/longhand cascade, reset, rollback, layout, paint,
interaction, and semantic owners.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-227.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- The bounded `gap` shorthand accepts standalone, case-insensitive `inherit`
  in addition to its finite one-/two-value pixel forms and existing
  `initial|unset|revert|revert-layer` forms.
- A winning `gap: inherit` resolves the computed parent row gap into the row
  owner and the computed parent column gap into the column owner. At the root,
  or when a direct unit computation has no supplied parent values, both
  components use the existing bounded `0` fallback.
- Omitted `gap` remains local and resolves to `0`; explicit finite values and
  shorthand/longhand source-order precedence remain unchanged. Mixed or
  token-bearing forms such as `inherit 1px`, `1px inherit`, or unsupported
  longhand `row-gap: inherit` and `column-gap: inherit` remain invalid and do
  not replace a preceding valid candidate. Existing important priority,
  reset semantics, and `revert-layer` rollback remain unchanged.
- Resolved inherited values continue through the existing flex row/column and
  wrapped layout owners, gap/margin mapping, display-list, fixed-cell raster,
  PNG, point-hit, semantic/source-order, and typed-diagnostic paths. No public
  computed-style field, layout schema, dependency, feature default, or crate
  boundary is added.

## Boundary and tradeoffs

- Carry only the private computed row/column gap pair in `NativeInheritedStyle`
  and the existing DOM style walk. Do not create a public gap inheritance API,
  duplicate layout state, or reorder semantic/source traversal.
- This slice intentionally supports only the `gap` shorthand inheritance path.
  Direct `row-gap: inherit` and `column-gap: inherit`, percentages, intrinsic
  values, additional origins, transitions, animations, grid track sizing,
  generic CSS-wide machinery, browser parity, and browser-wide gap conformance
  remain outside the boundary.
- Explicit inheritance copies computed pixel components rather than source
  declarations. The root `0` fallback is deterministic but is not a claim of
  browser-wide CSS conformance.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive standalone `inherit`,
  supplied parent row/column values, root/default fallback, omitted local
  fallback, finite one-/two-value forms, invalid mixed tokens, `!important`,
  shorthand/longhand precedence, reset handling, and `revert-layer`.
- Run one public fixture through inherited row and column spacing, wrapped or
  column placement, complete artifact movement, display-list, fixed-cell
  raster/PNG, point-hit, semantic/source-order preservation, and typed
  diagnostics for excluded longhand/mixed forms.
- Near issue completion retain the full native integration, feature-library,
  strict lint, rustdoc, paired two-crate, package, security/fuzz, static
  documentation, workspace all-target/all-feature, clean-install, issue-sync,
  and bounded cleanup gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Cleanup

Only exact task-specific regenerable targets and reports may be removed after
Cargo/Rust process and open-handle checks. Source, durable data, repository
targets, issue snapshots, and unrelated workloads remain untouched.

## Completion evidence

Implementation is committed at `978ae3b5`, with the compatibility fixture
follow-up at `b645585b`. The focused gap parser/cascade group passed 11/11; the
public inherited row/column layout/artifact integration passed 1/1; the full
native integration passed 266/266; and the feature library passed 1,045 tests
with 1 ignored. Formatting, the scoped workspace check, strict Clippy, paired
crate builds, and warnings-as-errors rustdoc passed locally. Static
documentation and release audits are recorded by the issue-40 synchronization
checkpoint. Remote CI, push, release, tag, registry publication,
browser-parity, security-boundary certification, and promotion remain outside
this local task.
