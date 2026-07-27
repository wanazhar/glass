# Transactional workflow runtime (`#21`)

## Scope

Issue #21 is the first post-0.1.18 roadmap item. It extends the guarded action
runtime into declared, resumable workflows without claiming rollback of browser
or server-side effects. Work remains local until the eventual `0.2.0` release.

The implementation is deliberately serial: each phase is committed and
verified before the next phase starts. Later roadmap issues (#22–#27) are not
part of this delivery.

## Phase decomposition

1. **Definition foundation** — versioned workflow definitions, typed inputs,
   stable step identifiers, budgets, terminal conditions, deterministic JSON,
   and path-aware validation.
2. **Step state and linear execution** — durable step states and a minimal
   linear runner that uses the existing `BrowserSession` action executor.
3. **Transaction safety** — transaction classifications, idempotency markers,
   dispatch-aware failure states, and duplicate-effect prevention.
4. **Checkpoint and resume** — bounded, versioned, deterministic, redacted
   checkpoints plus fresh browser-state reconciliation on resume.
5. **Control flow and results** — bounded branches/loops, typed extraction,
   terminal-condition proofs, and workflow-level result envelopes.
6. **Evidence and interfaces** — workflow traces, recorder foundations, TUI
   inspection, cross-interface conformance, deterministic fixtures, and the
   reliability scorecard.
7. **Release preparation** — documentation, migration notes, and a local-only
   `0.1.19` tag. No push, GitHub release, crates.io publication, or npm/PyPI
   publication is allowed in this phase. The public workflow milestone remains
   `0.2.0`.

## Contract boundaries

- Workflow definitions are data, not an implicit planner.
- Invalid definitions fail before browser startup or mutation.
- Unknown, non-idempotent, or post-dispatch outcomes are never silently
  retried.
- `indeterminate` is distinct from both success and a proven pre-dispatch
  failure.
- Checkpoints contain bounded runtime state, not secrets, cookies, raw page
  content, or unbounded traces.
- Completion requires a verified terminal condition.

## Verification strategy

Every phase adds focused unit/contract tests and runs the relevant existing
test suite. Integration phases must use the real action/session implementation;
mock-only boundary tests are insufficient. The final phase must leave the
working tree clean except for intentionally ignored build output.
