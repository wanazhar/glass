//! Human-readable projections for Development TUI surfaces.
//!
//! Raw tool results remain available behind explicit inspect toggles, but the
//! primary presentation for every core surface is a designed projection, never
//! pretty-printed JSON.

use serde_json::Value;
use std::collections::BTreeSet;

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
    if let Some(backend) = value.get("backend").and_then(Value::as_str) {
        lines.push(format!("  backend {backend}"));
    }
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
    if tool == "glass.browser.screenshot" {
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
            .get("browserRevision")
            .and_then(Value::as_u64)
            .or_else(|| {
                value
                    .pointer("/accessibility/revision")
                    .and_then(Value::as_u64)
            });
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

/// Extract a useful final line from a bounded external-agent event stream.
pub fn external_agent_output(value: &Value) -> String {
    let output = value.get("output").and_then(Value::as_str).unwrap_or("");
    let detail = external_stream_text(output)
        .or_else(|| {
            value
                .get("stderr")
                .and_then(Value::as_str)
                .filter(|stderr| !stderr.trim().is_empty())
                .map(|stderr| trimmed_lines(stderr, 1))
        })
        .or_else(|| (!output.trim().is_empty()).then(|| trimmed_lines(output, 1)))
        .unwrap_or_else(|| "no output".into());
    detail.chars().take(240).collect()
}

fn external_stream_text(output: &str) -> Option<String> {
    let mut messages: Vec<String> = Vec::new();
    for line in output.lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
        let text = match kind {
            "item.completed" => event
                .get("item")
                .and_then(|item| item.get("text").and_then(Value::as_str)),
            "assistant" => event
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(stream_content_text),
            "result" => event.get("result").and_then(Value::as_str),
            "text" | "message" => event.get("text").and_then(Value::as_str).or_else(|| {
                event
                    .get("part")
                    .and_then(|part| part.get("text").and_then(Value::as_str))
            }),
            "content_block_delta" => event
                .get("delta")
                .and_then(|delta| delta.get("text").and_then(Value::as_str)),
            "error" | "api_error" => event
                .get("error")
                .and_then(Value::as_str)
                .or_else(|| event.get("message").and_then(Value::as_str)),
            "system" if event.get("subtype").and_then(Value::as_str) == Some("api_retry") => {
                event.get("error").and_then(Value::as_str)
            }
            _ => None,
        };
        if let Some(text) = text.filter(|text| !text.trim().is_empty())
            && !messages.iter().any(|message| message == text)
        {
            messages.push(text.to_string());
        }
    }
    (!messages.is_empty()).then(|| messages.join("\n"))
}

fn stream_content_text(content: &Value) -> Option<&str> {
    content
        .as_array()?
        .iter()
        .find_map(|item| item.get("text").and_then(Value::as_str))
}

/// Visual role of one Agent-surface conversation bubble.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationKind {
    User,
    Assistant,
    System,
    Alert,
    Error,
}

/// One rendered transcript bubble, optionally pinned to a Core session entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationEntry {
    pub kind: ConversationKind,
    pub text: String,
    pub streaming: bool,
    pub entry_id: Option<String>,
    pub client_message_id: Option<String>,
    pub tool_name: Option<String>,
    /// Shared Core tool details, including the stable runtime call ID.
    pub tool_card: Option<glass_tui::ToolCard>,
}

impl ConversationEntry {
    pub(super) fn user(
        text: impl Into<String>,
        entry_id: Option<String>,
        client_message_id: Option<String>,
    ) -> Self {
        Self {
            kind: ConversationKind::User,
            text: text.into(),
            streaming: false,
            entry_id,
            client_message_id,
            tool_name: None,
            tool_card: None,
        }
    }

    pub(super) fn assistant(
        text: impl Into<String>,
        streaming: bool,
        entry_id: Option<String>,
    ) -> Self {
        Self {
            kind: ConversationKind::Assistant,
            text: text.into(),
            streaming,
            entry_id,
            client_message_id: None,
            tool_name: None,
            tool_card: None,
        }
    }

    pub(super) fn system(text: impl Into<String>, tool_name: Option<String>) -> Self {
        Self {
            kind: ConversationKind::System,
            text: text.into(),
            streaming: false,
            entry_id: None,
            client_message_id: None,
            tool_name,
            tool_card: None,
        }
    }

    pub(super) fn error(text: impl Into<String>, tool_name: Option<String>) -> Self {
        Self {
            kind: ConversationKind::Error,
            text: text.into(),
            streaming: false,
            entry_id: None,
            client_message_id: None,
            tool_name,
            tool_card: None,
        }
    }

    /// Flatten one bubble to the Agent transcript heading format.
    pub fn render(&self) -> String {
        let body = trimmed_lines(&self.text, 24);
        match self.kind {
            ConversationKind::User => format!("YOU\n{body}"),
            ConversationKind::Assistant if self.text.is_empty() => "GLASS AGENT\nThinking…".into(),
            ConversationKind::Assistant if self.streaming => {
                format!("GLASS AGENT\n{body}\n… streaming")
            }
            ConversationKind::Assistant => format!("GLASS AGENT\n{body}"),
            ConversationKind::System => format!("SYSTEM\n{body}"),
            ConversationKind::Alert => format!("ALERT\n{body}"),
            ConversationKind::Error => format!("ERROR\n× {body}"),
        }
    }

