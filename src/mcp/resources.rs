//! MCP resource definitions.
//!
//! Static reference documents (contracts, guides) exposed through the MCP
//! `resources/list` and `resources/read` methods.

use serde_json::Value;

use crate::browser::session::BrowserResult;

struct ResourceDef {
    uri: &'static str,
    name: &'static str,
    description: &'static str,
    mime_type: &'static str,
    content: &'static str,
}

/// Return all MCP resources (static reference documents) as a JSON value
/// for the `resources/list` response.
pub fn list_resources() -> BrowserResult<Value> {
    let resources: Vec<Value> = RESOURCES
        .iter()
        .map(|r| {
            serde_json::json!({
                "uri": r.uri,
                "name": r.name,
                "description": r.description,
                "mimeType": r.mime_type,
            })
        })
        .collect();
    Ok(serde_json::json!({ "resources": resources }))
}

/// Return a specific MCP resource by URI with its markdown content, for
/// the `resources/read` response.
pub fn read_resource(uri: &str) -> BrowserResult<Value> {
    let resource =
        RESOURCES
            .iter()
            .find(|r| r.uri == uri)
            .ok_or_else(|| -> Box<dyn std::error::Error> {
                format!("resource not found: {uri}").into()
            })?;
    Ok(serde_json::json!({
        "contents": [{
            "uri": resource.uri,
            "mimeType": resource.mime_type,
            "text": resource.content,
        }]
    }))
}

// ── Resource content ────────────────────────────────────────────

const RESOURCE_LOCATORS: &str = r#"# Glass Locator Grammar

## Explicit forms

Every Glass target string must use an explicit prefix:

| Prefix | Syntax | Example |
|--------|--------|---------|
| `ref=` | Revisioned backend-node reference | `ref=r12:b456` |
| `name=` | Accessible name (case-sensitive, exact match) | `name=Submit` |
| `role=...;name=...` | ARIA role + accessible name | `role=button;name=Save` |
| `text=` | Visible text content (exact match) | `text=Click here` |
| `css=` | CSS selector (must resolve uniquely) | `css=button.save` |
| `ordinal=` | Zero-based position in last `observe` interactive list | `ordinal=3` |

## Fallback chains

Multiple strategies separated by ` | ` (pipe surrounded by spaces):

```
name=Save | css=button.save | css=[type='submit']
```

Rules:
- Max 8 segments, max 1024 bytes per segment.
- Evaluated left to right, short-circuit on first `Unique`.
- Only `NotFound` advances to the next segment.
- `Ambiguous` and `StaleReference` stop the chain immediately.
- Single locator strings without ` | ` behave identically to today.

## Resolution outcomes

- `Unique` — exactly one element matched; action proceeds.
- `Ambiguous` — multiple elements matched; up to 8 bounded candidate
  summaries returned. Agent must refine, not guess.
- `NotFound` — zero matches.
- `StaleReference` — revision prefix does not match current snapshot.
"#;

const RESOURCE_ERRORS: &str = r#"# Glass Typed Errors

## Revision-safe action contract

All targeted mutations accept an optional `expectedRevision`, including
`navigate`, `click`, `clickExpectPopup`, `doubleClick`, `type`, `clear`,
`check`, `uncheck`, `select`, `scroll`, keyboard actions, drag, upload, and
`fillForm`. Successful revision-aware actions return `status`,
`previousRevision`, `currentRevision`, and bounded `verification` metadata.
When the revision no longer matches, the action fails before mutation with
`kind: "stale_revision"`, `phase: "preflight"`, `recovery: "observe"`, and
`recoveryStrategy: "report"`.

The `verify` tool accepts bounded predicates for URL, title, visibility, text,
popup, dialog, download, revision, and Boolean `all`/`any`/`not` composition.
The `batch` tool supports `fixed`, `chain`, and `unguarded` revision modes.

## Targeting errors (`TargetError`)

