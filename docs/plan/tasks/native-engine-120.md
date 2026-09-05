---
id: native-engine-120
scope: glass-browser/native-engine/text-decoration-skip-spaces-unset
status: complete
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

Complete at `897bd648`. The private declaration state now distinguishes
`Unset`; the parser accepts the keyword case-insensitively, computed-style
resolution maps it to the existing inherited parent value, and the public
resolved enum, display-list, and rasterizer remain unchanged. The former
root-level `unset` fixture now verifies its `None` fallback, while a separate
fixture keeps `revert` and `revert-layer` diagnostics covered.

## Verification

Focused tests must prove case-insensitive declaration parsing, stylesheet and
inline precedence, parent-value resolution, root fallback, and continued
diagnostic rejection of `revert` and `revert-layer`. Native integration must
prove an explicit child `unset` preserves a parent decoration mode through
display-list construction and raster replay while a winning explicit resolved
value still overrides it. Full native, feature-library, strict lint,
warning-denied rustdoc, locked package, dependency, offline fuzz,
documentation, static, security, and formatting gates remain required.

Every local gate used the single isolated `/tmp/glass-120-focused` target so
feature, two-crate, package, and security checks could reuse artifacts without
creating a repository `target/`. Focused CSS and native tests passed 2/2 and
6/6. The full feature browser library passed 915 tests with 1 ignored; the
full native integration suite passed 157/157; and the `glass-dev` suite passed
365 unit tests, 4 integration tests, 15 PTY tests, and 1 doctest. All-feature
and no-default strict Clippy passed with `-D warnings`; warning-denied
workspace rustdoc and the all-feature debug build passed. Locked package
verification passed for `glass-browser` (196 files, 5.1 MiB); local no-verify
packaging passed for `glass-dev` (69 files, 2.6 MiB), and its packaged
dependency resolves `glass-browser` exactly at 0.3.14. The locked offline fuzz
all-target check, cargo-deny, cargo-audit, formatting, version/feature/TUI
parity, documentation truth/depth/coverage, reliability, adapter, and Web IR
checks all passed. Cargo emitted only the known duplicate dependency and
yanked `chacha20` warnings, while audit reported only the four already-allowed
warnings. Remote CI, browser parity, release, registry publication, and a
third crate are not claimed by this local task.

## Certification

Certified locally on 2026-09-05. Static documentation truth reported 534
Markdown documents, 83 current-version documents, 57 previous-version hits,
621 semantic audit hits, and 0 current-claim failures; coverage reported 345
full-product MCP tools (100 browser-only), 17 examples, and 22 public
modules. Reliability reported 6 scenarios across 4 targets without claiming
runtime certification, and the public read-only adapter inventory reported 5
adapters without claiming runtime certification. The immutable public
`glass-browser` 0.3.14 registry surface still lacks the current
`BrowserRuntime`, `browser_runtime`, and `browser_endpoint` APIs required for a
registry-backed `glass-dev` verification, so no upload was attempted.

## Cleanup

Before cleanup, the exact disposable inventory was one
`/tmp/glass-120-focused` tree at 10,460,390,372 bytes by `du -sb` containing
15,269 files, four bounded reports totaling 181,455 bytes, and 78 named test
scratch/lock entries totaling 2,943 apparent bytes. No Cargo, rustc, rustdoc,
Clippy, fuzz, rust-analyzer, or Glass test writer was active; `lsof +D /tmp`
reported no open handles. The target, reports, and generated test scratch
entries were removed with bounded exact-path deletion. The repository
`/home/ubuntu/work/glass/target` was absent before and after cleanup. The
pre-existing Glass processes 590083, 610599, and 611107 were left running;
they reference already-deleted old target executables and were not terminated.
All shared Cargo registries, toolchains, source trees, and other projects were
left untouched. Final absence and filesystem headroom are recorded in the
closeout comment and issue update.