    /// Compact one-line card used while a tool bubble is collapsed.
    pub fn card_line(&self) -> String {
        if let Some(name) = &self.tool_name {
            return match self.kind {
                ConversationKind::Alert => format!("⚠ {name} · approval required"),
                ConversationKind::Error => format!("× {name}"),
                _ => self
                    .text
                    .lines()
                    .next()
                    .unwrap_or(name.as_str())
                    .to_string(),
            };
        }
        self.text.lines().next().unwrap_or_default().to_string()
    }
}

#[derive(Default)]
struct SharedConversationProjection {
    timeline: glass_tui::Conversation,
    local_indices: Vec<usize>,
    latest_local: Option<usize>,
}

impl SharedConversationProjection {
    fn apply(
        &mut self,
        event: glass_tui::ConversationEvent,
        items: &mut Vec<ConversationEntry>,
        entry_id: Option<String>,
    ) -> Option<usize> {
        let shared_index = self.timeline.apply(event)?;
        while self.local_indices.len() <= shared_index {
            let item = &self.timeline.items()[self.local_indices.len()];
            let local_index = items.len();
            items.push(Self::local_entry(item, None));
            self.local_indices.push(local_index);
        }
        let local_index = self.local_indices[shared_index];
        let existing = items.get(local_index).cloned();
        items[local_index] =
            Self::local_entry(&self.timeline.items()[shared_index], existing.as_ref());
        if items[local_index].entry_id.is_none() {
            items[local_index].entry_id = entry_id;
        }
        self.latest_local = Some(local_index);
        Some(local_index)
    }

    fn finish_streaming(&mut self, items: &mut [ConversationEntry]) {
        self.timeline.apply(glass_tui::ConversationEvent::Finish);
        for (shared_index, local_index) in self.local_indices.iter().copied().enumerate() {
            if let Some(local) = items.get_mut(local_index)
                && let Some(shared) = self.timeline.items().get(shared_index)
                && local.kind == ConversationKind::Assistant
            {
                local.text.clone_from(&shared.entry.text);
                local.streaming = shared.streaming;
            }
        }
    }

    fn latest_local_index(&self) -> Option<usize> {
        self.latest_local
    }

    fn has_tool_call(&self, call_id: &str) -> bool {
        self.timeline.items().iter().any(|item| {
            item.entry
                .tool_card
                .as_ref()
                .is_some_and(|card| card.call_id == call_id)
        })
    }

    fn local_entry(
        shared: &glass_tui::ConversationItem,
        previous: Option<&ConversationEntry>,
    ) -> ConversationEntry {
        let entry_id = previous.and_then(|entry| entry.entry_id.clone());
        let client_message_id = previous.and_then(|entry| entry.client_message_id.clone());
        if let Some(card) = &shared.entry.tool_card {
            let name = shared
                .entry
                .text
                .lines()
                .next()
                .unwrap_or(&card.command)
                .to_string();
            let failed = card.status == "error"
                || card.status.starts_with("exit ") && card.status != "exit 0";
            let summary = if card.status == "running" {
                format!("→ {name} · running")
            } else if failed {
                format!("× {name}")
            } else {
                format!("✓ {name} · done")
            };
            let body = if card.output.is_empty() {
                summary
            } else {
                format!("{summary}\n{}", card.output)
            };
            let mut entry = if failed {
                ConversationEntry::error(body, Some(name))
            } else {
                ConversationEntry::system(body, Some(name))
            };
            entry.tool_card = Some(card.clone());
            entry.entry_id = entry_id;
            entry.client_message_id = client_message_id;
            entry
        } else {
            match shared.entry.role {
                glass_tui::Role::User => {
                    let mut entry = ConversationEntry::user(
                        shared.entry.text.clone(),
                        entry_id,
                        client_message_id,
                    );
                    entry.streaming = shared.streaming;
                    entry
                }
                glass_tui::Role::Assistant => ConversationEntry::assistant(
                    shared.entry.text.clone(),
                    shared.streaming,
                    entry_id,
                ),
                glass_tui::Role::System | glass_tui::Role::Meta => {
                    let mut entry = ConversationEntry::system(shared.entry.text.clone(), None);
                    entry.entry_id = entry_id;
                    entry.client_message_id = client_message_id;
                    entry
                }
                glass_tui::Role::Bash | glass_tui::Role::Tool => {
                    let mut entry = ConversationEntry::system(shared.entry.text.clone(), None);
                    entry.entry_id = entry_id;
                    entry.client_message_id = client_message_id;
                    entry
                }
            }
        }
    }
}

