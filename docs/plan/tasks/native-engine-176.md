---
id: native-engine-176
scope: glass-browser/native-engine/cascade-logical-border-radius-corner-longhands
status: planned
depends-on: [native-engine-175]
---

# Native bounded logical `border-radius` corner longhands

## Objective

Add the four flow-relative corner longhands to the native CSS surface:
`border-start-start-radius`, `border-start-end-radius`,
`border-end-start-radius`, and `border-end-end-radius`. Each property accepts
the same bounded single integer-pixel radius or standalone CSS-wide keyword as
the completed physical corner longhands, then maps to the existing physical
four-corner candidate streams before the unchanged layout, display-list,
raster, PNG capture, point-hit, and semantic owners run.

This is the smallest follow-up to the completed physical corner declarations.
It uses the already-resolved horizontal-tb `direction` owner and closes the
logical-corner declaration gap without claiming vertical writing modes,
text-orientation mapping, elliptical geometry, percentages, or a second
rounded-rendering path.

## Context

Native-engine-020 established the physical bounded `border-radius` shorthand;
native-engine-172 added its CSS-wide forms; native-engine-175 added the four
physical corner longhands and per-corner cascade streams. Native-engine-174
established horizontal-tb logical border projection through inherited `ltr` or
`rtl` direction. The physical radius owner is now ready for a matching
flow-relative projection pass.

Normative references:

- <https://www.w3.org/TR/css-logical-1/#border-radius>
- <https://www.w3.org/TR/css-backgrounds-3/#border-radius>
- <https://www.w3.org/TR/css-cascade-5/#defaulting-keywords>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-174.md`
- `docs/plan/tasks/native-engine-175.md`
- `docs/plan/tasks/native-engine-020.md`

## Contract

### Supported declarations

- The four logical corner longhands accept one bounded non-negative integer
  `Npx` value using the existing radius dimension bound.
- Each longhand accepts standalone, case-insensitive `inherit`, `unset`,
  `initial`, `revert`, and `revert-layer`. CSS-wide keywords cannot be mixed
  with dimensions or additional tokens.
- Logical corner values compete with the physical `border-radius` shorthand
  and physical corner longhands through the existing per-corner cascade
  streams. Invalid values preserve the earlier valid declaration under the
  existing bounded diagnostic/fallback contract.
- This slice does not add two-value elliptical corners, slash-separated
  shorthand radii, percentages, negative or fractional values, variables,
  arbitrary tokenization, or an `all`/logical-radius shorthand.

### Horizontal-tb mapping

The native boundary resolves `direction` first and maps only horizontal-tb
flow-relative corners. The first name component is the block-axis edge and the
second is the inline-axis edge:

| Logical property | `direction: ltr` | `direction: rtl` |
| --- | --- | --- |
| `border-start-start-radius` | top-left | top-right |
| `border-start-end-radius` | top-right | top-left |
| `border-end-start-radius` | bottom-left | bottom-right |
| `border-end-end-radius` | bottom-right | bottom-left |

The mapping is based on the element's resolved native direction. An inherited
RTL descendant maps its own logical declarations to its own physical corner,
while a descendant with an explicit LTR direction maps back independently.
Vertical writing modes, `text-orientation`, and containing-block mapping are
not inferred from this table.

### Cascade and resolution

- Logical corner declarations are collected in four private logical candidate
  streams in start-start, start-end, end-start, end-end order, retaining rule,
  layer, specificity, source, inline, and declaration order metadata.
- After the current element's direction is resolved, each logical stream is
  projected into the selected physical corner stream. The existing physical
  resolver then handles normal precedence, CSS-wide reset/inheritance,
  `revert-layer`, and the default zero-corner fallback.
- A logical declaration and a physical declaration mapped to the same corner
  compete by the same cascade order. A later shorthand or corner longhand can
  override only its mapped corner; a logical `revert-layer` rolls back only
  that mapped corner.
- Explicit `inherit` copies the effective parent value for the selected
  physical corner after mapping. `unset`, `initial`, and `revert` resolve to the
  existing zero-corner reset behavior; omission remains the existing zero
  fallback.
- Declaration-only logical state must not reach `NativeComputedStyle`, public
  radius values, layout boxes, display commands, capture bytes, raster pixels,
  hit results, semantic output, or diagnostics transport.

## Existing owners preserved

- The existing `NativeBorderRadius` shape, conservative normalization, rounded
  fill/border replay, clipping, opacity, scrolling, PNG capture, software
  raster, point hit testing, and semantic/source-order owners remain the only
  downstream owners.
- No public schema, dependency, feature default, layout algorithm, or crate
  boundary changes are allowed. `native-engine` remains default-off inside
  `glass-browser`; the workspace remains exactly `glass-browser` and
  `glass-dev`.
- The slice remains local-resource-only, horizontal-tb, fixed-cell,
  integer-pixel, non-table, and explicitly non-browser-parity. Writing-mode
  variants, text orientation, percentages, elliptical corners, animations,
  multiple origins, `!important` inversion, and browser-wide logical-radius
  conformance remain outside the boundary.

## Tradeoffs

- A private logical candidate array plus a direction-dependent projection keeps
  physical and logical declarations in one cascade owner, at the cost of a
  small pre-resolution mapping pass and four additional private slots.
- Supporting ltr/rtl horizontal-tb gives useful flow-relative coverage while
  making the unsupported writing-mode boundary explicit. Silently treating
  vertical writing as horizontal would produce the wrong corner and is not an
  acceptable fallback.
- Reusing the physical corner value parser and CSS-wide resolver avoids a new
  syntax or rendering path. The tradeoff is deliberate rejection of the
  standards' two-value elliptical corner grammar until geometry has an explicit
  bounded representation.
- No dependency is added. All behavior remains inside `glass-browser`'s
  default-off native feature, preserving the two-crate build boundary and
  current build profiles.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Verification

- Format and run one locked native-feature `cargo check` before tests in an
  isolated task target.
- Run focused parser/cascade unit coverage for all four logical properties,
  case-insensitive CSS-wide keywords, invalid preservation, ltr/rtl mapping,
  physical/logical precedence, source order, inline precedence, inheritance,
  and per-corner `revert-layer` rollback.
- Run one focused integration fixture covering ltr and rtl descendants,
  logical-to-physical corner values, layout geometry, rounded display commands,
  decoded raster/PNG capture, point-hit behavior, semantic/source order, and
  bounded unsupported diagnostics.
- Near completion run full native integration and feature-enabled browser
  library tests, strict affected-package Clippy, warning-denied rustdoc,
  paired locked `glass-dev` check/build, both package flows, security/fuzz
  checks, repository static/workspace gates, and bounded cleanup.
- Record exact command results, counts, known warnings, remote-CI boundary,
  issue update, and cleanup evidence here before marking the task complete.

## Cleanup

Task-specific isolated targets and reports are safe to remove only after all
Cargo/Rust processes and open handles exit. Only exact paths created for this
task may be reclaimed; source, durable data, repository history, issue
snapshots, and unrelated workloads must remain untouched. Cleanup evidence
will record exact paths, measured bytes/files, process/open-handle checks,
post-delete absence, and filesystem free-space delta.

## Evidence

Pending implementation, local certification, documentation synchronization,
issue update, and bounded regenerable-output cleanup. No remote CI, push,
release, tag, registry-publication, browser-parity, security-boundary, or
promotion claim is made while the checkout remains local-only.
