# Native-engine browser slice 557: root-relative font-size units

Status: complete local implementation checkpoint; issue #40 remains open.

## Objective

Close the bounded CSS `rem` gap left by the native inherited `font-size` path
while preserving the integer-pixel computed-style contract consumed by layout,
shaping, rasterization, and content-process snapshots.

## Scope

- Accept positive bounded `rem` element `font-size` declarations alongside the
  existing absolute, `em`, and percentage paths.
- Resolve `rem` against the resolved root element font size rather than the
  immediate parent, using the existing checked thousandths and half-up integer
  pixel conversion.
- Preserve CSS-wide declarations and fail closed when a root-relative candidate
  converts outside the native 1 through 256 px result range.
- Keep viewport units, unit algebra, malformed precision, and complete CSS
  font-size grammar outside this slice.

## Contract

- `1rem` represents a 1,000 milli-scale factor of the root element's computed
  font size; decimal factors use the existing bounded thousandths grammar.
- The document computed-style walk identifies the first element in the bounded
  root-to-target chain, records its resolved font size, and uses that value for
  descendant `rem` declarations. The root element itself resolves `rem` against
  the CSS initial 16 px size.
- Root-relative values use checked multiplication and half-up rounding. A result
  outside 1..=256 px blocks the candidate and lets the existing cascade fall
  through to a lower candidate or inherited value.

## Implementation

- `css.rs` adds a root-relative font-size declaration variant, threads the root
  size through `NativeInheritedStyle`, and resolves `rem` with the same bounded
  scale helper used for `em` and percentages.
- `dom.rs` tracks the first element's computed font size during the existing
  ancestor walk and passes it to each stylesheet computed-style evaluation.
- Parser coverage verifies `1rem`, zero, and unsupported boundaries; document
  coverage verifies rem resolution beneath a distinct 32 px nested parent and
  confirms an out-of-range `20rem` candidate preserves the inherited 32 px size.

## Verification

- Focused rem parser coverage passed:
  `RUST_MIN_STACK=8388608 cargo test -p glass-browser --lib --locked
  font_size_parser_accepts_bounded_relative_units -- --test-threads=1`
  reported 1 passed; the focused computed-style cascade test reported 1
  passed.
- The full locked native library suite passed with
  `RUST_MIN_STACK=8388608`: 1284 passed, 1 ignored, and 1283 filtered across
  the two emitted test suites.
- Package gates passed: `scripts/check-rust-workspace.sh fast-check`,
  `cargo build -p glass-browser --bin glass-browser --locked`, `cargo build
  -p glass-dev --bin glass --locked`, and locked metadata.
- Documentation depth passed: 93 current guides routed/audited and 19
  substantive contracts. Coverage passed for 1207 Markdown files, 346
  full-product MCP tools (101 browser-only), 17 examples, and 22 public
  modules. Release truth passed for the same 1207 Markdown documents with
  current=83, previous-version hits=63, semantic hits=1367, and zero
  current-claim failures.
- `cargo fmt --all -- --check` and `git diff --check` passed after final edits.
- No CDP fallback, remote issue mutation, push, or release action was used.
