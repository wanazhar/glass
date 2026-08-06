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

## `0.3.0` stabilization contract

The sidecar boundary above describes the first incremental delivery, not the
`0.3.0` release gate. The final release promotes the architecture only when all
of these conditions hold:

1. `GlassWebIrV1` is the stable, bounded observed contract. Draft aliases and
   experimental capability statuses are removed in one clean cutover.
2. Live extraction produces the IR from one consistent `PageContext`. Every
   requested source is either represented or listed as missing; region, frame,
   shadow, and opaque boundaries fail closed.
3. `compile_task_against_ir` accepts a validated `GlassTask` and compatible
   `GlassWebIrV1`. Its output records the source revision, selected entities,
   preconditions, postconditions, risk, confirmation requirement, revalidation
   policy, and bounded diagnostics.
4. Browser-backed task execution compiles against the same live IR used for
   preflight. The compiler never calls CDP; the executor continues to use the
   guarded session action and workflow boundaries.
5. Exact revisions execute only when unchanged. Compatible revisions require
   unique evidence-backed continuity. Re-extraction is explicit and bounded;
   ambiguous, disappeared, or risky rebound targets fail closed.
6. Rust, CLI JSON, MCP, and daemon clients share the canonical protocol
   operations and result types. Large diagnostic detail is projected or stored
   by reference instead of being dumped by default.
7. Golden fixtures cover evidence to IR, task plus IR to compiled workflow,
   typed failures, revision transitions, and cross-interface serialization.
8. Release evidence measures extraction and compilation latency, serialized
   bytes, estimated agent-visible tokens, and the pre-Web-IR comparison. Static
   fixture inventory alone is not runtime evidence.

### Stable ownership

| Boundary | Owner | Stable input | Stable output |
| --- | --- | --- | --- |
| Live extraction | `src/extraction.rs` | consistent bounded `PageContext` and `ExtractionRequest` | `ExtractionEvidence` |
| Semantic reconciliation | `src/web_ir.rs` | validated extraction evidence | `GlassWebIrV1` |
| Task compilation | `src/task_compiler.rs` | `GlassTask` plus `GlassWebIrV1` | `CompiledTaskWorkflow` |
| Guarded execution | `src/browser/session/task.rs` | compiled workflow plus approved values | `TaskExecutionResult` |
| Protocol transport | `src/protocol.rs` | canonical typed requests | canonical typed results |

The stable public contracts contain no CSS selectors, XPath, raw DOM, CDP node
IDs, or framework-specific identifiers. Revision-local semantic entity IDs are
allowed and are never interpreted as browser authority without fresh
evidence-backed resolution.

### Clean cutover rule

The `0.3.0` release does not retain public `Draft*` aliases, parallel task
compilers, or separate live and offline semantics. Offline validation remains,
but it validates the same stable contract used by live extraction and
execution. Low-level browser primitives remain available as explicit debugging
escape hatches.
