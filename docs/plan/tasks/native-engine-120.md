---
id: native-engine-120
scope: glass-browser/native-engine/text-decoration-skip-spaces-unset
status: planned
depends-on: [native-engine-119]
---

# Native bounded text-decoration skip-spaces unset keyword

## Objective

Accept the explicit CSS-wide `unset` keyword for the existing inherited
`text-decoration-skip-spaces` property. Resolve it as inherited parent state,
matching the property’s inherited classification, while keeping declaration
keywords out of the resolved display-list and raster value.

## Context

The 119 slice introduced the private declaration-only representation needed to
distinguish `inherit` from the finite resolved paint enum. `unset` is the next
dependency-ordered keyword for this inherited property: its bounded behavior
can reuse the same parent-style resolution owner without changing layout,
painting, or the public command schema. This slice extends that private
declaration state rather than adding general CSS-wide keyword machinery.

The normative property reference is:

- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-skip-spaces-property>

Read with the native-engine contract in:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-119.md`

## Contract

The property parser accepts case-insensitive `unset` as one explicit
single-token declaration value. The winning declaration resolves to the
`NativeInheritedStyle::text_decoration_skip_spaces` value supplied by the
DOM parent-style walk. Because this property is inherited, explicit `unset`
has the same bounded result as explicit `inherit`: a parent `all`, `start`,
`end`, or `start end` value is preserved, while the root fallback is `None`.

The finite `NativeTextDecorationSkipSpaces` value remains a resolved
`None|All|Start|End|StartAndEnd` paint value. `unset` is represented only
inside CSS declaration/cascade state and cannot reach `NativeDisplayCommand`
or the software rasterizer. Existing `none`, `all`, `start`, `end`, unordered
`start end`, `initial`, and `inherit` behavior remains unchanged. Omission
continues to use the native engine’s established inherited fallback.

`revert`, `revert-layer`, duplicate or mixed token forms, unknown values, and
empty values remain unsupported typed diagnostics without raw stylesheet echo.
No general CSS-wide keyword machinery is introduced.

The resolved value continues through the existing inherited computed style,
immutable text command, line-edge provenance, Unicode whitespace classifier,
and software decoration replay. Layout, fixed-cell geometry, whitespace
collapsing, spacing arithmetic, wrapping, alignment, overflow, clipping,
scrolling, opacity, capture, hit testing, semantics, and source order retain
their existing owners.

This remains a horizontal-tb, fixed-cell, local software-raster contract. It
does not claim `revert`, `revert-layer`, declaration-wide CSS keyword
machinery, changed omitted-value behavior, decoration-origin propagation,
atomic-inline or cross-fragment continuity, font metrics, shaping, bidi,
vertical writing, antialiasing, browser text-paint parity, or browser-wide
CSS conformance.

## Tradeoffs

- Reusing the 119 private declaration representation makes `unset` resolve at
  the existing inheritance boundary and keeps the public paint enum finite, at
  the cost of one additional declaration state.
- Treating `unset` as inherited is correct for this inherited property, but
  this slice intentionally does not generalize that rule to every property in
  the engine.
- Supporting only `unset` next preserves an auditable dependency order;
  `revert` and `revert-layer` still require separate origin/layer contracts
  and are not approximated.
- No dependency, layout path, display-list field, default feature, or crate
  boundary changes, so the build surface remains stable and the feature stays
  default-off.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Implementation

Pending. Extend the private declaration state and parser, resolve `unset`
through the existing inherited-style boundary, and add parser/cascade and
display-list/raster regressions without changing the public resolved value.

## Verification

Focused tests must prove case-insensitive declaration parsing, stylesheet and
inline precedence, parent-value resolution, root fallback, and continued
diagnostic rejection of `revert` and `revert-layer`. Native integration must
prove an explicit child `unset` preserves a parent decoration mode through
display-list construction and raster replay while a winning explicit resolved
value still overrides it. Full native, feature-library, strict lint,
warning-denied rustdoc, locked package, dependency, offline fuzz,
documentation, static, security, and formatting gates remain required.

Every local gate uses an isolated or intentionally shared target with a
recorded purpose. Completed validation removes exact regenerable output only
after active-writer and open-file checks. Remote CI, browser parity, release,
registry publication, and a third crate are not claimed by this local task.

## Certification

Pending implementation and local validation.

## Cleanup

Pending implementation and local validation. Record the exact target/report
inventory, active-writer/open-file checks, bounded deletion, final absence
check, and filesystem headroom before closing the task.
