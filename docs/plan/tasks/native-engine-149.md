---
id: native-engine-149
scope: glass-browser/native-engine/cascade-layers-display-visibility-revert-layer
status: done
depends-on: [native-engine-148]
---

# Native bounded `display`/`visibility: revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven through native-engine-148 to
add standalone, case-insensitive `revert-layer` to the existing local
`display` and `visibility` owners. A rollback must expose the highest-priority
lower concrete presentation value, or the existing normal-flow/visible
fallback, while preserving the current hidden-subtree, layout, point-hit,
display-list, capture, raster, and semantic/source-order owners.

## Context

The native engine already parses the bounded `display` grammar and
`visibility:hidden|visible` into compact computed values. `display:none` and
`visibility:hidden` are consumed by the existing hidden-subtree gate before
layout, paint, hit testing, and semantic projection; `display:contents` keeps
eligible descendants in normal flow. The stylesheet and inline paths currently
retain one concrete winner, so a higher-priority rollback cannot expose a
lower state or the established `display:auto`/visible defaults. This slice
adds only private candidate state and reuses the bounded 15-layer registry and
local resolver.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-display-3/#the-display-properties>
- <https://www.w3.org/TR/css2/visuren.html#propdef-visibility>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-148.md`
- `docs/plan/tasks/native-engine-005.md`
- `docs/plan/tasks/native-engine-007.md`

## Contract

### Declaration and cascade state

- `display` accepts one standalone, case-insensitive `revert-layer` token in
  addition to the existing bounded grammar. `visibility` accepts the same
  standalone token in addition to `hidden|visible`. Mixed tokens, other
  CSS-wide keywords, and unsupported values remain typed rejections.
- Each matching stylesheet rule and inline declaration contributes one
  bounded candidate to its own local owner. A winning rollback blocks only
  its current bounded layer and resolves through the highest-priority
  remaining concrete value; repeated rollback continues through lower
  candidates. Unlayered and inline declarations remain above named layers.
- No candidate resolves to the existing `display:auto` normal-flow fallback or
  the existing visible `visibility` fallback. Concrete display and visibility
  values retain their current parsing and specificity/source-order behavior.
- Existing first-appearance named-layer order remains authoritative.
  Specificity and source order decide ties inside a layer. Valid declarations
  before malformed declarations in one block retain the existing valid
  candidate. `!important` stripping remains in force and is not promoted to a
  separate origin.

### Existing owners preserved

- The resolved display/visibility values continue to feed the same hidden gate:
  `display:none` and `visibility:hidden` exclude the element's box and
  hidden-subtree artifacts, while `display:contents` preserves eligible
  descendants. `revert-layer` never reaches a public computed value as an
  unresolved keyword.
- Layout boxes, normal flow, overflow, point hit testing, semantic visibility
  and source order, display-list command shapes, capture dimensions, and
  raster bounds remain unchanged apart from the selected display/visibility
  state and its existing hidden-gate consequence.
- No public computed-style field, display-list command, raster schema,
  diagnostic transport, dependency, feature default, or crate boundary
  changes.

The slice remains fixture-relative, horizontal-tb, and bounded. It does not
add inherited visibility, `display` decomposition, formatting-context parity,
table/ruby/flow-root details, animation, multiple origins, or browser-wide CSS
display/visibility conformance.

## Tradeoffs

- Reusing `LocalCascadeDeclaration<DisplayValue>` and
  `LocalCascadeDeclaration<VisibilityValue>` keeps the two rollback owners
  independent while making their normal-flow/visible fallbacks explicit.
- Keeping both properties local matches the existing native semantics and
  avoids changing inheritance or hidden-state ownership, but does not model
  full CSS formatting-context or inherited visibility behavior.
- The integration regression exercises visible, hidden, `display:contents`,
  rollback, and no-candidate fallback through layout, hit testing,
  display-list/semantic projection, and decoded pixels. It protects the
  shared hidden-subtree boundary without claiming full CSS conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- standalone case-insensitive parsing and typed rejection of other CSS-wide,
  mixed, malformed, and unsupported forms for both properties;
- named-layer priority, repeated rollback, unlayered/inline precedence,
  `display:auto`/visible fallback, valid-before-invalid preservation, and
  `!important` stripping;
- hidden-subtree behavior for concrete and rolled-back `display:none`,
  `visibility:hidden`, and `display:contents`, with unchanged normal-flow
  geometry, point hit testing, capture, raster, semantic/source order, and
  display-list owners; and
- no false unsupported-value diagnostics or public rollback leakage, plus
  focused `glass-browser` check, targeted behavioral tests, full native
  integration/library tests, strict affected-package Clippy, formatting, and
  final static documentation gates. Remote CI remains unclaimed until an
  explicitly authorized push.

## Implementation

Implemented in `6a7dc305`; the diagnostics compatibility follow-up is
`3072f6e5`. The implementation keeps `display` and `visibility` as independent
private local candidate arrays, reuses the bounded 15-layer resolver, and adds
no public computed-style, display-list, raster, diagnostic transport,
dependency, feature-default, or crate-boundary changes.

## Evidence

Local certification completed:

- `CARGO_TARGET_DIR=/tmp/glass-149-focused cargo check -q -p glass-browser --features native-engine --tests --locked` passed.
- Focused parser/cascade unit: 1 passed, 959 filtered.
- Focused integration: 1 passed, 186 filtered.
- Full native integration: 187 passed, 0 failed.
- Affected library with `RUST_MIN_STACK=16777216`: 959 passed, 1 ignored.
- Strict affected-package Clippy with `-D warnings` passed.
- Feature rustdoc with `RUSTDOCFLAGS=-Dwarnings` passed.
- `cargo fmt --all` and `git diff --check` passed.
- Static gates passed: release documentation 563 Markdown files, 83 current
  documents, 57 previous-version hits, 656 semantic-audit hits, and 0
  current-claim failures; feature parity 14 capabilities across 4 targets;
  TUI 15 implementation help keys/63 documentation markers; documentation
  depth 93 guides/19 substantive contracts; reliability 6 scenarios across 4
  targets; read-only adapters 5; Web IR 8 fixtures/8 scenarios/11
  categories; and documentation coverage 563 Markdown files, 345 full-product
  MCP tools (100 browser-only), 17 examples, and 22 public modules.
- Remote CI remains unclaimed because the checkout is local-only.

## Cleanup

After all code and documentation gates, the exact non-symlink target
`/tmp/glass-149-focused` was inventoried at 6,730,055,680 bytes with 9,874
files and 1,204 directories. No Cargo, rustc, rustdoc, or Clippy process and
no open handle referenced it. It was removed with the bounded
`find -P /tmp/glass-149-focused -xdev -depth -delete` operation and verified
absent; the repository `target` directory also remained absent. Available
space increased from 77,131,821,056 to 83,861,868,544 bytes, a
6,730,047,488-byte filesystem delta. Documentation coverage passed immediately
before this cleanup-only task evidence edit; the final static documentation
pass follows this edit.
