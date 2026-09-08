---
id: native-engine-199
scope: glass-browser/native-engine/inherited-alignment-css-wide-resets
status: planned
depends-on: [native-engine-198]
---

# Native inherited alignment and direction CSS-wide resets

## Objective

Extend the existing bounded inherited text-alignment and direction owners with
standalone CSS-wide keywords without adding a generic cascade engine or new
computed-style/artifact state.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-198.md`

## Contract

- Inherited `text-align`, `text-align-last`, `text-justify`, and `direction`
  accept standalone, case-insensitive `inherit`, `initial`, `unset`, and
  one-author-origin `revert`, in addition to the existing `revert-layer`.
- `inherit` and `unset` resolve to the computed parent value. In the current
  one-author-origin engine, `revert` uses the same parent fallback because no
  lower author origin exists. `revert-layer` remains the only form that walks
  lower named-layer candidates.
- `initial` resolves to the existing finite root defaults: left alignment,
  auto last-line alignment, auto text justification, and `ltr` direction.
- A winning CSS-wide keyword is terminal for its property. Invalid later
  declarations preserve the preceding valid declaration, and existing
  specificity, source order, inline-important, important-over-normal, and
  logical `ltr`/`rtl` projection behavior remain unchanged.
- Mixed reset tokens, unsupported alignment values, vertical writing modes,
  additional origins, and generic CSS-wide machinery remain outside the
  bounded contract.

## Boundary and tradeoffs

- The four private declaration types converge on the existing inherited text
  fallback resolver while public finite enums and layout/display-list/raster
  consumers remain unchanged.
- `direction` continues to feed logical border projection and flex-axis
  mapping through its existing computed owner. The slice does not add browser
  bidi, writing-mode, shaping, or full CSS direction conformance.
- Keeping `text-align-last: auto` and `text-justify: auto` as the finite
  initial values preserves the existing line-placement fallback instead of
  inventing a second used-value representation.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser coverage on case-insensitive standalone forms and mixed-token
  rejection for all four owners.
- Focus cascade coverage on parent/root fallback, terminal reset behavior,
  explicit `inherit`, invalid-later preservation, `!important`, source order,
  and the `revert-layer` distinction.
- Run one public integration fixture through alignment/direction layout,
  display-list/raster/PNG, point-hit, semantic, and diagnostic consumers.
- Near issue completion retain full native integration, feature library,
  strict lint, rustdoc, paired two-crate, package, security/fuzz, static
  documentation, workspace all-target/all-feature, and bounded cleanup gates.

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

To be filled after implementation and local certification. Remote CI, push,
release, tag, registry publication, browser parity, security-boundary
certification, and promotion remain outside this local task.
