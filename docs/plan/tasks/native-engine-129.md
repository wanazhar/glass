---
id: native-engine-129
scope: glass-browser/native-engine/cascade-layers-text-alignment-revert-layer
status: complete
depends-on: [native-engine-128]
---

# Native bounded text-alignment `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven by native-engine-128 to add
the explicit CSS-wide `revert-layer` keyword to the existing inherited
`text-align`, `text-align-last`, and `text-justify` owners. Keep the finite
alignment enums, logical-direction resolution, final-line justification, and
fixed-cell line/artifact geometry unchanged.

## Context

The native engine already resolves these three properties through one bounded
DOM parent-style walk and one fixed-cell inline-flow owner:

- `text-align` supports the bounded physical/logical and justification values;
- `text-align-last` supports the bounded final-line values, including
  `justify`;
- `text-justify` controls the existing ASCII-separator expansion for ordinary
  and final-line justification.

The stylesheet and inline declaration paths currently store each concrete
value directly in a single cascade slot. This slice adds private
declaration-only rollback state beside those slots and reuses the existing
15 named-layer registry plus unlayered/inline bucket. It does not add a
generic CSS-wide keyword engine or a second line-layout representation.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-text-3/#text-align-property>
- <https://www.w3.org/TR/css-text-3/#text-align-last-property>
- <https://www.w3.org/TR/css-text-3/#text-justify-property>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-128.md`

## Contract

### Declaration and cascade state

- Each property accepts one case-insensitive `revert-layer` token and stores
  it only in a private declaration/candidate state. Existing concrete values
  remain the only values exposed by `NativeComputedStyle` and downstream
  layout, display-list, capture, and raster owners.
- Stylesheet candidates use the existing bounded first-appearance named-layer
  order, with selector specificity and source order deciding ties within a
  layer. The unlayered and inline bucket remains above named layers.
- A winning rollback blocks only its current layer and selects the highest
  priority remaining candidate. Repeated rollback continues through lower
  layers and finally the inherited value; unlayered/inline rollback selects
  the highest named candidate.
- With no candidate remaining, the existing inherited defaults remain in
  force: `text-align:left`, `text-align-last:auto`, and
  `text-justify:auto` at the root. Descendants with no local candidate retain
  the existing computed parent value.
- The shorthand/longhand and final-line semantics already represented by
  these properties do not change. `text-align-last:auto` continues to resolve
  from the computed `text-align`, and `text-justify` continues to gate only
  the existing bounded separator-expansion paths.
- Other CSS-wide keywords, `all`, mixed values, multiple origins,
  `!important` inversion, layer statements, nested/anonymous/comma layers,
  animation, script, and browser-wide text conformance remain typed or
  explicitly out of scope.

### Existing owners preserved

The resolved values continue through the existing inherited style, direction
mapping, line breaking, final-line flush, immutable text metadata, display
projection, overflow, scrolling, hit testing, capture, and fixed-cell raster
owners. No layout/display-list schema, raster geometry, dependency, feature
default, or crate boundary changes are permitted.

The slice remains a horizontal-tb, fixed-cell, bounded local-content contract;
it does not claim browser parity, full CSS cascade semantics, or a complete
browser engine.

## Tradeoffs

- Three small private declaration enums/rollback resolvers repeat the proven
  property-local shape. This keeps the public finite enums stable and avoids
  prematurely introducing a generic cascade abstraction whose origin and
  importance semantics are not defined here.
- Grouping the three related alignment properties makes one line-owner change
  observable across ordinary, final-line, and justification-control paths, at
  the cost of a slightly broader focused test than a one-property slice.
- The existing omitted-value defaults and inherited parent walk remain
  unchanged, so explicit rollback is distinguishable from omission and does
  not silently alter current fixtures.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Implementation

Implemented in `d26033af` (`feat(native-engine): support alignment
revert-layer`) in `css.rs` and `tests/native_engine.rs`. The implementation
adds private declaration enums for all three inherited alignment properties,
per-layer candidate arrays, a shared bounded rollback resolver, and
case-insensitive declaration parsing. A fixture-width assertion correction was
recorded separately in `c32aeafe` (`test(native-engine): correct alignment
rollback fixture`); no production behavior changed in that correction. The
diagnostic classifier was then synchronized with the declaration parsers in
`36a0f68` (`fix(native-engine): classify alignment rollback as supported`) so
valid rollback declarations do not produce false unsupported-value reports.

