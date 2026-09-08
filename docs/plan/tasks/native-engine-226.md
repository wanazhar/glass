---
id: native-engine-226
scope: glass-browser/native-engine/flex-basis-inherit
status: complete
depends-on: [native-engine-225]
---

# Native explicit `flex-basis: inherit`

## Objective

Expose the bounded CSS-wide inheritance path for the direct `flex-basis`
longhand. A standalone, case-insensitive `inherit` must copy the computed
parent basis component through the existing private ancestor-style chain while
preserving direct finite lengths, `auto`, shorthand projection,
reset/rollback semantics, and all current flex sizing and artifact consumers.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-225.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- The bounded `flex-basis` longhand accepts standalone, case-insensitive
  `inherit` in addition to its existing finite fixed-pixel length, `auto`,
  local reset, and `revert-layer` forms.
- A winning `flex-basis: inherit` resolves from the already computed parent
  basis component carried by the private inherited-style chain. At the root,
  or when a direct unit computation has no supplied parent value, the existing
  bounded `auto` fallback remains the result.
- Omitted `flex-basis` remains local and resolves to `auto`; finite longhand,
  `auto`, and shorthand component owners remain explicit. Mixed or token-bearing
  forms such as `inherit 1px`, `1px inherit`, or `inherit !important` after
  terminal marker extraction remain invalid and do not replace a preceding
  valid candidate. Existing shorthand/longhand source order, same-block
  component precedence, important/source order, reset semantics, and
  `revert-layer` rollback remain unchanged.
- The resolved basis continues through flex base-size selection, grow/shrink
  allocation, min/max constraints, row/column and wrapped layout, gap/margin
  mapping, complete-subtree movement, display-list, fixed-cell raster, PNG,
  point-hit, semantic/source order, and diagnostics. No public computed-style
  field, layout schema, dependency, feature default, or crate boundary is
  added.

## Boundary and tradeoffs

- Reuse the existing private basis component and ancestor traversal introduced
  for the `flex` shorthand. Do not add a second inherited-style object or a
  separate direct-longhand cascade path.
- Explicit `inherit` copies the computed parent basis value, not the parent's
  declaration. Omitted `flex-basis` remains non-inherited. The bounded root
  fallback is deterministic but is not a claim of browser-wide CSS
  conformance.
- Percentages, intrinsic sizing, additional origins, transitions, animations,
  generic CSS-wide machinery, grid, browser parity, security boundaries, and
  remote CI remain outside this slice.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on standalone case-insensitive `inherit`,
  supplied parent basis including `auto` and a finite length, root/default
  fallback, omitted local fallback, finite longhand/shorthand precedence,
  invalid token-bearing forms, `!important`, source order, reset handling, and
  `revert-layer` preservation.
- Run one public fixture through basis selection, grow/shrink allocation,
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

Implementation is `4ee2e1ed` (design `c52398fd`). The locked native-feature
check passed before tests. Focused flex parser/cascade coverage passed (35
tests), and the public direct-basis sizing/artifact fixture passed (1 test).
Full native integration passed (264/264), and the feature library passed
(1,044 passed, 1 ignored). Formatting, workspace check, strict Clippy, paired
package builds, and warning-denied rustdoc for both crates also passed locally.
Remote CI, push, release, tag,
registry publication, browser-parity, security-boundary certification, and
promotion remain outside this local task.
