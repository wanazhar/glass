//! Live alternate-screen views driven by the same pump as provider/tool work.
use crate::{agent::Progress, surface};
use glass_tui::{
    Entry, Role, Transcript,
    crossterm::{
        event::{Event, KeyCode, KeyModifiers, MouseEventKind},
        execute,
        terminal::{Clear, ClearType},
    },
    ratatui::{
        Terminal,
        backend::CrosstermBackend,
        style::Style,
        widgets::{Block, Borders, Paragraph},
    },
};
use std::collections::HashMap;
use std::io;
use std::time::Instant;
type Tui = Terminal<CrosstermBackend<glass_tui::theme::TerminalWriter<io::Stdout>>>;
pub struct Live {
    entries: Vec<Entry>,
    reasoning: String,
    phase: &'static str,
    provider_detail: Option<String>,
    view: Option<(Tui, Transcript, bool)>,
    parked_views: [Option<Transcript>; 2],
    dirty: bool,
    turn: usize,
    thinking_scroll: usize,
    thinking_follow: bool,
    content_revision: u64,
    tool_started: HashMap<String, Instant>,
    expanded_cards: std::collections::HashSet<(usize, String)>,
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
            thinking_scroll: 0,
            thinking_follow: true,
            content_revision: 0,
            tool_started: HashMap::new(),
            expanded_cards: std::collections::HashSet::new(),
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
                    card.expanded = self.expanded_cards.contains(&(e.turn, card.call_id.clone()));
                    crate::session::tool_card_text(
                        card,
                        self.expanded_cards
                            .contains(&(e.turn, card.call_id.clone())),
                    )
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
                    role: if *name == "bash" { Role::Bash } else { Role::Tool },
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
                call_id, output, summary,
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
                self.phase = "Provider fallback";
                self.provider_detail = Some(format!(
                    "{} · /status /model",
                    sanitize_provider_detail(text)
                ));
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
    pub fn toggle(&mut self, thinking: bool) -> io::Result<()> {
        if self.view.as_ref().is_some_and(|v| v.2 == thinking) {
            return self.close();
        }
        if self.active() {
            self.close()?;
        }
        surface::overlay(true)?;
        execute!(io::stdout(), Clear(ClearType::All))?;
        // Alternate screen uses full margins; the primary owner restores its own on close.
        use std::io::Write;
        write!(io::stdout(), "\x1b[r")?;
        let tui = Terminal::new(CrosstermBackend::new(glass_tui::theme::TerminalWriter(
            io::stdout(),
        )))?;
        let transcript = self
            .take_parked_view(thinking)
            .unwrap_or_else(|| Transcript::new(self.rows()));
        self.view = Some((tui, transcript, thinking));
        self.dirty = true;
        self.draw()
    }
    pub fn close(&mut self) -> io::Result<()> {
        if let Some((mut tui, view, thinking)) = self.view.take() {
            tui.show_cursor().ok();
            self.park_view(thinking, view);

            surface::overlay(false)?;
        }
        Ok(())
    }

    fn take_parked_view(&mut self, thinking: bool) -> Option<Transcript> {
        self.parked_views[usize::from(thinking)].take()
    }