/// Project agent events into structured transcript bubbles.
pub fn conversation_entries(events: &[crate::AgentEvent]) -> Vec<ConversationEntry> {
    let mut items = Vec::new();
    let mut shared = SharedConversationProjection::default();
    let mut host_tool_calls = BTreeSet::new();
    let mut turn = 0_usize;
    for event in events {
        let entry_id = event_entry_id(event);
        match event.kind.as_str() {
            "starting" | "ready" | "turn_end" | "message_start" | "agent_settled" => {}
            "turn_start" => {
                turn = turn.saturating_add(1);
                shared.finish_streaming(&mut items);
                shared.apply(
                    glass_tui::ConversationEvent::TurnStart { turn },
                    &mut items,
                    None,
                );
            }
            "agent_end" => shared.finish_streaming(&mut items),
            // The composer already shows the active turn state. Keep the
            // transcript reserved for messages and actionable tool results.
            "requestStarted" | "agent_start" | "waiting" => {}
            "reasoning" => {
                if let Some(text) = event_text(event) {
                    shared.apply(
                        glass_tui::ConversationEvent::ReasoningDelta(text),
                        &mut items,
                        None,
                    );
                }
            }
            "notice" | "status" => {
                if let Some(text) = event_text(event)
                    .or_else(|| event.payload.as_str().map(str::to_string))
                    .or_else(|| {
                        event
                            .payload
                            .get("status")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                {
                    shared.finish_streaming(&mut items);
                    items.push(ConversationEntry::system(text, None));
                }
            }
            "error" => {
                if let Some(text) =
                    event_text(event).or_else(|| event.payload.get("message").and_then(text_value))
                {
                    shared.finish_streaming(&mut items);
                    items.push(ConversationEntry::error(text, None));
                }
            }
            "message_update" => {
                if let Some(delta) = message_delta(&event.payload) {
                    shared.apply(
                        glass_tui::ConversationEvent::AssistantDelta(delta),
                        &mut items,
                        entry_id,
                    );
                }
            }
            "message_end" if hidden_custom_message(&event.payload) => {}
            "message_end" => {
                if let Some(text) = event_text(event) {
                    shared.apply(
                        glass_tui::ConversationEvent::AssistantFinal(text),
                        &mut items,
                        entry_id,
                    );
                } else {
                    shared.finish_streaming(&mut items);
                }
            }
            "glass_browser_evidence" => {
                shared.finish_streaming(&mut items);
                let evidence = browser_evidence(&event.payload);
                let tool = Some("glass.browser".into());
                if event
                    .payload
                    .get("ok")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    items.push(ConversationEntry::system(evidence, tool));
                } else {
                    items.push(ConversationEntry::error(evidence, tool));
                }
            }
            "glass_tool_evidence" => {
                shared.finish_streaming(&mut items);
                let name = tool_name(&event.payload);
                if event
                    .payload
                    .get("ok")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    items.push(ConversationEntry::system(
                        format!("✓ {name} · done"),
                        Some(name),
                    ));
                } else {
                    items.push(ConversationEntry::error(
                        format!(
                            "{name} · {}",
                            event
                                .payload
                                .get("error")
                                .and_then(Value::as_str)
                                .unwrap_or("tool failed")
                        ),
                        Some(name),
                    ));
                }
            }
            "glass_tool_rejected" => {
                shared.finish_streaming(&mut items);
                let recoverable = event
                    .payload
                    .get("recoverable")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let name = tool_name(&event.payload);
                items.push(ConversationEntry::error(
                    format!(
                        "{} · {}{}",
                        name,
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
                    ),
                    Some(name),
                ));
            }
            "tool_execution_start" => {
                let name = tool_name(&event.payload);
                let call_id = event
                    .payload
                    .get("callId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("legacy-{}", event.sequence));
                // Core owns the lifecycle of every call, but the resident
                // runtime also returns an attributed Glass integration
                // evidence event with the authoritative host result. Avoid
                // showing a second, potentially contradictory Core row for
                // the same host call.
                if name.starts_with("glass.") {
                    host_tool_calls.insert(call_id);
                    continue;
                }
                shared.apply(
                    glass_tui::ConversationEvent::ToolStart {
                        call_id,
                        name,
                        command: event
                            .payload
                            .get("command")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    },
                    &mut items,
                    None,
                );
            }
            "tool_progress" => {
                if let Some(call_id) = event
                    .payload
                    .get("callId")
                    .or_else(|| event.payload.pointer("/text/callId"))
                    .and_then(Value::as_str)
                {
                    if host_tool_calls.contains(call_id) {
                        continue;
                    }
                    let text = event_text(event).unwrap_or_default();
                    shared.apply(
                        glass_tui::ConversationEvent::ToolProgress {
                            call_id: call_id.to_string(),
                            text,
                        },
                        &mut items,
                        None,
                    );
                }
            }
            "tool_execution_end" => {
                let call_id = event
                    .payload
                    .get("callId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("legacy-{}", event.sequence));
                if host_tool_calls.remove(&call_id) {
                    continue;
                }
                let output = event_text(event).unwrap_or_default();
                // Core reports completion of its provider-side tool call, while
                // Glass separately reports the governed host execution as
                // glass_tool_evidence. If no matching start was projected,
                // suppress the empty completion row and let host evidence be
                // the visible result.
                if output.is_empty() && !shared.has_tool_call(&call_id) {
                    continue;
                }
                let status = if event.payload.get("isError").and_then(Value::as_bool) == Some(true)
                {
                    "error"
                } else {
                    "completed"
                };
                shared.apply(
                    glass_tui::ConversationEvent::ToolEnd {
                        call_id,
                        output,
                        status: status.into(),
                        elapsed_ms: None,
                    },
                    &mut items,
                    None,
                );
            }
            "completed" => {
                if let Some(text) = event_text(event) {
                    shared.apply(
                        glass_tui::ConversationEvent::AssistantFinal(text),
                        &mut items,
                        entry_id,
                    );
                } else {
                    shared.finish_streaming(&mut items);
                    items.push(ConversationEntry::system("✓ done", None));
                }
            }
            "failed" | "workerPanicked" | "budgetExceeded" => {
                shared.finish_streaming(&mut items);
                items.push(ConversationEntry::error(
                    event_text(event)
                        .or_else(|| {
                            event
                                .payload
                                .get("error")
                                .and_then(serde_json::Value::as_str)
                                .map(str::to_string)
                        })
                        .unwrap_or_else(|| "failed".into()),
                    None,
                ));
            }
            "cancelled" => {
                shared.finish_streaming(&mut items);
                items.push(ConversationEntry::error("cancelled", None));
            }
            "user" => {
                if let Some(text) = event_text(event) {
                    shared.apply(
                        glass_tui::ConversationEvent::User(
                            if event
                                .payload
                                .get("contextAttached")
                                .and_then(Value::as_bool)
                                == Some(true)
                            {
                                strip_legacy_composer_prefix(&text)
                            } else {
                                text
                            },
                        ),
                        &mut items,
                        entry_id.clone(),
                    );
                    if let Some(local) = shared.latest_local_index() {
                        items[local].entry_id = entry_id;
                        items[local].client_message_id = event_client_message_id(event);
                    }
                }
            }
            "response" => {
                if event.payload.get("operation").and_then(Value::as_str) == Some("models") {
                    if event.payload.get("ok").and_then(Value::as_bool) == Some(false) {
                        shared.finish_streaming(&mut items);
                        let error = event
                            .payload
                            .pointer("/error/message")
                            .and_then(Value::as_str)
                            .or_else(|| event.payload.get("error").and_then(Value::as_str))
                            .unwrap_or("model catalog request failed");
                        items.push(ConversationEntry::error(
                            format!("Core model list · {error}"),
                            Some("models/list".into()),
                        ));
                    } else if let Some(text) =
                        event.payload.get("result").and_then(model_catalog_text)
                    {
                        shared.finish_streaming(&mut items);
                        items.push(ConversationEntry::system(text, Some("models/list".into())));
                    }
                } else if matches!(
                    event.payload.get("operation").and_then(Value::as_str),
                    Some("coreCommands" | "coreCommand")
                ) {
                    shared.finish_streaming(&mut items);
                    let operation = event.payload["operation"].as_str().unwrap_or("coreCommand");
                    if event.payload.get("ok").and_then(Value::as_bool) == Some(false) {
                        let error = event
                            .payload
                            .pointer("/error/message")
                            .and_then(Value::as_str)
                            .or_else(|| event.payload.get("error").and_then(Value::as_str))
                            .unwrap_or("Core command request failed");
                        items.push(ConversationEntry::error(
                            format!("Glass Core command · {error}"),
                            Some(operation.into()),
                        ));
                    } else if let Some(result) = event.payload.get("result") {
                        let text = if operation == "coreCommands" {
                            core_command_catalog_text(result)
                        } else {
                            result
                                .get("message")
                                .and_then(Value::as_str)
                                .or_else(|| result.get("text").and_then(Value::as_str))
                                .map(str::to_owned)
                                .unwrap_or_else(|| result.to_string())
                        };
                        let text = if result.get("requiresInteractive").and_then(Value::as_bool)
                            == Some(true)
                        {
                            format!(
                                "{}\nOpen Glass Core with :agent command-ui, then press : to open its native command menu.",
                                text
                            )
                        } else {
                            text
                        };
                        items.push(ConversationEntry::system(text, Some(operation.into())));
                    }
                } else if event.payload.get("ok").and_then(Value::as_bool) == Some(false)
                    && matches!(
                        event.payload.get("operation").and_then(Value::as_str),
                        Some("prompt" | "followUp")
                    )
                {
                    shared.finish_streaming(&mut items);
                    let error = &event.payload["error"];
                    let kind = error
                        .pointer("/data/kind")
                        .and_then(Value::as_str)
                        .filter(|kind| {
                            matches!(
                                *kind,
                                "authentication"
                                    | "provider"
                                    | "protocol"
                                    | "unsupported_method"
                                    | "invalid_input"
                                    | "busy"
                                    | "limit"
                                    | "operation"
                            )
                        })
                        .unwrap_or("request");
                    let code = error
                        .get("code")
                        .and_then(Value::as_i64)
                        .map(|code| format!(" · {code}"))
                        .unwrap_or_default();
                    let retryable = error
                        .pointer("/data/retryable")
                        .and_then(Value::as_bool)
                        .map(|retryable| {
                            if retryable {
                                " · retryable"
                            } else {
                                " · not marked retryable"
                            }
                        })
                        .unwrap_or_default();
                    items.push(ConversationEntry::error(
                        format!(
                            "Core {} failed: {kind}{code}{retryable}",
                            event.payload["operation"].as_str().unwrap_or("turn")
                        ),
                        None,
                    ));
                } else if let Some(text) = event_text(event) {
                    shared.apply(
                        glass_tui::ConversationEvent::AssistantFinal(text),
                        &mut items,
                        entry_id,
                    );
                }
            }
            // Unknown and private event kinds are protocol details, not
            // transcript messages. Only explicitly handled public event kinds
            // above may create visible rows.
            _ => {}
        }
    }
    items
}

fn core_command_catalog_text(result: &Value) -> String {
    let mut lines = vec!["Glass Core built-in commands (catalogue from GC)".to_string()];
    if let Some(commands) = result.get("commands").and_then(Value::as_array) {
        for command in commands {
            let name = command.get("name").and_then(Value::as_str).unwrap_or("?");
            let mode = command
                .get("runtimeMode")
                .and_then(Value::as_str)
                .unwrap_or("interactive");
            let description = command
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("");
            lines.push(format!("/{name} · {description} · {mode}"));
        }
    }
    lines.push("Resident commands run through Glass; for other commands use :agent command-ui, then press : in GC.".into());
    lines.join("\n")
}

/// Accumulated provider reasoning for the explicit reasoning-only view.
/// Reasoning is never folded into normal conversation entries.
pub fn conversation_reasoning(events: &[crate::AgentEvent]) -> String {
    let mut reasoning = String::new();
    let mut segment_open = false;
    for event in events {
        if event.kind == "reasoning" {
            if let Some(text) = event_text(event) {
                if !segment_open && !reasoning.is_empty() {
                    reasoning.push_str("\n\n");
                }
                reasoning.push_str(&text);
                segment_open = true;
            }
        } else {
            segment_open = false;
        }
    }
    reasoning
}

fn model_catalog_text(result: &Value) -> Option<String> {
    let models = result.get("models")?.as_array()?;
    let provider = result.get("provider").and_then(Value::as_str);
    let mut lines = vec![match provider {
        Some(provider) => format!("Models · {provider} · {} available", models.len()),
        None => format!("Models · {} available", models.len()),
    }];
    for model in models.iter().take(100) {
        let id = model
            .get("id")
            .or_else(|| model.get("slug"))
            .and_then(Value::as_str)
            .unwrap_or("unknown model");
        let label = model.get("label").and_then(Value::as_str);
        let model_provider = model.get("provider").and_then(Value::as_str).or(provider);
        let name =
            model_provider.map_or_else(|| id.to_string(), |provider| format!("{provider}/{id}"));
        lines.push(match label.filter(|label| *label != id) {
            Some(label) => format!("  {name} · {label}"),
            None => format!("  {name}"),
        });
    }
    if models.len() > 100
        || result
            .get("nextCursor")
            .is_some_and(|cursor| !cursor.is_null())
    {
        lines.push("  more models available".into());
    }
    Some(lines.join("\n"))
}

pub fn conversation(events: &[crate::AgentEvent]) -> String {
    conversation_entries(events)
        .into_iter()
        .map(|item| item.render())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn event_entry_id(event: &crate::AgentEvent) -> Option<String> {
    [
        "/entryId",
        "/entry/id",
        "/message/id",
        "/id",
        "/assistantMessageEvent/id",
    ]
    .iter()
    .filter_map(|pointer| event.payload.pointer(pointer))
    .find_map(Value::as_str)
    .filter(|value| !value.is_empty())
    .map(str::to_string)
}

fn event_client_message_id(event: &crate::AgentEvent) -> Option<String> {
    event
        .payload
        .get("clientMessageId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .map(str::to_string)
}

/// Older Glass builds put surface playbooks into the user message. Remove only
/// those exact generated prefixes when projecting stored events.
fn strip_legacy_composer_prefix(text: &str) -> String {
    let mut prefixes = Vec::new();
    for surface in super::state::DevSurface::ALL {
        let playbook = super::playbooks::playbook(surface);
        prefixes.push(format!("{playbook}\n\n"));
    }
    prefixes
        .into_iter()
        .find_map(|prefix| text.strip_prefix(&prefix).map(str::to_string))
        .unwrap_or_else(|| text.to_string())
}

fn hidden_custom_message(payload: &serde_json::Value) -> bool {
    let Some(message) = payload.get("message") else {
        return false;
    };
    message.get("role").and_then(serde_json::Value::as_str) == Some("custom")
        && (message.get("display").and_then(serde_json::Value::as_bool) == Some(false)
            || message
                .get("customType")
                .and_then(serde_json::Value::as_str)
                == Some("glass.context"))
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
    fn core_model_response_surfaces_provider_and_model_labels() {
        let event = crate::AgentEvent {
            sequence: 1,
            agent_id: crate::AgentId::parse("agent-0002").unwrap(),
            timestamp_ms: 0,
            kind: "response".into(),
            payload: serde_json::json!({
                "operation": "models",
                "ok": true,
                "result": {
                    "provider": "anthropic",
                    "models": [
                        {"id": "claude-opus-4", "label": "Claude Opus 4"},
                        {"id": "claude-sonnet-4", "label": "claude-sonnet-4"}
                    ],
                    "nextCursor": null
                }
            }),
        };
        let rendered = conversation(&[event]);
        assert!(
            rendered.contains("Models · anthropic · 2 available"),
            "{rendered}"
        );
        assert!(rendered.contains("anthropic/claude-opus-4 · Claude Opus 4"));
        assert!(rendered.contains("anthropic/claude-sonnet-4"));
    }

    #[test]
    fn core_command_catalogue_and_native_handoff_are_rendered_from_core_response() {
        let event = crate::AgentEvent {
            sequence: 1,
            agent_id: crate::AgentId::parse("agent-0002").unwrap(),
            timestamp_ms: 0,
            kind: "response".into(),
            payload: serde_json::json!({"operation":"coreCommands","ok":true,"result":{"source":"glass-core","commands":[{"name":"status","description":"View session status","runtimeMode":"runtime"},{"name":"model","description":"Select model","runtimeMode":"interactive"}]}}),
        };
        let rendered = conversation(&[event]);
        assert!(
            rendered.contains("/status · View session status · runtime"),
            "{rendered}"
        );
        assert!(
            rendered.contains("/model · Select model · interactive"),
            "{rendered}"
        );

        let event = crate::AgentEvent {
            sequence: 2,
            agent_id: crate::AgentId::parse("agent-0002").unwrap(),
            timestamp_ms: 0,
            kind: "response".into(),
            payload: serde_json::json!({"operation":"coreCommand","ok":true,"result":{"name":"model","requiresInteractive":true,"message":"/model is available in Glass Core's native command menu."}}),
        };
        let rendered = conversation(&[event]);
        assert!(rendered.contains(":agent command-ui"), "{rendered}");
    }

    #[test]
    fn failed_core_turn_shows_safe_error_class_without_provider_message() {
        let event = crate::AgentEvent {
            sequence: 1,
            agent_id: crate::AgentId::parse("agent-0002").unwrap(),
            timestamp_ms: 0,
            kind: "response".into(),
            payload: serde_json::json!({
                "operation": "prompt",
                "ok": false,
                "error": {
                    "code": -32000,
                    "message": "raw provider response must stay hidden",
                    "data": {"kind": "provider", "retryable": false}
                }
            }),
        };
        let rendered = conversation(&[event]);
        assert!(rendered.contains("Core prompt failed: provider · -32000 · not marked retryable"));
        assert!(!rendered.contains("raw provider response"));
    }

    #[test]
    fn failed_core_turn_falls_back_for_unrecognized_error_class() {
        let event = crate::AgentEvent {
            sequence: 1,
            agent_id: crate::AgentId::parse("agent-0002").unwrap(),
            timestamp_ms: 0,
            kind: "response".into(),
            payload: serde_json::json!({
                "operation": "prompt",
                "ok": false,
                "error": {
                    "code": -32000,
                    "message": "untrusted detail stays hidden",
                    "data": {"kind": "unrecognized-sensitive-class", "retryable": false}
                }
            }),
        };
        let rendered = conversation(&[event]);
        assert!(rendered.contains("Core prompt failed: request · -32000"));
        assert!(!rendered.contains("unrecognized-sensitive-class"));
        assert!(!rendered.contains("untrusted detail"));
    }

    #[test]
    fn external_agent_output_prefers_final_stream_messages() {
        let value = serde_json::json!({
            "output": concat!(
                "{\"type\":\"thread.started\",\"thread_id\":\"thread-1\"}\n",
                "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"GLASS_REAL_OK\"}}\n",
                "{\"type\":\"turn.completed\",\"usage\":{\"output_tokens\":1}}\n"
            ),
            "stderr": "ignored diagnostic",
        });
        assert_eq!(external_agent_output(&value), "GLASS_REAL_OK");

        let claude = serde_json::json!({
            "output": concat!(
                "{\"type\":\"system\",\"subtype\":\"init\"}\n",
                "{\"type\":\"result\",\"result\":\"Claude result\"}\n"
            )
        });
        assert_eq!(external_agent_output(&claude), "Claude result");

        let retry = serde_json::json!({
            "output": "{\"type\":\"system\",\"subtype\":\"api_retry\",\"error\":\"rate_limit\"}\n"
        });
        assert_eq!(external_agent_output(&retry), "rate_limit");
    }

    #[test]
    fn browser_evidence_card_is_bounded_and_revision_aware() {
        let value = serde_json::json!({
            "type": "glass_browser_evidence",
            "toolName": "glass.browser.navigate",
            "ok": true,
            "backend": "native",
            "title": "Checkout",
            "url": "https://example.test/checkout?token=secret#payment",
            "browserRevision": 42,
            "targetId": "page-1",
            "semanticEntityCount": 3,
        });
        let projected = browser_evidence(&value);
        assert!(projected.contains("Checkout"));
        assert!(projected.contains("backend native"));
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
    fn screenshot_projection_matches_only_the_canonical_tool() {
        let value = serde_json::json!({"message":"extension screenshot inspection result"});
        assert_eq!(
            browser_result("extension.screenshot.inspect", &value),
            "message: extension screenshot inspection result"
        );
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
    fn conversation_projects_safe_notices_and_errors_but_suppresses_unknown_text() {
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let event = |sequence: u64, kind: &str, payload: serde_json::Value| crate::AgentEvent {
            sequence,
            agent_id: agent_id.clone(),
            timestamp_ms: 0,
            kind: kind.into(),
            payload,
        };
        let entries = conversation_entries(&[
            event(
                1,
                "notice",
                serde_json::json!({"text": "Core session ready"}),
            ),
            event(
                2,
                "status",
                serde_json::json!({"status": "Waiting for input"}),
            ),
            event(
                3,
                "error",
                serde_json::json!({"message": "Provider unavailable"}),
            ),
            event(
                4,
                "private_extension_payload",
                serde_json::json!({"text": "private extension data"}),
            ),
        ]);

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].kind, ConversationKind::System);
        assert!(entries[0].text.contains("Core session ready"));
        assert_eq!(entries[1].kind, ConversationKind::System);
        assert!(entries[1].text.contains("Waiting for input"));
        assert_eq!(entries[2].kind, ConversationKind::Error);
        assert!(entries[2].text.contains("Provider unavailable"));
        assert!(
            !entries
                .iter()
                .any(|entry| entry.text.contains("private extension data"))
        );
    }

    #[test]
    fn reasoning_is_accumulated_for_explicit_view_and_hidden_from_normal_transcript() {
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let event = |sequence: u64, kind: &str, payload: serde_json::Value| crate::AgentEvent {
            sequence,
            agent_id: agent_id.clone(),
            timestamp_ms: 0,
            kind: kind.into(),
            payload,
        };
        let events = [
            event(1, "reasoning", serde_json::json!({"text": "First thought"})),
            event(
                2,
                "message_update",
                serde_json::json!({"assistantMessageEvent": {"type": "text_delta", "delta": "Visible answer"}}),
            ),
            event(
                3,
                "reasoning",
                serde_json::json!({"text": "Second thought"}),
            ),
        ];

        assert_eq!(
            conversation_reasoning(&events),
            "First thought\n\nSecond thought"
        );
        assert!(conversation(&events).contains("Visible answer"));
        assert!(!conversation(&events).contains("First thought"));
        assert!(!conversation(&events).contains("Second thought"));
    }

    #[test]
    fn core_host_tool_evidence_between_deltas_does_not_break_assistant_projection() {
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let event = |sequence: u64, kind: &str, payload: serde_json::Value| crate::AgentEvent {
            sequence,
            agent_id: agent_id.clone(),
            timestamp_ms: 0,
            kind: kind.into(),
            payload,
        };
        let entries = conversation_entries(&[
            event(
                1,
                "user",
                serde_json::json!({"text": "offline prompt round trip"}),
            ),
            event(
                2,
                "tool_execution_end",
                serde_json::json!({"callId": "call-read", "isError": false}),
            ),
            event(
                3,
                "glass_tool_evidence",
                serde_json::json!({"toolName": "glass.workspace.list", "ok": true}),
            ),
            event(
                4,
                "message_update",
                serde_json::json!({
                    "assistantMessageEvent": {"type": "text_delta", "delta": "handled "}
                }),
            ),
            event(
                5,
                "message_update",
                serde_json::json!({
                    "assistantMessageEvent": {
                        "type": "text_delta",
                        "delta": "offline prompt round trip"
                    }
                }),
            ),
            event(6, "agent_end", serde_json::json!({})),
        ]);

        let rendered = entries
            .iter()
            .map(ConversationEntry::render)
            .collect::<Vec<_>>();
        assert!(
            rendered
                .iter()
                .any(|entry| entry == "YOU\noffline prompt round trip")
        );
        assert!(
            rendered
                .iter()
                .any(|entry| entry == "GLASS AGENT\nhandled offline prompt round trip")
        );
        assert!(
            rendered
                .iter()
                .any(|entry| entry.contains("glass.workspace.list"))
        );
        assert!(!rendered.iter().any(|entry| entry == "SYSTEM\n"));
    }

    #[test]
    fn next_turn_closes_the_previous_streaming_bubble() {
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let event = |sequence: u64, kind: &str, payload: serde_json::Value| crate::AgentEvent {
            sequence,
            agent_id: agent_id.clone(),
            timestamp_ms: 0,
            kind: kind.into(),
            payload,
        };
        let entries = conversation_entries(&[
            event(1, "turn_start", serde_json::json!({})),
            event(
                2,
                "message_update",
                serde_json::json!({
                    "assistantMessageEvent": {
                        "type": "text_delta",
                        "delta": "partial response"
                    }
                }),
            ),
            event(3, "turn_start", serde_json::json!({})),
        ]);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].text, "partial response");
        assert!(!entries[0].streaming);
    }

    #[test]
    fn host_tool_evidence_is_authoritative_over_core_completion_events() {
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let event = |sequence: u64, kind: &str, payload: serde_json::Value| crate::AgentEvent {
            sequence,
            agent_id: agent_id.clone(),
            timestamp_ms: 0,
            kind: kind.into(),
            payload,
        };
        let entries = conversation_entries(&[
            event(
                1,
                "tool_execution_start",
                serde_json::json!({
                    "callId": "call-1",
                    "toolName": "glass.browser.navigate",
                    "command": "https://example.test"
                }),
            ),
            event(
                2,
                "tool_progress",
                serde_json::json!({
                    "text": {
                        "callId": "call-1",
                        "text": "browser navigation result"
                    }
                }),
            ),
            event(
                3,
                "tool_execution_end",
                serde_json::json!({
                    "callId": "call-1",
                    "isError": false,
                    "text": "completed"
                }),
            ),
            event(
                4,
                "glass_browser_evidence",
                serde_json::json!({
                    "toolName": "glass.browser.navigate",
                    "ok": false,
                    "error": "page revision is stale"
                }),
            ),
        ]);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, ConversationKind::Error);
        assert!(entries[0].text.contains("page revision is stale"));
        assert!(!entries[0].text.contains("completed"));
        assert!(!entries[0].text.contains("browser navigation result"));
    }

    #[test]
    fn conversation_removes_only_exact_legacy_playbook_prefixes() {
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let playbook = super::super::playbooks::playbook(super::super::state::DevSurface::Code);
        let make_event = |sequence, payload| crate::AgentEvent {
            sequence,
            agent_id: agent_id.clone(),
            timestamp_ms: 0,
            kind: "user".into(),
            payload,
        };
        let events = vec![
            make_event(
                1,
                serde_json::json!({
                    "text": format!("{playbook}\n\nkeep this request"),
                    "contextAttached": true
                }),
            ),
            make_event(
                2,
                serde_json::json!({
                    "text": format!("{playbook}\n\nkeep this literal"),
                    "contextAttached": false
                }),
            ),
            make_event(
                3,
                serde_json::json!({
                    "text": "previous mode header\n\nkeep this literal",
                    "contextAttached": true
                }),
            ),
        ];
        let entries = conversation_entries(&events);
        assert_eq!(entries[0].text, "keep this request");
        assert_eq!(entries[1].text, format!("{playbook}\n\nkeep this literal"));
        assert_eq!(entries[2].text, "previous mode header\n\nkeep this literal");
    }

    #[test]
    fn conversation_entries_ignore_legacy_glass_approval_events() {
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let events = vec![
            crate::AgentEvent {
                sequence: 1,
                agent_id: agent_id.clone(),
                timestamp_ms: 0,
                kind: "user".into(),
                payload: serde_json::json!({
                    "text": "ship it",
                    "entryId": "entry-user",
                    "clientMessageId": "chat-17"
                }),
            },
            crate::AgentEvent {
                sequence: 2,
                agent_id: agent_id.clone(),
                timestamp_ms: 0,
                kind: "glass_tool_approval_request".into(),
                payload: serde_json::json!({
                    "toolName": "glass.file.write",
                    "arguments": {"path": "src/lib.rs", "content": "secret-token-value"}
                }),
            },
            crate::AgentEvent {
                sequence: 3,
                agent_id,
                timestamp_ms: 0,
                kind: "message_end".into(),
                payload: serde_json::json!({
                    "message": {
                        "id": "entry-assistant",
                        "role": "assistant",
                        "content": [{"type": "text", "text": "queued"}]
                    }
                }),
            },
        ];
        let entries = conversation_entries(&events);
        assert_eq!(entries[0].entry_id.as_deref(), Some("entry-user"));
        assert_eq!(entries[0].client_message_id.as_deref(), Some("chat-17"));
        assert_eq!(entries.len(), 2);
        assert!(!conversation(&events).contains("approval"));
        assert!(!conversation(&events).contains("secret-token-value"));
        assert_eq!(
            entries.last().and_then(|entry| entry.entry_id.as_deref()),
            Some("entry-assistant")
        );
    }

    #[test]
    fn conversation_hides_non_display_custom_context_messages() {
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let events = vec![
            crate::AgentEvent {
                sequence: 1,
                agent_id: agent_id.clone(),
                timestamp_ms: 0,
                kind: "message_end".into(),
                payload: serde_json::json!({
                    "message": {
                        "role": "custom",
                        "customType": "glass.context",
                        "display": false,
                        "content": [{
                            "type": "text",
                            "text": "{\"authority\":{\"mutationLeaseRequired\":false}}"
                        }]
                    }
                }),
            },
            crate::AgentEvent {
                sequence: 2,
                agent_id,
                timestamp_ms: 0,
                kind: "message_end".into(),
                payload: serde_json::json!({
                    "message": {
                        "role": "assistant",
                        "content": [{"type": "text", "text": "hello"}]
                    }
                }),
            },
        ];
        let projected = conversation(&events);
        assert_eq!(projected, "GLASS AGENT\nhello");
        assert!(!projected.contains("mutationLeaseRequired"));
    }

    #[test]
    fn conversation_ignores_legacy_glass_approval_bridge_events() {
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let events = [
            "glass_tool_approval_request",
            "glass_tool_approval_resolved",
        ]
        .into_iter()
        .enumerate()
        .map(|(sequence, kind)| crate::AgentEvent {
            sequence: sequence as u64 + 1,
            agent_id: agent_id.clone(),
            timestamp_ms: 0,
            kind: kind.into(),
            payload: serde_json::json!({
                "type": kind,
                "toolName": "glass.browser.act",
                "frameId": "frame-1",
                "approved": true,
                "arguments": {"action": "click", "target": "e12"}
            }),
        })
        .collect::<Vec<_>>();
        assert!(conversation_entries(&events).is_empty());
        assert!(conversation(&events).is_empty());
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

    #[test]
    fn failed_core_browser_evidence_keeps_the_host_error_authoritative() {
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let event = |sequence: u64, kind: &str, payload| crate::AgentEvent {
            sequence,
            agent_id: agent_id.clone(),
            timestamp_ms: 0,
            kind: kind.into(),
            payload,
        };
        let entries = conversation_entries(&[
            event(
                1,
                "tool_execution_start",
                serde_json::json!({
                    "callId": "call-browser-1",
                    "toolName": "glass.browser.navigate",
                    "command": "https://example.test"
                }),
            ),
            event(
                2,
                "tool_execution_end",
                serde_json::json!({
                    "callId": "call-browser-1",
                    "isError": false,
                    "text": "completed"
                }),
            ),
            event(
                3,
                "glass_browser_evidence",
                serde_json::json!({
                    "type": "glass_browser_evidence",
                    "toolName": "glass.browser.navigate",
                    "ok": false,
                    "error": "connection failure: no committed document generation"
                }),
            ),
        ]);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, ConversationKind::Error);
        assert!(entries[0].text.contains("glass.browser.navigate"));
        assert!(entries[0].text.contains("no committed document generation"));
        assert!(!entries[0].text.contains("completed"));
        assert!(!entries[0].text.contains("SYSTEM"));
    }
}
