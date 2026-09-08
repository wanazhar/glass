---
id: native-engine-218
scope: glass-browser/native-engine/local-align-self-inherit
status: complete
depends-on: [native-engine-217]
---

# Native explicit `align-self: inherit`

## Objective

Add one bounded explicit inheritance path for the local `align-self` owner,
reusing the private parent-style propagation while preserving the distinction
between an explicit inherited item override and `align-self:auto` delegation
to the containing flex parent's `align-items`.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-217.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- The bounded local `align-self` owner accepts standalone, case-insensitive
  `inherit` in addition to its existing `auto|flex-start|center|flex-end|
  stretch|normal` values, CSS-wide reset forms, and `revert-layer`.
- A winning `align-self: inherit` resolves to the computed `align-self` value
  supplied by the parent-style chain. At the root, or when a direct unit
  computation has no supplied parent value, the existing bounded `Auto`
  fallback remains the result.
- Omitted `align-self` remains local and resolves to `Auto`; it does not copy
  the parent's computed value. Explicit `auto` continues to delegate to the
  containing flex parent's `align-items`, while an inherited finite value
  remains an explicit item-level cross-axis override. Invalid mixed forms and
  invalid later declarations preserve the preceding valid candidate. Existing
  important/source-order, `revert-layer`, `align-items` interaction, and
  complete-subtree artifact movement remain unchanged.
- The resolved value continues through flex cross-axis placement, explicit and
  auto line sizing, wrapping, flex sizing, display-list, fixed-cell raster,
  PNG, point-hit, semantic, and diagnostic consumers. No public computed-style
  field or layout schema is added.

## Boundary and tradeoffs

- Add only the `align-self` value to `NativeInheritedStyle` and thread it
  through the existing document ancestor walk. `align-items` remains the
  containing flex parent's separate owner; `align-content` and
  `justify-content` inheritance remain their completed owners.
- Explicit `inherit` copies the already computed parent `align-self` value; it
  does not copy declarations, bypass the cascade, or make finite values
  implicitly inherited. The bounded root fallback is deterministic but is not
  a claim of browser-wide CSS conformance.
- Percentages, baseline/safe/unsafe forms, additional origins, transitions,
  animations, generic CSS-wide machinery, `place-items`, grid, browser parity,
  security boundaries, and remote CI remain outside the slice.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on standalone case-insensitive `inherit`, the
  supplied-parent value, root/default fallback, omitted-property local `Auto`,
  explicit `auto` delegation, invalid mixed/later forms, `!important`, source
  order, finite values, and `revert-layer`.
- Run one public three-level nested-flex fixture through parent-to-child
  computed-style propagation, `align-items` interaction, complete-subtree
  movement, flex sizing, display-list, fixed-cell raster/PNG, point-hit,
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

Implementation is `52adc7d1`; the design checkpoint is `ca9afd5f`. The locked
native-feature test-target check passed before tests. Focused `align-self`
parser/cascade coverage passed 4 tests, the public reset and inheritance
fixtures passed 5 tests, full native integration passed `256/256`, and the
serial native-feature library passed `1,039` tests with 1 ignored. The fixtures
cover standalone case-insensitive inheritance, supplied-parent propagation,
root/default fallback, omitted-property local `auto`, explicit `auto`
delegation, mixed-invalid preservation, important/source order, finite
cross-axis placement, complete-subtree movement, display-list, fixed-cell
raster/PNG, point-hit, semantic ordering, and typed unsupported-value
diagnostics. No public schema, dependency, feature default, or crate-boundary
change was made.

Workspace all-target/all-feature checking, strict two-crate Clippy, paired
browser/dev builds, warning-denied rustdoc for both crates, formatting, and diff
checks passed locally. The final static evidence also passed: release-document
truth `632 Markdown / 83 current / 59 previous-version / 780 semantic / 0
current-claim failures`, documentation coverage `632 Markdown / 345 full MCP
tools / 100 browser-only / 17 examples / 22 public modules`, feature parity
`14 capabilities across 4 targets`, TUI shortcuts `15 implementation keys /
63 documentation markers`, documentation depth `93 current guides / 19
substantive contracts`, version `0.3.14`, reliability `6 scenarios across 4
targets`, public read-only adapters `5`, knowledge migration `6 records`, and
Web IR `8 fixtures / 8 scenarios / 11 categories`. Issue #40 remains the
external source-of-truth record for this checkpoint and is synchronized after
this local closeout commit. Remote CI, push, release, tag, registry
publication, browser-parity, security-boundary certification, and promotion
remain outside this local task.