Targeting failures serialize `kind` and `failureKind`, with optional bounded
`reason`, `candidates`, `recovery`, and `diagnostics` fields:

| `kind` | `failureKind` | Meaning |
|--------|---------------|---------|
| `not_found` | `target_not_found` | Zero elements matched; re-observe |
| `ambiguous` | `ambiguous_target` | Multiple matches; refine the locator |
| `stale_reference` | `stale_revision` | Revision changed; re-observe |
| `not_actionable` | `verification_failed` | The matched element failed actionability checks |

`reason` is a bounded actionability enum such as `disabled`,
`hit_test_blocked`, or `outside_viewport`; it is not a top-level error kind.

## Policy errors (`PolicyErrorContract`)

Every policy failure is a versioned object with these fields:
`schemaVersion`, `policyVersion`, `kind`, optional `operation`, `ruleId`,
`phase`, `reason`, optional `remediation`, and `overridePossible`.
`kind` is one of:

| `kind` | `phase` | Meaning |
|--------|---------|---------|
| `denied` | `preflight` | Capability denied by the active policy |
| `confirmation_required` | `preflight` | One confirmation token is required |
| `invalid_configuration` | `configuration` | Policy configuration is invalid |

`operation` uses the stable snake-case capability name, for example
`read_sensitive_extraction`. A confirmation-required result is not approved
implicitly by an MCP request; configure a one-use token with
`--policy-confirm-once` or change the fixed server policy before retrying.
`overridePossible` indicates whether changing policy or supplying a token can
resolve the failure.

## Wait timeout (`WaitTimeout`)

A timeout serializes `condition`, `deadlineMs`, bounded `lastState`, `reason`,
and an optional bounded `observedPage`. It does not use a generic
`recoverable` flag; callers should use the condition and last state to choose
whether to retry.

## General rules

- Typed errors expose stable machine-readable fields before free-text details.
- Recovery metadata is bounded and must not be treated as permission to retry.
- Error payloads never contain raw page content, evaluated source, or secrets.

"#;

const RESOURCE_LIMITS: &str = r#"# Glass Observation Budgets & Limits

## Compact observe (`observe`)

| Budget | Limit |
|--------|-------|
| Interactive controls | 32 (`COMPACT_AX_MAX_INTERACTIVE`) |
| Accessibility nodes | 128 |
| Visible text | 16 KiB UTF-8 |
| AX label text total | 4 KiB UTF-8 |
| Role name | 64 bytes each |
| Topology events | 64 |
| Targets in list | 32 |
| Frames in list | 128 |

## `controls_truncated: true`

When the page has more than 32 interactive controls, Glass ranks them by
deterministic role priority and stable document order, then returns the top
32. The `omittedCount` field reports how many were excluded. Use
`ranking=document-order` in the library API when comparing against legacy
traversal order.

When `includeFormValues` is enabled, at most 16 known form controls are read;
values are capped at 256 bytes, select labels at 128 bytes, and sensitive
fields are redacted unless `read_sensitive_form_values` is explicitly allowed.

Structured extraction accepts at most 32 fields. Field names are limited to
128 bytes and paths to 512 bytes. `maxItems` is 1..256 (default 64), and
`maxBytes` is 1..256 KiB (default 64 KiB). `startIndex` and continuation
`nextIndex` are bounded to 256. Continuation route IDs are limited to 128
bytes and route URLs to 2 KiB.

Structured extraction of secret-like fields requires the dedicated
`read_sensitive_extraction` capability; form-value permissions do not grant it.

`preflight` performs a read-only resolution and hit test without pointer
events, focus, or scrolling. `clickAt` is the policy-gated exact-coordinate
escape hatch for canvas and map surfaces.

## `incomplete` reasons

