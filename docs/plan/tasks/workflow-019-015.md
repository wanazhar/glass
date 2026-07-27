# Workflow 019-015 — versioned workflow JSON Schema

## Status

Completed locally.

## Scope

Commit a machine-readable JSON Schema for workflow version 1 and document the
boundary between schema shape validation and runtime cross-field validation.

## Acceptance criteria

- [x] The schema pins `schemaVersion` 1 and all required top-level fields.
- [x] Input, budget, step, predicate, and output shapes are represented.
- [x] Bounds match the runtime limits for strings, maps, arrays, and budgets.
- [x] The schema is linked from the workflow guide.
- [x] Cross-field runtime checks are explicitly documented as additional rules.

## Validation

```text
python3 -m json.tool docs/schema/workflow-v1.schema.json
git diff --check
```
