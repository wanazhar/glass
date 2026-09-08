---
id: native-engine-225
scope: glass-browser/native-engine/flex-shrink-inherit
status: planned
depends-on: [native-engine-224]
---

# Native explicit `flex-shrink: inherit`

## Objective

Expose the bounded CSS-wide inheritance path for the direct `flex-shrink`
longhand. A standalone, case-insensitive `inherit` must copy the computed
parent shrink component through the existing private ancestor-style chain while
preserving direct finite values, shorthand projection, reset/rollback
semantics, and all current flex sizing and artifact consumers.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-224.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- The bounded `flex-shrink` longhand accepts standalone, case-insensitive
  `inherit` in addition to its existing finite, local reset, and
  `revert-layer` forms.
- A winning `flex-shrink: inherit` resolves from the already computed parent
  shrink component carried by the private inherited-style chain. At the root,
  or when a direct unit computation has no supplied parent value, the existing
  bounded `1` fallback remains the result.
- Omitted `flex-shrink` remains local and resolves to `1`; finite longhand and
  shorthand component owners remain explicit. Mixed or token-bearing forms
  such as `inherit 1`, `1 inherit`, or `inherit !important` after terminal
  marker extraction remain invalid and do not replace a preceding valid
  candidate. Existing shorthand/longhand source order, same-block component
  precedence, important/source order, reset semantics, and `revert-layer`
  rollback remain unchanged.
- The resolved shrink value continues through flex base sizing, shrink
  allocation, min/max constraints, row/column and wrapped layout, gap/margin
  mapping, complete-subtree movement, display-list, fixed-cell raster, PNG,
  point-hit, semantic/source order, and diagnostics. No public computed-style
  field, layout schema, dependency, feature default, or crate boundary is
  added.

## Boundary and tradeoffs

- Reuse the existing private shrink component and ancestor traversal introduced
  for the `flex` shorthand. Do not add a second inherited-style object or a
  separate direct-longhand cascade path.
- Explicit `inherit` copies the computed parent shrink value, not the parent's
  declaration. Omitted `flex-shrink` remains non-inherited. The bounded root
  fallback is deterministic but is not a claim of browser-wide CSS
  conformance.
- Direct `flex-basis: inherit`, additional origins, percentages, intrinsic
  sizing, transitions, animations, generic CSS-wide machinery, grid, browser
  parity, security boundaries, and remote CI remain outside this slice.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on standalone case-insensitive `inherit`,
  supplied parent shrink, root/default fallback, omitted local fallback,
  finite longhand/shorthand precedence, invalid token-bearing forms,
  `!important`, source order, reset handling, and `revert-layer` preservation.
- Run one public fixture through shrink allocation, base-size selection,
  min/max constraints, row/column and wrapped placement, complete-subtree
  movement, display-list, fixed-cell raster/PNG, point-hit, semantic/source
  order, and typed diagnostics for excluded forms.
- Near issue completion retain the full native integration, feature-library,
  strict lint, rustdoc, paired two-crate, package, security/fuzz, static
  documentation, workspace all-target/all-feature, clean-install, issue-sync,
  and bounded cleanup gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/css.rs`
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
