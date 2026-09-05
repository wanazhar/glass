---
id: native-engine-119
scope: glass-browser/native-engine/text-decoration-skip-spaces-inherit
status: planned
depends-on: [native-engine-118]
---

# Native bounded text-decoration skip-spaces inherit keyword

## Objective

Accept the explicit CSS-wide `inherit` keyword for the existing inherited
`text-decoration-skip-spaces` property. Resolve it from the winning child
declaration to the already-computed parent value without allowing a
declaration-only state to escape into display-list or raster replay.

## Context

The 118 slice added the explicit `initial` reset boundary while deliberately
leaving general CSS-wide keyword machinery outside the native contract. The
property is already inherited through `NativeInheritedStyle`, and the
declaration cascade already has one winning slot for stylesheet and inline
values. This slice adds only the next dependency-ordered keyword: a private
declaration representation distinguishes `inherit` from the finite resolved
paint enum, while the existing computed-style and artifact owners remain
unchanged.

The normative property reference is:

- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-skip-spaces-property>

Read with the native-engine contract in:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-118.md`

## Contract

The property parser accepts case-insensitive `inherit` as one explicit
single-token declaration value. The winning declaration resolves to the
`NativeInheritedStyle::text_decoration_skip_spaces` value supplied by the
DOM parent-style walk. It therefore preserves the parent value for stylesheet
and inline declarations, including when the explicit child declaration wins
over a different child rule.

The finite `NativeTextDecorationSkipSpaces` value remains a resolved
`None|All|Start|End|StartAndEnd` paint value. `inherit` is represented only
inside CSS declaration/cascade state and cannot reach `NativeDisplayCommand`
or the software rasterizer. Existing `none`, `all`, `start`, `end`, unordered
`start end`, and `initial` behavior remains unchanged. Omission continues to
use the native engine's established inherited fallback; on the root this is
`None`.

`unset`, `revert`, `revert-layer`, duplicate or mixed token forms, unknown
values, and empty values remain unsupported typed diagnostics without raw
stylesheet echo. No general CSS-wide keyword machinery is introduced.

The resolved value continues through the existing inherited computed style,
immutable text command, line-edge provenance, Unicode whitespace classifier,
and software decoration replay. Layout, fixed-cell geometry, whitespace
collapsing, spacing arithmetic, wrapping, alignment, overflow, clipping,
scrolling, opacity, capture, hit testing, semantics, and source order retain
their existing owners.

This remains a horizontal-tb, fixed-cell, local software-raster contract. It
does not claim `unset`, `revert`, `revert-layer`, declaration-wide CSS
keyword machinery, changed omitted-value behavior, decoration-origin
propagation, atomic-inline or cross-fragment continuity, font metrics,
shaping, bidi, vertical writing, antialiasing, browser text-paint parity, or
browser-wide CSS conformance.

## Tradeoffs

- A private declaration-only enum keeps `inherit` out of the public resolved
  paint value and makes the raster boundary fail closed by construction, at
  the cost of one small conversion during cascade resolution.
- Resolving against the parent style at the existing computed-style boundary
  preserves the single DOM inheritance owner and avoids a second style graph,
  but it does not provide a reusable general CSS-wide keyword framework for
  other properties.
- Supporting only `inherit` keeps the slice auditable and dependency-ordered;
  `unset`, `revert`, and `revert-layer` remain explicit future contracts
  rather than being approximated with potentially incorrect defaults.
- No dependency, layout path, display-list field, default feature, or crate
  boundary changes, so the build surface remains stable and the feature stays
  default-off.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Implementation

Pending. The parser, declaration storage, cascade resolution, and focused
regressions will be updated together. Tests must cover case-insensitive
parser acceptance, stylesheet and inline precedence, parent-value resolution,
and the continued rejection/diagnostic behavior for the remaining unsupported
CSS-wide keywords.

## Verification

Focused tests must prove declaration parsing and parent/inline cascade
resolution. Native integration must prove an inherited `all`, `start`, `end`,
or `start end` value survives an explicit child `inherit` declaration through
display-list construction and raster replay, while a winning explicit value
still overrides it. Full native, feature-library, strict lint,
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
