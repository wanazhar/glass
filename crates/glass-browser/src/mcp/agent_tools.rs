//! Agent tool profile: a small `browser_*` catalog written for models that
//! must discover and choose tools on their own.
//!
//! Each tool maps onto one existing MCP tool, so behaviour, policy and
//! revision guards are shared with the full catalog. Descriptions say when to
//! use a tool and what to use instead; errors gain a next step.

use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};

static AGENT_PROFILE: AtomicBool = AtomicBool::new(false);

/// Select whether `tools/list` returns the agent profile.
pub(crate) fn set_agent_profile(enabled: bool) {
    AGENT_PROFILE.store(enabled, Ordering::Relaxed);
}

pub(crate) fn agent_profile() -> bool {
    AGENT_PROFILE.load(Ordering::Relaxed)
}

const TARGET: &str = "Element to act on. Prefer the `ref` from the latest browser_snapshot (e.g. \"e12\"). Otherwise: \"role=button;name=Save\", \"name=Email\" (accessible name or label), \"text=Sign in\", \"css=#email\".";

fn target_schema() -> Value {
    json!({"type":"string","minLength":1,"maxLength":512,"description":TARGET,"examples":["e12","role=button;name=Create account","name=Work email","text=Load report"]})
}

fn timeout_schema(default: u64, maximum: u64) -> Value {
    json!({"type":"integer","minimum":100,"maximum":maximum,"default":default,"description":"Deadline in milliseconds."})
}

