---
id: native-engine-121
scope: glass-browser/native-engine/text-decoration-skip-spaces-revert
status: complete
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

Complete at `f361415a`. Extended the private declaration state and parser with
case-insensitive `Revert`, resolved it through the existing inherited-style
boundary, and added parser/cascade plus display-list/raster regressions without
changing the public resolved value or any dependency, feature, or crate
boundary. `revert-layer` remains an explicit unsupported diagnostic.

## Verification

Focused unit coverage passed 2/2 parser/cascade tests and focused native
integration coverage passed 7/7 tests. The full native feature library passed
915 tests with 1 ignored test, and the native integration suite passed 158/158.
The locked two-crate suite passed 365 `glass-dev` unit tests, 4 development
runtime integration tests, 15 PTY tests, and 1 doctest. Strict all-feature
Clippy passed for both packages and all targets (browser 8:16, dev 7:02),
browser no-default-features Clippy passed (4:38), warnings-denied workspace
rustdoc passed (3:11), and the all-feature workspace debug build passed (8:30)
with the existing duplicate `glass-browser` binary filename warning.

Locked package validation passed for `glass-browser` (196 files, 5.1 MiB,
including the 15:09 unpacked-source verification) and the local-patched
`glass-dev` package (69 files, 2.6 MiB); the packaged dependency checker
confirmed `glass-browser` exactly at 0.3.14. Locked offline fuzz all-target
checking passed (8:11). `cargo deny check` passed with the existing duplicate
dependency warnings, and `cargo audit` passed with the four configured
warnings: unmaintained `bincode`, unmaintained `yaml-rust`, the allowed `lru`
unsoundness advisory, and the yanked `chacha20` release.

Formatting and static validation passed: feature parity (14 capabilities / 4
targets), TUI shortcuts (15 keys / 63 markers), documentation depth (93 guides
/ 19 contracts), release documentation truth (535 Markdown documents, 83
current, 57 previous-version hits, 624 semantic hits, 0 current-claim
failures), documentation coverage (345 full-product MCP tools, 100
browser-only, 17 examples, 22 public modules), reliability (6 scenarios / 4
targets), read-only adapters (5), and Web IR (8 fixtures / 8 scenarios / 11
categories). Remote CI, browser parity, release, registry publication, and a
third crate remain outside this local certification boundary.

Every local gate uses an isolated or intentionally shared target with a
recorded purpose. Completed validation removes exact regenerable output only
after active-writer and open-file checks. Remote CI, browser parity, release,
registry publication, and a third crate are not claimed by this local task.

## Certification

Certified locally at `f361415a` under the bounded one-author-origin contract.
The public resolved value remains finite and unchanged; root `revert` resolves
to `None`, descendants resolve to the existing inherited parent value, and
`revert-layer` remains unsupported. No remote CI or cross-browser conformance
claim is made.

## Cleanup

The isolated target `/tmp/glass-121-focused` measured 11,079,259,234 bytes
across 15,540 files before deletion. Exact generated reports were
`/tmp/glass-121-release-doc.json`, `/tmp/glass-121-reliability.json`,
`/tmp/glass-121-adapters.json`, `/tmp/glass-121-glass-help.txt`, and
`/tmp/glass-121-browser-help.txt`; the Web IR report was the exact generated
repository file `benchmarks/results/validation/native-engine-121-web-ir.json`.
The test suite also left only the exact PID-1616141 context, TUI,
tool-router, trust, and PID-1630343 development-runtime scratch entries in
`/tmp`, all regenerable.

Before deletion, no Cargo, rustc, rustdoc, Clippy, cargo-fuzz,
rust-analyzer, or Glass test writer was active and `lsof +D /tmp` showed no
open handle for the target or reports. The three pre-existing Glass processes
(PIDs 590083, 610599, and 611107) were not terminated; they reference deleted
historical repository binaries and were outside this cleanup scope. The exact
target, report, Web IR, and scratch paths were deleted with bounded `find -P`
cleanup. Final checks found no current `/tmp/glass-*` candidates, no
repository `target/`, and no open handles. Filesystem availability increased
from 74,315,735,040 to 85,443,485,696 bytes, reclaiming 11,127,750,656 bytes.
No process was terminated; the three pre-existing Glass processes remained
outside this cleanup scope.
