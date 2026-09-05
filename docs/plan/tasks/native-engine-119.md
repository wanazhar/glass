---
id: native-engine-119
scope: glass-browser/native-engine/text-decoration-skip-spaces-inherit
status: complete
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

The parser, declaration storage, and cascade now use
`NativeTextDecorationSkipSpacesDeclaration` to keep `Inherit` private to CSS
declaration state. The computed-style owner resolves that state against the
supplied `NativeInheritedStyle`, while the public resolved enum and display
list remain finite. Parser/cascade and native raster regressions are recorded
at implementation checkpoint `1118bf2b`.

## Verification

Focused tests proved case-insensitive declaration parsing and parent/inline
cascade resolution. Native integration proved stylesheet and inline
`inherit` preserve a parent `all` value through display-list construction and
raster replay, while an explicit child `none` still overrides it. Full native,
feature-library, strict lint, warning-denied rustdoc, locked package,
dependency, offline fuzz, documentation, static, security, and formatting
gates passed.

Every local gate uses an isolated or intentionally shared target with a
recorded purpose. Completed validation removes exact regenerable output only
after active-writer and open-file checks. Remote CI, browser parity, release,
registry publication, and a third crate are not claimed by this local task.

## Certification

Focused CSS parser/cascade tests passed 2/2 and focused native integration
tests passed 5/5. The full feature-enabled `glass-browser` library passed 915
tests with 1 ignored and 0 failures; the full native integration suite passed
156/156. Serial `glass-dev` passed 365 unit tests, 4 integration tests, 15 PTY
tests, and 1 doctest.

All-feature workspace Clippy and no-default-feature browser Clippy passed with
warnings denied. Warning-denied workspace rustdoc and the all-feature debug
workspace build passed; the build emitted the existing non-fatal duplicate
`glass-browser`/`.dwp` output-name warning between the two packages.

Locked `glass-browser` packaging passed with 196 files and a 5.1 MiB archive.
A local no-verify `glass-dev` archive passed with 69 files and a 2.6 MiB
archive; the packaged dependency validator confirmed an exact
`glass-browser 0.3.14` dependency. The public-registry limitation is
unchanged: immutable published `glass-browser 0.3.14` lacks the current
`BrowserRuntime`, `browser_runtime`, and `browser_endpoint` APIs required by
`glass-dev 0.3.14`; no upload was attempted. The known yanked `chacha20
0.10.1` lockfile warning remained non-fatal.

Locked fuzz fetch plus offline all-target checking passed. `cargo deny check`
passed with the existing duplicate-dependency warnings, and `cargo audit`
passed with the four already-allowed warnings: unmaintained `bincode`,
unmaintained `yaml-rust`, the recorded `lru` unsoundness advisory, and yanked
`chacha20`.

Formatting and diff checks passed. The final static audit passed with 533
Markdown files, 83 current-version documents, 57 previous-version hits, 620
semantic hits, and zero current-claim failures. Coverage passed with 345
full-product MCP tools, 100 browser-only tools, 17 examples, and 22 public
modules; TUI passed 15/63, depth 93/19, reliability 6/4, adapters 5, and Web
IR 8/8/11. Remote CI, browser parity, release, registry publication, and a
third crate are not claimed by this local checkpoint.

## Cleanup

The final exact inventory found `/tmp/glass-119-focused` at 10,460,150,421
bytes across 15,268 files (about 9.74 GiB). Generated reports measured
171,934 bytes for the release-documentation report, 3,482 bytes for the
reliability report, 1,418 bytes for the adapter report, and 3,934 bytes for
the Web IR log. No Cargo, rustc, rustdoc, Clippy, fuzz, rust-analyzer, or
Glass test writer and no open descriptor referenced the target. The
repository `target/` was absent. The exact target and four reports were
removed with bounded deletion after the writer/open-file checks. The same
cleanup removed 222 empty/small Glass test-scratch directories totaling 7,833
bytes and the 12 remaining generated trust/lock files totaling 994 bytes;
the final bounded candidate scan is empty. Filesystem headroom is 59% used
with 85,498,941,440 bytes available (about 80 GiB). The three pre-existing
Glass processes with deleted target paths were preserved; no process was
killed. Shared Cargo registries, toolchains, source, durable data, and other
projects were not touched.
