# Action contract

This document records the compatibility contract introduced in `0.1.18`.
Glass keeps the Rust API, CLI JSON, MCP results, and client libraries aligned around
the same execution model:

```text
observe → preflight → resolve → dispatch → effects → verify → recover/report
```

An action is a bounded execution attempt against one browser route. A page
revision is an observation-validity boundary; it is not a count of DOM
mutations and it does not promise that every browser event was observed.

## Contract shape

Successful action results use these canonical contract fields:

```json
{
  "status": "succeeded",
  "action": "click",
  "executionId": "act_42",
  "target": {"label": "Save", "reference": "r41:b17"},
  "revision": 42,
  "previousRevision": 41,
  "currentRevision": 42,
  "target_id": "page-target",
  "frame_id": "main-frame",
  "verification": {
    "revisionDelta": 1,
    "urlChanged": false,
    "titleChanged": false,
    "targetChanged": false,
    "frameChanged": false
  }
}
```

Failure results preserve the execution identity and identify where the
operation stopped:

```json
{
  "kind": "stale_revision",
  "phase": "preflight",
  "recoveryStrategy": "report",
  "executionId": "act_43",
  "expectedRevision": 41,
  "currentRevision": 43,
  "recovery": "observe"
}
```

The envelope is additive to the compatibility-preserving legacy methods. A
caller that does not supply an expected revision may continue to use the
unguarded methods, while guarded interfaces use the same canonical fields.

## Failure phases

Every typed failure belongs to one phase:

- `policy`
- `preflight`
- `target_resolution`
- `dispatch`
- `browser_effect`
- `verification`
- `transport`

Revision failures include an `executionId` when the browser session had begun
an attempt. Compatibility constructors may omit that field when a failure is
created independently of a session. Postcondition failures use the
`verification` phase and carry the same bounded identity in their typed error.

Failures are bounded and must not include raw page contents, credentials,
typed values, arbitrary JavaScript, or unbounded CDP payloads.

## Recovery policy

Recovery is explicit:

- `none`: stop and report the failure;
- `report`: include a bounded reconciliation suggestion without retrying;
- `retry_safe`: retry only when the original action was not dispatched, one
  strong reference match exists, the operation is retry-safe, and the retry
  limit has not been reached.

Automatic recovery is disabled by default. A stale reference is never silently
relocated and executed.

Typed failures also expose `recoveryStrategy`: `none`, `report`, or
`retry_safe`. The current guarded revision and verification failures use
`report`; `retry_safe` is reserved for an explicit caller-selected retry policy
after the action has been proven not to dispatch.

## Delivery status

The action reliability scope is implemented locally: execution IDs,
revision guards across supported mutations, bounded verification predicates,
effect witnesses, explicit recovery diagnostics, revision-aware batch modes,
cross-interface helpers, deterministic fixture coverage, and TUI action
evidence are available. Publication and hosted release-matrix evidence remain
release operations; they are not claims about this checkout.
