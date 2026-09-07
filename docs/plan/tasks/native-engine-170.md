---
id: native-engine-170
scope: glass-browser/native-engine/cascade-border-width-css-wide-keywords
status: complete
depends-on: [native-engine-169]
---

# Native bounded physical `border-width` CSS-wide keywords

## Objective

Accept the exact case-insensitive `inherit`, `unset`, `initial`, and `revert`
keywords for the existing local physical `border-width` owner: the
one-to-four-value `border-width` shorthand and the four physical width
longhands. Resolve them at the existing computed-style boundary while
preserving literal fixed-pixel widths, `revert-layer`, independent side
cascade, border style/color composition, content/outer geometry, and every
existing layout, display-list, capture, raster, clipping, opacity, scrolling,
point-hit, and semantic/source-order consumer.

This slice extends the bounded CSS-wide keyword family to the local border
geometry component. It does not introduce a generic CSS value graph, logical
sides, percentages, medium/thin/thick defaults, table conflict resolution, or
browser-wide border conformance.

## Context

Native-engine-050 established bounded min/max geometry; native-engine-161/162
through native-engine-169 established the physical border component cascade and
CSS-wide private-state pattern for color owners. The physical border-width
parser still treats the four reset keywords as invalid values.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#inheritance>
- <https://www.w3.org/TR/css-cascade-5/#defaulting-keywords>
- <https://www.w3.org/TR/css-border-3/#border-width-properties>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-169.md`
- `docs/plan/tasks/native-engine-154.md`
- `docs/plan/tasks/native-engine-145.md`

## Contract

### Declaration and resolution state

- `border-width` and each physical width longhand accept the existing bounded
  non-negative integer `Npx` grammar, case-insensitive `revert-layer`, and one
  exact case-insensitive token each for `inherit`, `unset`, `initial`, and
  `revert`. A shorthand CSS-wide token expands to all four physical sides;
  mixed CSS-wide/numeric tokens remain unsupported.
- A private declaration value distinguishes a fixed width, `inherit`, `unset`,
  `initial`, and one-author-origin `revert` until the winning side-local
  candidate is resolved. `revert-layer` remains the existing lower-layer
  sentinel. No declaration-only variant may leak into public computed-style
  fields, border commands, capture bytes, or raster data.
- The property remains local/non-inherited for ordinary omission: no width
  candidate resolves to the existing zero-width fallback. Explicit `inherit`
  copies the parent's effective concrete width for that physical side,
  including a parent width that is not currently paintable because its style or
  color component is missing. A bounded root with no parent uses zero width.
- `unset`, `initial`, and one-author-origin `revert` resolve to zero width.
  They are explicit width candidates but have the same bounded numeric result
  as ordinary omission; style-only declarations still do not invent a border.
- The computed-style walk carries only a private four-side effective parent
  width array. It must not turn an omitted child width into an inherited
  candidate. Only explicit `inherit` uses that array. Side order is physical
  top/right/bottom/left and remains independent through shorthand expansion,
  longhand precedence, layers, specificity, source order, and inline style.

### Existing owners preserved

- Border style/color resolution, no-paint sentinels, width/style/color
  composition, content/outer box geometry, display commands, border replay,
  clipping, opacity, scrolling, capture, software raster, point-hit, and
  semantic/source-order owners remain unchanged downstream.
- The transient parent width array is private to the style walk. Public
  computed-style fields, display-list commands, raster schemas, diagnostics
  transport, dependencies, feature defaults, and the two-crate boundary remain
  unchanged. `native-engine` remains default-off inside `glass-browser`.
- The slice remains local-resource-only, fixture-relative, horizontal-tb,
  integer-pixel, fixed-cell, non-table, and explicitly non-browser-parity.

## Tradeoffs

- One batched declaration family reuses the existing four-side cascade arrays
  and `revert-layer` resolver instead of adding a generic CSS-wide engine.
- Carrying four concrete effective parent widths makes explicit `inherit`
  observable without exposing parser sentinels or changing border artifacts.
- Reset forms intentionally resolve to zero, retaining the current bounded
  initial value and no-paint behavior; ordinary omission remains the existing
  zero-width fallback. This keeps reset and omission semantically distinct in
  the private cascade while preserving downstream geometry.
