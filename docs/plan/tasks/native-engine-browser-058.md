id: native-engine-browser-058
scope: glass-browser/native-engine/form-validation-temporal
status: done
depends-on: [native-engine-browser-057]
---

# BE-03ac/BE-04an: bounded temporal form validation

## Objective

Extend the shared local/child form-validation owner with deterministic validity
for the common `date`, `month`, `time`, and `datetime-local` input types.

## Contract

- Values use strict bounded calendar/time parsing, including leap-day and
  month-length checks, before the existing ordered `invalid` event path.
- `min` and `max` are applied in the input's temporal unit, and positive
  numeric `step` values use the HTML date/day, month, or second unit as
  appropriate; `step="any"` bypasses the step remainder check.
- Valid values continue through the existing submit-event and navigation path
  in both local and process-backed owners.

## Deliberate boundary and tradeoffs

The implementation intentionally does not claim pattern, file, custom-validity,
`ValidityState`/`checkValidity` Web IDL, timezone, picker, or browser-wide
temporal conformance. Invalid or unsupported constraint syntax remains bounded
by the existing fallback policy rather than adding a new dependency or runtime
path.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine temporal -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine form_validation -- --nocapture` — 5 passed

Remote CI, source push, release, tag, registry publication, browser parity,
and issue-closure claims remain pending the wider browser-complete gates.
