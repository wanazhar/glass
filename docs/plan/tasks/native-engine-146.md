---
id: native-engine-146
scope: glass-browser/native-engine/cascade-layers-overflow-revert-layer
status: complete
depends-on: [native-engine-145]
---

# Native bounded overflow `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven through native-engine-145 to
add standalone, case-insensitive `revert-layer` to the existing local
`overflow`, `overflow-x`, and `overflow-y` owners. The shorthand must feed both
axis candidates while longhands remain independent, preserving the existing
paint clip, viewport projection, point-hit, root-overflow, capture, and
semantic/source-order contracts.

## Context

The native engine already accepts the bounded `hidden`/`clip` overflow values
and maps either value to the same rectangular clip owner. Its stylesheet and
inline paths currently retain one concrete winner per axis, so a higher-priority
rollback cannot expose a lower clip or the established visible/no-clip
fallback. This slice adds only private declaration/candidate state and reuses
the bounded 15-layer registry and optional local resolver; it does not add
nested scrolling or a second overflow representation.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-overflow-3/#propdef-overflow>
- <https://www.w3.org/TR/css-overflow-3/#propdef-overflow-x>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-145.md`
- `docs/plan/tasks/native-engine-049.md`

## Contract

### Declaration and cascade state

- `overflow`, `overflow-x`, and `overflow-y` accept one standalone,
  case-insensitive `revert-layer` token in addition to the existing bounded
  grammar. The rollback is private and cannot reach public computed-style
  fields, display-list commands, capture bytes, or software raster replay as
  an unresolved keyword.
- A shorthand `overflow:revert-layer` supplies the same rollback candidate to
  both axis owners. `overflow-x` and `overflow-y` remain independent and can
  override one shorthand axis without changing the other.
- Each axis receives an independent bounded candidate sequence. A winning
  rollback blocks only its current bounded layer and resolves through the
  highest-priority remaining concrete value; repeated rollback continues
  through lower candidates. Unlayered and inline declarations remain above
  named layers.
- The local fallback is visible/no clip for either axis. Concrete `hidden` and
  `clip` continue to map to the same internal clip bit, while recognized
  `visible`, `auto`, and `scroll` remain typed unsupported values with the
  existing no-clip behavior and diagnostics.
- Existing first-appearance named-layer order remains authoritative.
  Specificity and source order decide ties inside a layer; valid declarations
  before malformed declarations in one block retain the existing valid
  candidate. `!important` stripping remains in force and is not promoted to a
  separate origin.

### Existing owners preserved

- Resolved axis clip bits continue through the existing document-space
  ancestor clip intersection used by fill/text paint, viewport projection, and
  point hit testing.
- Root horizontal/vertical overflow derivation, scroll offsets, capture,
  opacity, display-list/raster replay, and semantic/source-order behavior remain
  unchanged.
- `overflow:hidden` and `overflow:clip` remain equivalent within this bounded
  owner. No nested scroll container, scrollbars, scroll chaining, smooth
  scrolling, or public overflow value is introduced.
- No public computed-style field, display-list command, raster schema,
  diagnostic transport, dependency, feature default, or crate boundary changes.

The slice remains fixture-relative, horizontal-tb, and bounded. It does not
add nested scrolling, scrollbars, visible-overflow used-value parity, general
`auto`/`scroll` behavior, multiple origins, animation, script, or browser-wide
CSS overflow conformance.

## Tradeoffs

- Reusing `LocalCascadeDeclaration<OverflowValue>` and the optional local
  resolver keeps the rollback algorithm small and makes the no-clip fallback
  explicit, while two candidate arrays preserve independent axis behavior.
- Expanding the shorthand at parse time preserves the current declaration
  boundary and keeps source-order precedence predictable, but it means this
  slice does not model a later CSS shorthand reset beyond the supported
  `hidden`/`clip`/rollback grammar.
- Treating `hidden` and `clip` as the same internal clip bit preserves all
  existing consumers and avoids a schema change, at the cost of excluding
  their broader browser distinctions.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Implementation

- Design checkpoint: `d1cd1eab7ec92593c1f510dabed2ad168d98798a`.
- Source checkpoint: `462d2a70`.
- The source checkpoint updates `css.rs` and `native_engine.rs` only; the
  public computed-style and artifact schemas, feature defaults, dependencies,
  and two-crate boundary remain unchanged.
- Product and authoritative planning documentation is synchronized in the
  closeout checkpoint for this task.

## Verification

The completed gate covered:

- standalone case-insensitive parsing and typed rejection of other CSS-wide,
  mixed, malformed, and unsupported forms for `overflow`, `overflow-x`, and
  `overflow-y`;
- shorthand expansion, independent axis longhands, named-layer priority,
  repeated rollback, unlayered/inline precedence, visible/no-clip fallback,
  recognized unsupported-value behavior, and valid-before-invalid preservation;
- hidden/clip paint and software-surface clipping, viewport projection, point
  hit testing, root overflow/scroll derivation, capture, and semantic/source
  order after concrete selection and rollback; and
- no false unsupported-value diagnostics or public rollback leakage, plus
  focused `glass-browser` check, targeted behavioral tests, full native
  integration/library tests, strict affected-package Clippy, formatting, and
  final static documentation gates. Remote CI remains unclaimed until an
  explicitly authorized push.

## Evidence

- Focused locked `glass-browser` check passed.
- Overflow parser/cascade tests passed 2/2.
- Targeted native integration passed 1/1 with 183 filtered.
- Full native integration passed 184/184.
- Feature-enabled `glass-browser` library tests passed 956, with 1 ignored.
- Strict affected-package Clippy passed with warnings denied.
- Locked `glass-dev` and `glass-browser` inventory binaries passed in the
  isolated focused target; documentation coverage was run with those explicit
  binary paths and passed with 560 Markdown files, 345 full-product MCP tools
  (100 browser-only), 17 examples, and 22 public modules.
- Final static truth gates passed: version sync at 0.3.14; release
  documentation with 560 Markdown files, 83 current-document records, 57
  previous-version hits, 655 semantic-audit hits, and zero current-claim
  failures; documentation depth with 93 current guides and 19 substantive
  contracts; feature parity for 14 capabilities across 4 targets (baseline
  0.3.0, next 0.3.14, checkout 0.3.14); TUI shortcut parity with 15
  implementation help keys and 63 documentation markers; 5 public read-only
  adapters; reliability with 6 scenarios across 4 targets; and Web IR with 8
  fixtures, 8 scenarios, and 11 categories.
- Formatting and diff checks passed.
- After all checks completed, no Cargo/Rust process or open handle referenced
  the exact regenerable target. `/tmp/glass-146-focused` measured
  6,304,055,296 bytes (7,167 files, 1,046 directories) and was removed with
  bounded same-filesystem deletion; the exact path is absent.
  `/home/ubuntu/work/glass/target` remained absent. `/tmp` availability increased from
  77,576,105,984 to 83,880,128,512 bytes, reclaiming 6,304,022,528 bytes.
