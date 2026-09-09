id: native-engine-browser-061
scope: glass-browser/native-engine/form-validation-pattern
status: done
depends-on: [native-engine-browser-060]
---

# BE-03af/BE-04aq: bounded pattern validation

## Objective

Complete the common text-control validation path with Rust-owned `pattern`
constraint checks for local and process-backed native documents.

## Contract

- Text-like input controls apply an implicitly whole-value-matched regular
  expression from the `pattern` attribute and expose `patternMismatch` and
  the existing bounded validation message when the value does not match.
- The pattern is not applied to hidden, choice, file, date/time, numeric,
  range, color, or button-state controls; empty values continue through the
  existing `required` behavior.
- Invalid or unsupported regex syntax is ignored according to the HTML
  invalid-pattern fallback rather than making a document unloadable.
- The Rust validity owner is used by both local and sandboxed content-process
  documents, including explicit `checkValidity()` calls and submission
  preflight.

## Deliberate boundary and tradeoffs

This is the native engine's bounded Rust-regex subset, not full JavaScript
RegExp `v`-flag or browser Unicode-set parity. The optional `regex` dependency
is enabled only with `native-engine`; it was already present transitively in
the workspace lockfile, but remains an explicit feature cost for this
validation capability. File controls, picker/UI behavior, and full live
`ValidityState` Web IDL identity remain open.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/Cargo.toml`
- `Cargo.lock`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine pattern_constraints -- --nocapture` — 2 passed

Remote CI, source push, release, tag, registry publication, browser parity,
and issue-closure claims remain pending the wider browser-complete gates.
