# Actions and revisions

Glass can run an action without a revision guard. Use a revision guard when the
page may change between observation and execution.

An action is not a selector shortcut. Glass resolves one current target,
checks policy and revision state, dispatches a bounded browser operation, then
collects post-action evidence. Classification, historical knowledge, and
visual similarity do not authorize input.

## Choose a target

Prefer references returned by the current `interactive` observation. Other
locators are available for human use and compatibility:

| Form | Example | Rule |
|---|---|---|
| revisioned reference | `r7:b42` | Must belong to the active target/frame and current revision. |
| accessible role/name | `role=button;name=Save` | Role and accessible name must resolve uniquely. |
| accessible name | `name=Save` | Fails when more than one candidate has the name. |
| visible text | `text=Continue` | Uses bounded visible semantic text. |
| CSS | `css=button.primary` | Explicit deep targeting; still must resolve uniquely. |
| ordinal | `ordinal=2` | Fragile compatibility form; prefer a revisioned reference. |

Use `find-target` to inspect bounded candidates and `preflight` to check one
target without input. Neither operation grants permission for a later action;
observe again when the page changes.

## Guard an action

1. Run `observe`.
2. Save the page revision and target reference.
3. Pass both values to the action.
4. Read the typed result.
5. Run `observe` again when Glass reports a stale revision.

Run the sequence inside one resident TUI, MCP, daemon, or Rust session. For the
TUI, start it and enter the three unprefixed commands in its command input:

```console
glass --incognito tui
```

```text
navigate https://example.com
observe
click r7:b42 --expected-revision 7
```

Three separately prefixed one-shot CLI processes do not share a browser
revision. For a non-interactive multi-step flow, keep one MCP/SDK session or
author a workflow.

The guard is available for navigation, click, popup click, double-click, type,
clear, check, uncheck, select, scroll, fill-form, drag, keyboard, and upload
operations.

## Result

A successful action returns bounded evidence:

```json
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
```

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

## Dispatch and idempotency

Actions such as `check` and `uncheck` describe a desired state and can often
verify that state without toggling twice. Click, type, upload, submit, shortcut,
and coordinate input are not generally idempotent. Do not retry them solely
because the caller missed a response.

Use `act-and-verify` when the caller can state a bounded postcondition. Use a
workflow when multiple steps need retry policy, checkpointing, outputs, and
recovery. `retry_safe` applies only when Glass proves that dispatch did not
occur; it is not a generic transport retry.

Coordinate clicks require current viewport geometry and the
`coordinate-click` capability. Remote View and terminal pointer coordinates
also carry the displayed browser revision. Stale geometry or revision fails
before input.

## Interface mapping

| Interface | Session behavior | Exact contract source |
|---|---|---|
| CLI | one process-scoped session unless attached/resident | `glass COMMAND --help` |
| TUI | one long-lived Browser Workspace with recovery | command palette and Browser view |
| MCP | one session per initialized stdio/socket namespace | live `tools/list` |
| Rust | caller owns `BrowserSession` and close behavior | docs.rs and `BrowserSession` methods |

All interfaces use the same target resolution, revision, policy, dispatch, and
verification contracts. They may expose different convenience projections.

## Troubleshooting

| Result | Inspect next | Do not do |
|---|---|---|
| stale revision | fresh structured observation and active target/frame | substitute the old reference into a new revision |
| ambiguous target | bounded candidates from `find-target` | choose the first candidate automatically |
| confirmation required | policy decision and one-use confirmation token | log or reuse a consumed token |
| verification failed | current page, dialog, popup, download, and revision evidence | assume the action had no effect |
| indeterminate transport | execution ID and recovery result | blindly repeat a non-idempotent action |
| target/frame changed | current topology and new observation | retain old coordinate or backend-node state |

The stable machine-readable action contract is
[action-v1.schema.json](schema/glass-action-v1.schema.json). The compatibility
contract remains available in [Action contract](action-contract.md).
