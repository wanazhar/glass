---
id: native-engine-131
scope: glass-browser/native-engine/cascade-layers-line-height-revert-layer
status: complete
depends-on: [native-engine-130]
---

# Native bounded `line-height: revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven by native-engine-130 to add
the explicit CSS-wide `revert-layer` keyword to the existing inherited
positive-pixel `line-height` owner. Preserve the current fixed-cell flow
minimums, inline auto-height precedence, explicit-height behavior, and all
layout/display-list/hit-test/raster consumers.

## Context

The native engine currently accepts only positive bounded pixel values for
`line-height`. A valid value is inherited through the existing DOM style walk
and reaches the line-flow minimum and inline auto-height owners; omitted and
invalid values retain the existing `None` fallback. This slice adds private
declaration-only rollback state and per-layer candidates without changing the
public `Option<u32>` computed value or its root default.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-inline-3/#line-height-property>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-130.md`

## Contract

### Declaration and cascade state

- `line-height` accepts one standalone, case-insensitive `revert-layer`
  token. The token exists only in private declaration/candidate state;
  positive bounded pixel values remain the only public computed values.
- Stylesheet candidates use the existing bounded first-appearance named-layer
  order. Selector specificity and source order decide ties within a layer;
  unlayered and inline declarations remain above named layers.
- A winning rollback blocks only its current layer and selects the highest
  priority remaining concrete candidate. Repeated rollback continues through
  lower layers and then the inherited `Option<u32>` value; unlayered/inline
  rollback can expose the highest named candidate.
- With no candidate remaining, the root result remains `None`, so the current
  fixed flow minimum and default line-height behavior are unchanged. A
  descendant with no local candidate retains the existing inherited pixel
  value.
- Valid rollback declarations produce no false unsupported-value diagnostic.
  Other CSS-wide keywords, zero/negative/relative/percentage values, mixed
  forms, multiple origins, `!important` inversion, layer statements,
  nested/anonymous/comma layers, animation, script, and browser-wide
  line-height conformance remain outside the contract.

### Existing owners preserved

Resolved values continue through the existing inherited style walk, line-flow
minimum, inline auto-height, explicit-height precedence, box geometry,
display-list, viewport projection, hit testing, capture, and raster owners. No
public computed-style or layout/display-list schema, dependency, feature
default, or crate boundary changes are permitted.

The slice remains a horizontal-tb, fixed-cell, bounded local-content contract;
it does not claim browser parity, full CSS cascade semantics, font metrics, or
a complete browser engine.

## Implementation and local result

The implementation is complete in `e35a13fd` (`feat(native-engine): support
line-height rollback`). `LineHeightDeclaration` keeps `revert-layer` private,
stores stylesheet and inline candidates in the existing bounded cascade-layer
array, resolves repeated rollback before inherited/root fallback, and leaves
the public `Option<u32>` computed value and all downstream flow/artifact owners
unchanged. The supported-value diagnostic classifier uses the same declaration
parser, so valid case variants do not produce false unsupported-value
diagnostics.

The focused and affected-package gates passed:

- `cargo check -p glass-browser --features native-engine --test native_engine
  --locked` — passed in 2m15s.
- line-height parser/cascade filters — 5 tests passed; the new integration
  regression plus the two existing line-height regressions — 3 passed.
- `cargo test -p glass-browser --features native-engine --lib --locked` with
  `RUST_MIN_STACK=33554432` — 931 passed, 1 ignored.
- `cargo test -p glass-browser --features native-engine --test native_engine
  --locked` — 168 passed.
- `cargo clippy -p glass-browser --features native-engine --all-targets
  --locked -- -D warnings` — passed in 1m35s.

The 131 integration fixture initially exposed only an over-wide test string:
the existing fixed-cell wrapping correctly produced a 96px box where the
test expected 32px. The fixture was narrowed to one-cell strings and the same
three-test filter passed; no production behavior was changed. Full native,
two-crate, strict, fuzz/security, static-documentation, and remote-CI gates
remain issue-level gates. Remote CI is unclaimed because this checkout has
not been pushed.

The documentation closeout also passed: version sync, feature parity, release
truth (545 Markdown documents, 83 current documents, 57 previous-version
references, 642 semantic audit hits, and 0 current-claim failures), release
documentation unit tests (9/9), TUI shortcut inventory (15/63), documentation
depth (93 guides/19 contracts), reliability (6 scenarios/4 targets),
read-only adapters (5), Web IR (8 fixtures/8 scenarios/11 categories), and
live documentation coverage (545 Markdown files, 345 full-product MCP tools,
100 browser-only tools, 17 examples, and 22 public modules) all passed.
Formatting and diff checks passed. After process and `/proc` descriptor checks
found no target consumer, `/tmp/glass-131-focused` measured 5,020,057,600
bytes across 6,647 files and was removed, along with the exact 131 logs and
release report; no repository target, source, fixture, durable data, or
unrelated temporary path was removed. Available filesystem bytes increased
from 80,515,428,352 to 85,535,768,576, a measured delta of 5,020,340,224
bytes.

## Tradeoffs

- A private declaration enum and per-layer candidate array add property-local
  state while keeping the public `Option<u32>` contract and existing omitted
  fallback intact.
- Cascade rollback is tested through named, repeated, unlayered, inline,
  inherited, and root cases, while line-height geometry is asserted through
  the existing line-flow and artifact owners. This separates declaration
  selection from already-certified pixel-flow semantics.
- The bounded positive-pixel grammar remains unchanged, so unsupported values
  stay typed diagnostics rather than entering layout as an accidental default.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and issue records

## Verification

The focused gate must cover:

- case-insensitive standalone parsing and typed rejection of other CSS-wide,
  mixed, malformed, and non-positive forms;
- named-layer ordering before specificity, repeated rollback,
  unlayered/inline precedence, inherited descendants, and root `None` fallback;
- preservation of positive-pixel line-height flow minimums, inline auto-height,
  explicit-height precedence, layout/display-list/hit-test coordinates, and
  decoded raster evidence;
- no false unsupported-value diagnostics and no public declaration-keyword
  leakage.

Targeted checks follow the completed behavioral unit. Full native, two-crate,
strict, package, fuzz/security, and static documentation gates are final
validation only; remote CI remains unclaimed until an explicitly authorized
push.
