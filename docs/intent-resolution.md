# Intent resolution

Intent resolution maps one bounded, caller-supplied phrase to candidates in a
fresh semantic observation. It reports the evidence and policy decision before
an action can cross the guarded execution boundary. It is not a workflow
planner and it does not invent a target when the observation is ambiguous.

## Resolve, then execute

The request is versioned and explicit about the action, scope, constraints, and
certainty policy:

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

Resolve it with the CLI or MCP:

```console
glass resolve-intent request.json
```

The result includes the current route, revision, candidates, confidence,
evidence, exclusions, and selected candidate. Candidate references remain
scoped to the observed target, frame, and revision.

Execution repeats the observation and resolution before dispatch. The caller
must identify the candidate returned by resolution:

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

```console
glass execute-intent execution.json
```

An execution response separates the resolution evidence from the optional
action outcome. `status` is `executed` only after the guarded action returns;
otherwise it is `not_executed` with the reason and fresh resolution.

## Resolution and policy outcomes

The resolver reports one of these classifications:

- `exact`: the declared role and accessible name identify one candidate;
- `uniqueHighConfidence`: one candidate has high-confidence semantic evidence;
- `uniqueLowConfidence`: one candidate has only medium or low evidence;
- `ambiguous`: more than one candidate remains;
- `notFound`: no candidate satisfies the request;
- `staleRevision`: the caller supplied a revision that is no longer current;
- `policyRejected`: a candidate exists but does not meet the selected policy;
- `unsupportedIntent`: the phrase or request is outside the bounded contract.

Policies are caller-selected. `reportOnly` never dispatches. `requireExact`
accepts only an exact result; `requireUniqueHighConfidence` also accepts a
unique high-confidence result; `allowUniqueMediumConfidence` permits a unique
medium-confidence candidate; and `interactiveConfirmation` requires the
caller to name a candidate before execution. Ambiguous, stale, not-found, and
unsupported results do not dispatch.

The guarded execution boundary currently supports click-like actions, typing,
clearing, checking and unchecking, selecting, submitting, opening and closing,
searching, filtering, sorting, pagination, expanding, and collapsing. Toggle,
download, upload, inspect, and extract intent actions are reported as
unsupported at this boundary until their action-specific contracts are ready.

## Semantic workflow steps

Workflows can use the same contract instead of embedding a raw locator:

```json
{
  "schemaVersion": 1,
  "workflow": {
    "id": "checkout",
    "transaction": "idempotent",
    "steps": [{
      "id": "continue",
      "intent": {
        "action": "click",
        "purpose": "continueCheckout",
        "scope": {"regionKind": "checkoutSummary"},
        "resolutionPolicy": "requireUniqueHighConfidence"
      }
    }]
  }
}
```

Workflow traces and checkpoints retain the resolution ID, candidate ID,
revision, policy decision, confidence, evidence, and target fingerprint. A
resume operation re-observes the browser and resolves the pending suffix
again; it does not replay an old target reference.

## Interfaces and limits

- Rust: `BrowserSession::resolve_intent` and
  `BrowserSession::execute_intent` expose the typed contract.
- CLI: `resolve-intent` reports candidates and `execute-intent` performs one
  guarded action after explicit candidate selection.
- MCP: `resolveIntent` and `executeIntent` expose the same fields with strict
  schemas.
- TUI: `resolve-intent FILE` shows candidates and evidence; Up/Down selects a
  candidate and `intent execute [VALUE]` submits it.

Requests, candidates, evidence, values, and suggestions are bounded. Intent
payloads do not include cookies, profile data, screenshots, or raw DOM. Keep
the revision and route with the result when presenting a candidate for review.

The machine-readable resolution schema is
[`docs/schema/intent-resolution-v1.schema.json`](schema/intent-resolution-v1.schema.json).
