//! Live alternate-screen views driven by the same pump as provider/tool work.
use crate::{agent::Progress, surface};
use glass_tui::{
    Entry, Role,
    crossterm::{
        event::{Event, KeyCode, KeyModifiers},
        execute,
        terminal::{Clear, ClearType},
    },
};
#[cfg(test)]
use glass_tui::{Transcript, crossterm::event::MouseEventKind};
use std::collections::HashMap;
use std::io;
use std::time::Instant;
pub struct Live {
    entries: Vec<Entry>,
    reasoning: String,
    phase: &'static str,
    provider_detail: Option<String>,
    view: Option<(crate::reader::Reader, bool)>,
    parked_views: [Option<crate::reader::Reader>; 2],
    dirty: bool,
    turn: usize,
    content_revision: u64,
    tool_started: HashMap<String, Instant>,
}
impl Live {
    pub fn new() -> Self {
        Self {
            entries: vec![],
            reasoning: String::new(),
            phase: "Waiting for provider",
            provider_detail: None,
            view: None,
            parked_views: [None, None],
            dirty: true,
            turn: 0,
            content_revision: 0,
            tool_started: HashMap::new(),
        }
    }
    pub fn seed(&mut self, entries: Vec<Entry>) {
        self.turn = entries.last().map(|e| e.turn).unwrap_or(0);
        self.entries = entries;
        self.reasoning.clear();
        self.phase = "Waiting for provider";
        self.provider_detail = None;
        self.dirty = true;
        self.content_revision = self.content_revision.wrapping_add(1);
    }
    pub fn seed_reasoning(&mut self, reasoning: String) {
        self.reasoning = reasoning;
        self.dirty = true;
    }
    fn rows(&self) -> Vec<Entry> {
        self.entries
            .iter()
            .map(|e| {
                let mut tool_card = e.tool_card.clone();
                let text = if let Some(card) = &mut tool_card {
                    if card.status == "running" {
                        card.elapsed_ms = self.tool_started.get(&card.call_id).map(|start| {
                            start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
                        });
                    }
                    crate::session::tool_card_text(card, card.expanded)
                } else {
                    e.text.clone()
                };
                Entry {
                    role: e.role,
                    text,
                    turn: e.turn,
                    tool_card,
                }
            })
            .collect()
    }
    pub fn snapshot(&self) -> Vec<Entry> {
        self.rows()
    }
    pub fn content_revision(&self) -> u64 {
        self.content_revision
    }
    pub fn append_entry(&mut self, role: Role, text: impl Into<String>) {
        self.entries.push(Entry {
            role,
            text: text.into(),
            turn: self.turn,
            tool_card: None,
        });
        self.content_revision = self.content_revision.wrapping_add(1);
        self.dirty = true;
    }
    pub fn progress(&mut self, p: &Progress<'_>) {
        match p {
            Progress::Waiting => {
                self.phase = "Waiting for provider";
                self.provider_detail = None;
            }
            Progress::Reasoning(t) => {
                if self.phase != "Provider reasoning" && !self.reasoning.is_empty() {
                    self.reasoning.push_str("\n\n");
                }
                self.reasoning.push_str(t);
                self.phase = "Provider reasoning";
            }
            Progress::Text(t) => {
                self.phase = "Answer streaming";
                self.provider_detail = None;
                if let Some(e) = self
                    .entries
                    .last_mut()
                    .filter(|e| e.role == Role::Assistant)
                {
                    e.text.push_str(t)
                } else {
                    self.entries.push(Entry {
                        role: Role::Assistant,
                        text: (*t).into(),
                        turn: self.turn,
                        tool_card: None,
                    })
                }
            }
            Progress::ToolStart {
                call_id,
                name,
                command,
            } => {
                self.phase = "Tool running";
                self.provider_detail = None;
                self.tool_started
                    .insert((*call_id).to_string(), Instant::now());
                self.entries.push(Entry {
                    role: if *name == "bash" {
                        Role::Bash
                    } else {
                        Role::Tool
                    },
                    text: format!("{name}\n{}", command.unwrap_or("")),
                    turn: self.turn,
                    tool_card: Some(glass_tui::ToolCard {
                        expanded: false,
                        call_id: (*call_id).to_string(),
                        command: command.unwrap_or(name).to_string(),
                        output: String::new(),
                        status: "running".into(),
                        elapsed_ms: None,
                        spill: None,
                        truncated: false,
                    }),
                })
            }
            Progress::ToolEnd {
                call_id,
                output,
                summary,
            } => {
                if let Some(entry) = self.entries.iter_mut().rev().find(|entry| {
                    entry
                        .tool_card
                        .as_ref()
                        .is_some_and(|card| card.call_id == *call_id)
                }) {
                    if let Some(card) = entry.tool_card.as_mut() {
                        card.output = (*output).to_string();
                        card.status = glass_tui::tool_output::status(summary);
                        card.elapsed_ms = self.tool_started.remove(*call_id).map(|started| {
                            started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
                        });
                        entry.text = format!("{}\n{}", card.command, output);
                    }
                } else {
                    self.entries.push(Entry {
                        role: Role::Tool,
                        text: (*output).into(),
                        turn: self.turn,
                        tool_card: None,
                    });
                }
                self.phase = "Tool finished";
            }
            Progress::Steered(text) => self.append_entry(Role::User, (*text).to_string()),
            Progress::BlockedByGuard(text) => {
                self.append_entry(Role::System, format!("Write refused: {text}"));
            }
            Progress::Retrying {
                attempt,
                delay_s,
                reason,
            } => {
                self.phase = "Retrying provider";
                let cause = sanitize_provider_detail(reason);
                self.provider_detail = Some(format!(
                    "Retry {attempt} in {delay_s}s · {cause} · /status /retry"
                ));
                self.append_entry(
                    Role::System,
                    format!("Retry {attempt} in {delay_s}s: {cause}"),
                );
            }
            Progress::ProviderNotice(text) => {
                let cooldown = text.starts_with("Provider cooldown");
                self.phase = if cooldown {
                    "Provider cooldown"
                } else {
                    "Provider fallback"
                };
                self.provider_detail = Some(if cooldown {
                    sanitize_provider_detail(
                        text.strip_prefix("Provider cooldown · ").unwrap_or(text),
                    )
                } else {
                    format!("{} · /status /model", sanitize_provider_detail(text))
                });
                self.append_entry(
                    Role::System,
                    format!("Provider notice: {}", sanitize_provider_detail(text)),
                );
            }
            Progress::ProviderFailure(text) => {
                self.phase = "Provider error";
                self.provider_detail = Some(format!(
                    "{} · recover with /retry, /status or /login",
                    sanitize_provider_detail(text)
                ));
            }
            Progress::Compacted { kept, dropped } => self.append_entry(
                Role::Meta,
                format!("Context compacted · dropped {dropped}, kept {kept}"),
            ),
        }
        if matches!(
            p,
            Progress::Text(_) | Progress::ToolStart { .. } | Progress::ToolEnd { .. }
        ) {
            self.content_revision = self.content_revision.wrapping_add(1);
        }
        self.dirty = true;
    }
    pub fn active(&self) -> bool {
        self.view.is_some()
    }
    pub fn phase(&self) -> &str {
        self.phase
    }
    pub fn provider_detail(&self) -> Option<&str> {
        self.provider_detail.as_deref()
    }
    pub fn finish(&mut self) {
        if !matches!(self.phase, "Provider error" | "Provider fallback") {
            self.phase = "Turn finished";
        }
        self.dirty = true;
    }
    fn view_entries(&self, thinking: bool) -> Vec<Entry> {
        if !thinking {
            return self.rows();
        }
        let body = if self.reasoning.is_empty() {
            "No provider reasoning has been returned. Some providers expose no reasoning text or public summary."
        } else {
            &self.reasoning
        };
        vec![Entry {
            role: Role::Assistant,
            text: body.to_string(),
            turn: self.turn,
            tool_card: None,
        }]
    }
    pub fn toggle(&mut self, thinking: bool) -> io::Result<()> {
        if self.view.as_ref().is_some_and(|v| v.1 == thinking) {
            return self.close();
        }
        self.close()?;
        surface::overlay(true)?;
        execute!(io::stdout(), Clear(ClearType::All))?;
        use std::io::Write;
        write!(io::stdout(), "\x1b[r")?;
        let entries = self.view_entries(thinking);
        let result = (|| {
            let mut reader = match self.parked_views[usize::from(thinking)].take() {
                Some(mut reader) if thinking => {
                    reader.reopen_latest(entries, self.phase == "Provider reasoning")?;
                    reader
                }
                Some(mut reader) => {
                    reader.restore_after_overlay()?;
                    reader.update(entries)?;
                    reader
                }
                None => crate::reader::Reader::overlay(
                    entries,
                    if thinking {
                        "Provider reasoning"
                    } else {
                        "Conversation"
                    },
                )?,
            };
            if thinking {
                reader.set_reasoning_streaming(self.phase == "Provider reasoning")?;
            }
            reader.hide_cursor();
            self.view = Some((reader, thinking));
            self.dirty = false;
            Ok(())
        })();
        if result.is_err() {
            surface::overlay(false).ok();
        }
        result
    }
    pub fn close(&mut self) -> io::Result<()> {
        if let Some((reader, thinking)) = self.view.take() {
            self.parked_views[usize::from(thinking)] = Some(reader);
            surface::overlay(false)?;
        }
        Ok(())
    }
    pub fn event(&mut self, ev: &Event) -> bool {
        if !self.active() {
            return false;
        }
        if let Event::Key(k) = ev {
            let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
            if ctrl && k.code == KeyCode::Char('c') {
                self.close().ok();
                return false;
            }
            if ctrl && matches!(k.code, KeyCode::Char('g' | 't')) {
                self.toggle(k.code == KeyCode::Char('t')).ok();
                return true;
            }
            if k.code == KeyCode::Char('q') && !ctrl && !self.view.as_ref().unwrap().0.searching() {
                self.close().ok();
                return true;
            }
        }
        let result = self.view.as_mut().unwrap().0.event(ev, false);
        match result {
            Ok(crate::reader::Action::Close | crate::reader::Action::CloseAndPass) | Err(_) => {
                self.close().ok();
            }
            _ => {}
        }
        true
    }
    pub fn draw(&mut self) -> io::Result<()> {
        if !self.dirty || !self.active() {
            return Ok(());
        }
        let thinking = self.view.as_ref().unwrap().1;
        let rows = self.view_entries(thinking);
        let reader = &mut self.view.as_mut().unwrap().0;
        if thinking {
            reader.set_reasoning_streaming(self.phase == "Provider reasoning")?;
        }
        reader.update(rows)?;
        self.dirty = false;
        Ok(())
    }
}

