# Workflow 019-014 — typed output evidence

## Status

Completed locally.

## Scope

Complete the scalar extraction contract with strict conversions, cumulative
byte budgeting, and bounded revision/source provenance on each output.

## Acceptance criteria

- [x] String, URL, integer, number, and boolean values have strict conversion
  rules.
- [x] Conversion failures are explicit and do not silently coerce values.
- [x] `maxExtractedBytes` applies to the total extracted text for a run.
- [x] Outputs carry source and browser revision evidence.
- [x] Output declarations validate source/type compatibility.
- [x] Focused tests cover accepted and rejected scalar conversions.
- [x] Documentation describes output evidence and the cumulative budget.

## Validation

```text
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
git diff --check
```
