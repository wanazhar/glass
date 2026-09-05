---
id: native-engine-118
scope: glass-browser/native-engine/text-decoration-skip-spaces-initial
status: planned
depends-on: [native-engine-117]
---

# Native bounded text-decoration skip-spaces initial keyword

## Objective

Accept the explicit CSS-wide `initial` keyword for the existing inherited
`text-decoration-skip-spaces` property. Resolve it to the property’s bounded
`start end` value without changing the native engine’s established omitted
declaration fallback of `none`.

## Context

The 117 slice owns Unicode whitespace classification for the inherited
`text-decoration-skip-spaces` value and reuses the 116 line-edge provenance.
The finite native value already contains `StartAndEnd`, and the cascade already
supports an explicitly parsed value overriding an inherited value. This slice
adds the smallest missing initial-value boundary at the parser/cascade owner;
it does not add another style or raster owner.

The normative property reference is:

- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-skip-spaces-property>

Read with the native-engine contract in:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-117.md`

## Contract

The property parser accepts case-insensitive `initial` as one explicit
single-token value and maps it to `NativeTextDecorationSkipSpaces::StartAndEnd`.
An explicit `initial` declaration therefore resets an inherited `none` or
`all` value to both line-edge skip modes. Omission remains the native
engine’s explicit `None` fallback so existing documents do not change merely
because this keyword is now recognized. The existing `none`, `all`, `start`,
`end`, and unordered `start end` values remain unchanged.

The keyword is supported only for this property in this slice. `inherit`,
`unset`, `revert`, `revert-layer`, duplicate or mixed token forms, unknown
values, and empty values remain bounded typed diagnostics without raw
stylesheet echo. No general CSS-wide keyword machinery is introduced.

The resolved value continues through the existing inherited computed style,
immutable text command, line-edge provenance, Unicode whitespace classifier,
and software decoration replay. Layout, fixed-cell geometry, whitespace
collapsing, spacing arithmetic, wrapping, alignment, overflow, clipping,
scrolling, opacity, capture, hit testing, semantics, and source order retain
their existing owners.

This remains a horizontal-tb, fixed-cell, local software-raster contract. It
does not claim a changed omitted initial value, general CSS-wide keyword
support, decoration-origin propagation, atomic-inline or cross-fragment
continuity, font metrics, shaping, bidi, vertical writing, antialiasing,
browser text-paint parity, or browser-wide CSS conformance.

## Tradeoffs

- Reusing `StartAndEnd` keeps the explicit initial path aligned with the
  property grammar and avoids a duplicate computed-value representation, but
  the finite enum cannot distinguish an explicit pair from an explicit
  `initial` after cascade resolution.
- Keeping omission as `None` preserves existing experimental-engine output,
  but it intentionally differs from treating every omitted declaration as the
  standards initial value; callers must write `initial` to request the
  standards-oriented bounded value.
- Accepting only `initial` keeps the change auditable and local, but
  inheritance-control keywords remain unsupported until a separately designed
  cascade-wide contract exists.
- No dependency, layout path, display-list field, or crate boundary changes,
  so the build surface remains stable and the feature stays default-off.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Implementation

The parser and diagnostic support will recognize the case-insensitive single
token `initial` and map it to `StartAndEnd`. Parser, inheritance/cascade, and
native raster integration regressions will prove that explicit `initial`
overrides inherited values while omitted declarations preserve `None`.

## Verification

Focused tests must prove parser acceptance, diagnostic acceptance, explicit
initial resolution through inheritance and inline precedence, and rejection of
the other CSS-wide keywords. Native integration must prove both leading and
trailing Unicode whitespace are skipped for explicit `initial`, while an
omitted declaration and an inherited `none` retain their existing output.
Full native, feature-library, strict lint, warning-denied rustdoc, locked
package, dependency, offline fuzz, documentation, static, security, and
formatting gates remain required.

Every local gate uses an isolated or intentionally shared target with a
recorded purpose. Completed validation removes exact regenerable output only
after active-writer and open-file checks. Remote CI, browser parity, release,
registry publication, and a third crate are not claimed by this local task.

## Certification

To be filled after implementation and the complete local gate pass. The task
must record exact test counts, package/dependency evidence, static audit
results, known non-fatal warnings, and cleanup inventory. The epic remains
local-only unless the branch is explicitly pushed and remote CI is observed.

## Cleanup

To be filled after final certification. Only exact regenerable targets and
reports created for this slice may be removed, after process and open-file
checks. Shared Cargo registries, toolchains, source, durable data, other
projects, and pre-existing Glass processes must be preserved.