pub(crate) fn sanitize_provider_detail(text: &str) -> String {
    const REDACTED: &str = "[redacted]";
    const MAX_DETAIL_CHARS: usize = 512;
    fn key_name(word: &str) -> String {
        word.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
            .to_ascii_lowercase()
    }
    fn sensitive_key(key: &str) -> bool {
        matches!(
            key,
            "authorization"
                | "api_key"
                | "api-key"
                | "apikey"
                | "x-api-key"
                | "token"
                | "access_token"
                | "refresh_token"
                | "password"
                | "secret"
                | "client_secret"
                | "key"
        )
    }

    let text: String = text
        .chars()
        .filter(|ch| !ch.is_control() || ch.is_whitespace())
        .collect();
    let words: Vec<_> = text.split_whitespace().collect();
    let mut clean = Vec::with_capacity(words.len());
    let mut index = 0;
    while index < words.len() {
        let word = words[index];
        let lower = word.to_ascii_lowercase();
        let key = key_name(word.split('=').next().unwrap_or(word));

        if word.contains("://") {
            clean.push("[provider detail redacted]".to_string());
            index += 1;
            continue;
        }

        if let Some((name, value)) = word.split_once(':')
            && sensitive_key(&key_name(name))
        {
            let name_key = key_name(name);
            clean.push(format!("{name}: [redacted]"));
            index += 1;
            if name_key == "authorization" {
                if value.trim().eq_ignore_ascii_case("bearer") {
                    index = (index + 1).min(words.len());
                } else if value.trim().is_empty() {
                    if words
                        .get(index)
                        .is_some_and(|next| next.eq_ignore_ascii_case("bearer"))
                    {
                        index = (index + 2).min(words.len());
                    } else if index < words.len() {
                        index += 1;
                    }
                }
            } else if value.trim().is_empty() && index < words.len() {
                index += 1;
            }
            continue;
        }

        if let Some((name, value)) = word.split_once('=') {
            let name_key = key_name(name);
            if name_key == "bearer" {
                clean.push(format!("Bearer {REDACTED}"));
                index += 1;
                if value.is_empty() && index < words.len() {
                    index += 1;
                }
                continue;
            }
            if sensitive_key(&name_key) {
                clean.push(format!("{name}={REDACTED}"));
                index += 1;
                if name_key == "authorization"
                    && value.eq_ignore_ascii_case("bearer")
                    && index < words.len()
                {
                    index += 1;
                }
                continue;
            }
        }

        if key == "authorization" {
            clean.push(format!("{word} {REDACTED}"));
            index += 1;
            if words.get(index) == Some(&"=") {
                index += 1;
            }
            if words
                .get(index)
                .is_some_and(|value| value.eq_ignore_ascii_case("bearer"))
            {
                index += 2;
            } else if index < words.len() {
                index += 1;
            }
            continue;
        }

        if key == "bearer" || lower.starts_with("bearer:") {
            clean.push(format!("Bearer {REDACTED}"));
            index += 1;
            if !lower.starts_with("bearer:") && index < words.len() {
                index += 1;
            }
            continue;
        }

        if sensitive_key(&key) {
            clean.push(format!("{word} {REDACTED}"));
            index += 1;
            if words.get(index) == Some(&"=") {
                index += 1;
            }
            if index < words.len() && !words[index].ends_with(':') {
                index += 1;
            }
            continue;
        }

        clean.push(word.to_string());
        index += 1;
    }
    let text = clean.join(" ");
    if text.chars().count() > MAX_DETAIL_CHARS {
        format!(
            "{}…",
            text.chars().take(MAX_DETAIL_CHARS).collect::<String>()
        )
    } else {
        text
    }
}

