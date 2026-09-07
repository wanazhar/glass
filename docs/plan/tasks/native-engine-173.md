---
id: native-engine-173
scope: glass-browser/native-engine/cascade-complete-border-css-wide-keywords
status: complete
depends-on: [native-engine-172]
---

# Native bounded complete `border` CSS-wide keywords

## Objective

Accept the exact case-insensitive `inherit`, `unset`, `initial`, and `revert`
keywords for the existing complete physical border shorthand owners:
`border`, `border-top`, `border-right`, `border-bottom`, and `border-left`.
Resolve them through the existing independent width/style/color candidate
streams while preserving complete concrete parsing, omitted-component
`none`/`hidden`, `currentColor`, `revert-layer`, side composition, box-model
geometry, border replay, raster, capture, point-hit, and semantic consumers.

This slice closes the complete-border member of the bounded CSS-wide family.
It does not introduce a generic CSS-wide engine, arbitrary omitted defaults,
logical sides, table conflict resolution, or browser-wide border conformance.

## Context

Native-engine-145 established bounded local cascade-layer rollback for the
complete physical border owners. Native-engine-159 through native-engine-171
established the private component streams and their CSS-wide values for border
none/hidden, color, width, and style. Native-engine-172 established the same
private family for physical `border-radius` and the effective parent style walk.
Before this slice, the complete border parser accepted bounded concrete forms,
omitted-component `none`/`hidden`, `currentColor`, and `revert-layer`, but
rejected standalone CSS-wide values. This slice closes that parser and
projection gap without changing the public border surface.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#defaulting-keywords>
- <https://www.w3.org/TR/css-backgrounds-3/#border-shorthands>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-172.md`
- `docs/plan/tasks/native-engine-159.md`
- `docs/plan/tasks/native-engine-169.md`
- `docs/plan/tasks/native-engine-170.md`
- `docs/plan/tasks/native-engine-171.md`

## Contract

### Declaration and projection state

- Each complete physical border shorthand accepts its existing concrete
  bounded grammar, standalone case-insensitive `revert-layer`, and one exact
  case-insensitive token each for `inherit`, `unset`, `initial`, and `revert`.
  CSS-wide values cannot be mixed with width, style, color, `currentColor`,
  or omitted-component tokens.
- A private complete-border declaration distinguishes concrete painted,
  `none`, `hidden`, and CSS-wide values until it is projected into the
  existing independent width/style/color candidate streams. No declaration
  sentinel may reach public computed values, border commands, capture bytes,
  raster data, or diagnostics transport.
- Explicit `inherit` projects inherited width, style, and color candidates for
  the affected physical sides. It copies the parent's effective side values,
  including private `none`/`hidden`, zero-width, and unpainted effective
  states. `border:inherit` projects all four sides; a physical side shorthand
  projects only its corresponding side.
- `unset`, `initial`, and one-author-origin `revert` project the bounded local
  reset: width zero, private style `none`, and the current element color in the
  private color stream. This keeps the current no-paint fallback while allowing
  later independently cascaded component declarations to compose according to
  existing source-order rules.
- Ordinary omission remains ordinary omission: it creates no local complete
  border candidate and therefore retains the existing per-component fallback.
  `revert-layer` continues to expose lower candidates through the existing
  bounded layer resolver.

### Existing owners preserved

- Concrete complete-border parsing, `none`/`hidden` private distinctions,
  `currentColor` substitution, independent side declaration order, shorthand/
  longhand precedence, width/style/color composition, box-model geometry,
  display-list replay, clipping, opacity, scrolling, software raster, PNG
  capture, point-hit, and semantic/source order remain downstream owners.
- No public schema, dependency, feature default, layout algorithm, or crate
  boundary changes are allowed. `native-engine` remains default-off inside
  `glass-browser`; the workspace remains exactly `glass-browser` and
  `glass-dev`.
- The slice remains local-resource-only, fixture-relative, horizontal-tb,
  integer-pixel, fixed-cell, non-table, and explicitly non-browser-parity.

## Tradeoffs

- Reusing the existing three component projections keeps this slice small and
  preserves the hard-won rule that complete shorthands do not bypass the
  width/style/color cascade owners.
