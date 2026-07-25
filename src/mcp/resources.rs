use serde_json::Value;

use crate::browser::session::BrowserResult;

struct ResourceDef {
    uri: &'static str,
    name: &'static str,
    description: &'static str,
    mime_type: &'static str,
    content: &'static str,
}

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

## Targeting errors (`TargetError`)

| `kind` | `recoverable` | `suggested_tool` | Meaning |
|--------|--------------|-------------------|---------|
| `not_found` | `true` | `observe` | Zero elements matched |
| `ambiguous` | `false` | — | Multiple matches; agent must refine |
| `stale_reference` | `true` | `observe` | Revision changed; re-observe |
| `disabled` | `false` | — | Element exists but is disabled |
| `hit_test_blocked` | `true` | `dismissDialog` or manual | Overlay/obstruction |
| `outside_viewport` | `true` | `scroll` | Out of visible area |

## Policy errors (`PolicyError`)

| `kind` | `recoverable` | Meaning |
|--------|--------------|---------|
| `denied` | `false` | Capability denied by active policy preset |
| `confirmation_required` | `false` | Requires one-time approval (not via MCP) |

## Wait timeout (`WaitTimeout`)

| `kind` | `recoverable` | Meaning |
|--------|--------------|---------|
| `timeout` | `true` | Condition not met within deadline |

## General rules

- `recoverable: true` means retrying the same operation may succeed
  (after re-observing, scrolling, dismissing overlays, etc.).
- `recoverable: false` means retrying the same operation will fail
  identically — change locator, policy, or approach.
- Error payloads are bounded; they never contain raw page content,
  evaluated source, or secrets.
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
relevance (viewport visibility, focus, form membership, role priority)
and returns the top 32. The `omittedCount` field reports how many were
excluded.

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

// ── Resource definitions ────────────────────────────────────────

const RESOURCES: &[ResourceDef] = &[
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
        description: "Error kinds with recoverable flag and suggested recovery tool",
        mime_type: "text/markdown",
        content: RESOURCE_ERRORS,
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
        assert_eq!(resources.len(), 4);
        let uris: Vec<&str> = resources
            .iter()
            .map(|r| r["uri"].as_str().unwrap())
            .collect();
        assert!(uris.contains(&"glass://contract/locators"));
        assert!(uris.contains(&"glass://contract/errors"));
        assert!(uris.contains(&"glass://contract/limits"));
        assert!(uris.contains(&"glass://contract/topology"));
    }

    #[test]
    fn reads_each_resource_with_correct_mime_type() {
        for uri in &[
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