- No dependency, public schema, layout algorithm, default feature, or crate
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
  and physical longhands alongside fixed-pixel values and `revert-layer`, plus
  typed rejection of malformed, mixed, unsupported, and non-width values;
- explicit side-wise inherit from parent painted/unpainted/zero effective
  widths, root fallback, reset zero behavior, ordinary omission zero fallback,
  layer/unlayered/inline precedence, shorthand/longhand order, and
  valid-before-invalid preservation; and
- unchanged style/color composition and border geometry through border commands,
  decoded raster, clipping, opacity, capture, point-hit, semantic/source order,
  and private/public separation.

Focused feature check/tests, full native integration/library tests, strict
affected-package Clippy, rustdoc, paired-crate check/build, packaging,
formatting, documentation, final static gates, workspace all-target/
all-feature testing, and bounded regenerable-target cleanup are required.
Remote CI remains unclaimed until an explicitly authorized push.

## Implementation

Completed in `cf19800f` from this design contract. The private
`NativeBorderWidthValue` distinguishes fixed non-negative pixel widths,
`inherit`, `unset`, `initial`, and one-author-origin `revert`; the existing
`LocalCascadeDeclaration::RevertLayer` remains the lower-layer rollback
sentinel. The computed-style walk carries only four effective parent widths;
explicit `inherit` reads that private array, reset forms resolve to zero, and
ordinary omission retains the zero fallback. Fixed widths remain concrete
before border composition, and no declaration-only value reaches public
computed-style, display-list, capture, raster, diagnostics, dependency,
feature, or crate-boundary surfaces.

The parser and computed-style unit coverage was added alongside a full native
consumer fixture. The fixture covers painted, unpainted, and zero-width parent
effective widths; shorthand and all four physical longhands; reset and
omitted-width behavior; layer/unlayered/inline precedence; valid-before-invalid
preservation; width/style/color composition; geometry; point hit testing;
semantic visibility/source order; opacity; display; raster; and PNG capture.
The focused locked feature check, width unit coverage, focused consumer test,
full native integration (208/208), and feature-enabled library tests (972
passed, 1 ignored) passed. Strict Clippy, warning-denied feature rustdoc, and
paired locked `glass-dev` check/build also passed. Packaging, static
documentation, workspace replay, and exact cleanup evidence are recorded in
the certification section below.

## Certification evidence

The focused locked feature check, border-width parser/computed-style unit
coverage, and focused consumer test passed. Full native integration passed
208/208; feature-enabled `glass-browser` library tests passed 972 with 1
ignored; and the locked workspace all-target/all-feature replay passed with
973 library tests and 1 ignored, 208 native integration tests, 365 `glass-dev`
tests, and all auxiliary targets.

Strict all-target/all-feature Clippy with warnings denied, warning-denied
feature rustdoc, and paired locked `glass-dev` check/build passed. The
release-certified package path generated both archives and the packaged
dependency checker confirmed exact `glass-browser = 0.3.14` for `glass-dev`
with no path or feature dependency. Static truth passed at version 0.3.14:
feature parity 14 capabilities/4 targets; 585 Markdown documents/83 current
documents/57 previous-version hits/673 semantic audit hits/0 current-claim
failures; documentation coverage 585 Markdown/345 full-product MCP tools/100
browser-only tools/17 examples/22 public modules; TUI 15/63; depth 93/19;
reliability 6/4; adapters 5; Web IR 8/8/11; and release-documentation unit
tests 9/9. No remote CI, push, release, tag, registry publication,
browser-parity, security-boundary, or promotion claim is made.

## Cleanup

After all Cargo processes and open handles exited, bounded exact-path cleanup
removed `/tmp/glass-170-focused` (5,871,772,894 bytes),
`/tmp/glass-170-package` (2,370,667,292 bytes),
`/tmp/glass-170-workspace` (4,951,732,258 bytes),
`/tmp/glass-170-release-documentation.json` (186,042 bytes), and
`/tmp/glass-170-release-documentation-final.json` (186,086 bytes). The
measured deletion total was 13,194,544,572 bytes. Final filesystem free space
was 82,361,376,768 bytes. All five exact paths are absent; no Cargo/Rust
process or open handle remains; and unrelated source, build outputs, durable
data, and temporary namespaces were not touched.
