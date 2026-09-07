---
id: native-engine-166
scope: glass-browser/native-engine/cascade-color-css-wide-keywords
status: complete
depends-on: [native-engine-165]
---

# Native bounded inherited `color` CSS-wide keywords

## Objective

Accept the exact case-insensitive `inherit`, `unset`, `initial`, and `revert`
keywords for the existing inherited `color` property. Resolve each accepted
keyword at the existing computed-style boundary while preserving the private
`currentColor` path, public `Option<NativeColor>`, and every existing
background, border, decoration, glyph, display-list, capture, raster, and
layout consumer.

This slice batches the remaining one-property CSS-wide keyword behavior into
one auditable unit. It does not introduce a generic CSS value graph or imply
that the same keyword semantics are implemented for unrelated properties.

## Context

Native-engine-145 established bounded `color: revert-layer` rollback through
the named-layer and unlayered candidate streams. Native-engine-165 added the
deferred `color: currentColor` value and resolves it from the already-computed
inherited color without self-recursion. This slice completes the bounded
CSS-wide keyword family for the inherited color owner without introducing a
generic reset graph.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#inheritance>
- <https://www.w3.org/TR/css-cascade-5/#defaulting-keywords>
- <https://www.w3.org/TR/css-cascade-5/#cascade-origin>
- <https://www.w3.org/TR/css-color-4/#the-color-property>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-165.md`
- `docs/plan/tasks/native-engine-145.md`
- `docs/plan/tasks/native-engine-121.md`

## Contract

### Declaration and resolution state

- `color` accepts the existing bounded literal/alpha grammar,
  case-insensitive `currentColor`, case-insensitive `revert-layer`, and one
  exact case-insensitive token each for `inherit`, `unset`, `initial`, and
  `revert`.
- A private declaration value distinguishes `Color`, `CurrentColor`,
  `Inherit`, `Unset`, `Initial`, and `Revert` until the winning local
  declaration is resolved. None of these declaration-only variants may leak
  into `NativeComputedStyle`, display commands, capture bytes, or raster data.
- `inherit`, `unset`, and `revert` resolve to the supplied inherited computed
  color. If the bounded direct computation has no inherited color, their
  resolved value is the native initial black color. `initial` always resolves
  to that same bounded black color, regardless of the parent value.
- An omitted local declaration retains the established optional behavior in
  direct stylesheet computation: an undeclared root remains `None`. The
  layout helper's existing root walk continues to seed the initial black
  color, so layout and paint retain their current black fallback.
- `revert` is intentionally modeled at the native engine's single author
  origin: it exposes the inherited fallback rather than pretending to model
  user, user-agent, animation, or multiple-origin cascades. `revert-layer`
  remains the separate lower-layer rollback operation.
- Named-layer priority, unlayered/inline precedence, specificity, source
  order, same-block order, valid-before-invalid preservation, and existing
  `!important` stripping remain unchanged. Malformed values, mixed tokens,
  other CSS-wide keywords, gradients, image functions, system colors,
  color-space functions, percentages, custom properties, and arbitrary
  functions remain typed unsupported-value diagnostics.

### Existing owners preserved

- The resolved concrete color continues through descendant inheritance,
  background current-color substitution, border current-color substitution,
  decoration current-color substitution, glyph text paint, clipping,
  opacity, scrolling, capture, software raster, point-hit, and
  semantic/source-order owners without downstream keyword handling.
- Public computed-style fields, display-list commands, raster schemas,
  diagnostics transport, dependencies, feature defaults, and the two-crate
  boundary remain unchanged. `native-engine` remains default-off inside
  `glass-browser`.
- The slice remains local-resource-only, fixture-relative, horizontal-tb,
  integer-pixel, fixed-cell, non-table, and explicitly non-browser-parity.

## Tradeoffs

- Batching all four remaining keywords reuses one small private enum and one
  resolver branch, reducing repeated full-suite rebuilds while keeping the
  behavior limited to one inherited property.
