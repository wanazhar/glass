---
id: native-engine-148
scope: glass-browser/native-engine/cascade-layers-opacity-revert-layer
status: complete
depends-on: [native-engine-147]
---

# Native bounded `opacity: revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven through native-engine-147 to
add standalone, case-insensitive `revert-layer` to the existing local bounded
`opacity` owner. A rollback must expose the highest-priority lower concrete
alpha, or the existing full-opacity fallback, while preserving reduced-opacity
display-list groups, software compositing, layout, point-hit, capture, and
semantic/source-order behavior.

## Context

The native engine already parses finite opacity numbers and percentages into a
quantized 8-bit local value. Reduced-opacity elements are represented by the
existing immutable begin/end display-list markers and composited through the
bounded software rasterizer; opacity is not inherited and does not alter layout
or semantic visibility. The stylesheet and inline paths currently retain one
concrete winner, so a higher-priority rollback cannot expose a lower alpha or
the established full-opacity default. This slice adds only private candidate
state and reuses the bounded 15-layer registry and local resolver.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-color-4/#transparency>
- <https://www.w3.org/TR/css-color-4/#propdef-opacity>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-147.md`
- `docs/plan/tasks/native-engine-051.md`

## Contract

### Declaration and cascade state

- `opacity` accepts one standalone, case-insensitive `revert-layer` token in
  addition to the existing bounded finite number/percentage grammar. The
  rollback is private and cannot reach public computed-style fields,
  display-list commands, capture bytes, or software-raster replay as an
  unresolved keyword.
- Each matching stylesheet rule and inline declaration contributes one
  bounded candidate to the existing local owner. A winning rollback blocks
  only its current bounded layer and resolves through the highest-priority
  remaining concrete value; repeated rollback continues through lower
  candidates. Unlayered and inline declarations remain above named layers.
- The local fallback is full opacity (`255`). Concrete values retain the
  existing three-decimal quantization and percentage conversion, including
  `0`, `1`, and `100%`.
- Existing first-appearance named-layer order remains authoritative.
  Specificity and source order decide ties inside a layer. Valid declarations
  before malformed declarations in one block retain the existing valid
  candidate. `!important` stripping remains in force and is not promoted to a
  separate origin.

### Existing owners preserved

- Only resolved opacity controls whether the existing begin/end opacity-group
  markers are emitted. `255` retains the no-group/full-opacity behavior;
  reduced values retain the existing transparent-layer compositing path.
- Layout boxes, normal flow, overflow, point hit testing, semantic visibility
  and source order, display-list command shapes, capture dimensions, and
  raster bounds remain unchanged apart from the selected alpha/group marker.
- No public computed-style field, display-list command, raster schema,
  diagnostic transport, dependency, feature default, or crate boundary
  changes.

The slice remains fixture-relative, horizontal-tb, and bounded. It does not
add inherited opacity, stacking-context or blending parity, filters, masks,
transforms, animation, multiple origins, or browser-wide CSS opacity
conformance.

## Tradeoffs

- Reusing `LocalCascadeDeclaration<u8>` and the optional local resolver keeps
  the rollback algorithm small and makes the full-opacity fallback explicit,
  while preserving the current public `Option<u8>` storage.
- Keeping opacity local avoids changing inheritance or group ownership, but
  does not model the full CSS stacking-context/compositing interaction.
- The integration test exercises reduced, zero, and full opacity through group
  markers and decoded pixels while checking layout/hit/semantic stability. It
  is broader than parser-only coverage but protects the existing shared
  artifact boundary.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- standalone case-insensitive parsing and typed rejection of other CSS-wide,
  mixed, malformed, and unsupported forms for `opacity`;
- named-layer priority, repeated rollback, unlayered/inline precedence,
  full-opacity fallback, quantization, zero/reduced/full values, and
  valid-before-invalid preservation;
- opacity-group marker selection and software compositing after concrete
  selection and rollback, with unchanged layout, point hit testing, capture,
  overflow, and semantic/source order; and
- no false unsupported-value diagnostics or public rollback leakage, plus
  focused `glass-browser` check, targeted behavioral tests, full native
  integration/library tests, strict affected-package Clippy, formatting, and
  final static documentation gates. Remote CI remains unclaimed until an
  explicitly authorized push.

## Implementation

Implemented in `d3f89a6c` with design contract checkpoint `d882d846`.
The source change is confined to
`crates/glass-browser/src/browser/native_engine/css.rs`, with behavioral
coverage in `crates/glass-browser/tests/native_engine.rs`.

## Evidence

Local certification passed: focused locked `glass-browser` check; opacity
parser/cascade unit coverage passed 5/5; the targeted integration regression
passed 1/1 with 185 tests filtered; full native integration passed 186/186;
the feature-enabled `glass-browser` library passed 958 tests with 1 ignored
under `RUST_MIN_STACK=16777216`; warnings-denied affected-package Clippy and
formatting passed. The default library test harness also exposed the known
stack-sensitive `agent_readiness_commands_are_explicit` overflow; the same
package gate passed with the established larger test stack. Synchronized
static documentation gates passed with 562 Markdown documents, 83 current
documents, 57 previous-version hits, 656 semantic-audit hits, and 0
current-claim failures; documentation coverage passed with 562 Markdown files,
345 full-product MCP tools (100 browser-only), 17 examples, and 22 public
modules; depth passed with 93 guides and 19 substantive contracts; parity
passed for 14 capabilities across 4 targets; TUI passed at 15 implementation
help keys/63 documentation markers; adapters passed at 5; reliability passed
at 6 scenarios across 4 targets; and Web IR passed at 8 fixtures/8
scenarios/11 categories. Remote CI is unclaimed because no push was
authorized. The complete semantic audit report is retained at
`/tmp/glass-release-documentation-148.json`.
