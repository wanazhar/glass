---
id: native-engine-212
scope: glass-browser/native-engine/local-align-self-css-wide-resets
status: complete
depends-on: [native-engine-211]
---

# Native align-self CSS-wide resets

## Objective

Extend the existing bounded local `align-self` owner with standalone CSS-wide
reset keywords without changing parent `align-items` resolution, explicit item
overrides, or the two-crate boundary.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-211.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`

## Contract

- The bounded non-inherited `align-self` owner accepts standalone,
  case-insensitive `initial`, `unset`, and one-author-origin `revert`, in
  addition to its existing finite item values and `revert-layer`.
- Each reset form resolves through the existing local resolver to its bounded
  `Auto` fallback. `Auto` continues to delegate to the parent `align-items`
  placement path; a winning reset is terminal for the local declaration,
  invalid later declarations preserve the preceding valid declaration, and
  important/source order plus named-layer `revert-layer` behavior remain
  unchanged.
- The resolved value continues through existing parent/item cross-axis
  placement, complete-subtree artifact translation, flex sizing, display-list,
  fixed-cell raster, PNG, point-hit, semantic, and diagnostic consumers. No
  public computed-style field or layout schema is added.

## Boundary and tradeoffs

- Reuse the existing private `align-self` candidate array and resolver. Only
  this local parser and fallback path changes; parent `align-items`,
  `align-content`, `place-content`, inherited `direction`, and unrelated CSS
  parsers remain stable.
- Keeping `inherit`, percentages, baseline/safe/unsafe forms, additional
  origins, transitions, animations, generic CSS-wide machinery, and
  browser-wide alignment conformance outside the slice avoids implying parent
  propagation or a general cascade rewrite.
- Resetting to `Auto` preserves the current omitted-value behavior and the
  parent-controlled placement contract. It does not turn `align-self` resets
  into a second parent-value copy or add a public inheritance model.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive reset forms, the bounded
  `Auto` fallback, parent delegation, terminal reset behavior, invalid-later
  preservation, `!important`, source order, finite values, and `revert-layer`.
- Run one public fixture through parent/item cross-axis placement, complete
  subtree movement, flex sizing, display-list, fixed-cell raster/PNG,
  point-hit/semantic consumers, and typed unsupported-value diagnostics for
  excluded forms.
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

Implementation is `0a624642`; the design checkpoint is `68f42ebe`. The locked
native-feature check passed in an isolated task target. Focused flex parser and
cascade coverage passed 30 tests plus a dedicated align-self cascade test, the
public reset fixture passed, full native integration passed 250/250, and the
serial native-feature library passed 1,033 tests with one ignored. Workspace
all-target/all-feature checking, strict Clippy, paired browser/dev builds,
warning-denied rustdoc for both crates, formatting, and diff checks passed
locally. The fixture covers parent `align-items` delegation, finite item
overrides, important/source order, invalid-later preservation, complete-subtree
movement, flex sizing, display-list, fixed-cell raster/PNG, point-hit,
semantics, and unsupported-value diagnostics. No public schema, dependency,
feature default, or crate-boundary change was made.

The preceding task's six-target nightly fuzz certification remains current
because this slice changes only CSS flex parsing/cascade. Package and security
evidence is retained from the preceding dependency-stable task until the next
issue-level validation boundary. Remote CI, push, release, tag, registry
publication, browser-parity, security-boundary certification, and promotion
remain outside this local task.
