---
id: native-engine-169
scope: glass-browser/native-engine/cascade-border-color-css-wide-keywords
status: complete
depends-on: [native-engine-168]
---

# Native bounded physical `border-color` CSS-wide keywords

## Objective

Accept the exact case-insensitive `inherit`, `unset`, `initial`, and `revert`
keywords for the existing local physical `border-color` owner: the
one-to-four-value `border-color` shorthand and the four physical color
longhands. Resolve them at the existing computed-style boundary while
preserving literal/alpha colors, `currentColor`, `revert-layer`, independent
side cascade, border width/style composition, and every existing layout,
display-list, capture, raster, clipping, opacity, point-hit, and
semantic/source-order consumer.

This slice extends the CSS-wide keyword family to the next local paint-color
owner. It does not introduce logical sides, a generic CSS value graph, or
browser-wide border conformance.

## Context

Native-engine-161/162 established physical border-color parsing and
`currentColor`; native-engine-122/145 established bounded named-layer and
unlayered `revert-layer`; native-engine-166/167/168 established private
CSS-wide declaration state for inherited and non-inherited color owners. The
physical border-color parser still treats the four reset keywords as invalid
values.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#inheritance>
- <https://www.w3.org/TR/css-cascade-5/#defaulting-keywords>
- <https://www.w3.org/TR/css-border-3/#border-color-properties>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-168.md`
- `docs/plan/tasks/native-engine-161.md`
- `docs/plan/tasks/native-engine-145.md`

## Contract

### Declaration and resolution state

- `border-color` and each physical color longhand accept the existing bounded
  literal/alpha grammar, case-insensitive `currentColor`, case-insensitive
  `revert-layer`, and one exact case-insensitive token each for `inherit`,
  `unset`, `initial`, and `revert`. A shorthand CSS-wide token expands to all
  four physical sides; mixed CSS-wide/color tokens remain unsupported.
- A private declaration value distinguishes concrete color, `currentColor`,
  `inherit`, `unset`, `initial`, `revert`, and `revert-layer` until the winning
  side-local candidate is resolved. No declaration-only variant may leak into
  `NativeComputedStyle`, border commands, capture bytes, or raster data.
- The property remains local/non-inherited for ordinary omission: no color
  candidate retains the current black side fallback when border width/style
  paint is composed. Explicit `inherit` copies the parent's effective
  concrete color for that physical side, including a parent `currentColor`
  result. A bounded root with no parent uses its effective black initial color.
- `unset`, `initial`, and one-author-origin `revert` resolve to the current
  element's concrete color (`currentColor` in this bounded engine). This is an
  explicit color candidate; ordinary omission remains the existing black
  border-side fallback.
- The computed-style walk carries only a private four-side effective parent
  color array. It must not turn omitted child border color into an inherited
  candidate. Only explicit `inherit` uses that array. Side order is physical
  top/right/bottom/left and remains independent through shorthand expansion,
  longhand precedence, layers, specificity, source order, and inline style.

### Existing owners preserved

- Width/style/no-paint composition, border geometry, display commands, border
  replay, clipping, opacity, scrolling, capture, software raster, point-hit,
  and semantic/source-order owners remain unchanged downstream.
- The transient parent color array is private to the style walk. Public
  computed-style fields, display-list commands, raster schemas, diagnostics
  transport, dependencies, feature defaults, and the two-crate boundary remain
  unchanged. `native-engine` remains default-off inside `glass-browser`.
- The slice remains local-resource-only, fixture-relative, horizontal-tb,
  integer-pixel, fixed-cell, non-table, and explicitly non-browser-parity.

## Tradeoffs

- One batched declaration family reuses the existing four-side cascade arrays
  and `revert-layer` resolver instead of adding a generic CSS-wide engine.
- Carrying four concrete effective parent colors makes explicit `inherit`
  observable without exposing parser sentinels or changing border artifacts.
- Reset forms intentionally resolve to local currentColor while omitted colors
  retain the established black fallback; this preserves compatibility with the
  current border composition and keeps the distinction testable.
- No dependency, public schema, layout geometry, default feature, or crate
  boundary changes are allowed, keeping the build surface stable.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, product docs, and
  issue records

## Verification

The completed slice must cover:

- case-insensitive parser acceptance for all four keywords across shorthand
  and physical longhands alongside literal, alpha, `currentColor`, and
  `revert-layer`, plus typed rejection of malformed/mixed/unsupported values;
