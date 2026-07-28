# Actions and revisions

Glass can run an action without a revision guard. Use a revision guard when the
page may change between observation and execution.

## Guard an action

1. Run `observe`.
2. Save the page revision and target reference.
3. Pass both values to the action.
4. Read the typed result.
5. Run `observe` again when Glass reports a stale revision.

Example:

`console
glass navigate https://example.com
glass observe
glass click r7:b42 --expected-revision 7
`

The guard is available for navigation, click, popup click, double-click, type,
clear, check, uncheck, select, scroll, fill-form, drag, keyboard, and upload
operations.

## Result

A successful action returns bounded evidence:

`json
{
  "status": "succeeded",
  "action": "click",
  "executionId": "act_42",
  "target": {"label": "Save", "reference": "r7:b42"},
  "revision": 8,
  "previousRevision": 7,
  "currentRevision": 8,
  "verification": {
    "revisionDelta": 1,
    "urlChanged": false,
    "titleChanged": false,
    "targetChanged": false,
    "frameChanged": false
  }
}
`

The result may include URL and title changes, target and frame changes, a
revision delta, count-only accessibility changes, popup evidence, dialog
evidence, or download evidence.

The `executionId` identifies one attempt. A retry has a new ID.

An unguarded action remains available for compatibility.

## Failure kinds

Branch on typed fields. Do not match display text.

| Kind | Meaning | Action |
|---|---|---|
| `stale_revision` | The expected revision is not current. | Observe again. Use the new revision. |
| `ambiguous_target` | More than one target matches. | Refine the locator. |
| `target_not_found` | No target matches. | Check the locator and current page. |
| `denied` | Policy blocks the operation. | Change fixed policy or operation. |
| `confirmation_required` | Policy requires approval. | Supply the configured token. |
| `transport` | CDP or protocol failed. | Check browser health. |
| `verification_failed` | The action ran but the postcondition failed. | Inspect the page before retry. |

Legacy errors retain `kind`. The normalized `failureKind` is also present.

Candidate data and evidence are bounded. Glass does not copy page text,
selectors, typed values, or raw CDP details into the ordinary result.

## Execution phases

The action contract uses these phases:

`policy` → `preflight` → `target_resolution` → `dispatch` →
`browser_effect` → `verification` → `transport`

A failure before dispatch is different from a failure after dispatch.

## Recovery

Recovery is explicit:

- `none` stops and reports the failure;
- `report` returns a bounded reconciliation suggestion; and
- `retry_safe` permits a retry only after Glass proves that dispatch did not
  occur and the operation is safe to retry.

Automatic recovery is disabled by default. Glass never moves a stale reference
to a new target without a new observation.

The stable machine-readable action contract is
[action-v1.schema.json](schema/glass-action-v1.schema.json). The compatibility
contract remains available in [Action contract](action-contract.md).
