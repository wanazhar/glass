//! Human-readable projections for Development TUI surfaces.
//!
//! Raw tool results remain available behind explicit inspect toggles, but the
//! primary presentation for every core surface is a designed projection, never
//! pretty-printed JSON.

use serde_json::Value;

/// Render workspace-trust configuration as an authority/risk table.
pub fn trust_items(items: &[crate::customization::CustomizationInspectionItem]) -> String {
    if items.is_empty() {
        return "No executable Glass settings found in this repository.".into();
    }
    let mut lines = Vec::new();
    for item in items {
        let risk = match item.risk {
            crate::customization::CustomizationRisk::Executable => "! executable",
            crate::customization::CustomizationRisk::AgentContext => "? agent context",
            crate::customization::CustomizationRisk::Static => "✓ static",
        };
        lines.push(format!(
            "{} {} · {} · authority {} · source {}{}",
            risk,
            item.name,
            item.kind,
            match item.authority {
                crate::customization::CustomizationAuthority::GlassBuiltIn => "built-in",
                crate::customization::CustomizationAuthority::UserGlobal => "user-global",
                crate::customization::CustomizationAuthority::TrustedProject => "trusted-project",
                crate::customization::CustomizationAuthority::UntrustedProject =>
                    "untrusted-project",
                crate::customization::CustomizationAuthority::ExternalClient => "external-client",
            },
            item.source.display(),
            item.command
                .as_ref()
                .map(|command| format!("\n    runs `{command}`"))
                .unwrap_or_default(),
        ));
    }
    lines.join("\n")
}

/// Project the resident browser workflow state.
pub fn workflow(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return "idle · no workflow run in this workspace".into();
    };
    let state = value
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let active = value.get("active").and_then(Value::as_str);
    let last = value.get("last").and_then(Value::as_str);
    let steps = value.get("steps").and_then(Value::as_u64);
    match (active, last) {
        (Some(active), _) => format!(
            "→ {active} · {state}{}",
            steps
                .map(|steps| format!(" · {steps} recorded steps"))
                .unwrap_or_default()
        ),
        (None, Some(last)) => format!("✓ last {last} · {state}"),
        (None, None) => format!("{state} · no workflow selected"),
    }
}

/// Project `glass.git.*` results without replacing the status projection.
pub fn git(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if let Some(staged) = value.get("staged").and_then(Value::as_array) {
        return format!("✓ staged {} path(s)", staged.len());
    }
    if let Some(unstaged) = value.get("unstaged").and_then(Value::as_array) {
        return format!("✓ unstaged {} path(s)", unstaged.len());
    }
    if let Some(branch) = value.get("branch").and_then(Value::as_str) {
        let ahead = value.get("ahead").and_then(Value::as_u64).unwrap_or(0);
        let behind = value.get("behind").and_then(Value::as_u64).unwrap_or(0);
        return format!("{branch} · ↑{ahead} ↓{behind}");
    }
    first_meaningful(value)
}

/// Project `glass.test.*` results as pass/fail/case counts.
pub fn tests(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    let mut lines = Vec::new();
    if let Some(finished) = value.get("finished").and_then(Value::as_array) {
        for run in finished.iter().rev().take(8) {
            lines.push(format!(
                "{} {} · {} ms · {} case(s)",
                match run.get("exitCode").and_then(Value::as_i64) {
                    Some(0) => "✓",
                    Some(_) => "×",
                    None => "○",
                },
                run.get("suiteId")
                    .and_then(Value::as_str)
                    .unwrap_or("suite"),
                run.get("durationMs").and_then(Value::as_u64).unwrap_or(0),
                run.get("cases")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len),
            ));
        }
    }
    if let Some(running) = value.get("running").and_then(Value::as_array) {
        for run in running {
            lines.push(format!(
                "→ {} running",
                run.get("suiteId")
                    .and_then(Value::as_str)
                    .unwrap_or("suite")
            ));
        }
    }
    if lines.is_empty() {
        lines.push("no finished runs in this result".into());
    }
    lines.join("\n")
}

