---
id: native-engine-147
scope: glass-browser/native-engine/cascade-layers-border-radius-revert-layer
status: complete
depends-on: [native-engine-146]
---

# Native bounded `border-radius: revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven through native-engine-146 to
add standalone, case-insensitive `revert-layer` to the existing local bounded
`border-radius` shorthand. A rollback must expose the highest-priority lower
concrete declaration, or the existing zero-corner fallback, while preserving
the rounded fill, border, point-hit, capture, raster, overflow, and
semantic/source-order owners.

## Context

The native engine already parses one-to-four non-negative integer-pixel
`border-radius` values, expands them to the four physical corners, and shares
that normalized `NativeBorderRadius` value across rounded fill, border, and
point-hit behavior. Its stylesheet and inline paths currently retain one
concrete winner, so a higher-priority rollback cannot expose a lower radius or
the established zero-corner fallback. This slice adds only private candidate
state and reuses the bounded 15-layer registry and optional local resolver; it
does not add a second radius representation or a new geometry owner.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-backgrounds-3/#border-radius>
- <https://www.w3.org/TR/css-backgrounds-3/#propdef-border-radius>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-146.md`
- `docs/plan/tasks/native-engine-020.md`

## Contract

### Declaration and cascade state

- `border-radius` accepts one standalone, case-insensitive `revert-layer`
  token in addition to the existing bounded one-to-four-value integer-pixel
  shorthand. The rollback is private and cannot reach public computed-style
  fields, display-list commands, capture bytes, or software-raster replay as
  an unresolved keyword.
- Each matching stylesheet rule and inline declaration contributes one
  bounded candidate to the existing local owner. A winning rollback blocks
  only its current bounded layer and resolves through the highest-priority
  remaining concrete value; repeated rollback continues through lower
  candidates. Unlayered and inline declarations remain above named layers.
- The local fallback is `NativeBorderRadius::default()`, with all four corners
  zero. A lower concrete declaration retains the current one-to-four-value
  expansion and conservative normalization behavior.
- Existing first-appearance named-layer order remains authoritative.
  Specificity and source order decide ties inside a layer. Valid declarations
  before malformed declarations in one block retain the existing valid
  candidate. `!important` stripping remains in force and is not promoted to a
  separate origin.

### Existing owners preserved

- The resolved `NativeBorderRadius` continues through the current layout
  metadata and shared rounded fill and border display-list commands.
- The same resolved corners continue to drive rounded point-hit behavior and
  the existing capture, software-raster, ancestor-clip, overflow, and
  semantic/source-order paths.
- No public computed-style field, display-list command, raster schema,
  diagnostic transport, dependency, feature default, or crate boundary
  changes.

The slice remains fixture-relative, horizontal-tb, and bounded. It does not
add elliptical or percentage radii, slash-separated syntax, corner longhands,
nested rounded clips, anti-aliasing, transforms, animation, multiple origins,
or browser-wide CSS `border-radius` conformance.

## Tradeoffs

- Reusing `LocalCascadeDeclaration<NativeBorderRadius>` and the optional local
  resolver keeps the rollback algorithm small and makes the zero-corner
  fallback explicit, while one candidate array preserves the existing single
  shorthand owner.
- Keeping the current shorthand parser and normalization avoids a second
  geometry representation and preserves all existing rounded consumers, but
  leaves elliptical, percentage, and corner-longhand forms outside the
  contract.
- The integration test deliberately exercises display-list, raster, hit-test,
  clipping, and semantic preservation together. This costs a wider focused
  test than parser-only coverage, but guards the shared geometry owner against
  rollback-specific divergence.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- standalone case-insensitive parsing and typed rejection of other CSS-wide,
  mixed, malformed, and unsupported forms for `border-radius`;
- named-layer priority, repeated rollback, unlayered/inline precedence,
  zero-corner fallback, one-to-four-value expansion/normalization, and
  valid-before-invalid preservation;
- rounded layout metadata, fill and border display-list commands, software
  raster corner/interior behavior, point hit testing, ancestor clipping,
  capture, overflow, and semantic/source order after concrete selection and
  rollback; and
- no false unsupported-value diagnostics or public rollback leakage, plus
  focused `glass-browser` check, targeted behavioral tests, full native
  integration/library tests, strict affected-package Clippy, formatting, and
  final static documentation gates. Remote CI remains unclaimed until an
  explicitly authorized push.

## Implementation

- Design checkpoint: `f76720f775ed1204f982734a59e070b7bb6ee203`.
- Source checkpoint: `74cc1cf9783b67487b585c1ea54b283e6e66b39c`.
- The source checkpoint updates `css.rs` and `tests/native_engine.rs` only; the
  public computed-style and artifact schemas, feature defaults, dependencies,
  and two-crate boundary remain unchanged.
- Product and authoritative planning documentation is synchronized in this
  closeout checkpoint.

## Evidence

- Focused locked `glass-browser` check passed.
- Border-radius parser/cascade unit tests passed 3/3.
- Targeted native integration passed 1/1 with 184 filtered.
- Full native integration passed 185/185.
- Feature-enabled `glass-browser` library tests passed 957, with 1 ignored.
- Strict affected-package Clippy passed with warnings denied.
- Locked isolated `glass-dev` and `glass-browser` inventory binaries passed
  check/build gates; documentation coverage was run with explicit binary paths
  and passed with 561 Markdown files, 345 full-product MCP tools (100
  browser-only), 17 examples, and 22 public modules.
- Static truth gates passed: version sync at 0.3.14; release documentation
  with 561 Markdown files, 83 current-document records, 57 previous-version
  hits, 656 semantic-audit hits, and zero current-claim failures;
  documentation-depth with 93 current guides and 19 substantive contracts;
  feature parity for 14 capabilities across 4 targets (baseline 0.3.0, next
  0.3.14, checkout 0.3.14); TUI shortcut parity with 15 implementation help
  keys and 63 documentation markers; 5 public read-only adapters; reliability
  with 6 scenarios across 4 targets; and Web IR with 8 fixtures, 8 scenarios,
  and 11 categories.
- The semantic audit report is retained at
  `/tmp/glass-release-documentation-147.json`; it contains all 656 entries,
  classified as 164 current, 338 historical, and 154 record hits, with zero
  unresolved current claims.
- Feature-enabled `glass-browser` rustdoc passed with warnings denied and no
  dependencies.
- Formatting and diff checks passed.

- After all checks completed, no Cargo/Rust process or open handle referenced
  the exact regenerable target. `/tmp/glass-147-focused` measured
  5,312,073,728 bytes (8,962 files, 1,179 directories) and was removed with
  bounded same-filesystem deletion; the exact path is absent.
  `/home/ubuntu/work/glass/target` remained absent. `/tmp` availability
  increased from 78,562,713,600 to 83,874,627,584 bytes, reclaiming
  5,311,913,984 bytes. The complete semantic audit JSON remains at
  `/tmp/glass-release-documentation-147.json` (181,388 bytes).
