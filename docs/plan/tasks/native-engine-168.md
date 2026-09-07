---
id: native-engine-168
scope: glass-browser/native-engine/cascade-text-decoration-color-css-wide-keywords
status: complete
depends-on: [native-engine-167]
---

# Native bounded `text-decoration-color` CSS-wide keywords

## Objective

Accept the exact case-insensitive `inherit`, `unset`, `initial`, and `revert`
keywords for the existing local `text-decoration-color` property. Resolve them
at the existing computed-style boundary while preserving the private
`currentColor` and `revert-layer` paths, public `Option<NativeColor>`, separate
glyph/decoration paint, and every existing text layout, display-list, capture,
raster, clipping, opacity, scrolling, point-hit, and semantic/source-order
consumer.

This slice extends the CSS-wide keyword work to one local decoration-color
owner. It does not introduce a generic CSS value graph, alter decoration-line
inheritance, or imply browser-wide text-decoration conformance.

## Context

Native-engine-107 established the local decoration-color owner and its
omitted-color fallback. Native-engine-164 added deferred local
`text-decoration-color: currentColor`, and native-engine-122/145 established
the bounded named-layer/unlayered `revert-layer` resolver pattern. The local
decoration-color parser still treats the other CSS-wide keywords as typed
unsupported values.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#inheritance>
- <https://www.w3.org/TR/css-cascade-5/#defaulting-keywords>
- <https://www.w3.org/TR/css-cascade-5/#cascade-origin>
- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-color-property>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-167.md`
- `docs/plan/tasks/native-engine-164.md`
- `docs/plan/tasks/native-engine-145.md`
- `docs/plan/tasks/native-engine-107.md`

## Contract

### Declaration and resolution state

- `text-decoration-color` accepts the existing bounded literal/alpha grammar,
  case-insensitive `currentColor`, case-insensitive `revert-layer`, and one
  exact case-insensitive token each for `inherit`, `unset`, `initial`, and
  `revert`.
- A private declaration value distinguishes `Value`, `CurrentColor`,
  `Inherit`, `Unset`, `Initial`, `Revert`, and `RevertLayer` until the winning
  local declaration is resolved. None of these declaration-only variants may
  leak into `NativeComputedStyle`, display commands, capture bytes, or raster
  data.
- The property remains local/non-inherited for ordinary omission: no local
  declaration retains the established public `None` value and paint fallback
  to the text run's resolved glyph color. Explicit `inherit` copies the
  parent's effective concrete decoration color, including the parent's
  current-color fallback; if the bounded root has no parent, that effective
  value is native initial black.
- `unset`, `initial`, and one-author-origin `revert` resolve to the current
  element's concrete `color` (`currentColor` in this bounded engine), so they
  become explicit concrete decoration paint while preserving the existing
  public optional representation for omitted declarations. `revert-layer`
  remains lower-layer rollback.
- The computed-style walk carries only a private effective parent decoration
  color. It must not turn an omitted child declaration into inherited paint;
  only the explicit `inherit` keyword uses that input. `inherit` is tested
  against a parent with an explicit color, an explicit decoration color, and
  the omitted/current-color fallback.
- Named-layer priority, unlayered/inline precedence, specificity, source
  order, same-block order, valid-before-invalid preservation, existing
  `!important` stripping, separate glyph/line paint, and all current
  decoration-line propagation remain unchanged. Malformed values, mixed
  tokens, other CSS-wide keywords, gradients, image functions, system colors,
  color-space functions, percentages, custom properties, and arbitrary
  functions remain typed unsupported-value diagnostics.

### Existing owners preserved

- Concrete decoration color continues through text-run commands, distinct
  glyph/line replay, clipping, opacity, scrolling, capture, software raster,
  point-hit, and semantic/source-order owners without downstream keyword
  handling.
- The transient parent decoration color is private to the style walk; public
  computed-style fields, display-list commands, raster schemas, diagnostics
  transport, dependencies, feature defaults, and the two-crate boundary remain
  unchanged. `native-engine` remains default-off inside `glass-browser`.
- The slice remains local-resource-only, fixture-relative, horizontal-tb,
  integer-pixel, fixed-cell, non-table, and explicitly non-browser-parity.

## Tradeoffs

- Batching all four keywords reuses the existing private declaration and
  layer-resolution pattern while making the local-property difference between
  omitted fallback and explicit `inherit` observable.
- Carrying one concrete effective parent decoration color avoids a second style
  graph. It preserves the current public `None` omission behavior and lets
  explicit `inherit` copy a parent's current-color fallback without exposing a
  declaration-only state downstream.
- `unset` and one-author-origin `revert` intentionally map to currentColor,
  matching the bounded initial value for this local property. Modeling multiple
  origins, animation, or full decoration propagation requires a separate
  contract.
- No dependency, public schema, layout geometry, default feature, or crate
  boundary changes, so the build surface stays stable.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, product docs, and
  issue records

## Verification

The completed slice must cover:

- case-insensitive parser acceptance for all four keywords alongside literal,
  alpha, `currentColor`, and `revert-layer`, plus typed rejection of malformed,
  mixed, unrelated, and unsupported CSS values;
- explicit inherit from parent concrete/explicit/current-color decoration,
  omitted child fallback, root fallback, `unset`/`initial`/`revert` current-color
  reset, layer/unlayered/inline precedence, same-block order, rollback, and
  valid-before-invalid preservation;
- separate glyph and decoration colors through text-run display/raster
  artifacts, line propagation, clipping, opacity, scrolling, capture,
  point-hit, semantic/source order, and private/public separation; and
- focused feature check/tests, full native integration/library tests, strict
  affected-package Clippy, rustdoc, paired-crate check/build, packaging,
  formatting, documentation, final static gates, workspace all-target/
  all-feature testing, and bounded regenerable-target cleanup. Remote CI
  remains unclaimed until an explicitly authorized push.

## Implementation

Completed in `4521f151` from this design contract (`0bb67e8b`). The private
`NativeTextDecorationColorDeclaration` now distinguishes `Value`,
`CurrentColor`, `Inherit`, `Unset`, `Initial`, `Revert`, and `RevertLayer`.
The computed-style walk carries one concrete effective parent decoration color:
explicit `inherit` copies it, while `unset`, `initial`, and one-author-origin
`revert` resolve to the current element color. Ordinary omission remains the
existing public `None` and paint fallback to the text run's glyph color. The
public computed-style and display/raster schemas remain unchanged.

Synchronized current product documentation is `2184d98a`. Focused locked
feature check, the decoration-color parser/cascade unit (4/4), and the new
CSS-wide keyword integration fixture (1/1) passed. Full native integration
passed 206/206 and feature-enabled library tests passed 970 with 1 ignored.
Strict all-target/all-feature Clippy with warnings denied, warning-denied
feature rustdoc, paired locked `glass-dev` check/build, both locked package
archives, and the exact packaged `glass-browser = 0.3.14` dependency check
passed. Static truth passed at version 0.3.14, feature parity 14
capabilities/4 targets, 582 Markdown documents with 83 current documents/57
previous-version hits/672 semantic audit hits/0 current-claim failures,
documentation coverage 582 Markdown/345 full-product MCP tools/100
browser-only tools/17 examples/22 public modules, TUI 15/63, depth 93/19,
reliability 6/4, adapters 5, Web IR 8/8/11, and release-documentation unit
tests 9/9. The locked workspace all-target/all-feature replay passed with 971
library tests and 1 ignored, native integration 206/206, `glass-dev` 365/365,
and all auxiliary targets.

Exact regenerable cleanup removed `/tmp/glass-168-focused`,
`/tmp/glass-168-package`, and `/tmp/glass-168-workspace` only after Cargo/Rust
process and open-handle checks. The exact measured inventories and free-space
delta are recorded in the final closeout checkpoint alongside this task. The
temporary release-documentation report was also removed after verification.
Remote CI, push, release, tag, registry publication, browser parity, and
promotion remain unclaimed.