/// The agent catalog, in the order a model usually needs it.
pub(crate) fn agent_tools() -> Vec<Value> {
    let tool = |name: &str, description: &str, schema: Value| json!({"name": name, "description": description, "inputSchema": schema});
    vec![
        tool(
            "browser_navigate",
            "Open a URL in the shared browser tab. Use first for any web page task, including localhost and pages that need clicks, forms or JavaScript; the page stays open for later browser_* calls. Not for plain static reads where a fetch tool suffices. Example: {\"url\":\"https://example.com/login\"}.",
            json!({"type":"object","properties":{"url":{"type":"string","minLength":1,"maxLength":8192,"description":"Absolute http(s) or file URL.","examples":["https://example.com"]},"timeoutMs":timeout_schema(20000,120000)},"required":["url"],"additionalProperties":false}),
        ),
        tool(
            "browser_snapshot",
            "Read the current page as an accessibility snapshot: headings, text, links, buttons and form fields with a `ref` for each interactive element. Use after navigating or after any action that changes the page, before clicking or typing, and to read page content. Use browser_screenshot instead only for canvas, images or visual layout. Example: {}.",
            json!({"type":"object","properties":{"region":{"type":"string","minLength":1,"maxLength":128,"description":"Optional semantic region to focus, such as \"main\" or \"form\"."}},"additionalProperties":false}),
        ),
        tool(
            "browser_get_text",
            "Return all visible text of the current page. Use to read long articles or lists quickly; use browser_snapshot when you also need element refs to act on. Example: {}.",
            json!({"type":"object","properties":{},"additionalProperties":false}),
        ),
        tool(
            "browser_click",
            "Click a link, button, checkbox, tab or menu item identified by ref, role and name, label or text. Use for normal page controls. For canvas or map surfaces with no element, use browser_click_at; for select dropdowns use browser_select_option. Example: {\"target\":\"role=button;name=Create account\"}.",
            json!({"type":"object","properties":{"target":target_schema()},"required":["target"],"additionalProperties":false}),
        ),
        tool(
            "browser_type",
            "Type text into one input or editor. Give `target` to focus it first; omit it to type into the focused element. For several fields at once use browser_fill_form; to press Enter, Tab or arrows use browser_press_key. Example: {\"target\":\"name=Search\",\"text\":\"kuala\"}.",
            json!({"type":"object","properties":{"target":target_schema(),"text":{"type":"string","maxLength":8192}},"required":["text"],"additionalProperties":false}),
        ),
        tool(
            "browser_fill_form",
            "Fill several text fields in one call (up to 16), resolving every field before changing any. Use for sign-up, checkout and settings forms; then use browser_select_option, browser_check and browser_click for dropdowns, checkboxes and the submit button. Example: {\"fields\":[{\"target\":\"name=Full name\",\"value\":\"Aisyah Karim\"},{\"target\":\"name=Work email\",\"value\":\"a@example.com\"}]}.",
            json!({"type":"object","properties":{"fields":{"type":"array","minItems":1,"maxItems":16,"items":{"type":"object","properties":{"target":target_schema(),"value":{"type":"string","maxLength":8192}},"required":["target","value"],"additionalProperties":false}}},"required":["fields"],"additionalProperties":false}),
        ),
        tool(
            "browser_select_option",
            "Choose an option in a native <select> dropdown by its value or visible label. Not for custom comboboxes built from text inputs; for those type with browser_type and pick the suggestion with browser_click or browser_press_key. Example: {\"target\":\"name=Plan\",\"value\":\"Team\"}.",
            json!({"type":"object","properties":{"target":target_schema(),"value":{"type":"string","minLength":1,"maxLength":1024}},"required":["target","value"],"additionalProperties":false}),
        ),
        tool(
            "browser_check",
            "Set a checkbox or radio button to checked or unchecked; does nothing if it is already in that state. Example: {\"target\":\"name=I agree to the terms\",\"checked\":true}.",
            json!({"type":"object","properties":{"target":target_schema(),"checked":{"type":"boolean","default":true}},"required":["target"],"additionalProperties":false}),
        ),
        tool(
            "browser_press_key",
            "Press one key or shortcut in the page: Enter, Tab, Escape, ArrowDown, ArrowUp, Backspace, PageDown, or a chord such as Control+A. Use for keyboard-driven widgets (autocomplete lists, menus), submitting with Enter and closing popovers. Example: {\"key\":\"ArrowDown\"}.",
            json!({"type":"object","properties":{"key":{"type":"string","minLength":1,"maxLength":64,"examples":["Enter","ArrowDown","Escape","Control+A","Shift+Tab"]}},"required":["key"],"additionalProperties":false}),
        ),
        tool(
            "browser_scroll",
            "Scroll the page to reveal more content, including infinite lists that load as you scroll; then call browser_snapshot or browser_get_text to read what appeared. Not needed before clicking an element that already has a ref. Example: {\"direction\":\"down\",\"amount\":1200}.",
            json!({"type":"object","properties":{"direction":{"type":"string","enum":["down","up","right","left"],"default":"down"},"amount":{"type":"integer","minimum":1,"maximum":20000,"default":800,"description":"CSS pixels."}},"additionalProperties":false}),
        ),
        tool(
            "browser_wait_for",
            "Wait until the page reaches a state: text appears, a URL is reached, or an element becomes visible or hidden. Use after clicks that load content asynchronously (spinners, \"Loading…\", delayed reports) instead of re-reading in a loop. Give exactly one condition. Example: {\"text\":\"Net revenue\",\"timeoutMs\":15000}.",
            json!({"type":"object","properties":{"text":{"type":"string","minLength":1,"maxLength":1024},"url":{"type":"string","minLength":1,"maxLength":8192,"description":"URL prefix to wait for."},"visible":target_schema(),"hidden":target_schema(),"timeoutMs":timeout_schema(15000,120000)},"minProperties":1,"additionalProperties":false}),
        ),
        tool(
            "browser_screenshot",
            "Capture a PNG of the viewport (or full page) to see visual content: canvas, charts, images, colours and layout. Pixel coordinates in the image are CSS pixels you can pass to browser_click_at. Use browser_snapshot instead to read text or find controls. Example: {}.",
            json!({"type":"object","properties":{"fullPage":{"type":"boolean","default":false},"target":target_schema()},"additionalProperties":false}),
        ),
        tool(
            "browser_click_at",
            "Click viewport coordinates in CSS pixels, read from browser_screenshot. Use only for canvas, maps, games or other drawn surfaces with no clickable element; prefer browser_click otherwise. Example: {\"x\":512,\"y\":300}.",
            json!({"type":"object","properties":{"x":{"type":"number","minimum":0,"maximum":20000},"y":{"type":"number","minimum":0,"maximum":20000}},"required":["x","y"],"additionalProperties":false}),
        ),
        tool(
            "browser_hover",
            "Move the pointer over an element to reveal hover menus or tooltips. Example: {\"target\":\"text=Products\"}.",
            json!({"type":"object","properties":{"target":target_schema()},"required":["target"],"additionalProperties":false}),
        ),
        tool(
            "browser_tabs",
            "List, open, switch to or close browser tabs. Use when a link opens a new tab or window (target=_blank): call with action \"list\", then \"select\" the new tab's id before reading it. Example: {\"action\":\"select\",\"id\":\"8F2C…\"}.",
            json!({"type":"object","properties":{"action":{"type":"string","enum":["list","new","select","close"]},"id":{"type":"string","minLength":1,"maxLength":128,"description":"Tab id from action \"list\"; required for select and close."},"url":{"type":"string","minLength":1,"maxLength":8192,"description":"URL for action \"new\"."}},"required":["action"],"additionalProperties":false}),
        ),
        tool(
            "browser_handle_dialog",
            "Accept or dismiss the open JavaScript alert, confirm or prompt dialog. Use when an action reports a blocking dialog, or right after clicking something that asks \"Are you sure?\". Example: {\"accept\":true}.",
            json!({"type":"object","properties":{"accept":{"type":"boolean"}},"required":["accept"],"additionalProperties":false}),
        ),
        tool(
            "browser_upload_file",
            "Attach local files to a file input (<input type=file>) without opening a file picker. Paths are absolute or relative to the working directory. Then click the form's submit button. Example: {\"target\":\"name=Receipt file\",\"paths\":[\"receipt.pdf\"]}.",
            json!({"type":"object","properties":{"target":target_schema(),"paths":{"type":"array","minItems":1,"maxItems":16,"items":{"type":"string","minLength":1,"maxLength":4096}}},"required":["target","paths"],"additionalProperties":false}),
        ),
        tool(
            "browser_download",
            "Wait for the next file download and save it into an existing directory; returns the saved path. Start the download with browser_click on the link in the same turn first, or call this right before clicking. Example: {\"directory\":\".\"}.",
            json!({"type":"object","properties":{"directory":{"type":"string","minLength":1,"maxLength":4096,"default":"."},"timeoutMs":timeout_schema(30000,30000)},"additionalProperties":false}),
        ),
        tool(
            "browser_clipboard",
            "Read or write the system clipboard text used by the browser, for copy and paste flows. Example: {\"action\":\"write\",\"text\":\"INV-42\"}.",
            json!({"type":"object","properties":{"action":{"type":"string","enum":["read","write"]},"text":{"type":"string","maxLength":8192}},"required":["action"],"additionalProperties":false}),
        ),
    ]
}