The public `TextAlignValue`, `TextAlignLastValue`, and `TextJustifyValue`
enums, inherited-style shape, direction mapping, line flush, display-list,
capture, raster, layout, and two-crate boundary remain unchanged.

## Verification

The focused gate must cover:

- case-insensitive parsing and typed rejection of unsupported reset/mixed
  forms for all three properties;
- named-layer ordering before specificity, repeated rollback,
  unlayered/inline precedence, inherited descendants, and root defaults;
- interaction of `text-align-last:auto` with a rolled-back `text-align`, and
  `text-justify:none|auto|inter-word` with both ordinary and final-line
  justification;
- unchanged line coordinates, immutable text metadata, and decoded raster
  behavior for physical/logical/final-line alignment paths.

Targeted checks follow the completed behavioral unit. Full native, two-crate,
strict, package, fuzz/security, and static documentation gates are final
validation only; remote CI remains unclaimed until an explicitly authorized
push.

### Results

- `cargo check -p glass-browser --features native-engine --test
  native_engine --locked` passed in 2m14s.
- Focused parser/cascade coverage passed: the new declaration parser 1/1 and
  the existing text-alignment unit filter 5/5.
- Focused native integration coverage passed: physical/logical alignment 3/3,
  final-line alignment 2/2, and justification control 1/1. The first run
  caught an incorrect 32px fixture expectation; the fixture was corrected to
  the established 45px final-line shape and the rerun passed 1/1.
- Full affected-package tests passed: `glass-browser` library 927 passed with
  1 ignored and native integration 166/166.
- `cargo clippy -p glass-browser --features native-engine --all-targets
  --locked -- -D warnings` passed in 1m36s.
- After the diagnostic-classifier fix, the locked affected-target check passed
  and `native_text_alignment_revert_layer_preserves_inheritance_and_owner_paths`
  passed 1/1, including the no-false-diagnostic regression.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Documentation truth passed: release audit reported 543 Markdown documents,
  83 current documents, 57 previous-version references, 641 semantic audit
  hits, and 0 current-claim failures; its contract suite passed 9/9.
- Documentation coverage passed with 543 Markdown files, 345 full-product MCP
  tools, 100 browser-only MCP tools, 17 examples, and 22 public modules.
  Documentation depth passed with 93 routed/audited guides and 19 substantive
  contracts. Version sync passed at 0.3.14; feature parity passed for 14
  capabilities across 4 targets; TUI inventory passed with 15 implementation
  keys and 63 documentation markers; the read-only adapter inventory passed
  with 5 adapters; the reliability matrix passed with 6 scenarios across 4
  targets; and the Web IR corpus passed with 8 fixtures, 8 scenarios, and 11
  categories with recorded runtime goldens.
- No remote CI, push, release, tag, registry publication, or browser-parity
  claim is made by this task.

## Cleanup

The isolated `/tmp/glass-129-focused` target and exact validator logs are
regenerable outputs. The final documentation/static gate is complete; after
writer/open-handle checks they will be removed with bounded exact path
deletion. Source, fixtures, durable data, and unrelated temporary paths are
not cleanup candidates.

Final cleanup completed without terminating any process: the focused target
was measured at 5,736,705,777 bytes across 7,111 files and the 20 exact
`/tmp/glass-129-*.log` reports totaled 128,245 bytes; both exact scopes are
absent. The separate repository `target/` was also verified as Cargo output,
measured at 426,058,047 bytes across 1,739 files, and removed; `/tmp/target/`
was absent. Available filesystem bytes increased from 79,366,098,944 to
85,557,026,816, a measured delta of 6,190,927,872 bytes. No source,
fixture, durable data, or unrelated temporary path was removed.

## Certification

The implementation, affected-package verification, documentation audit, and
exact-output cleanup are complete locally at `3cb2f02c`, pending only final
issue synchronization. Remote CI remains pending because this checkout is
local-only.