    fn park_view(&mut self, thinking: bool, view: Transcript) {
        self.parked_views[usize::from(thinking)] = Some(view);
    }
    pub fn event(&mut self, ev: &Event) -> bool {
        if !self.active() {
            return false;
        }
        let height = self
            .view
            .as_ref()
            .and_then(|v| v.0.size().ok())
            .map(|s| s.height)
            .unwrap_or(24);
        let transcript_height = Transcript::content_rect(glass_tui::ratatui::layout::Rect::new(0, 0, 1, height)).height;
        match ev {
            Event::Key(k) => {
                let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
                if ctrl && k.code == KeyCode::Char('c') {
                    self.close().ok();
                    return false;
                }
                if matches!(k.code, KeyCode::Esc | KeyCode::Char('q'))
                    || ctrl && matches!(k.code, KeyCode::Char('g') | KeyCode::Char('t'))
                {
                    let switch = ctrl
                        && matches!(k.code, KeyCode::Char('g') | KeyCode::Char('t'))
                        && self
                            .view
                            .as_ref()
                            .is_some_and(|v| v.2 != (k.code == KeyCode::Char('t')));
                    if switch {
                        self.toggle(k.code == KeyCode::Char('t')).ok();
                    } else {
                        self.close().ok();
                    }
                    return true;
                }
                if self.view.as_ref().unwrap().2 {
                    let amount = (usize::from(height) / 2).max(1);
                    match k.code {
                        KeyCode::Up => {
                            self.thinking_scroll = self.thinking_scroll.saturating_sub(1);
                            self.thinking_follow = false;
                        }
                        KeyCode::Down => {
                            self.thinking_scroll += 1;
                            self.thinking_follow = false;
                        }
                        KeyCode::PageUp => {
                            self.thinking_scroll = self.thinking_scroll.saturating_sub(amount);
                            self.thinking_follow = false;
                        }
                        KeyCode::PageDown => {
                            self.thinking_scroll += amount;
                            self.thinking_follow = false;
                        }
                        KeyCode::Home => {
                            self.thinking_scroll = 0;
                            self.thinking_follow = false;
                        }
                        KeyCode::End | KeyCode::Char('f') => self.thinking_follow = true,
                        _ => {}
                    }
                }
                let is_thinking = self.view.as_ref().unwrap().2;
                if !is_thinking && k.code == KeyCode::Enter {
                    let selected = self
                        .view
                        .as_ref()
                        .map(|(_, view, _)| view.layout.entry_at_line(view.scroll));
                    let card_key = selected.and_then(|index| {
                        self.entries.get(index).and_then(|entry| {
                            entry
                                .tool_card
                                .as_ref()
                                .map(|card| (entry.turn, card.call_id.clone()))
                        })
                    });
                    if let Some(key) = card_key {
                        if !self.expanded_cards.insert(key.clone()) {
                            self.expanded_cards.remove(&key);
                        }
                        let rows = self.rows();
                        if let Some((_, view, _)) = self.view.as_mut() {
                            view.replace_entries(rows);
                        }
                    } else if let Some((_, view, _)) = self.view.as_mut() {
                        view.toggle_collapse();
                    }
                    self.dirty = true;
                    return true;
                }
                let (_, view, _) = self.view.as_mut().unwrap();
                match k.code {
                    KeyCode::Up => view.step(-1, transcript_height),
                    KeyCode::Down => view.step(1, transcript_height),
                    KeyCode::PageUp => view.page(-1, transcript_height),
                    KeyCode::PageDown => view.page(1, transcript_height),
                    KeyCode::Home => view.top(),
                    KeyCode::End | KeyCode::Char('f') => view.follow_tail(transcript_height),
                    _ => {}
                }
            }
            Event::Resize(_, _) => {
                self.view.as_mut().unwrap().0.autoresize().ok();
            }
            Event::Mouse(mouse) => {
                if let Some(delta) = wheel_lines(mouse.kind) {
                    if self.view.as_ref().is_some_and(|view| view.2) {
                        if delta < 0 {
                            self.thinking_scroll =
                                self.thinking_scroll.saturating_sub((-delta) as usize);
                            self.thinking_follow = false;
                        } else {
                            self.thinking_follow = true;
                        }
                    } else if let Some((_, view, _)) = self.view.as_mut() {
                        view.step(delta, transcript_height);
                    }
                }
            }
            _ => {}
        }
        self.dirty = true;
        true
    }
    pub fn draw(&mut self) -> io::Result<()> {
        if !self.dirty || !self.active() {
            return Ok(());
        }
        let rows = self.rows();
        let phase = self.phase;
        let reasoning = glass_tui::markdown::sanitize(&self.reasoning);
        let thinking_scroll = &mut self.thinking_scroll;
        let thinking_follow = self.thinking_follow;
        if let Some((tui, view, thinking)) = self.view.as_mut() {
            view.replace_entries(rows);
            tui.draw(|f|{
                if *thinking {
                    let body=if reasoning.is_empty(){format!("{phase}\n\nNo provider reasoning has been returned. Some providers expose no reasoning text or public summary.\n\nCtrl-T / Esc close · work continues")}else{format!("{phase}\n\n{reasoning}\n\nCtrl-T / Esc close · work continues")};
                    let lines=glass_tui::text::wrap_words(&body,usize::from(f.area().width.saturating_sub(2).max(1)));
                    let max=lines.len().saturating_sub(usize::from(f.area().height.saturating_sub(2)));
                    *thinking_scroll=if reasoning.is_empty(){0}else if thinking_follow {max} else {(*thinking_scroll).min(max)};
                    f.render_widget(Paragraph::new(lines.iter().enumerate().map(|(i,line)|glass_tui::ratatui::text::Line::styled(line.clone(),Style::default().fg(glass_tui::theme::color(if i==0 {glass_tui::theme::Tone::Secondary}else{glass_tui::theme::Tone::Body})))).collect::<Vec<_>>()).scroll(((*thinking_scroll).min(u16::MAX as usize) as u16,0)).style(Style::default().fg(glass_tui::theme::color(glass_tui::theme::Tone::Body))).block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(glass_tui::theme::color(glass_tui::theme::Tone::Accent))).title(glass_tui::ratatui::text::Span::styled(" Provider reasoning ",Style::default().fg(glass_tui::theme::color(glass_tui::theme::Tone::Accent))))),f.area());
                }else{view.draw(f,&[],"live");}
            })?;
        }
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
    fn live_view_wheel_deltas_only_scroll_for_wheel_events() {
        assert_eq!(wheel_lines(MouseEventKind::ScrollUp), Some(-3));
        assert_eq!(wheel_lines(MouseEventKind::ScrollDown), Some(3));
        assert_eq!(wheel_lines(MouseEventKind::Moved), None);
    }

    #[test]
    fn transcript_and_reasoning_views_keep_independent_paused_positions() {
        let mut live = Live::new();
        let mut transcript = Transcript::new(transcript_entries(80));
        transcript.top();
        transcript.step(23, 10);
        let paused_line = transcript.scroll;
        let paused_entry = transcript.layout.entry_at_line(paused_line);
        let paused_text = transcript
            .layout
            .entry_text(paused_entry)
            .unwrap()
            .to_string();
        live.park_view(false, transcript);

        let mut reasoning = Transcript::new(transcript_entries(30));
        reasoning.top();
        reasoning.step(7, 10);
        let reasoning_line = reasoning.scroll;
        live.park_view(true, reasoning);

        // Switching to reasoning and back must not evict the transcript state.
        let reasoning = live.take_parked_view(true).unwrap();
        assert_eq!(reasoning.scroll, reasoning_line);
        live.park_view(true, reasoning);
        let mut transcript = live.take_parked_view(false).unwrap();
        assert_eq!(transcript.scroll, paused_line);
        transcript.replace_entries(transcript_entries(100));
        let entry = transcript.layout.entry_at_line(transcript.scroll);
        assert_eq!(
            transcript.layout.entry_text(entry),
            Some(paused_text.as_str())
        );
        live.park_view(false, transcript);

        assert_eq!(live.take_parked_view(true).unwrap().scroll, reasoning_line);
        assert_eq!(live.take_parked_view(false).unwrap().scroll, paused_line);
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
