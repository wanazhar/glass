---
id: native-engine-browser-050
scope: glass-browser/native-engine/form-constraint-validation
status: done
depends-on: [native-engine-browser-049]
---

# BE-03z/BE-04af: bounded common constraint validation

## Objective

Expand interactive form validation beyond `required` for the common
application controls used by the native navigation path.

## Contract

- Local and child-owned forms validate required text, email, URL, textarea,
  checkbox/radio, and select controls before `submit` and navigation.
- `minlength`/`maxlength` count UTF-16 code units for text and textarea values.
- Numeric `number`/`range` controls enforce finite values, `min`, `max`, and
  bounded positive `step` values, with `step="any"` accepted.
- Invalid controls dispatch the existing bounded non-bubbling `invalid` event
  in document order; valid controls retain the existing submitter and request
  handoff behavior.

## Deliberate boundary and tradeoffs

This is the common constraint subset, not a claim of full HTML constraint
validation. Pattern/regular-expression semantics, date/time parsing, file
controls, `ValidityState`/`checkValidity` Web IDL identity, custom validity,
and browser-specific validation UI remain open. Invalid numeric attributes use
the existing fail-soft HTML-style default step behavior; malformed values do
not broaden the accepted request surface.

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

- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `native_local_form_validation_covers_common_constraints`
- `native_content_process_form_validation_covers_common_constraints`
- existing `form_validation` subset: 4 passed
- `cargo fmt --all -- --check`
- `git diff --check`

Remote CI, source push, release, tag, registry publication, browser-parity,
and issue-closure claims remain pending the wider browser-complete gates.
