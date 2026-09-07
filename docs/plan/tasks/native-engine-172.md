---
id: native-engine-172
scope: glass-browser/native-engine/cascade-border-radius-css-wide-keywords
status: complete
depends-on: [native-engine-171]
---

# Native bounded physical `border-radius` CSS-wide keywords

## Objective

Accept the exact case-insensitive `inherit`, `unset`, `initial`, and `revert`
keywords for the existing local physical `border-radius` shorthand. Resolve
them at the computed-style boundary while preserving bounded one-to-four-value
integer-pixel corner expansion, conservative normalization, `revert-layer`,
rounded layout/hit geometry, display-list replay, software raster, and PNG
capture.

This slice extends the bounded CSS-wide family to the remaining physical
border geometry owner. It does not add corner longhands, elliptical radii,
percentages, logical sides, or browser-wide border conformance.

## Context

Native-engine-020 established the physical `border-radius` shorthand and
rounded fill/border/hit consumers; native-engine-145 established the local
layer rollback pattern; native-engine-166 through native-engine-171 establish
the private CSS-wide declaration pattern for inherited and local owners. The
radius parser currently accepts concrete pixel values and `revert-layer` but
rejects the four reset keywords.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#defaulting-keywords>
- <https://www.w3.org/TR/css-backgrounds-3/#border-radius>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-171.md`
- `docs/plan/tasks/native-engine-020.md`
- `docs/plan/tasks/native-engine-145.md`

## Contract

### Declaration and resolution state

- `border-radius` accepts the existing concrete one-to-four-value bounded
  non-negative integer `Npx` grammar, standalone case-insensitive
  `revert-layer`, and one exact case-insensitive token each for `inherit`,
  `unset`, `initial`, and `revert`. CSS-wide keywords cannot be mixed with
  concrete values or slash-separated radii.
- A private declaration value distinguishes concrete `NativeBorderRadius`,
  `inherit`, `unset`, `initial`, and one-author-origin `revert` until the local
  candidate resolves. `revert-layer` remains the existing lower-layer
  sentinel. No declaration-only state may leak into public radius values,
  display commands, capture bytes, raster data, or diagnostics transport.
- The property remains local/non-inherited for ordinary omission: omission
  resolves to `NativeBorderRadius::default()`. Explicit `inherit` copies the
  parent's effective concrete four-corner radius. A bounded root without a
  parent uses the default radius.
- `unset`, `initial`, and one-author-origin `revert` resolve to the default
  zero-corner radius. This preserves the current no-rounding fallback while
  keeping reset candidates distinct in the private cascade.
- The DOM style walk carries only one private effective parent radius. It must
  not turn an omitted child radius into an inherited candidate. Existing
  physical corner order remains top-left, top-right, bottom-right,
  bottom-left through normalization and all consumers.

### Existing owners preserved

- Concrete radius expansion, conservative corner normalization, rounded fill
  and border replay, clipping, opacity, scrolling, capture, software raster,
  point-hit, semantic/source order, and the public `NativeBorderRadius` shape
  remain the downstream owners.
- No dependency, public schema, layout algorithm, default feature, or crate
  boundary changes are allowed. `native-engine` remains default-off inside
  `glass-browser`; the workspace remains exactly two installable crates.
- The slice remains local-resource-only, fixture-relative, horizontal-tb,
  integer-pixel, fixed-cell, non-table, and explicitly non-browser-parity.

## Tradeoffs

- One private radius declaration wrapper reuses the existing bounded candidate
  array and `revert-layer` resolver instead of introducing generic CSS-wide
  value graphs.
- Carrying one effective parent radius is sufficient for explicit `inherit`
  without exposing parser sentinels or changing the public radius structure.
- Reset forms intentionally resolve to zero corners, preserving the existing
  bounded initial behavior. Elliptical and percentage geometry remain outside
  the engine's deterministic integer-pixel contract.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, product docs, and
  issue records

## Verification

The completed slice must cover:

