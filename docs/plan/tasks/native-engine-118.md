---
id: native-engine-118
scope: glass-browser/native-engine/text-decoration-skip-spaces-initial
status: complete
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

The parser and diagnostic support recognize the case-insensitive single token
`initial` and map it to `StartAndEnd`. Parser, inheritance/cascade, and native
raster integration regressions prove that explicit `initial` overrides
inherited values while omitted declarations preserve `None`. The
implementation checkpoint is `6f8e89fc`.

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

The focused CSS parser/cascade filter passed 2/2 tests and the focused native
integration filter passed 4/4 tests. The final feature-enabled browser
library passed 915 tests with 1 ignored and 0 failures; the full native
integration suite passed 155/155 tests. The serial `glass-dev` suite passed
365 unit tests, 4 integration tests, 15 PTY tests, and 1 doctest.

Workspace all-feature Clippy and no-default-feature browser Clippy passed with
warnings denied. Warning-denied workspace rustdoc and the all-feature debug
workspace build passed; the build emitted the existing non-fatal duplicate
`glass-browser`/`.dwp` output-name warning between the two packages.

Locked `glass-browser` packaging passed with 196 files and a 5.1 MiB archive.
A local no-verify `glass-dev` archive passed with 69 files and a 2.6 MiB
archive; the packaged dependency validator confirmed an exact
`glass-browser 0.3.14` dependency. The public-registry verification
limitation is unchanged: immutable published `glass-browser 0.3.14` lacks the
current `BrowserRuntime`, `browser_runtime`, and `browser_endpoint` API
required by `glass-dev 0.3.14`; no upload was attempted. The known yanked
`chacha20 0.10.1` lockfile warning remained non-fatal.

Locked fuzz fetch plus offline all-target checking passed. `cargo deny check`
passed with the existing duplicate-dependency warnings, and `cargo audit`
passed with the repository's four already-allowed warnings: unmaintained
`bincode`, unmaintained `yaml-rust`, the recorded `lru` unsoundness advisory,
and yanked `chacha20`. Formatting and diff checks passed.

The final static audit passed with 532 Markdown files, 83 current-version
documents, 57 previous-version hits, 615 semantic hits, and zero current-claim
failures. Coverage passed with 345 full-product MCP tools, 100 browser-only
tools, 17 examples, and 22 public modules; TUI passed 15/63, depth 93/19,
reliability 6/4, adapters 5, and Web IR 8/8/11. Remote CI, browser parity,
release, registry publication, and a third crate are not claimed by this
local checkpoint.

## Cleanup

The final exact inventory found the single `/tmp/glass-118-focused` target at
10,488,023,187 bytes across 15,389 files (about 9.77 GiB), plus
`/tmp/glass-118-release-doc.json` (171,674 bytes),
`/tmp/glass-118-reliability.json` (3,482 bytes),
`/tmp/glass-118-adapters.json` (1,418 bytes), and
`/tmp/glass-118-web-ir.txt` (3,934 bytes). No Cargo, rustc, rustdoc, Clippy,
fuzz, rust-analyzer, or Glass test writer and no open descriptor referenced
those exact candidates. Bounded deletion removed the target and reports after
the writer/open-file checks, and the empty repository `target` directory was
absent. Two unrelated already-removed OpenClaw temporary entries raced with
the bounded `/tmp` scan; no Glass candidate was affected. Shared Cargo
registries, toolchains, source, durable data, and other projects were not
touched. The three pre-existing Glass processes were preserved. After cleanup,
all exact candidates were absent and `/` was 59% used with 85,520,547,840
bytes available (about 80 GiB).
