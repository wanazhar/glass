//! MCP prompt definitions.
//!
//! Static agent-facing prompt templates exposed through the MCP
//! `prompts/list` and `prompts/get` methods.

use serde_json::Value;

use crate::browser::session::BrowserResult;

struct PromptDef {
    name: &'static str,
    description: &'static str,
    content: &'static str,
}

/// Return all MCP prompts (agent-facing templates) as a JSON value for the
/// `prompts/list` response.
pub fn list_prompts() -> BrowserResult<Value> {
    let prompts: Vec<Value> = PROMPTS
        .iter()
        .map(|p| {
            serde_json::json!({
                "name": p.name,
                "description": p.description,
            })
        })
        .collect();
    Ok(serde_json::json!({ "prompts": prompts }))
}

/// Return a specific MCP prompt by name, including its content, for the
/// `prompts/get` response.
pub fn get_prompt(name: &str) -> BrowserResult<Value> {
    let prompt =
        PROMPTS
            .iter()
            .find(|p| p.name == name)
            .ok_or_else(|| -> Box<dyn std::error::Error> {
                format!("prompt not found: {name}").into()
            })?;
    Ok(serde_json::json!({
        "description": prompt.description,
        "messages": [{
            "role": "user",
            "content": {
                "type": "text",
                "text": prompt.content,
            }
        }]
    }))
}

// ── Prompt content ──────────────────────────────────────────────

const PROMPT_SAFE_NAVIGATION: &str = r#"# Glass Safe Navigation Loop

Glass is a deterministic browser control plane for AI agents. Follow this
pattern for every task.

## Core loop

1. `navigate` to the target URL.
2. `observe` (compact context) — this is your primary page view.
   - Returns page identity, bounded visible text, and up to 32 interactive
     controls with revisioned refs (e.g. `r12:b456`).
   - Default `includeDom: false` and `includeScreenshot: false` to keep
     context small and fast.
3. Choose one action based on `observe` output.
4. After every action, re-`observe` — actions invalidate snapshot revisions.
5. Repeat steps 3–4 until the task is complete.

## Revision rules

- Every `observe` response includes a `revision` number.
- Element refs are `r<revision>:b<backend-node-id>` — the revision prefix
  ties each ref to the snapshot that produced it.
- Using a ref from an older revision returns `StaleReference` — this is
  deliberate safety, not a bug.
- After navigation, form submission, client-side routing (SPA), or any
  DOM-mutating action, call `observe` again to get fresh refs.

## When to escalate

- `observe` returns `incomplete: ["ShadowBoundary"]` — consider `getDOM`.
- `observe` returns `controls_truncated: true` — critical controls may be
  omitted; re-observe or use `getDOM`.
- The task requires exact pixel positioning (canvas, maps) — use
  `click-at(x, y)` if available, otherwise `screenshot` + `getDOM`.

Glass sessions run under a named policy preset:

| Preset | Evaluate | Screenshot | Upload | Download | Raw CDP | Profiles |
|--------|----------|------------|--------|----------|---------|----------|
| `dev` | Allowed | Allowed | Allowed | Allowed | Allowed | Allowed |
| `ci` | Allowed | Allowed | Allowed | Allowed | Denied | Denied |
| `hardened` | Confirm | Denied | Denied | Denied | Denied | Confirm |
| `untrusted-mcp` | Confirm | Confirm | Confirm | Confirm | Denied | Denied |

- Policy denials are typed JSON errors with `kind: "denied"`.
- Do not retry a denied operation — adjust the task to work within policy.
- Use `listPolicy` to inspect the current session's policy state.
"#;

const PROMPT_TARGET_SELECTION: &str = r#"# Glass Target Selection & Locator Grammar

Glass requires **unique** resolution for every action. There is no
"pick the first match" shortcut.

## Locator forms (in descending specificity order)

| Form | Example | When to use |
|------|---------|-------------|
| `ref=r12:b456` | Revisioned reference from `observe` | Always preferred — fastest, safest |
| `name=Submit` | Accessible name match | When ref is stale and name is unique |
| `role=button;name=Save` | Role + accessible name | Disambiguate when name alone matches multiple |
| `text=Click here` | Visible text content | Plain text links or buttons |
| `css=button.save` | CSS selector | Structural targeting when AX insufficient |
| `ordinal=3` | Ordinal position in `observe` list | Last resort; fragile across layout changes |

## Ambiguity rules (NEVER guess)

- `Ambiguous` returns up to 8 candidate summaries. **Stop and ask the user**
  which one they meant, or refine your locator.
- Do not iterate candidates or "pick the best one."
- `NotFound` means zero matches — check spelling, wait for lazy content, or
  use `observe` to re-discover controls.
- `StaleReference` means the revision has changed — re-`observe` and use a
  fresh ref.

## Locator fallback chains

Glass supports pipe-delimited fallback chains (max 8 segments):
`name=Save | css=button.save | css=[type='submit']`

- Evaluated left to right.
- First `Unique` match wins.
- Only `NotFound` advances to the next segment.
- `Ambiguous` and `StaleReference` stop the chain immediately.
- Single-segment locators behave identically to today.
"#;

const PROMPT_TOPOLOGY: &str = r#"# Glass Topology: Targets, Frames & Popups

Glass manages multi-target sessions explicitly. There is no implicit
tab-switching.