- case-insensitive parser acceptance for all four keywords across the
  shorthand, concrete one-to-four-value expansion, standalone
  `revert-layer`, and typed rejection of mixed, slash-separated, fractional,
  percentage, negative, oversized, and unsupported values;
- explicit inherit from painted/rounded, default, and root-fallback parent
  radii; reset and ordinary omission; named-layer/unlayered/inline precedence;
  valid-before-invalid preservation; and deterministic corner order; and
- unchanged rounded layout, border/fill display commands, clipping, opacity,
  scrolling, capture, software raster, point-hit, semantic/source order, and
  public/private separation.

Focused feature check/tests, full native integration/library tests, strict
affected-package Clippy, rustdoc, paired-crate check/build, packaging,
formatting, documentation, final static gates, workspace all-target/
all-feature testing, and bounded regenerable-target cleanup are required.
Remote CI remains unclaimed until an explicitly authorized push.

## Implementation

Completed in `b0bbe45a` (`feat(native-engine): support border radius css-wide
keywords`). `NativeBorderRadiusValue` now keeps concrete radii and the four
CSS-wide declaration-only values private until computed-style resolution.
The existing physical shorthand parser remains the owner of bounded
one-to-four-value expansion and standalone `revert-layer`; exact
case-insensitive `inherit`, `unset`, `initial`, and `revert` are accepted as
single tokens, while mixed and slash-separated forms remain rejected.

The computed-style boundary resolves explicit `inherit` from one private
effective parent radius, reset forms and ordinary omission to the default
zero-corner radius, and lower-layer candidates through the existing rollback
resolver. The DOM walk carries only the effective parent radius. Public
`NativeBorderRadius`, layout/display/raster/capture values, diagnostics, and
the two-crate/default-feature boundaries remain unchanged. The integration
fixture covers painted, unpainted, and zero-border-width parents; concrete and
CSS-wide children; invalid preservation; named-layer/unlayered/inline
precedence; geometry; rounded point-hit; semantic/source order; fill/border
replay; software raster; and PNG capture.

## Certification evidence

The locked focused feature check passed. Border-radius parser/cascade unit
coverage passed 4/4, the focused consumer fixture passed 1/1, full native
integration passed 210/210, and feature-enabled `glass-browser` library tests
passed 974 with 1 ignored. Strict all-target/all-feature Clippy with warnings
denied, warning-denied feature rustdoc, and paired locked `glass-dev`
check/build passed. Both package archives were generated; the packaged
dependency checker confirmed exact `glass-browser = 0.3.14` for `glass-dev`
with no path or feature dependency. The plain registry package verification
remains the known registry API mismatch; the patched local-release archive is
exact.

The locked workspace all-target/all-feature replay passed with 975 library
tests and 1 ignored, 210 native integration tests, 365 `glass-dev` tests, and
all auxiliary targets. Formatting and the final static documentation gates
passed at version 0.3.14: 586 Markdown documents, 83 current documents, 57
previous-version hits, 675 semantic audit hits, and 0 current-claim failures;
documentation coverage 586 Markdown/345 full-product MCP tools/100
browser-only tools/17 examples/22 public modules; TUI 15/63; depth 93/19;
reliability 6/4; adapters 5; Web IR 8/8/11; and release-documentation unit
tests 9/9. No remote CI, push, release, tag, registry publication,
browser-parity, security-boundary, or promotion claim is made.

## Cleanup

After all Cargo processes and open handles exited, bounded exact-path cleanup
removed `/tmp/glass-172-focused` (5,456,537,779 bytes),
`/tmp/glass-172-package` (2,370,708,338 bytes),
`/tmp/glass-172-workspace` (4,954,695,376 bytes),
`/home/ubuntu/work/glass/target` (2,162,014,874 bytes),
`/tmp/glass-172-release-documentation.json` (186,544 bytes), and
`/tmp/glass-172-release-documentation-final.json` (186,544 bytes). The
measured deletion total was 14,944,329,455 bytes. Final filesystem free space
was 82,336,038,912 bytes. All six exact paths are absent; no Cargo/Rust
process or open handle remains; and source, durable data, repository history,
and unrelated workloads were not touched.
