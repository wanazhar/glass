# Workflow definitions

Glass 0.1.19 introduces the validated definition contract for the transactional
workflow runtime. This first phase describes workflows as data; execution,
checkpoints, retries, and resume reconciliation are being added in later
phases.

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
      "expect": { "titleContains": "Example" }
    }
  ],
  "terminalCondition": { "urlEquals": "https://example.com/" },
  "outputs": {}
}
```

`WorkflowDefinition::from_json` and `WorkflowDefinition::from_value` reject
missing required fields, unsupported schema versions, duplicate step IDs,
invalid URLs, unbounded budgets, invalid input values, and unsafe arbitrary
JavaScript steps. `to_canonical_json` returns deterministic JSON with ordered
map keys for hashing or audit records.

## Current boundary

The definition contract currently accepts navigation, pointer and form
actions, scrolling, waits, observations, screenshots, and dialog decisions.
Arbitrary `evaluate` steps are intentionally excluded. The accepted action
shape reuses the existing batch action schema, including bounded verification
predicates.

This is a local, unreleased 0.1.19 development surface. It is not a promise
that workflow execution or resume support is complete yet. The eventual public
release target for the complete roadmap is 0.2.0.
