---
id: native-engine-121
scope: glass-browser/native-engine/text-decoration-skip-spaces-revert
status: planned
depends-on: [native-engine-120]
---

# Native bounded text-decoration skip-spaces revert keyword

## Objective

Accept the explicit CSS-wide `revert` keyword for the existing inherited
`text-decoration-skip-spaces` property. Resolve it through the native engine's
current single-author-origin cascade boundary: a non-root declaration falls
back to the inherited parent computed value, while a root declaration falls
back to the native root `None` value.

## Context

The 119 and 120 slices added private declaration-only states for `inherit` and
`unset` without widening the public paint enum. `revert` is the next keyword
that can be modeled without inventing an origin or layer system: this engine
currently has one bounded author-style cascade plus its established omitted
property fallback. A winning `revert` declaration therefore removes the
property's author declaration at the current element and exposes that existing
inherited fallback. The declaration-only state remains distinct so future
origin/layer work can refine it without changing the artifact contract.

The normative property reference is:

- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-skip-spaces-property>

Read with the native-engine contract in:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-120.md`

## Contract

The property parser accepts case-insensitive `revert` as one explicit
single-token declaration value. In the current bounded cascade, the winning
declaration resolves to the inherited `NativeInheritedStyle::text_decoration_skip_spaces`
value supplied by the DOM parent-style walk. At the root, the same boundary
resolves to `NativeTextDecorationSkipSpaces::None`. Stylesheet and inline
precedence remain governed by the existing specificity/order rules; a winning
inline `revert` can therefore expose the parent value over an earlier
stylesheet value on the same element.

The finite `NativeTextDecorationSkipSpaces` value remains a resolved
`None|All|Start|End|StartAndEnd` paint value. `revert` is represented only
inside CSS declaration/cascade state and cannot reach `NativeDisplayCommand` or
the software rasterizer. Existing `none`, `all`, `start`, `end`, unordered
`start end`, `initial`, `inherit`, and `unset` behavior remains unchanged.

`revert-layer`, `@layer` ordering, user-origin and user-agent-origin style
stores, duplicate or mixed token forms, unknown values, and empty values
remain unsupported typed diagnostics without raw stylesheet echo. This slice
does not claim complete CSS origin semantics; it makes the one-origin fallback
explicit and preserves a future extension point.

The resolved value continues through the existing inherited computed style,
immutable text command, line-edge provenance, Unicode whitespace classifier,
and software decoration replay. Layout, fixed-cell geometry, whitespace
collapsing, spacing arithmetic, wrapping, alignment, overflow, clipping,
scrolling, opacity, capture, hit testing, semantics, and source order retain
their existing owners.

This remains a horizontal-tb, fixed-cell, local software-raster contract. It
does not claim `revert-layer`, CSS cascade layers, multiple style origins,
declaration-wide CSS keyword machinery, changed omitted-value behavior,
decoration-origin propagation, atomic-inline or cross-fragment continuity,
font metrics, shaping, bidi, vertical writing, antialiasing, browser text-paint
parity, or browser-wide CSS conformance.

## Tradeoffs

- A private `Revert` state preserves the distinction between an author reset
  and an ordinary inherited declaration, at the cost of one declaration state.
- Treating `revert` as the current inherited fallback is exact for this
  engine's one-author-origin model, but it is deliberately not presented as a
  general implementation of CSS origin or layer precedence.
- Supporting `revert` before `revert-layer` keeps the cascade boundary honest;
  layers need their own source-order and rollback contract and are not
  approximated.
- No dependency, layout path, display-list field, default feature, or crate
  boundary changes, so the build surface remains stable and the feature stays
  default-off.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Implementation

Pending. Extend the private declaration state and parser, resolve `revert`
through the existing inherited-style boundary, and add parser/cascade and
display-list/raster regressions without changing the public resolved value.

## Verification

Focused tests must prove case-insensitive declaration parsing, stylesheet and
inline precedence, inherited and root fallback resolution, and continued
diagnostic rejection of `revert-layer`. Native integration must prove a
winning explicit `revert` exposes the parent decoration mode through
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
