# Issue 30: Semantic Execution Engine foundation

Status: local development started after the `0.2.2` release.

Issue: [#30](https://github.com/wanazhar/glass/issues/30)

## Decision

Implement the roadmap incrementally, beginning with Pillar 0 (fixture corpus and
baseline measurement). The corpus is a regression contract for later evidence,
Web IR, and compiler work; it does not claim that Web IR exists yet.

The first delivery remains sidecar-only. Existing semantic observation, intent
resolution, and guarded workflow paths remain unchanged.

## First delivery scope

- Add a versioned fixture manifest covering representative page structures.
- Add deterministic HTML fixtures without credentials, remote dependencies, or
  production captures.
- Record expected semantic entities, relationships, coverage, and risk hints as
  test metadata rather than pretending they are runtime Web IR output.
- Add a validator that checks fixture paths, unique IDs, category coverage, and
  bounded fixture size.
- Emit a reproducible static baseline report containing fixture bytes, HTML
  element counts, and declared entity/relationship counts.

## Fixture categories

The initial corpus covers forms, custom controls, tables, repeated collections,
navigation, dialogs, pagination, dynamic/stale state, frames, shadow boundaries,
and opaque controls. Each fixture must identify unsupported or incomplete areas
explicitly.

## Non-goals

- No stable public Web IR type yet.
- No task compiler or autonomous planner.
- No replacement of semantic observation or intent resolution.
- No direct CDP dependency in a new public contract.
- No browser mutation during validation.

## Exit gate

The corpus validator passes deterministically, every required category is
represented, all fixture paths resolve, fixture size is bounded, and the baseline
report is reproducible from the checked-in corpus.

## Follow-on sequence

1. Evidence scope and extractor budgets.
2. Accessibility, DOM/form, and layout evidence prototypes.
3. Draft form-focused Web IR and reconciliation.
4. Revision identity and deterministic diffs.
5. Form Task Protocol and compiler dry-run.