#[cfg(test)]
fn wheel_lines(kind: MouseEventKind) -> Option<isize> {
    match kind {
        MouseEventKind::ScrollUp => Some(-3),
        MouseEventKind::ScrollDown => Some(3),
        _ => None,
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.close().ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transcript_entries(count: usize) -> Vec<Entry> {
        (0..count)
            .map(|turn| Entry {
                role: Role::User,
                text: format!("history marker {turn}"),
                turn,
                tool_card: None,
            })
            .collect()
    }

    #[test]
    fn cooldown_is_waiting_not_provider_fallback() {
        let mut live = Live::new();
        live.progress(&Progress::ProviderNotice(
            "Provider cooldown · 2.0s · waiting",
        ));
        assert_eq!(live.phase(), "Provider cooldown");
        assert_eq!(live.provider_detail(), Some("2.0s · waiting"));
        live.finish();
        assert_eq!(live.phase(), "Turn finished");
    }
    #[test]
    fn live_view_wheel_deltas_only_scroll_for_wheel_events() {
        assert_eq!(wheel_lines(MouseEventKind::ScrollUp), Some(-3));
        assert_eq!(wheel_lines(MouseEventKind::ScrollDown), Some(3));
        assert_eq!(wheel_lines(MouseEventKind::Moved), None);
    }

    #[test]
    fn shared_transcript_preserves_paused_anchor_on_live_updates() {
        let mut transcript = Transcript::new(transcript_entries(80));
        transcript.top();
        transcript.step(23, 10);
        let line = transcript.scroll;
        let index = transcript.layout.entry_at_line(line);
        let text = transcript.layout.entry_text(index).unwrap().to_string();
        transcript.replace_entries(transcript_entries(100));
        assert_eq!(
            transcript
                .layout
                .entry_text(transcript.layout.entry_at_line(transcript.scroll)),
            Some(text.as_str())
        );
    }

    #[test]
    fn live_tool_cards_match_results_by_exact_call_id_and_provider_errors_redact_urls() {
        let mut live = Live::new();
        live.progress(&Progress::ToolStart {
            call_id: "id-a",
            name: "bash",
            command: Some("same command"),
        });
        live.progress(&Progress::ToolStart {
            call_id: "id-b",
            name: "bash",
            command: Some("same command"),
        });
        live.progress(&Progress::ToolEnd {
            call_id: "id-a",
            summary: "done".into(),
            output: "identical output",
        });
        live.progress(&Progress::ToolEnd {
            call_id: "id-b",
            summary: "done".into(),
            output: "identical output",
        });
        let cards = live
            .snapshot()
            .into_iter()
            .filter_map(|entry| entry.tool_card)
            .collect::<Vec<_>>();
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].call_id, "id-a");
        assert_eq!(cards[1].call_id, "id-b");
        assert_eq!(cards[0].output, cards[1].output);
        live.progress(&Progress::ProviderFailure(
            "failed at https://host/path?token=secret",
        ));
        assert_eq!(live.phase(), "Provider error");
        assert!(
            live.provider_detail()
                .unwrap()
                .contains("recover with /retry")
        );
        assert!(!live.provider_detail().unwrap().contains("https://"));
        assert!(!live.provider_detail().unwrap().contains("secret"));
        live.progress(&Progress::ProviderFailure(
            "401 Authorization: Bearer sk-provider-secret-sentinel",
        ));
        assert!(
            !live
                .provider_detail()
                .unwrap()
                .contains("sk-provider-secret-sentinel")
        );
        assert!(
            live.snapshot()
                .iter()
                .all(|entry| { !entry.text.contains("sk-provider-secret-sentinel") })
        );
        live.finish();
        assert_eq!(live.phase(), "Provider error");
    }

    #[test]
    fn provider_details_consume_authorization_values_and_bound_untrusted_text() {
        let secret = "sk-provider-secret-sentinel";
        let detail = sanitize_provider_detail(&format!(
            "Authorization: Bearer {secret} authorization BEARER {secret} api_key={secret} token = {secret} https://host/?access_token={secret}"
        ));
        assert!(detail.contains("Authorization: [redacted]"), "{detail}");
        assert!(!detail.contains(secret), "{detail}");
        assert!(!detail.contains("https://"), "{detail}");
        assert!(sanitize_provider_detail(&"x".repeat(2048)).chars().count() <= 513);
    }
}