- Mapping `inherit`/`unset`/`revert` through the existing inherited color input
  avoids a second style graph. The explicit black fallback makes root behavior
  deterministic without changing the established omitted-root `None` result.
- `initial` is kept distinct from inherited keywords even though both can
  resolve to black at the root; this preserves the CSS meaning and makes
  parent-color reset behavior testable.
- Modeling `revert` at one author origin is useful and honest for the current
  engine, but it is not a claim of full cascade-origin semantics. Supporting
  multiple origins or `!important` inversion requires a separate contract.
- No dependency, layout owner, artifact schema, default feature, or crate
  boundary changes, so the build surface stays stable.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, product docs, and
  issue records

## Verification

The completed slice must cover:

- case-insensitive parser acceptance for all four keywords alongside literal,
  alpha, `currentColor`, and `revert-layer`, plus typed rejection of malformed,
  mixed, unrelated, and unsupported CSS values;
- local concrete override, inherited parent resolution, explicit initial
  black reset, root fallback, omitted-root `None`, `revert` one-origin
  fallback, layer/unlayered/inline precedence, specificity, same-block order,
  repeated rollback, and valid-before-invalid preservation;
- non-recursive descendant inheritance and propagation through glyph,
  background, border, and decoration display/raster artifacts, with clipping,
  opacity, capture, point-hit, semantic/source order, and private/public
  separation; and
- focused feature check/tests, full native integration/library tests, strict
  affected-package Clippy, rustdoc, paired-crate check/build, packaging,
  formatting, documentation, final static gates, workspace all-target/
  all-feature testing, and bounded regenerable-target cleanup. Remote CI
  remains unclaimed until an explicitly authorized push.

## Implementation

Completed in `b57ba2b8` from this design contract (`d148f766`). The private
`NativeColorValue` now distinguishes `Color`, `CurrentColor`, `Inherit`,
`Unset`, `Initial`, and `Revert`. Exact case-insensitive keyword parsing feeds
the existing cascade candidate stream; inherited/unset/revert values resolve
from the supplied parent color with bounded black fallback, while initial
resets to black. The concrete `Option<NativeColor>` public surface and all
layout/display-list/capture/raster/hit/semantic owners remain unchanged.

The synchronized current product documentation is `d26a9059`. Focused locked
feature check, the parser/resolver unit (1/1), the new keyword integration
fixture (1/1), full native integration (204/204), and feature-enabled library
tests (968 passed, 1 ignored) passed. Strict all-target/all-feature Clippy
with warnings denied, feature rustdoc with warnings denied, paired locked
`glass-dev` check/build, both locked package archives, and the exact packaged
`glass-browser = 0.3.14` dependency check passed. Static truth passed at
version 0.3.14, feature parity 14 capabilities/4 targets, 580 Markdown
documents with 83 current documents/57 previous-version hits/670 semantic
audit hits/0 current-claim failures, documentation coverage 580 Markdown/
345 full-product MCP tools/100 browser-only tools/17 examples/22 public
modules, TUI 15/63, depth 93/19, reliability 6/4, adapters 5, Web IR
8/8/11, and release-documentation unit tests 9/9. The locked workspace
all-target/all-feature gate passed with 969 library tests and 1 ignored,
native integration 204/204, `glass-dev` 365/365, and all auxiliary targets.

Exact regenerable cleanup removed `/tmp/glass-166-focused` (5,406,130,484
bytes, 9,071 files, 1,185 directories), `/tmp/glass-166-package`
(3,134,139 bytes, 5 files, 3 directories), and `/tmp/glass-166-workspace`
(4,141,759,121 bytes, 5,541 files, 624 directories) after no active Cargo/
Rust processes and no open handles were found. Measured deletion was
9,551,023,744 bytes; filesystem free space rose from 73,002,225,664 to
82,596,917,248 bytes (a 9,594,691,584-byte filesystem delta). All exact
targets and the run-created experiment directories were absent afterward.
Remote CI, push, release, tag, registry publication, browser parity, and
promotion remain unclaimed.
