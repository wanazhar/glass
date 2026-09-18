# Native engine browser slice 518: wire font-face variation defaults

Status: complete locally on the current source line.

## Objective

Make the `font-variation-settings` `@font-face` descriptor and the matching
script-created `FontFace` descriptor affect the native selected face instead
of remaining inert metadata. The descriptor must survive stylesheet parsing,
FontFace host commands, content-process snapshots, and native shaping.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-517.md`

## Contract

- Parse the bounded quoted four-byte axis/value grammar already used by the
  element property for `@font-face font-variation-settings`.
- Expose the canonical descriptor through the static `FontFace` projection
  and transmit the dynamic `FontFace` descriptor in the install command.
- Validate the descriptor at the native document admission boundary and carry
  it through content-process serialization without weakening existing bounds
  or legacy wire defaults.
- Use descriptor coordinates as selected-face defaults; authored element
  `font-variation-settings` values override matching axes and retain priority
  for new axes within the fixed native bound.
- Preserve the two-installable-crate boundary, fallback behavior, and
  fail-closed malformed-input behavior.

## Tradeoffs and explicit limits

The descriptor and element property share a fixed eight-axis, thousandths
representation so command and snapshot payloads remain bounded and
deterministic. The element property receives priority when descriptor and
element axes collide. The current HarfRust shaping path consumes the merged
coordinates, while fontdue's bitmap rasterizer still has no variation-aware
instance API; contour-accurate variable glyph rasterization, automatic
`wght`/`wdth` axis mapping, color tables, and complete FontFace/Web IDL parity
remain separate issue #40 gates.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/font.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- focused parser, wire, FontFace, CSSOM, merge, and shaping tests
- affected `font_` regression group
- full locked `glass-browser` library gate near slice completion
- repository documentation validators and `git diff --check`

The slice is complete only when implementation, focused behavior, affected
package regression, documentation, and local resource accounting agree. No
remote CI, push, release, tag, registry publication, or native/CDP parity
certification is implied by this task.

## Results

Results:

- `cargo fmt --all -- --check`, `git diff --check`, and the scoped compiler
  check passed; the final `cargo check --quiet -p glass-browser --lib --locked`
  completed without warnings in 14.74 seconds.
- The focused variation parser/CSSOM/shaping suite passed 4/4 tests; the
  descriptor-default precedence witness passed 1/1 test.
- The affected `font_` regression group passed 85/85 tests in 28.14 seconds
  of test time.
- The full locked `glass-browser` library gate passed 1,220 tests with 1
  ignored and 0 failures in 62.46 seconds of test time.
- Release-documentation truth passed over 1,168 Markdown documents with 83
  current documents, 63 previous-version hits, 1,343 classified semantic
  hits, and 0 current-claim failures. Documentation depth passed with 93
  routed current guides and 19 substantive contracts; the TUI inventory
  passed with 15 implementation help keys and 63 documentation markers; and
  documentation coverage passed with 346 full-product MCP tools (101
  browser-only), 17 examples, and 22 public modules.
- Issue-40 checkpoint verification follows this task's commit; no remote CI,
  push, release, tag, registry publication, or native/CDP parity
  certification is claimed.
