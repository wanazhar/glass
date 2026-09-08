---
id: native-engine-223
scope: glass-browser/native-engine/flex-shorthand-inherit
status: planned
depends-on: [native-engine-222]
---

# Native explicit `flex: inherit`

## Objective

Add the bounded CSS-wide inheritance path for the existing `flex` shorthand.
A standalone explicit `inherit` must copy the computed parent grow, shrink,
and basis components through the existing private ancestor-style chain while
preserving finite shorthand expansion, longhand precedence, reset/rollback
semantics, and all current flex sizing and artifact consumers.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-208.md`
- `docs/plan/tasks/native-engine-222.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- The bounded `flex` shorthand accepts standalone, case-insensitive `inherit`
  in addition to its existing finite grow/shrink/basis forms, CSS-wide reset
  forms, and `revert-layer`.
- A winning `flex: inherit` projects inheritance to the existing private
  `flex-grow`, `flex-shrink`, and `flex-basis` component owners. Each component
  resolves from the already computed parent-style chain. At the root, or when
  a direct unit computation has no supplied parent values, the existing
  bounded `0 1 auto` fallback remains the result.
- Omitted `flex` remains local and resolves to `0 1 auto`; finite shorthand
  forms remain explicit local owners. Mixed forms such as `inherit 1 auto`,
  `1 inherit auto`, and `inherit auto` remain invalid and do not replace a
  preceding valid candidate. Existing shorthand/longhand source order,
  same-block component precedence, important/source order, reset semantics,
  and `revert-layer` rollback remain unchanged.
- The resolved tuple continues through flex base sizing, grow/shrink
  allocation, min/max constraints, row/column and wrapped layout, gap/margin
  mapping, complete-subtree movement, display-list, fixed-cell raster, PNG,
  point-hit, semantic/source order, and diagnostics. No public computed-style
  field, layout schema, dependency, feature default, or crate boundary is
  added.

## Boundary and tradeoffs

- Reuse the existing finite component resolver and add only the computed parent
  tuple to the private inherited-style chain. Do not add a separate public
  flex-style object or a second ancestor traversal.
- Explicit `inherit` copies computed component values, not declarations, and
  does not make finite flex sizing implicitly inherited. The bounded root
  fallback is deterministic but is not a claim of browser-wide CSS
  conformance.
- Percentages, intrinsic sizing, additional origins, transitions, animations,
  generic CSS-wide machinery, grid, browser parity, security boundaries, and
  remote CI remain outside this slice.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on standalone case-insensitive `inherit`, all
  three supplied parent components, root/default fallback, omitted local
  fallback, finite shorthand expansion, invalid mixed/later forms,
  `!important`, source order, longhand precedence, and `revert-layer`
  preservation.
- Run one public flex-sizing fixture through shorthand inheritance, base-size
  selection, grow/shrink allocation, min/max constraints, row/column and
  wrapped placement, complete-subtree movement, display-list, fixed-cell
  raster/PNG, point-hit, semantic/source order, and typed diagnostics for
  excluded mixed forms.
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
