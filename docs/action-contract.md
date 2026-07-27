# Action contract

This document is the source of truth for the `0.1.18` action contract. Glass
keeps the Rust API, CLI JSON, MCP results, and client libraries aligned around
the same execution model:

```text
observe → preflight → resolve → dispatch → effects → verify → recover/report
```

An action is a bounded execution attempt against one browser route. A page
revision is an observation-validity boundary; it is not a count of DOM
mutations and it does not promise that every browser event was observed.

## Contract shape

Successful action results will use these canonical camelCase fields:

```json
{
  "status": "succeeded",
  "action": "click",
  "executionId": "act_42",
  "previousRevision": 41,
  "currentRevision": 42,
  "target": {
    "reference": "r41:b17",
    "role": "button",
    "name": "Save"
  },
  "effects": {
    "urlChanged": false,
    "titleChanged": false,
    "routeChanged": false,
    "popupOpened": false,
    "dialogOpened": false,
    "downloadStarted": false,
    "accessibilityChanged": true
  },
  "verification": {"status": "not_requested"},
  "recovery": null
}
```

Failure results preserve the execution identity and identify where the
operation stopped:

```json
{
  "status": "failed",
  "action": "click",
  "executionId": "act_43",
  "failure": {
    "kind": "stale_revision",
    "phase": "preflight",
    "message": "Expected revision 41, current revision is 43",
    "retryable": true
  },
  "previousRevision": 41,
  "currentRevision": 43,
  "recovery": {"strategy": "observe", "retryable": true}
}
```

The envelope is additive to the compatibility-preserving legacy methods. A
caller that does not supply an expected revision may continue to use the
legacy methods, but new guarded interfaces use the canonical fields.

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

## Delivery status

The initial delivery slices add stable execution IDs, the internal request
boundary, revision guards across supported mutations, and batch modes. Bounded
verification predicates, effect witnesses, recovery data, and diagnostics are
still being delivered under issue #20.