| Reason | Meaning |
|--------|---------|
| `ShadowBoundary` | Open/closed shadow DOM detected; controls inside shadows may be missing |
| `ControlsTruncated` | Exceeded 32-control budget |
| `TextTruncated` | Visible text exceeded 16 KiB |
| `MutationRace` | Page mutated during collection; snapshot is inconsistent |

## Other limits

| Resource | Limit |
|----------|-------|
| Candidate summaries in `Ambiguous` | 8 |
| Candidate label text each | 160 bytes |
| Locator fallback chain segments | 8 |
| Locator segment bytes | 1024 |
| Dialog wait timeout | 500 ms |
| Download timeout | 30 s |
| Diagnostic duration | 30 s |
| Wait timeout default | 10 s |
"#;

const RESOURCE_ACTIONS: &str = r#"# Glass Action Contract

Every action result has a session-local `executionId`, a page `revision`, and
bounded verification evidence. Revisioned actions reject stale observations
before dispatch. Failures carry a typed `kind`, `phase`, `recoveryStrategy`,
and bounded recovery hint.

Use `verify` for explicit postconditions. Its predicates are finite and
deadline-bound; arbitrary JavaScript is not accepted. Use batch `mode: fixed`
to reuse one revision, `mode: chain` to carry revisions forward, or
`mode: unguarded` for compatibility behavior.
"#;

const RESOURCE_TOPOLOGY: &str = r#"# Glass Target & Frame Topology

## Target lifecycle

- `createTarget(url)` creates a new page target. It is not automatically
  selected.
- `selectTarget(id)` switches the active target. All subsequent operations
  (observe, click, type, etc.) run against this target.
- `closeTarget(id)` closes a target. Glass never auto-selects a replacement
  — the session is left without an active target until `selectTarget` is
  called.
- `listTargets()` returns up to 32 bounded page targets with ID, URL, and
  title.

## Frame selection

- `selectFrame(id)` sets the frame execution context within the active
  target.
- `listFrames()` returns up to 128 frames with parent linkage.
- Without explicit frame selection, operations run in the top-level frame.

## Popup management

- `click` on an opener element, then `listTargets` to discover the newly
  opened page, then `selectTarget` to work with it.
- Glass never silently selects a new popup or tab.
- Topology events (in `observe.topology`) report target creation and
  closure with `kind` + `id`.

## Attach mode

- `--attach` connects to an existing Chrome instance via CDP port.
- In attach mode, Glass does not own the browser lifecycle (does not
  close Chrome on exit).
- Attach mode reports `attachMode: true` in session metadata.
"#;

const RESOURCE_EXPERIENCE: &str = r#"# Glass Experience Layer

CLI, MCP, daemon, and Rust callers share capability-oriented operations:

- `workspace/*` identifies lifecycle, profile ownership, and scoped resource references.
- `memory/*` is advisory only; fresh Web IR, revision, policy, and capability evidence remain authoritative.
- `surfaces/*` reports understanding, coverage, and provenance; opaque and coordinate-only surfaces cannot compile semantic actions.
- `backend/*` reports declared capabilities and portability. Omitted capabilities fail closed.
- `replay/*` validates, compares, and attaches bounded redacted evidence without browser side effects.

Experience responses always expose a schema version, interface provenance, and
typed `resourceRefs` (when a workspace resource is known). A resource reference
is scoped to a workspace/profile and ephemeral generation; it is not an
executable locator or a mutation grant.

One daemon actor lease serializes workspace mutation. `observe`, `inspect`,
`extract`, `resolve`, `verify`, and replay operations are read-only and do not
acquire that lease. Mutation requires the current lease token and revision;
expiry or scope mismatch fails closed.

Replay diff/attach validates exact scenario and recording hashes, redacted event
shape, and bounded input size. Attach means attaching evidence metadata only;
it never starts Chrome, connects to an external browser, or grants takeover.

Every result is bounded and carries provenance, policy, portability, verification, and
workflow timeline fields when available. Partial backend or surface gates are reported
honestly and are never promoted to real-browser parity.
"#;

