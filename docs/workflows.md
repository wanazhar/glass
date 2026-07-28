# Workflow definitions

The 0.2.0 checkout supports validated workflow definitions and a linear
workflow runner. The checkout also supports bounded retries, traces,
checkpoints, resume reconciliation, and offline authoring.

These features are local-only until the 0.2.0 release gates pass.

## Definition

A workflow definition contains:

- a schema version;
- a workflow name and version;
- typed inputs;
- resource budgets;
- optional preconditions;
- named steps;
- a terminal condition; and
- declared outputs.

Glass validates the definition before it starts Chrome or dispatches an action.

Example:

`json
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
`

Use [workflow-v1.schema.json](schema/workflow-v1.schema.json) for the external
JSON contract.

`WorkflowDefinition::from_json` and `WorkflowDefinition::from_value` reject:

- missing required fields;
- unsupported schema versions;
- duplicate step IDs;
- invalid URLs;
- unbounded budgets;
- invalid input values;
- unsafe JavaScript steps; and
- unknown fields.

Validation errors include the JSON path. Canonical JSON uses ordered map keys.
The runtime accepts the issue-era `type` alias for inputs and outputs. It
writes the canonical field `valueType`.

## Actions and conditions

The current definition accepts navigation, pointer and form actions, scrolling,
waits, observations, screenshots, and dialog decisions. It reuses the batch
action contract. It does not accept arbitrary `evaluate` steps.

Action strings may use the bounded `${inputs.name}` form. Glass validates the
input before it substitutes the value.

A step may declare `when`. Glass evaluates the condition before dispatch. A
false condition records `skipped`. A predicate error fails before dispatch.

A step may declare `repeat` from 1 through 8. Repeated steps count toward
`maxSteps` and must use a retry-safe transaction class.

## Retry safety

Each step declares one transaction class:

- `read_only`;
- `idempotent`;
- `conditionally_idempotent`;
- `non_idempotent`; or
- `unknown`.

A conditional step must declare a bounded `idempotencyKey`. It may also
declare `beforeRetry` to identify an existing success marker.

Glass retries only failures that occurred before dispatch and only when the
transaction class is retry-safe. Glass does not retry a possible external
effect. It does not claim that an external effect was rolled back.

If dispatch was acknowledged but verification did not complete, the run status
is `resume_required`. If Glass cannot determine whether the effect occurred,
the step state is `indeterminate`. A failed postcondition after dispatch is
`failed_after_dispatch`.

## Outputs and evidence

Outputs may use these bounded sources:

- `page_url`;
- `page_title`; and
- `visible_text`.

Glass checks the declared type. It does not use arbitrary JavaScript as an
output source.

A sensitive output returns `null` with `redacted: true`. It does not return
the value. Each output includes its source and browser revision.

Each run includes a bounded, versioned trace. The trace records step states,
dispatch evidence, effects, verification, retries, and the run ID. It does not
retain page contents or input values.

## Checkpoints and resume

`export_workflow_checkpoint` creates deterministic JSON capped at 8 KiB. The
checkpoint contains workflow identity, a definition hash, redacted step state,
bounded execution evidence, and route and revision metadata.

The checkpoint does not contain input values, cookies, passwords, page content,
or step error strings.

`reconcile_workflow_checkpoint` checks the workflow, route, and next step. It
does not dispatch an action. It rejects route changes, definition changes, and
a next state that could represent an already dispatched effect.

`BrowserSession::resume_workflow` reconciles the state and runs only the safe
pending suffix. It does not replay the committed prefix. It refuses an
already-complete checkpoint.

`WorkflowTrace::replay` checks workflow identity, schema version, event order,
and legal state transitions. It does not contact Chrome or replay an effect.

Glass enforces `maxSteps` and `maxDurationMs` during execution. When a
budget ends, the run status is `budget_exhausted`. Remaining steps become
`skipped`.

## Authoring

`glass workflow compile`, `format`, `validate`, `lint`, `preview`, and
`diff` are offline operations. They do not start Chrome.

Read [Workflow authoring](workflow-authoring.md) for source formats,
diagnostics, semantic recording, preview, and diff behavior.

## Scorecard

Run the local workflow scorecard:

`console
GLASS_WORKFLOW_SCORECARD_ITERATIONS=10 cargo run --release --example workflow_scorecard
`

The scorecard uses `benchmarks/scenarios/workflow-v1.json` and the local
`tests/fixtures/scorecard.html` fixture. It checks linear completion,
conditional skip, budget exhaustion, marker reconciliation, and terminal-proof
refusal.

The scorecard is local evidence. It is not a public release claim.