- explicit side-wise inherit from parent concrete/current-color/omitted
  effective colors, root fallback, reset current-color behavior, ordinary
  omission black fallback, layer/unlayered/inline precedence, shorthand /
  longhand order, and valid-before-invalid preservation; and
- separate border geometry and color through border commands, decoded raster,
  clipping, opacity, capture, point-hit, semantic/source order, and private /
  public separation without changing width/style behavior.

Focused feature check/tests, full native integration/library tests, strict
affected-package Clippy, rustdoc, paired-crate check/build, packaging,
formatting, documentation, final static gates, workspace all-target/
all-feature testing, and bounded regenerable-target cleanup are required.
Remote CI remains unclaimed until an explicitly authorized push.

## Implementation

Completed in `4d9e6979` from this design contract (`b88177dc`). The private
`NativeBorderColorValue` now distinguishes concrete colors, `currentColor`,
`inherit`, `unset`, `initial`, and one-author-origin `revert`; the existing
`LocalCascadeDeclaration::RevertLayer` remains the layer rollback sentinel.
The computed-style walk carries four effective parent side colors and resolves
only explicit `inherit` from that array. Reset forms resolve to the local
current color, while ordinary omission remains the black border-side fallback.
The complete-border parser continues to reject CSS-wide values, and no public
computed-style, display-list, capture, raster, dependency, feature, or
two-crate schema changed.

The parser/cascade unit and the new consumer fixture are covered by the
additional test-only checkpoints `a0e77c84` and `801f7b19`. The focused locked
feature check, border-color unit, full native integration, and feature-enabled
library gates passed; the final workspace replay passed with 972 library tests
and 1 ignored, 207 native integration tests, 365 `glass-dev` tests, and all
auxiliary targets. Strict all-target/all-feature Clippy with warnings denied,
warning-denied native rustdoc, paired locked `glass-dev` check/build, both
locked package archives, and the exact packaged `glass-browser = 0.3.14`
dependency check passed. Static truth passed at version 0.3.14, feature parity
14 capabilities/4 targets, 584 Markdown documents with 83 current documents,
57 previous-version hits, 673 semantic audit hits, and 0 current-claim
failures; coverage passed with 345 full-product MCP tools, 100 browser-only
tools, 17 examples, and 22 public modules. TUI/depth/reliability/adapter/Web
IR/release-unit gates passed at 15/63, 93/19, 6/4, 5, 8/8/11, and 9/9.

The first workspace attempt also exposed one unrelated parallel extension-test
`ETXTBSY` temporary-script race; the failing test passed in isolation, and the
next complete workspace replay passed. This is recorded as environmental
evidence rather than hidden as a native-engine result. The plain `glass-dev`
package verification still resolves the registry's older `glass-browser =
0.3.14` API and fails; the release-certified path uses the existing
`--no-verify` step with a local crates.io patch, followed by the normalized
archive dependency check. This is a packaging-environment distinction, not a
relaxed dependency contract.

## Cleanup

After all Cargo processes and open handles exited, bounded exact-path cleanup
removed `/tmp/glass-169-focused` (6,615,230,854 bytes, 9,844 files, 1,196
directories), `/tmp/glass-169-package` (3,432,285,983 bytes, 3,638 files,
607 directories), `/tmp/glass-169-workspace` (7,008,811,742 bytes, 10,251
files, 1,068 directories), `/tmp/glass-169-release-documentation.json`
(186,042 bytes), and the failed extension-test residue
`/tmp/glass-extension-host-2900665` (98 bytes, 1 file, 1 directory). The
measured deletion total was 17,056,514,719 bytes. Filesystem free space rose
from 65,439,412,224 to 82,560,074,240 bytes. All five exact targets are absent
and no Cargo/Rust process or open handle remains; unrelated source, build
outputs, durable data, and existing temporary namespaces were not touched.

A final binary-backed documentation-coverage replay then used and removed
`/tmp/glass-169-final` (2,963,286,079 bytes, 5,239 files, 875 directories) and
`/tmp/glass-169-release-documentation-final.json` (186,042 bytes), after a
fresh no-process/no-handle check. That second cleanup removed 2,963,472,121
bytes, bringing all exact cleanup actions for this slice to 20,019,986,840
bytes. Both additional paths are absent and final filesystem free space is
82,559,643,648 bytes; no source, durable data, or unrelated temporary output
was removed.