/// Project debugger snapshots as session lines.
pub fn debugger(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    let state = value
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let breakpoints = value
        .get("breakpoints")
        .and_then(Value::as_object)
        .map(|files| {
            files
                .values()
                .filter_map(|lines| lines.as_array())
                .map(Vec::len)
                .sum::<usize>()
        })
        .unwrap_or(0);
    let watches = value
        .get("watches")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    format!(
        "● {} · pid {} · {} breakpoints · {} watches",
        state,
        value
            .get("adapterProcessId")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        breakpoints,
        watches
    )
}

/// Project kernel results.
pub fn kernels(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if let Some(name) = value.get("name").and_then(Value::as_str) {
        return format!("● kernel {name} ready");
    }
    first_meaningful(value)
}

/// Project LSP responses; diagnostics get a dedicated count projection.
pub fn lsp(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if let Some(diagnostics) = value.get("diagnostics").and_then(Value::as_array) {
        if diagnostics.is_empty() {
            return "✓ no diagnostics".into();
        }
        let mut lines = vec![format!("! {} diagnostic(s)", diagnostics.len())];
        for diagnostic in diagnostics.iter().take(8) {
            let line = diagnostic
                .get("start")
                .and_then(|start| start.get("line"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            lines.push(format!(
                "  {}:{} · {}",
                diagnostic
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or("?"),
                line.saturating_add(1),
                diagnostic
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("message unavailable")
            ));
        }
        return lines.join("\n");
    }
    first_meaningful(value)
}

/// Project the bounded browser evidence emitted after an agent tool call.
pub fn browser_evidence(value: &Value) -> String {
    let ok = value.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let tool = value
        .get("toolName")
        .and_then(Value::as_str)
        .unwrap_or("browser tool");
    let mut lines = vec![format!(
        "{} {} · browser evidence",
        if ok { "✓" } else { "×" },
        tool
    )];
    if let Some(title) = value.get("title").and_then(Value::as_str) {
        lines.push(format!("  page {title}"));
    }
    if let Some(url) = value.get("url").and_then(Value::as_str) {
        let url = url.split(['?', '#']).next().unwrap_or(url);
        lines.push(format!("  url {url}"));
    }
    if let Some(revision) = value
        .get("browserRevision")
        .or_else(|| value.get("currentRevision"))
        .and_then(Value::as_u64)
    {
        lines.push(format!("  browser revision {revision}"));
    }
    if let Some(target) = value.get("targetId").and_then(Value::as_str) {
        lines.push(format!("  target {target}"));
    }
    if let Some(state) = value.get("workflowState").and_then(Value::as_str) {
        lines.push(format!("  workflow {state}"));
    }
    if let Some(count) = value.get("semanticEntityCount").and_then(Value::as_u64) {
        lines.push(format!("  semantic entities {count}"));
    }
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        lines.push(format!("  error {}", trimmed_lines(error, 2)));
    }
    lines.join("\n")
}

/// Project browser tool results. Screenshots never inline base64 payloads.
pub fn browser_result(tool: &str, value: &Value) -> String {
    if tool.contains("screenshot") {
        let bytes = value.get("bytes").and_then(Value::as_u64).unwrap_or(0);
        return format!(
            "✓ screenshot captured · {} KB · raw payload available in Inspect",
            bytes / 1024
        );
    }
    if tool == "glass.browser.observe" {
        let title = value
            .pointer("/page/title")
            .and_then(Value::as_str)
            .unwrap_or("Untitled");
        let interactive = value
            .pointer("/accessibility/interactive")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let revision = value
            .pointer("/accessibility/revision")
            .and_then(Value::as_u64);
        return format!(
            "✓ observed {title} · {interactive} interactive · rev {}",
            revision.map_or_else(|| "—".into(), |revision| revision.to_string())
        );
    }
    if tool == "glass.browser.targets" {
        let targets = value.get("targets").and_then(Value::as_array);
        let mut lines = targets
            .map(|targets| {
                targets
                    .iter()
                    .map(|target| {
                        format!(
                            "{} {} · {}",
                            if target.get("active").and_then(Value::as_bool) == Some(true) {
                                "◆"
                            } else {
                                "○"
                            },
                            target.get("title").and_then(Value::as_str).unwrap_or("?"),
                            target.get("url").and_then(Value::as_str).unwrap_or(""),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if lines.is_empty() {
            lines.push("no page targets".into());
        }
        return lines.join("\n");
    }
    first_meaningful(value)
}

/// First meaningful scalar lines of a result for generic projections.
pub fn first_meaningful(value: &Value) -> String {
    match value {
        Value::Null => "no result".into(),
        Value::String(text) => trimmed_lines(text, 8),
        Value::Array(items) => items
            .iter()
            .take(8)
            .map(first_meaningful)
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(map) => {
            let mut lines = Vec::new();
            for (key, inner) in map.iter().take(8) {
                match inner {
                    Value::String(text) => lines.push(format!("{key}: {text}")),
                    Value::Number(number) => lines.push(format!("{key}: {number}")),
                    Value::Bool(flag) => lines.push(format!("{key}: {flag}")),
                    Value::Null => lines.push(format!("{key}: —")),
                    Value::Array(items) => lines.push(format!("{key}: {} item(s)", items.len())),
                    Value::Object(inner) => {
                        if let Some(text) = inner.get("text").and_then(Value::as_str) {
                            lines.push(format!("{key}: {}", trimmed_lines(text, 1)));
                        } else {
                            lines.push(format!("{key}: {} field(s)", inner.len()));
                        }
                    }
                }
            }
            lines.join("\n")
        }
        other => other.to_string(),
    }
}

pub fn trimmed_lines(text: &str, limit: usize) -> String {
    text.lines()
        .take(limit)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

pub fn conversation(events: &[crate::AgentEvent]) -> String {
    let mut items = Vec::new();
    for event in events {
        match event.kind.as_str() {
            "starting" | "ready" | "turn_start" | "turn_end" | "message_start"
            | "agent_settled" => {}
            "agent_end" => finish_streaming(&mut items),
            "requestStarted" | "agent_start" => {
                push_thinking(&mut items);
            }
            "message_update" => {
                if let Some(delta) = message_delta(&event.payload) {
                    push_assistant_delta(&mut items, delta);
                }
            }
            "message_end" => {
                if let Some(text) = event_text(event) {
                    push_assistant_final(&mut items, text);
                } else {
                    finish_streaming(&mut items);
                }
            }
            "glass_browser_evidence" => {
                items.push(ConversationItem::System(browser_evidence(&event.payload)));
            }
            "glass_tool_evidence" => {
                let name = tool_name(&event.payload);
                if event
                    .payload
                    .get("ok")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    items.push(ConversationItem::System(format!("✓ {name} · done")));
                } else {
                    items.push(ConversationItem::Error(format!(
                        "{name} · {}",
                        event
                            .payload
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("tool failed")
                    )));
                }
            }
            "glass_tool_rejected" => {
                let recoverable = event
                    .payload
                    .get("recoverable")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                items.push(ConversationItem::Error(format!(
                    "{} · {}{}",
                    tool_name(&event.payload),
                    event
                        .payload
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("tool rejected"),
                    if recoverable {
                        " · retrying with registered Glass tools"
                    } else {
                        " · turn stopped"
                    }
                )));
            }
            "glass_tool_approval_request" => {
                items.push(ConversationItem::Alert(format!(
                    "⚠ {} · approval required · press Y/Enter to allow or N/Esc to deny\n  {}",
                    tool_name(&event.payload),
                    event
                        .payload
                        .get("arguments")
                        .map(Value::to_string)
                        .unwrap_or_else(|| "{}".into())
                )));
            }
            "glass_tool_approval_resolved" => {
                let approved = event
                    .payload
                    .get("approved")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                items.push(ConversationItem::System(format!(
                    "{} {} · approval {}",
                    if approved { "✓" } else { "×" },
                    event
                        .payload
                        .get("toolName")
                        .and_then(Value::as_str)
                        .unwrap_or("tool"),
                    if approved { "granted" } else { "denied" }
                )));
            }
            "tool_execution_start" => {
                items.push(ConversationItem::System(format!(
                    "→ {} · running",
                    tool_name(&event.payload)
                )));
            }
            "tool_execution_end" => {
                items.push(ConversationItem::System(format!(
                    "✓ {} · done",
                    tool_name(&event.payload)
                )));
            }
            "completed" => {
                if let Some(text) = event_text(event) {
                    push_assistant_final(&mut items, text);
                } else {
                    items.push(ConversationItem::System("✓ done".into()));
                }
            }
            "failed" | "workerPanicked" | "budgetExceeded" => items.push(ConversationItem::Error(
                event_text(event)
                    .or_else(|| {
                        event
                            .payload
                            .get("error")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| "failed".into()),
            )),
            "cancelled" => items.push(ConversationItem::Error("cancelled".into())),
            "user" => {
                if let Some(text) = event_text(event) {
                    items.push(ConversationItem::User(text));
                }
            }
            "response" => {
                if let Some(text) = event_text(event) {
                    push_assistant_final(&mut items, text);
                }
            }
            _ => {
                if let Some(text) = event_text(event) {
                    push_assistant_final(&mut items, text);
                } else if !event.payload.is_null() {
                    items.push(ConversationItem::System(format!("· {}", event.kind)));
                }
            }
        }
    }
    items
        .into_iter()
        .map(render_conversation_item)
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[derive(Debug)]
enum ConversationItem {
    User(String),
    Assistant { text: String, streaming: bool },
    System(String),
    Alert(String),
    Error(String),
}

fn render_conversation_item(item: ConversationItem) -> String {
    match item {
        ConversationItem::User(text) => format!("YOU\n{}", trimmed_lines(&text, 24)),
        ConversationItem::Assistant { text, streaming } => {
            if text.is_empty() {
                "GLASS AGENT\nThinking…".into()
            } else if streaming {
                format!("GLASS AGENT\n{}\n… streaming", trimmed_lines(&text, 24))
            } else {
                format!("GLASS AGENT\n{}", trimmed_lines(&text, 24))
            }
        }
        ConversationItem::System(text) => format!("SYSTEM\n{}", trimmed_lines(&text, 24)),
        ConversationItem::Alert(text) => format!("ALERT\n{}", trimmed_lines(&text, 24)),
        ConversationItem::Error(text) => format!("ERROR\n× {}", trimmed_lines(&text, 24)),
    }
}

fn push_thinking(items: &mut Vec<ConversationItem>) {
    if !matches!(
        items.last(),
        Some(ConversationItem::Assistant {
            streaming: true,
            ..
        })
    ) {
        items.push(ConversationItem::Assistant {
            text: String::new(),
            streaming: true,
        });
    }
}

fn push_assistant_delta(items: &mut Vec<ConversationItem>, delta: String) {
    if delta.is_empty() {
        return;
    }
    if let Some(ConversationItem::Assistant { text, streaming }) = items.last_mut()
        && *streaming
    {
        text.push_str(&delta);
        return;
    }
    items.push(ConversationItem::Assistant {
        text: delta,
        streaming: true,
    });
}

fn push_assistant_final(items: &mut Vec<ConversationItem>, text: String) {
    if text.is_empty() {
        return;
    }
    if let Some(ConversationItem::Assistant {
        text: current,
        streaming,
    }) = items.last_mut()
        && (*streaming || current.is_empty() || current == &text)
    {
        current.clone_from(&text);
        *streaming = false;
        return;
    }
    items.push(ConversationItem::Assistant {
        text,
        streaming: false,
    });
}

fn finish_streaming(items: &mut [ConversationItem]) {
    if let Some(ConversationItem::Assistant { streaming, .. }) = items.last_mut() {
        *streaming = false;
    }
}

fn event_text(event: &crate::AgentEvent) -> Option<String> {
    let pointers = match event.kind.as_str() {
        "message_update" => &[
            "/assistantMessageEvent/delta",
            "/assistantMessageEvent/text",
            "/delta",
            "/text",
        ][..],
        _ => &[
            "/message/content",
            "/message/text",
            "/result/text",
            "/result/message/content",
            "/text",
            "/content",
        ][..],
    };
    pointers
        .iter()
        .filter_map(|pointer| event.payload.pointer(pointer))
        .find_map(text_value)
        .filter(|text| !text.is_empty())
}

fn message_delta(payload: &serde_json::Value) -> Option<String> {
    if payload
        .pointer("/assistantMessageEvent/type")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|kind| kind != "text_delta")
    {
        return None;
    }
    ["/assistantMessageEvent/delta", "/delta", "/text"]
        .iter()
        .filter_map(|pointer| payload.pointer(pointer))
        .find_map(text_value)
        .filter(|text| !text.is_empty())
}

fn text_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Array(values) => {
            let text = values.iter().filter_map(text_value).collect::<String>();
            (!text.is_empty()).then_some(text)
        }
        serde_json::Value::Object(map) => ["text", "delta", "content", "message", "result"]
            .iter()
            .find_map(|key| map.get(*key).and_then(text_value)),
        _ => None,
    }
}

fn tool_name(payload: &serde_json::Value) -> String {
    [
        "/tool",
        "/toolName",
        "/tool_name",
        "/toolCall/name",
        "/toolCall/toolName",
        "/name",
    ]
    .iter()
    .filter_map(|pointer| payload.pointer(pointer))
    .find_map(serde_json::Value::as_str)
    .unwrap_or("tool")
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_items_project_authority_and_risk() {
        let items = vec![crate::customization::CustomizationInspectionItem {
            kind: "tool".into(),
            name: "probe".into(),
            source: "glass.toml".into(),
            authority: crate::customization::CustomizationAuthority::TrustedProject,
            risk: crate::customization::CustomizationRisk::Executable,
            command: Some("echo unsafe".into()),
            declared_mutating: None,
            trust_required: true,
            governance: None,
        }];
        let projected = trust_items(&items);
        assert!(projected.contains("! executable"));
        assert!(projected.contains("authority trusted-project"));
        assert!(projected.contains("runs `echo unsafe`"));
    }

    #[test]
    fn browser_evidence_card_is_bounded_and_revision_aware() {
        let value = serde_json::json!({
            "type": "glass_browser_evidence",
            "toolName": "glass.browser.navigate",
            "ok": true,
            "title": "Checkout",
            "url": "https://example.test/checkout?token=secret#payment",
            "browserRevision": 42,
            "targetId": "page-1",
            "semanticEntityCount": 3,
        });
        let projected = browser_evidence(&value);
        assert!(projected.contains("Checkout"));
        assert!(projected.contains("browser revision 42"));
        assert!(projected.contains("https://example.test/checkout"));
        assert!(!projected.contains("token=secret"));
    }

    #[test]
    fn screenshot_never_inlines_base64() {
        let value = serde_json::json!({
            "mimeType": "image/png",
            "bytes": 204_800,
            "base64": "aGVsbG8="
        });
        let projected = browser_result("glass.browser.screenshot", &value);
        assert!(projected.contains("screenshot captured"));
        assert!(projected.contains("200 KB"));
        assert!(!projected.contains("aGVsbG8"));
    }

    #[test]
    fn workflow_projection_prefers_active_runs() {
        let active = serde_json::json!({"state": "running", "active": "checkout", "steps": 3});
        assert_eq!(
            workflow(Some(&active)),
            "→ checkout · running · 3 recorded steps"
        );
        let idle = serde_json::json!({"state": "idle", "active": null, "last": null});
        assert_eq!(workflow(Some(&idle)), "idle · no workflow selected");
    }

    #[test]
    fn test_results_project_pass_and_running_lines() {
        let value = serde_json::json!({
            "finished": [
                {"suiteId": "unit", "exitCode": 0, "durationMs": 12, "cases": [{"id": "a"}]},
                {"suiteId": "e2e", "exitCode": 1, "durationMs": 40, "cases": []}
            ],
            "running": [{"suiteId": "watch"}]
        });
        let projected = tests(Some(&value));
        assert!(projected.contains("✓ unit · 12 ms · 1 case(s)"));
        assert!(projected.contains("× e2e · 40 ms · 0 case(s)"));
        assert!(projected.contains("→ watch running"));
    }
    #[test]
    fn conversation_renders_user_messages_and_coalesces_streaming_text() {
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let event = |sequence: u64, kind: &str, payload: serde_json::Value| crate::AgentEvent {
            sequence,
            agent_id: agent_id.clone(),
            timestamp_ms: 0,
            kind: kind.into(),
            payload,
        };
        let events = vec![
            event(1, "user", serde_json::json!({"text": "fix the login bug"})),
            event(2, "requestStarted", serde_json::json!("request-1")),
            event(
                3,
                "message_update",
                serde_json::json!({
                    "assistantMessageEvent": {
                        "type": "text_delta",
                        "delta": "I will inspect "
                    }
                }),
            ),
            event(
                4,
                "message_update",
                serde_json::json!({
                    "assistantMessageEvent": {
                        "type": "text_delta",
                        "delta": "the failing path."
                    }
                }),
            ),
            event(
                5,
                "message_end",
                serde_json::json!({
                    "message": {
                        "role": "assistant",
                        "content": [{"type": "text", "text": "I will inspect the failing path."}]
                    }
                }),
            ),
            event(6, "agent_end", serde_json::json!({})),
        ];
        let projected = conversation(&events);
        assert!(projected.contains("YOU\nfix the login bug"));
        assert!(projected.contains("GLASS AGENT\nI will inspect the failing path."));
        assert!(!projected.contains("request-1"));
        assert!(!projected.contains("agent_end"));
        assert!(!projected.contains("… streaming"));
    }

    #[test]
    fn conversation_renders_agent_tool_approval_as_a_decision_card() {
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let event = crate::AgentEvent {
            sequence: 1,
            agent_id,
            timestamp_ms: 0,
            kind: "glass_tool_approval_request".into(),
            payload: serde_json::json!({
                "type": "glass_tool_approval_request",
                "toolName": "glass.browser.act",
                "arguments": {"action": "click", "target": "e12"}
            }),
        };
        let projected = conversation(&[event]);
        assert!(projected.contains("approval required"));
        assert!(projected.contains("glass.browser.act"));
        assert!(projected.contains("ALERT"));
        assert!(projected.contains("Y/Enter"));
    }

    #[test]
    fn conversation_renders_rejected_tool_with_recovery_reason() {
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let event = crate::AgentEvent {
            sequence: 1,
            agent_id,
            timestamp_ms: 0,
            kind: "glass_tool_rejected".into(),
            payload: serde_json::json!({
                "toolName": "glass.fs.list",
                "reason": "turn aborted instead of retrying",
            }),
        };
        let projected = conversation(&[event]);
        assert!(projected.contains("glass.fs.list"));
        assert!(projected.contains("ERROR"));
        assert!(projected.contains("turn aborted instead of retrying"));
    }

    #[test]
    fn conversation_renders_browser_evidence_as_an_activity_card() {
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let event = crate::AgentEvent {
            sequence: 1,
            agent_id,
            timestamp_ms: 0,
            kind: "glass_browser_evidence".into(),
            payload: serde_json::json!({
                "type": "glass_browser_evidence",
                "toolName": "glass.browser.observe",
                "ok": true,
                "title": "Dashboard",
                "browserRevision": 9,
            }),
        };
        let projected = conversation(&[event]);
        assert!(projected.contains("glass.browser.observe"));
        assert!(projected.contains("SYSTEM"));
        assert!(projected.contains("browser revision 9"));
    }
}
