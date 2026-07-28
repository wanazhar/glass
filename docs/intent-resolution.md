# Intent resolution

Intent resolution maps one caller-supplied phrase to candidates in a fresh
semantic observation.

It returns evidence and a policy decision. It does not plan a workflow. It does
not select a target when the evidence is ambiguous.

## Resolve

Create a versioned request:

```json
{
  "schemaVersion": 1,
  "intent": "open settings",
  "action": "click",
  "scope": {"pageKind": "dashboard", "regionKind": "navigation"},
  "constraints": {"role": "button", "name": "Settings"},
  "resolutionPolicy": "requireExact",
  "expectedRevision": 42
}
```

Resolve without dispatch:

```console
glass resolve-intent request.json
```

The result contains the route, revision, candidates, confidence, evidence,
exclusions, and selected candidate.

A candidate is valid only for the observed target, frame, and revision.

## Execute

Provide the candidate ID from the resolution result:

```json
{
  "request": {
    "schemaVersion": 1,
    "intent": "open settings",
    "action": "click",
    "scope": {"pageKind": "dashboard", "regionKind": "navigation"},
    "constraints": {"role": "button", "name": "Settings"},
    "resolutionPolicy": "requireExact",
    "expectedRevision": 42
  },
  "candidateId": "candidate_1"
}
```

Run:

```console
glass execute-intent execution.json
```

Glass observes and resolves again before dispatch. The response separates
resolution evidence from the action result.

`executed` means that the guarded action returned. `not_executed` means
that Glass did not dispatch the action and returns the reason.

## Result classifications

| Classification | Meaning |
|---|---|
| `exact` | One exact role and accessible-name match. |
| `uniqueHighConfidence` | One candidate has high-confidence evidence. |
| `uniqueLowConfidence` | One candidate has lower-confidence evidence. |
| `ambiguous` | More than one candidate remains. |
| `notFound` | No candidate satisfies the request. |
| `staleRevision` | The supplied revision is not current. |
| `policyRejected` | A candidate exists but policy rejects it. |
| `unsupportedIntent` | The request is outside the supported contract. |

## Resolution policies

- `reportOnly` never dispatches.
- `requireExact` accepts only `exact`.
- `requireUniqueHighConfidence` accepts an exact or unique high-confidence
  result.
- `allowUniqueMediumConfidence` accepts one unique medium-confidence result.
- `interactiveConfirmation` requires the caller to select a candidate.

Ambiguous, stale, not-found, and unsupported results do not dispatch.

## Supported actions

The guarded boundary supports click-like actions, typing, clearing, checking,
unchecking, selecting, submitting, opening, closing, searching, filtering,
sorting, pagination, expanding, and collapsing.

Toggle, download, upload, inspect, and extract intent actions are reported as
unsupported until their action contracts are ready.

## Workflow use

A workflow may use an intent step instead of a raw locator. The trace and
checkpoint retain the resolution ID, candidate ID, revision, policy decision,
confidence, evidence, and target fingerprint.

On resume, Glass observes and resolves the pending step again. It does not use
an old target reference.

## Interfaces and limits

- Rust provides `BrowserSession::resolve_intent` and
  `BrowserSession::execute_intent`.
- CLI provides `resolve-intent` and `execute-intent`.
- MCP provides `resolveIntent` and `executeIntent`.
- TUI displays candidates and evidence and supports explicit selection.

Glass bounds requests, candidates, evidence, values, and suggestions. Intent
payloads do not contain cookies, profile data, screenshots, or raw DOM.

The schema is [intent-resolution-v1.schema.json](schema/intent-resolution-v1.schema.json).
