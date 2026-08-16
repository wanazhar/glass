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
    events
        .iter()
        .filter(|event| !matches!(event.kind.as_str(), "starting" | "ready" | "requestStarted"))
        .map(agent_event)
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Render a single agent event; tool calls collapse to activity rows.
fn agent_event(event: &crate::AgentEvent) -> String {
    let text = event
        .payload
        .pointer("/message/content")
        .or_else(|| event.payload.pointer("/result/text"))
        .or_else(|| event.payload.get("text"))
        .and_then(serde_json::Value::as_str);
    if let Some(tool) = event
        .payload
        .get("tool")
        .and_then(serde_json::Value::as_str)
    {
        return format!("→ {tool} · {}", event.kind);
    }
    match event.kind.as_str() {
        "completed" => text.map_or_else(
            || "✓ done".into(),
            |text| format!("GLASS AGENT\n{}", trimmed_lines(text, 24)),
        ),
        "failed" | "workerPanicked" | "budgetExceeded" => format!(
            "× {}",
            text.or_else(|| event
                .payload
                .get("error")
                .and_then(serde_json::Value::as_str))
                .unwrap_or("failed")
        ),
        "cancelled" => "× cancelled".into(),
        "user" => format!("YOU\n{}", text.unwrap_or_default()),
        _ => match text {
            Some(text) => format!("GLASS AGENT\n{}", trimmed_lines(text, 24)),
            None if event.payload.is_null() => String::new(),
            None => "· activity recorded".into(),
        },
    }
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
}
