---
id: native-engine-200
scope: glass-browser/native-engine/inherited-text-decoration-style-css-wide-resets
status: planned
depends-on: [native-engine-199]
---

# Native inherited text-decoration-style CSS-wide resets

## Objective

Extend the existing bounded inherited `text-decoration-style` owner with
standalone CSS-wide reset keywords without adding generic cascade machinery,
new computed-style state, or a new crate.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-199.md`

## Contract

- Inherited `text-decoration-style` accepts standalone, case-insensitive
  `inherit`, `initial`, `unset`, and one-author-origin `revert`, in addition to
  the existing `revert-layer` and finite `solid|dashed|dotted|double|wavy`
  values.
- `inherit` and `unset` resolve to the computed parent style. In the current
  one-author-origin engine, `revert` uses the same parent fallback because no
  lower author origin exists. `revert-layer` remains the only form that walks
  lower named-layer candidates.
- `initial` resolves to the existing finite `solid` root fallback.
- A winning CSS-wide keyword is terminal for this property. Invalid later
  declarations preserve the preceding valid declaration, and existing
  important/source-order behavior plus the decoration display-list/raster
  consumers remain unchanged.
- Mixed reset tokens, unsupported decoration values, additional origins,
  browser font metrics, and generic CSS-wide machinery remain outside this
  bounded contract.

## Boundary and tradeoffs

- Reuse `InheritedTextDeclaration<NativeTextDecorationStyle>` and the existing
  bounded inherited resolver rather than introducing another public enum or
  computed-style field.
- The finite decoration style continues through the existing text decoration
  command and fixed-cell raster owner. This slice does not change decoration
  geometry, thickness, offset, skip behavior, line propagation, or paint color.
- `text-decoration-style` is intentionally isolated from the still-unsupported
  CSS-wide forms on other decoration properties so parser, cascade, and artifact
  ownership remain easy to audit.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive standalone forms, root and
  parent fallback, terminal reset behavior, invalid-later preservation,
  `!important`, source order, and the `revert-layer` distinction.
- Run one public decoration fixture through style inheritance, display-list,
  raster/PNG, and unchanged semantic/diagnostic consumers.
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

To be filled after implementation and local certification. Remote CI, push,
release, tag, registry publication, browser parity, security-boundary
certification, and promotion remain outside this local task.
