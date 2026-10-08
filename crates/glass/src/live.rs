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
use std::io;
type Tui = Terminal<CrosstermBackend<glass_tui::theme::TerminalWriter<io::Stdout>>>;
pub struct Live {
    entries: Vec<Entry>,
    reasoning: String,
    phase: &'static str,
    view: Option<(Tui, Transcript, bool)>,
    dirty: bool,
    turn: usize,
    thinking_scroll: usize,
    thinking_follow: bool,
    content_revision: u64,
}
impl Live {
    pub fn new() -> Self {
        Self {
            entries: vec![],
            reasoning: String::new(),
            phase: "Waiting for provider",
            view: None,
            dirty: true,
            turn: 0,
            thinking_scroll: 0,
            thinking_follow: true,
            content_revision: 0,
        }
    }
    pub fn seed(&mut self, entries: Vec<Entry>) {
        self.turn = entries.last().map(|e| e.turn).unwrap_or(0);
        self.entries = entries;
        self.reasoning.clear();
        self.phase = "Waiting for provider";
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
            .map(|e| Entry {
                role: e.role,
                text: e.text.clone(),
                turn: e.turn,
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
        });
        self.content_revision = self.content_revision.wrapping_add(1);
        self.dirty = true;
    }
    pub fn progress(&mut self, p: &Progress<'_>) {
        match p {
            Progress::Waiting => self.phase = "Waiting for provider",
            Progress::Reasoning(t) => {
                if self.phase != "Provider reasoning" && !self.reasoning.is_empty() {
                    self.reasoning.push_str("\n\n");
                }
                self.reasoning.push_str(t);
                self.phase = "Provider reasoning";
            }
            Progress::Text(t) => {
                self.phase = "Answer streaming";
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
                    })
                }
            }
            Progress::ToolStart { name, command } => {
                self.phase = "Tool running";
                self.entries.push(Entry {
                    role: Role::Tool,
                    text: format!("{name}\n{}", command.unwrap_or("")),
                    turn: self.turn,
                })
            }
            Progress::ToolEnd { output, .. } => {
                self.entries.push(Entry {
                    role: Role::Tool,
                    text: (*output).into(),
                    turn: self.turn,
                });
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
            } => self.append_entry(
                Role::System,
                format!("Retry {attempt} in {delay_s}s: {reason}"),
            ),
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
    pub fn finish(&mut self) {
        self.phase = "Turn finished";
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
        self.view = Some((tui, Transcript::new(self.rows()), thinking));
        self.dirty = true;
        self.draw()
    }
    pub fn close(&mut self) -> io::Result<()> {
        if let Some((mut tui, _, _)) = self.view.take() {
            tui.show_cursor().ok();

            surface::overlay(false)?;
        }
        Ok(())
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
                let (_, view, _) = self.view.as_mut().unwrap();
                match k.code {
                    KeyCode::Up => view.step(-1, height),
                    KeyCode::Down => view.step(1, height),
                    KeyCode::PageUp => view.page(-1, height),
                    KeyCode::PageDown => view.page(1, height),
                    KeyCode::Home => view.top(),
                    KeyCode::End | KeyCode::Char('f') => view.follow_tail(height),
                    KeyCode::Enter => view.toggle_collapse(),
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
                        view.step(delta, height);
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

    #[test]
    fn live_view_wheel_deltas_only_scroll_for_wheel_events() {
        assert_eq!(wheel_lines(MouseEventKind::ScrollUp), Some(-3));
        assert_eq!(wheel_lines(MouseEventKind::ScrollDown), Some(3));
        assert_eq!(wheel_lines(MouseEventKind::Moved), None);
    }
}