// ── Resource definitions ────────────────────────────────────────

const RESOURCES: &[ResourceDef] = &[
    ResourceDef {
        uri: "glass://contract/actions",
        name: "Action Execution Contract",
        description: "Revision guards, execution identities, verification predicates, and batch modes",
        mime_type: "text/markdown",
        content: RESOURCE_ACTIONS,
    },
    ResourceDef {
        uri: "glass://contract/locators",
        name: "Locator Grammar",
        description: "Locator forms, ambiguity rules, and delimiter syntax for targeting",
        mime_type: "text/markdown",
        content: RESOURCE_LOCATORS,
    },
    ResourceDef {
        uri: "glass://contract/errors",
        name: "Typed Errors & Recovery",
        description: "Typed targeting, policy, and timeout errors with bounded recovery metadata",
        mime_type: "text/markdown",
        content: RESOURCE_ERRORS,
    },
    ResourceDef {
        uri: "glass://contract/experience",
        name: "Experience Layer",
        description: "Shared workspace, memory, surface, backend, replay, policy, and result contracts",
        mime_type: "text/markdown",
        content: RESOURCE_EXPERIENCE,
    },
    ResourceDef {
        uri: "glass://contract/limits",
        name: "Observation Budgets & Limits",
        description: "Observation caps (32 controls, 16 KiB text) and incompleteness reasons",
        mime_type: "text/markdown",
        content: RESOURCE_LIMITS,
    },
    ResourceDef {
        uri: "glass://contract/topology",
        name: "Target & Frame Topology",
        description: "Target and frame selection semantics, attach rules, and popup management",
        mime_type: "text/markdown",
        content: RESOURCE_TOPOLOGY,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_all_resources() {
        let result = list_resources().unwrap();
        let resources = result["resources"].as_array().unwrap();
        assert_eq!(resources.len(), 6);
        let uris: Vec<&str> = resources
            .iter()
            .map(|r| r["uri"].as_str().unwrap())
            .collect();
        assert!(uris.contains(&"glass://contract/actions"));
        assert!(uris.contains(&"glass://contract/locators"));
        assert!(uris.contains(&"glass://contract/errors"));
        assert!(uris.contains(&"glass://contract/limits"));
        assert!(uris.contains(&"glass://contract/topology"));
        assert!(uris.contains(&"glass://contract/experience"));
    }

    #[test]
    fn reads_each_resource_with_correct_mime_type() {
        for uri in &[
            "glass://contract/actions",
            "glass://contract/locators",
            "glass://contract/errors",
            "glass://contract/limits",
            "glass://contract/topology",
        ] {
            let result = read_resource(uri).unwrap();
            let contents = result["contents"].as_array().unwrap();
            assert_eq!(contents.len(), 1);
            assert_eq!(contents[0]["uri"].as_str().unwrap(), *uri);
            assert_eq!(contents[0]["mimeType"].as_str().unwrap(), "text/markdown");
            let text = contents[0]["text"].as_str().unwrap();
            assert!(!text.is_empty());
        }
    }

    #[test]
    fn limits_resource_documents_dedicated_extraction_capability() {
        let resource = read_resource("glass://contract/limits").unwrap();
        let text = resource["contents"][0]["text"].as_str().unwrap();
        assert!(text.contains("read_sensitive_extraction"));
        assert!(text.contains("form-value permissions do not grant it"));
    }

    #[test]
    fn rejects_unknown_resource() {
        assert!(read_resource("glass://nonexistent").is_err());
    }

    #[test]
    fn total_static_payload_under_32kib() {
        let mut total = 0usize;
        for resource in RESOURCES {
            total += resource.uri.len()
                + resource.name.len()
                + resource.description.len()
                + resource.content.len();
        }
        assert!(
            total <= 32 * 1024,
            "total resource payload {} exceeds 32 KiB",
            total
        );
    }
}