- Reset color is retained privately as `currentColor` even though zero width
  and `none` suppress current paint; this preserves the existing component
  composition behavior if an independently declared later width/style makes a
  border paintable.
- CSS-wide values remain property-local rather than introducing a generic
  value graph. Multiple origins, `!important` inversion, and browser defaults
  remain outside the deterministic one-author-origin boundary.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, product docs, and
  issue records

## Verification

The completed slice must cover:

- case-insensitive parser acceptance for all four keywords on `border` and
  each physical side shorthand alongside concrete, omitted-component,
  `currentColor`, and `revert-layer` forms; typed rejection of mixed,
  incomplete, unsupported, and malformed values; and valid-before-invalid
  preservation;
- explicit complete-shorthand inherit across all four parent sides, physical
  side-shorthand inherit, reset and omission behavior, inherited `none`/
  `hidden`/zero-width/unpainted states, named-layer/unlayered/inline
  precedence, and deterministic physical side order; and
- unchanged width/style/color composition through outer/content geometry,
  border commands, fill/raster replay, clipping, opacity, scrolling, capture,
  point-hit, semantic/source order, diagnostics, and public/private separation.

Focused feature check/tests, full native integration/library tests, strict
affected-package Clippy, rustdoc, paired-crate check/build, packaging,
formatting, documentation, final static gates, workspace all-target/
all-feature testing, and bounded regenerable-target cleanup are required.
Remote CI remains unclaimed until an explicitly authorized push.

## Implementation

Completed in `f5f53cec` (`feat(native-engine): support complete border css-wide
keywords`). `NativeBorderDeclaration` now retains exact case-insensitive
`inherit`, `unset`, `initial`, and one-author-origin `revert` values privately
for the complete `border` and four physical side shorthands. The parser keeps
CSS-wide values standalone, preserving concrete, omitted-component,
`currentColor`, and `revert-layer` grammars while rejecting mixed forms and
preserving valid-before-invalid declarations.

The existing width/style/color projection owners now translate explicit
`inherit` into the effective parent side streams and translate reset forms into
zero-width, private `none`, and `currentColor` candidates. The existing
computed-style composition then retains later independent component
declarations, side order, layer/unlayered/inline precedence, and private
none/hidden/zero/unpainted behavior. The new integration fixture exercises
layout/content geometry, display-list paint, raster/PNG capture, point hit
testing, semantics, diagnostics, invalid preservation, and component
composition. No public schema, dependency, feature default, or crate boundary
changed.

## Certification evidence

The locked focused native-feature check passed. Focused parser/cascade tests
passed 2/2, the focused complete-border consumer fixture passed 1/1, affected
border library regressions passed 23/23, affected border integration
regressions passed 25/25, full native integration passed 211/211, and the
feature-enabled `glass-browser` library passed 975 with 1 ignored. Strict
all-target/all-feature Clippy with warnings denied, warning-denied native
feature rustdoc, and paired locked `glass-dev` check/build passed.

Locked packages were generated for both crates: `glass-browser` contained 196
files and `glass-dev` 69 files. The packaged-dependency checker confirmed
`glass-dev` resolves `glass-browser` exactly at `0.3.14` without a path or
feature dependency; Cargo emitted only the existing yanked `chacha20` warning.
Static gates passed at source version `0.3.14`: version sync, feature parity
(14 capabilities across 4 targets), release documentation (587 Markdown
documents, 83 current documents, 57 previous-version hits, 677 semantic audit
hits, 0 current-claim failures), documentation coverage (587 Markdown/345
full-product MCP tools/100 browser-only tools/17 examples/22 public modules),
depth (93 current guides/19 substantive contracts), TUI shortcut parity
(15/63), reliability (6 scenarios/4 targets), public read-only adapters (5),
Web IR (8 fixtures/8 scenarios/11 categories), and GitHub release records (40
published tags/4 retained failed candidates).

The locked workspace all-target/all-feature replay passed with 976 library
tests and 1 ignored, 211 native integration tests, 365 `glass-dev` tests, and
all auxiliary targets. No remote CI, push, release, tag, registry publication,
browser-parity, security-boundary, or promotion claim is made.

## Cleanup

Pending exact-path regenerable-output cleanup after all Cargo processes and
open-handle checks complete.
