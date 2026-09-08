---
id: native-engine-214
scope: glass-browser/native-engine/local-place-content-css-wide-resets
status: complete
depends-on: [native-engine-213]
---

# Native place-content CSS-wide resets

## Objective

Extend the existing bounded local `place-content` shorthand projection with
standalone CSS-wide reset keywords without changing its finite two-component
expansion, independent component cascade, or the two-crate boundary.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-213.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`

## Contract

- The bounded `place-content` owner accepts standalone, case-insensitive
  `initial`, `unset`, and one-author-origin `revert`, in addition to its
  existing finite one- or two-value line-distribution/justification forms and
  `revert-layer`.
- A winning shorthand reset projects to the existing private component
  fallbacks: `align-content: flex-start` and `justify-content: flex-start`.
  The projection remains terminal for both local components; invalid later
  declarations preserve the preceding valid shorthand, while
  important/source order and named-layer `revert-layer` behavior remain
  unchanged.
- The resolved component values continue through existing wrapped-line
  distribution, main-axis free-space placement, item alignment, flex sizing,
  display-list, fixed-cell raster, PNG, point-hit, semantic, and diagnostic
  consumers. No public computed-style field or layout schema is added.

## Boundary and tradeoffs

- Reuse the existing private shorthand parser and component candidate arrays.
  Only standalone reset projection changes; finite one-/two-value expansion,
  source-order interaction with longhands, and independent component
  resolution remain stable.
- Mixed reset/finite tokens, `inherit`, percentages, baseline/safe/unsafe
  forms, additional origins, transitions, animations, generic CSS-wide
  machinery, `place-items`, browser-wide alignment conformance, and remote CI
  remain outside the contract.
- The shorthand reset intentionally uses the current bounded initial
  fallbacks rather than copying a parent or adding a general inheritance
  model. `revert-layer` remains the distinct named-layer rollback form.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on standalone case-insensitive reset forms,
  two-component fallback projection, terminal reset behavior, invalid-later
  preservation, `!important`, source order, finite one-/two-value forms, and
  `revert-layer`.
- Run one public fixture through both main- and cross-axis placement, wrapped
  line distribution, item alignment, flex sizing, display-list, fixed-cell
  raster/PNG, point-hit/semantic consumers, and typed diagnostics for excluded
  mixed/unsupported forms.
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

Implementation is `24ae15a7`; the design checkpoint is `9f26239d`. The locked
native-feature test-target check passed in the isolated task target. Focused
flex parser/cascade coverage passed 31 tests plus a dedicated place-content
cascade test, the public two-axis reset fixture passed, full native integration
passed 252/252, and the serial native-feature library passed 1,035 tests with
one ignored. Workspace all-target/all-feature checking, strict Clippy, paired
browser/dev builds, warning-denied rustdoc for both crates, formatting, and
diff checks passed locally. The fixture covers standalone case-insensitive
reset parsing, two-component `flex-start` fallback projection, terminal reset,
invalid-later preservation, `!important`, source order, named-layer
`revert-layer`, wrapped-line distribution, main-axis placement, item
alignment, flex sizing, display-list, fixed-cell raster/PNG, point-hit,
semantics, and typed unsupported-value diagnostics for excluded mixed/invalid
forms. No public schema, dependency, feature default, or crate-boundary change
was made.

The preceding task's six-target nightly fuzz certification remains current
because this slice changes only CSS flex shorthand parsing/cascade. Package and
security evidence is retained from the preceding dependency-stable task until
the next issue-level validation boundary. Remote CI, push, release, tag,
registry publication, browser-parity, security-boundary certification, and
promotion remain outside this local task.
