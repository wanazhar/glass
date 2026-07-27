# Workflow definitions

Glass 0.1.19 introduces the validated definition contract and a linear runner
for the transactional workflow runtime. Definitions are validated before
execution, and each step records its state transition history. Checkpoints,
retries, and resume reconciliation are being added in later phases.

## Contract

A workflow definition contains a schema version, a caller-owned workflow
version, typed inputs, resource budgets, preconditions, named steps, a terminal
condition, and declared outputs. Definitions are validated before a browser is
started or an action is dispatched.

```json
{
  "schemaVersion": 1,
  "name": "open-example",
  "workflowVersion": "1.0.0",
  "inputs": {},
  "budgets": {
    "maxSteps": 1,
    "maxDurationMs": 30000,
    "maxRetries": 0,
    "maxExtractedBytes": 4096
  },
  "preconditions": [],
  "steps": [
    {
      "id": "open",
      "action": "navigate",
      "url": "https://example.com",
      "timeoutMs": 20000,
      "transaction": "read_only",
      "expect": { "titleContains": "Example" }
    }
  ],
  "terminalCondition": { "urlEquals": "https://example.com/" },
  "outputs": {
    "title": {
      "valueType": "string",
      "source": "page_title",
      "required": true
    }
  }
}
```

`WorkflowDefinition::from_json` and `WorkflowDefinition::from_value` reject
missing required fields, unsupported schema versions, duplicate step IDs,
invalid URLs, unbounded budgets, invalid input values, and unsafe arbitrary
JavaScript steps. `to_canonical_json` returns deterministic JSON with ordered
map keys for hashing or audit records.

Action strings may reference declared values with the bounded
`${inputs.name}` form. Glass resolves these placeholders after input type
validation and before browser dispatch; unknown, missing, non-scalar, or
oversized substitutions fail before an action can run.

## Current boundary

The definition contract currently accepts navigation, pointer and form
actions, scrolling, waits, observations, screenshots, and dialog decisions.
Arbitrary `evaluate` steps are intentionally excluded. The accepted action
shape reuses the existing batch action schema, including bounded verification
predicates.

## Retry safety

Each step may declare `transaction` as `read_only`, `idempotent`,
`conditionally_idempotent`, `non_idempotent`, or `unknown`. Conditional steps
must provide an `idempotencyKey`. `maxRetries` is bounded by the workflow
budget and is rejected for non-idempotent or unknown steps.

The runner retries only failures proven to occur before dispatch and only for a
retry-safe class. A failure after dispatch is recorded and stops the workflow;
Glass does not claim that an external effect was rolled back or replay it
automatically.

For bounded control flow, a step may set `repeat` from 1 through 8. Repeated
steps count toward `maxSteps` and require a retry-safe transaction class, so
`repeat` cannot be used to replay an unknown or non-idempotent effect.

Outputs use bounded sources such as `page_url`, `page_title`, or
`visible_text`. Their declared type is checked before the value is returned;
arbitrary JavaScript is not an output source.

Every run result also includes a deterministic trace of step-state transitions.
The trace has contiguous sequence numbers and a fixed event budget, making it
suitable for replay inspection without retaining page contents or input
values.

## Checkpoints and resume

`export_workflow_checkpoint` produces deterministic JSON capped at 8 KiB. It
stores workflow identity, a definition hash, redacted step states, and bounded
target/frame/URL/title/revision metadata. It does not store input values,
cookies, passwords, or page content.

`reconcile_workflow_checkpoint` checks that the definition and route still
match and returns the next safe step without dispatching an action. It rejects
changed routes and any checkpoint whose next state could represent an already
dispatched effect. Callers remain responsible for executing the returned plan.

`WorkflowTrace::replay` checks that a trace belongs to the declared workflow,
follows legal state transitions, and preserves attempt boundaries without
contacting Chrome or replaying an effect. This makes traces suitable for
offline diagnostics and test fixtures.

This is a local, unreleased 0.1.19 development surface. Checkpoint persistence
and resume reconciliation are not complete yet. The eventual public release
target for the complete roadmap is 0.2.0.