pub(crate) fn is_agent_tool(name: &str) -> bool {
    name.starts_with("browser_")
}

fn string<'a>(arguments: &'a Value, key: &str) -> Option<&'a str> {
    arguments.get(key).and_then(Value::as_str)
}

fn need<'a>(arguments: &'a Value, key: &str, tool: &str) -> Result<&'a str, String> {
    string(arguments, key).ok_or_else(|| {
        format!(
            "{tool} requires a string `{key}`. Example: {}",
            example(tool)
        )
    })
}

fn example(tool: &str) -> String {
    agent_tools()
        .into_iter()
        .find(|t| t["name"] == tool)
        .and_then(|t| {
            t["description"]
                .as_str()
                .and_then(|d| d.split("Example: ").nth(1))
                .map(|e| e.trim_end_matches('.').to_string())
        })
        .unwrap_or_else(|| "{}".into())
}

fn reject_unknown(arguments: &Value, tool: &str) -> Result<(), String> {
    let Some(catalog) = agent_tools().into_iter().find(|t| t["name"] == tool) else {
        return Err(format!(
            "Unknown tool `{tool}`. Available browser tools: {}.",
            agent_tools()
                .iter()
                .filter_map(|t| t["name"].as_str().map(str::to_owned))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };
    if let Some(object) = arguments.as_object() {
        let allowed = catalog["inputSchema"]["properties"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        if let Some(key) = object.keys().find(|key| !allowed.contains_key(*key)) {
            return Err(format!(
                "{tool} does not accept `{key}`. Allowed: {}. Example: {}",
                allowed.keys().cloned().collect::<Vec<_>>().join(", "),
                example(tool)
            ));
        }
    } else if !arguments.is_null() {
        return Err(format!(
            "{tool} arguments must be an object. Example: {}",
            example(tool)
        ));
    }
    Ok(())
}

/// Map an agent tool call onto the full-catalog tool that implements it.
pub(crate) fn translate(name: &str, arguments: &Value) -> Result<(&'static str, Value), String> {
    reject_unknown(arguments, name)?;
    let a = arguments;
    Ok(match name {
        "browser_navigate" => {
            let mut out = json!({"url": need(a, "url", name)?});
            if let Some(timeout) = a.get("timeoutMs") {
                out["timeoutMs"] = timeout.clone();
            }
            ("navigate", out)
        }
        "browser_snapshot" => {
            let mut out = json!({"level": "structured"});
            if let Some(region) = string(a, "region") {
                out["region"] = json!(region);
            }
            ("observe", out)
        }
        "browser_get_text" => ("getText", json!({})),
        "browser_click" => ("click", json!({"target": need(a, "target", name)?})),
        "browser_hover" => ("hover", json!({"target": need(a, "target", name)?})),
        "browser_type" => {
            let mut out = json!({"text": need(a, "text", name)?});
            if let Some(target) = string(a, "target") {
                out["target"] = json!(target);
            }
            ("type", out)
        }
        "browser_fill_form" => {
            let fields = a
                .get("fields")
                .and_then(Value::as_array)
                .filter(|fields| !fields.is_empty())
                .ok_or_else(|| {
                    format!(
                        "{name} requires a non-empty `fields` array. Example: {}",
                        example(name)
                    )
                })?;
            ("fillForm", json!({"fields": fields}))
        }
        "browser_select_option" => (
            "select",
            json!({"target": need(a, "target", name)?, "value": need(a, "value", name)?}),
        ),
        "browser_check" => {
            let target = need(a, "target", name)?;
            let checked = a.get("checked").and_then(Value::as_bool).unwrap_or(true);
            (
                if checked { "check" } else { "uncheck" },
                json!({"target": target}),
            )
        }
        "browser_press_key" => {
            let key = need(a, "key", name)?;
            let key = normalize_key(key);
            if key.len() > 1 && key.contains('+') {
                ("shortcut", json!({"shortcut": key}))
            } else {
                ("key", json!({"key": key}))
            }
        }
        "browser_scroll" => {
            let amount = a.get("amount").and_then(Value::as_f64).unwrap_or(800.0);
            let (dx, dy) = match string(a, "direction").unwrap_or("down") {
                "down" => (0.0, amount),
                "up" => (0.0, -amount),
                "right" => (amount, 0.0),
                "left" => (-amount, 0.0),
                other => {
                    return Err(format!(
                        "{name} direction must be down, up, left or right, not `{other}`."
                    ));
                }
            };
            ("scroll", json!({"dx": dx, "dy": dy}))
        }
        "browser_wait_for" => {
            let conditions = [
                ("text", "text"),
                ("url", "url-prefix"),
                ("visible", "target-visible"),
                ("hidden", "target-hidden"),
            ]
            .into_iter()
            .filter_map(|(key, kind)| string(a, key).map(|value| format!("{kind}={value}")))
            .collect::<Vec<_>>();
            let [condition] = conditions.as_slice() else {
                return Err(format!(
                    "{name} needs exactly one of `text`, `url`, `visible` or `hidden`. Example: {}",
                    example(name)
                ));
            };
            (
                "wait",
                json!({"condition": condition, "timeoutMs": a.get("timeoutMs").and_then(Value::as_u64).unwrap_or(15000)}),
            )
        }
        "browser_screenshot" => {
            let mut out = json!({"format": "png"});
            if let Some(full) = a.get("fullPage") {
                out["fullPage"] = full.clone();
            }
            if let Some(target) = string(a, "target") {
                out["target"] = json!(target);
            }
            ("screenshot", out)
        }
        "browser_click_at" => {
            let coordinate = |key: &str| {
                a.get(key)
                    .and_then(Value::as_f64)
                    .ok_or_else(|| format!("{name} requires numeric `x` and `y` in CSS pixels from browser_screenshot. Example: {}", example(name)))
            };
            (
                "clickAt",
                json!({"x": coordinate("x")?, "y": coordinate("y")?}),
            )
        }
        "browser_tabs" => match need(a, "action", name)? {
            "list" => ("listTargets", json!({})),
            "new" => (
                "createTarget",
                json!({"url": string(a, "url").unwrap_or("about:blank")}),
            ),
            "select" => ("selectTarget", json!({"id": need(a, "id", name)?})),
            "close" => ("closeTarget", json!({"id": need(a, "id", name)?})),
            other => {
                return Err(format!(
                    "{name} action must be list, new, select or close, not `{other}`."
                ));
            }
        },
        "browser_handle_dialog" => {
            let accept = a.get("accept").and_then(Value::as_bool).ok_or_else(|| {
                format!(
                    "{name} requires boolean `accept`. Example: {}",
                    example(name)
                )
            })?;
            (
                if accept {
                    "acceptDialog"
                } else {
                    "dismissDialog"
                },
                json!({}),
            )
        }
        "browser_upload_file" => {
            let paths = a
                .get("paths")
                .and_then(Value::as_array)
                .filter(|paths| !paths.is_empty())
                .ok_or_else(|| {
                    format!(
                        "{name} requires a non-empty `paths` array. Example: {}",
                        example(name)
                    )
                })?;
            let files = paths
                .iter()
                .map(|path| {
                    let path = path.as_str().ok_or("upload paths must be strings")?;
                    let path = std::path::Path::new(path);
                    let absolute = if path.is_absolute() {
                        path.to_path_buf()
                    } else {
                        std::env::current_dir()
                            .map_err(|_| "working directory is unavailable")?
                            .join(path)
                    };
                    Ok(json!(absolute.to_string_lossy()))
                })
                .collect::<Result<Vec<_>, &str>>()?;
            (
                "upload",
                json!({"target": need(a, "target", name)?, "files": files}),
            )
        }
        "browser_download" => {
            let directory = string(a, "directory").unwrap_or(".");
            let directory = std::path::Path::new(directory);
            let absolute = if directory.is_absolute() {
                directory.to_path_buf()
            } else {
                std::env::current_dir()
                    .map_err(|_| "working directory is unavailable".to_string())?
                    .join(directory)
            };
            (
                "download",
                json!({"destination": absolute.to_string_lossy(), "timeoutMs": a.get("timeoutMs").and_then(Value::as_u64).unwrap_or(30000)}),
            )
        }
        "browser_clipboard" => match need(a, "action", name)? {
            "read" => ("clipboardRead", json!({})),
            "write" => ("clipboardWrite", json!({"text": need(a, "text", name)?})),
            other => {
                return Err(format!(
                    "{name} action must be read or write, not `{other}`."
                ));
            }
        },
        _ => unreachable!("reject_unknown accepts only catalog tools"),
    })
}

/// Accept the key spellings models commonly use.
pub(crate) fn normalize_key(key: &str) -> String {
    key.split('+')
        .map(|part| match part.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => "Control".to_string(),
            "cmd" | "command" | "meta" | "super" | "win" => "Meta".to_string(),
            "alt" | "option" => "Alt".to_string(),
            "shift" => "Shift".to_string(),
            "esc" | "escape" => "Escape".to_string(),
            "enter" | "return" => "Enter".to_string(),
            "tab" => "Tab".to_string(),
            "space" => " ".to_string(),
            "up" | "arrowup" => "ArrowUp".to_string(),
            "down" | "arrowdown" => "ArrowDown".to_string(),
            "left" | "arrowleft" => "ArrowLeft".to_string(),
            "right" | "arrowright" => "ArrowRight".to_string(),
            "pagedown" => "PageDown".to_string(),
            "pageup" => "PageUp".to_string(),
            "backspace" => "Backspace".to_string(),
            "delete" | "del" => "Delete".to_string(),
            "home" => "Home".to_string(),
            "end" => "End".to_string(),
            _ => part.trim().to_string(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Add the next step a model should take after an agent tool failed.
pub(crate) fn recovery_hint(tool: &str, error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase().replace(['_', '-'], " ");
    if lower.contains("dialog") {
        "Next: a JavaScript dialog is blocking the page; call browser_handle_dialog with {\"accept\":true} or {\"accept\":false}."
    } else if lower.contains("stale") || lower.contains("revision") || lower.contains("reference") {
        "Next: the page changed since your last snapshot; call browser_snapshot and use a fresh ref."
    } else if lower.contains("ambiguous") || lower.contains("multiple") {
        "Next: several elements match; call browser_snapshot and pass the exact ref, or use \"role=<role>;name=<name>\"."
    } else if lower.contains("not found")
        || lower.contains("no element")
        || lower.contains("notfound")
        || lower.contains("no match")
    {
        "Next: call browser_snapshot to see the current elements and refs; if the element is further down, browser_scroll first; if it is in another tab, use browser_tabs."
    } else if lower.contains("timed out") || lower.contains("timeout") || lower.contains("deadline")
    {
        if tool == "browser_wait_for" {
            "Next: the condition never became true; call browser_snapshot or browser_screenshot to see the page, the content may need a click or scroll first."
        } else {
            "Next: call browser_snapshot to check whether the page is still loading, then retry or use browser_wait_for."
        }
    } else if lower.contains("policy") || lower.contains("not allowed") || lower.contains("denied")
    {
        "Next: this capability is disabled by the browser policy; tell the user which permission is needed instead of retrying."
    } else if lower.contains("no active")
        || lower.contains("no page")
        || lower.contains("not started")
    {
        "Next: call browser_navigate with a URL first, or browser_tabs {\"action\":\"list\"} and select a tab."
    } else {
        "Next: call browser_snapshot to re-read the page, then retry with a target from it."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_tools_have_when_to_use_text_examples_and_closed_schemas() {
        let tools = agent_tools();
        assert!(tools.len() <= 20, "agent profile must stay small");
        for tool in &tools {
            let name = tool["name"].as_str().unwrap();
            assert!(name.starts_with("browser_"), "{name}");
            let description = tool["description"].as_str().unwrap();
            assert!(
                description.contains("Example: {"),
                "{name} lacks an example"
            );
            assert!(description.len() <= 600, "{name} description is too long");
            assert_eq!(tool["inputSchema"]["additionalProperties"], false, "{name}");
            let example: Value = serde_json::from_str(&example(name)).unwrap_or_else(|error| {
                panic!("{name} example is not JSON: {error}");
            });
            translate(name, &example)
                .unwrap_or_else(|error| panic!("{name} example fails: {error}"));
        }
    }

    #[test]
    fn translate_maps_agent_calls_onto_full_catalog_tools() {
        assert_eq!(
            translate("browser_press_key", &json!({"key":"ctrl+a"})).unwrap(),
            ("shortcut", json!({"shortcut":"Control+a"}))
        );
        assert_eq!(
            translate("browser_press_key", &json!({"key":"esc"})).unwrap(),
            ("key", json!({"key":"Escape"}))
        );
        assert_eq!(
            translate("browser_wait_for", &json!({"text":"Net revenue"})).unwrap(),
            (
                "wait",
                json!({"condition":"text=Net revenue","timeoutMs":15000})
            )
        );
        assert_eq!(
            translate("browser_scroll", &json!({"direction":"up","amount":300})).unwrap(),
            ("scroll", json!({"dx":0.0,"dy":-300.0}))
        );
        assert_eq!(
            translate("browser_tabs", &json!({"action":"select","id":"T1"})).unwrap(),
            ("selectTarget", json!({"id":"T1"}))
        );
        assert_eq!(
            translate("browser_handle_dialog", &json!({"accept":false}))
                .unwrap()
                .0,
            "dismissDialog"
        );
        assert_eq!(
            translate(
                "browser_check",
                &json!({"target":"name=Terms","checked":false})
            )
            .unwrap(),
            ("uncheck", json!({"target":"name=Terms"}))
        );
    }

    #[test]
    fn translate_errors_tell_the_model_what_to_send() {
        let error = translate("browser_wait_for", &json!({"text":"a","url":"b"})).unwrap_err();
        assert!(error.contains("exactly one"), "{error}");
        assert!(error.contains("Example:"), "{error}");
        let error = translate("browser_click", &json!({"selector":"#x"})).unwrap_err();
        assert!(error.contains("does not accept `selector`"), "{error}");
        assert!(error.contains("target"), "{error}");
        let error = translate("browser_open", &json!({})).unwrap_err();
        assert!(error.contains("browser_navigate"), "{error}");
    }

    #[test]
    fn recovery_hints_name_the_next_tool() {
        assert!(
            recovery_hint(
                "browser_click",
                r#"{"kind":"not_found","failureKind":"target_not_found"}"#
            )
            .contains("browser_snapshot")
        );
        assert!(
            recovery_hint("browser_click", "a JavaScript dialog is open")
                .contains("browser_handle_dialog")
        );
        assert!(recovery_hint("browser_wait_for", "wait timed out").contains("browser_screenshot"));
    }
}
