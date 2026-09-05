---
id: native-engine-122
scope: glass-browser/native-engine/cascade-layers-revert-layer
status: complete
depends-on: [native-engine-121]
---

# Native bounded cascade layers and `revert-layer`

## Objective

Add a bounded, deterministic CSS cascade-layer model to the existing native
stylesheet owner and support the explicit `revert-layer` keyword for the
inherited `text-decoration-skip-spaces` property. This advances the cascade
boundary without widening the public paint value or introducing a second style
system.

## Context

The 119, 120, and 121 slices added declaration-only handling for
`inherit`, `unset`, and `revert` while keeping the resolved
`NativeTextDecorationSkipSpaces` enum finite. `revert-layer` needs a real layer
ordering and rollback boundary; treating it as another spelling of `revert`
would erase the distinction this slice is meant to establish.

The normative property reference is:

- <https://www.w3.org/TR/css-cascade-5/#layer-order>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-skip-spaces-property>

Read with the native-engine contract in:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-121.md`

## Contract

### Layer collection and priority

- The parser accepts only top-level named blocks of the form
  `@layer <identifier> { ... }`.
- A named layer's rank is assigned by first appearance across the stylesheet
  source list. Reopening the same name reuses its rank and contributes normal
  source order within that layer.
- The implementation is bounded to 15 named layers. The implicit unlayered
  author bucket is always above those named layers for ordinary declarations.
- A rule inside a named layer participates in the existing selector matching,
  specificity, source-order, and inline-style paths. Layer priority is inserted
  before selector specificity, so a later named layer wins over an earlier one
  even when the earlier selector is more specific. Within one layer, the
  existing precedence rules remain unchanged.
- The layer rank is private cascade metadata. It must not enter
  `NativeDisplayCommand`, layout geometry, raster data, semantic output, or any
  public API.
- Inline declarations remain in the unlayered bucket and therefore outrank
  ordinary named-layer declarations through the existing inline precedence.
- `@layer` statements, anonymous layers, comma-separated layer names, nested
  layer blocks, and other at-rules remain unsupported typed diagnostics. A
  malformed or over-limit layer must not leak raw stylesheet content.

### `revert-layer`

- `text-decoration-skip-spaces: revert-layer` is accepted as one
  case-insensitive token and stored only as a private declaration state.
- The winning candidate is resolved by removing the candidate layer and then
  selecting the highest-priority remaining declaration for the same property.
  A named-layer declaration rolls back to the next lower layer; an unlayered
  declaration rolls back to the highest named layer.
- Repeated `revert-layer` declarations in descending layers continue the same
  bounded rollback until a concrete value or a CSS-wide fallback is found. If
  no lower declaration exists, the property uses its existing inherited
  computed fallback; root fallback remains `None`.
- `revert` continues to use the current one-author-origin inherited fallback;
  `inherit`, `unset`, and `initial` retain their existing meanings.
- `revert-layer` is intentionally supported only for
  `text-decoration-skip-spaces` in this slice. The token remains an explicit
  unsupported-value diagnostic for other supported properties.
- The public resolved value remains the finite
  `None|All|Start|End|StartAndEnd` paint value. No unresolved keyword can reach
  `NativeDisplayCommand` or the software rasterizer.

### Existing owners preserved

The resolved value continues through the existing inherited computed style,
immutable text command, line-edge provenance, Unicode whitespace classifier,
and software decoration replay. Layout, fixed-cell geometry, whitespace
collapsing, spacing arithmetic, wrapping, alignment, overflow, clipping,
scrolling, opacity, capture, hit testing, semantics, and source order retain
their current owners.

This remains a horizontal-tb, fixed-cell, local software-raster contract. It
does not claim full CSS cascade layers, multiple origins, `!important` layer
inversion, `@import`/`@media`/`@scope`, nested or anonymous layers, CSS-wide
keyword machinery for every property, layer statements, CSS-wide selector
grammar, font metrics, shaping, bidi, vertical writing, antialiasing,
browser-wide CSS conformance, or browser parity.

## Tradeoffs

- Fifteen named layers and a bounded selector-specificity field keep parsing
  and cascade metadata finite, but reject unusually large stylesheets instead
  of silently degrading their order.
- Encoding the private layer rank into the existing cascade comparison avoids
  duplicating every supported property's cascade storage. It makes layer
  precedence apply consistently to the currently supported declarations, while
  keeping the encoded value entirely internal.
- Only top-level named blocks are accepted. This gives deterministic ownership
  and useful rollback semantics now, at the cost of rejecting common nested
  layer syntax until a dedicated grammar boundary is designed.
- `revert-layer` is fully meaningful for one inherited property in this slice,
  but other properties continue to report unsupported values. This prevents an
  incomplete general keyword implementation from changing unrelated behavior.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and issue docs

## Implementation

Complete at `1291fc2c`, with the strict-Clippy parser-context follow-up at
`efb5bdfc`. The native stylesheet parser now registers first-appearance named
layers, reopens an existing layer at its original rank, and encodes the
private layer rank ahead of selector specificity without changing the public
style or display-list types. It accepts only bounded top-level named layer
blocks, keeps the unlayered bucket above the 15 named-layer bound, and emits
typed diagnostics for statements, anonymous/comma-separated names, nested
layers, malformed preludes, and over-limit layers. The private
`RevertLayer` declaration state rolls back candidates from the unlayered or
winning named layer through lower layers before reaching the established
inherited/root fallback. Focused parser/cascade, display-list, and decoded
raster regressions cover ordering, reopening, invalid forms, rollback, and
case-insensitive parsing. No package dependency, feature default, crate
boundary, public paint value, layout owner, or unrelated style owner changed.

## Verification

The focused native feature gates passed after the initial clean compile
(`CARGO_TARGET_DIR=/tmp/glass-122-focused`, 13:21):

- `RUST_MIN_STACK=33554432 CARGO_TARGET_DIR=/tmp/glass-122-focused cargo test -p glass-browser --features native-engine --lib` — 918 passed, 0 failed, 1 ignored; 7.43s test body.
- `CARGO_TARGET_DIR=/tmp/glass-122-focused cargo test -p glass-browser --features native-engine --test native_engine` — 159 passed, 0 failed; 3.68s test body.
- `CARGO_TARGET_DIR=/tmp/glass-122-focused cargo test -p glass-dev` — 365 unit tests, 4 development-runtime integration tests, 15 PTY tests, and 1 doctest passed; 171.38s test body and 12.07s doctest phase.

The first unqualified native library run reproduced the pre-existing large
Clap parser stack overflow in `cli::args::tests::agent_readiness_commands_are_explicit`.
The explicit `RUST_MIN_STACK=33554432` rerun passed all 918 tests; this is an
environmental test-runner limitation, not a 122 failure.

Strict lint and documentation gates passed with isolated targets:

- `CARGO_TARGET_DIR=/tmp/glass-122-focused cargo clippy -p glass-browser --all-targets --all-features -- -D warnings` — passed in 5m35s.
- `CARGO_TARGET_DIR=/tmp/glass-122-focused cargo clippy -p glass-dev --all-targets --all-features -- -D warnings` — passed in 6m56s.
- `CARGO_TARGET_DIR=/tmp/glass-122-focused cargo clippy -p glass-browser --no-default-features --all-targets -- -D warnings` — passed in 4m43s.
- `RUSTDOCFLAGS="-D warnings" CARGO_TARGET_DIR=/tmp/glass-122-focused cargo doc --workspace --all-features --no-deps` — passed in 3m25s.
- Fresh default-feature documentation binaries built in `/tmp/glass-122-doc-target` (browser 12m10s, dev 12m34s), then `python3 scripts/check-documentation-coverage.py --glass /tmp/glass-122-doc-target/debug/glass --glass-browser /tmp/glass-122-doc-target/debug/glass-browser` passed with 536 Markdown files, 345 full-product MCP tools, 100 browser-only tools, 17 examples, and 22 public modules.

Locked packaging and supply-chain gates passed using `/tmp/glass-122-gates`:

- `CARGO_TARGET_DIR=/tmp/glass-122-gates cargo package -p glass-browser --locked --no-verify` — 196 files, 5.1 MiB.
- `CARGO_TARGET_DIR=/tmp/glass-122-gates cargo package -p glass-dev --locked --no-verify --config 'patch.crates-io.glass-browser.path="crates/glass-browser"'` — 69 files, 2.6 MiB.
- `python3 scripts/check-packaged-dependency.py /tmp/glass-122-gates/package/glass-dev-0.3.14.crate --version 0.3.14` — exact `glass-browser` 0.3.14 dependency confirmed.
- `cargo fetch --manifest-path fuzz/Cargo.toml --locked && CARGO_TARGET_DIR=/tmp/glass-122-gates cargo check --manifest-path fuzz/Cargo.toml --locked --offline --all-targets` — passed in 8m16s.
- `cargo deny check` — passed with the existing duplicate dependency warnings.
- `cargo audit` — passed with the four configured warnings: unmaintained `bincode`, unmaintained `yaml-rust`, the allowed `lru` unsoundness advisory, and yanked `chacha20`.

Formatting and repository validators passed: `cargo fmt --all -- --check`;
version sync; feature parity (14 capabilities / 4 targets); TUI shortcuts
(15 implementation keys / 63 documentation markers); documentation depth (93
guides / 19 contracts); release documentation truth (536 Markdown documents,
83 current documents, 57 previous-version hits, 625 semantic hits, 0 current
claim failures); documentation coverage (above); reliability (6 scenarios / 4
targets); public read-only adapters (5); and Web IR (8 fixtures / 8 scenarios
/ 11 categories). The release, browser-parity, remote-CI, registry-
publication, and third-crate boundaries remain unclaimed.

## Cleanup

All expensive gates used isolated task targets. The first focused target
`/tmp/glass-122-focused` measured 6,629,189,574 bytes across 10,639 files and
was removed before the package/static rerun after confirming no active
Cargo/rustc/rustdoc/Clippy/fuzz/test writer and no open handle. The later
package/fuzz target `/tmp/glass-122-gates` measured 679,050,614 bytes and the
documentation binary target `/tmp/glass-122-doc-target` measured
2,898,338,235 bytes. Exact generated reports were
`/tmp/glass-122-release-doc.json` (173,575 bytes),
`/tmp/glass-122-reliability.json` (3,482 bytes), and
`/tmp/glass-122-adapters.json` (1,418 bytes). The second cleanup observed a
3,596,587,008-byte filesystem availability increase. Both isolated targets
and all three reports were deleted with bounded `find -P` cleanup after fresh
process/open-handle checks; no process was terminated. The three pre-existing
Glass processes (PIDs 590083, 610599, and 611107) reference historical
`target/debug/glass` paths and remained outside this task's cleanup scope.
Final verification found no `/tmp/glass-122-*` candidates and no repository
`target/`; the task target and reports are not left behind.

## Certification

Locally certified at `efb5bdfc` after implementation, native/two-crate tests,
strict lint, rustdoc, packaging, fuzz, dependency, formatting, documentation,
reliability, adapter, Web IR, and exact regenerable-output cleanup. The
public resolved value remains finite and unchanged; layer rank and
`revert-layer` rollback remain private to the CSS cascade. Issue #40 must be
synchronized with this evidence, while remote CI, browser parity, release,
and registry publication remain explicitly unclaimed from this local checkout.