## Targets (browser tabs/pages)

- `listTargets` returns up to 32 bounded page targets with IDs, URLs, and
  titles.
- `createTarget(url)` creates a new page without selecting it.
- `selectTarget(id)` selects the active target for subsequent tools.
- `closeTarget(id)` closes a target. Glass never auto-selects a replacement.

## Frames

- `listFrames` returns up to 128 frames in the active target.
- `selectFrame(id)` selects the execution context.
- If you do not explicitly select a frame, operations run in the default
  (top-level) frame.
- Child frames are reported in `listFrames` with parent linkage.

## Popups and new windows

- Instead of deprecated `clickExpectPopup`: `click` the opener element,
  then `listTargets` to discover the new target, then `selectTarget`
  to switch to it.
- Glass never silently selects a newly opened popup or tab.
- Always `listTargets` after a click that may open a new window.

## Topology events

- The `topology` field in `observe` includes bounded event history (up to
  64 events) — targets created, closed, and frame navigations.
- Events are summary-only (`kind` + `id`). Full target metadata is in
  `listTargets`.
"#;

const PROMPT_RECOVERY: &str = r#"# Glass Error Recovery Guide

Glass errors are typed and bounded. Use the error `kind` to decide your
next step.

## Targeting errors

| Error kind | Meaning | Recovery |
|------------|---------|----------|
| `not_found` | Zero elements matched | Check spelling; wait for lazy content; re-`observe` |
| `ambiguous` | Multiple elements matched | Refine locator or use `role=...;name=...` |
| `stale_reference` | Revision changed since last `observe` | Re-`observe` and use a fresh ref |
| `disabled` | Element is present but disabled | Report to user; cannot interact |
| `hit_test_blocked` | Overlay or other element on top | Dismiss overlay first, then retry |
| `outside_viewport` | Element scrolled out of view | Scroll into view, then retry |

## Revision-safe actions

`navigate`, `click`, `type`, and `fillForm` accept `expectedRevision` from the
latest `observe`. Successful guarded actions return `status`, the previous and
current revision, and bounded verification evidence. A mismatched revision
returns typed `stale_revision` before browser mutation; observe again before
retrying.

## Policy errors

- `kind: "denied"` — operation blocked by active policy preset.
- `kind: "confirmation_required"` — operation needs one-time approval.
- Do not retry denied operations. Change the approach, not the policy.

## Dialog handling

- After an action that may trigger `alert`, `confirm`, or `prompt`,
  check for dialogs with `observe` (topology reports dialog state).
- `acceptDialog` / `dismissDialog` resolve the current dialog.
- Multiple concurrent dialogs are not supported — resolve one at a time.

## General principles

1. If a ref is stale → `observe` again.
2. If a click is blocked → check for overlays, dialogs, or scroll position.
3. If the page changed (SPA navigation) → `observe` again.
4. If policy denies an operation → change your approach.
5. Never retry the same failing operation more than twice without
   re-observing.
"#;

// ── Prompt definitions ──────────────────────────────────────────

const PROMPTS: &[PromptDef] = &[
    PromptDef {
        name: "glass-safe-navigation",
        description: "The canonical navigate → observe → act loop with revision rules",
        content: PROMPT_SAFE_NAVIGATION,
    },
    PromptDef {
        name: "glass-target-selection",
        description: "Locator syntax, ambiguity handling, and when to use ref= vs role=...;name=...",
        content: PROMPT_TARGET_SELECTION,
    },
    PromptDef {
        name: "glass-topology",
        description: "Targets, frames, and popups — when to call listTargets / selectFrame",
        content: PROMPT_TOPOLOGY,
    },
    PromptDef {
        name: "glass-recovery",
        description: "Stale refs, actionability failures, and typed error recovery paths",
        content: PROMPT_RECOVERY,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_all_prompts() {
        let result = list_prompts().unwrap();
        let prompts = result["prompts"].as_array().unwrap();
        assert_eq!(prompts.len(), 4);
        let names: Vec<&str> = prompts
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"glass-safe-navigation"));
        assert!(names.contains(&"glass-target-selection"));
        assert!(names.contains(&"glass-topology"));
        assert!(names.contains(&"glass-recovery"));
    }

    #[test]
    fn gets_each_prompt_within_budget() {
        for name in &[
            "glass-safe-navigation",
            "glass-target-selection",
            "glass-topology",
            "glass-recovery",
        ] {
            let result = get_prompt(name).unwrap();
            assert!(!result["description"].as_str().unwrap().is_empty());
            let messages = result["messages"].as_array().unwrap();
            assert_eq!(messages.len(), 1);
            let text = messages[0]["content"]["text"].as_str().unwrap();
            assert!(
                text.len() <= 4096,
                "prompt {name} body is {} bytes, exceeds 4 KiB",
                text.len()
            );
        }
    }

    #[test]
    fn rejects_unknown_prompt() {
        assert!(get_prompt("nonexistent").is_err());
    }

    #[test]
    fn total_static_payload_under_32kib() {
        let mut total = 0usize;
        for prompt in PROMPTS {
            total += prompt.name.len() + prompt.description.len() + prompt.content.len();
        }
        assert!(
            total <= 32 * 1024,
            "total prompt payload {} exceeds 32 KiB",
            total
        );
    }
}
