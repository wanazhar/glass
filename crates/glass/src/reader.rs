//! Inline conversation reader for the owned chat body region.
use glass_tui::{
    Entry, Transcript,
    crossterm::event::{Event, KeyCode, KeyModifiers, MouseEventKind},
    ratatui::{Terminal, TerminalOptions, Viewport, backend::CrosstermBackend, layout::Rect},
    theme::TerminalWriter,
};
use std::collections::HashSet;
use std::io;

type Tui = Terminal<CrosstermBackend<TerminalWriter<io::Stdout>>>;

pub enum Action {
    Handled,
    Close,
    CloseAndPass,
    PassThrough,
}

pub struct Reader {
    terminal: Tui,
    transcript: Transcript,
    unread: usize,
    search: Option<String>,
    match_count: usize,
    entries: Vec<Entry>,
    expanded_cards: HashSet<(usize, String)>,
    selected_card: Option<(usize, String)>,
}

impl Reader {
    pub fn new(entries: Vec<Entry>, page_up: bool) -> io::Result<Self> {
        let rect = body_rect();
        let terminal = make_clean_terminal(rect)?;
        let mut transcript = Transcript::new(render_entries(&entries, &HashSet::new()));
        transcript.set_width(rect.width);
        transcript.follow_tail(rect.height);
        if page_up {
            transcript.page(-1, rect.height);
        }
        let mut reader = Self {
            terminal,
            transcript,
            unread: 0,
            search: None,
            match_count: 0,
            entries,
            expanded_cards: HashSet::new(),
            selected_card: None,
        };
        reader.draw()?;
        Ok(reader)
    }

    pub fn update(&mut self, entries: Vec<Entry>) -> io::Result<()> {
        let following = self.transcript.follow;
        self.entries = entries;
        if !self
            .transcript
            .replace_entries(render_entries(&self.entries, &self.expanded_cards))
        {
            return Ok(());
        }
        if let Some(query) = self.search.clone() {
            self.search_matches(&query);
        }
        if following {
            self.unread = 0;
        } else {
            self.unread = self.unread.saturating_add(1);
        }
        self.draw()
    }

    /// Repaint after an overlay has written directly to the same body rows.
    pub fn restore_after_overlay(&mut self) -> io::Result<()> {
        self.terminal = make_clean_terminal(body_rect())?;
        self.draw()
    }

    pub fn resize(&mut self) -> io::Result<()> {
        self.terminal = make_clean_terminal(body_rect())?;
        self.draw()
    }

    pub fn event(&mut self, event: &Event) -> io::Result<Action> {
        let area = body_rect();
        let draw;
        match event {
            Event::Resize(_, _) => {
                draw = true;
            }
            Event::Key(key) => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                if let Some(query) = self.search.as_mut() {
                    let mut refresh_search = false;
                    match key.code {
                        KeyCode::Esc => {
                            self.search = None;
                            self.match_count = 0;
                        }
                        KeyCode::Enter | KeyCode::Down => {
                            self.transcript.search_next(false, area.height);
                            self.align_selected_card_end(area.height);
                        }
                        KeyCode::Up => {
                            self.transcript.search_next(true, area.height);
                            self.align_selected_card_end(area.height);
                        }
                        KeyCode::Backspace => {
                            query.pop();
                            refresh_search = true;
                        }
                        KeyCode::Char(c) if !ctrl => {
                            query.push(c);
                            refresh_search = true;
                        }
                        _ => return Ok(Action::Handled),
                    }
                    if refresh_search {
                        let query = self.search.clone().unwrap_or_default();
                        self.search_matches(&query);
                    }
                    draw = true;
                    if draw {
                        self.draw()?;
                    }
                    return Ok(Action::Handled);
                }
                match key.code {
                    KeyCode::Esc => return Ok(Action::Close),
                    KeyCode::End => {
                        self.transcript.follow_tail(area.height);
                    }
                    KeyCode::Enter => {
                        let index = self.selected_tool_card_index();
                        if let Some(index) = index
                            && let Some(card) = self.entries[index].tool_card.as_ref()
                        {
                            let key = (self.entries[index].turn, card.call_id.clone());
                            if !self.expanded_cards.insert(key.clone()) {
                                self.expanded_cards.remove(&key);
                            }
                            self.transcript.replace_entries(render_entries(
                                &self.entries,
                                &self.expanded_cards,
                            ));
                            if let Some(line) = self.transcript.layout.line_for_entry(index) {
                                self.transcript.scroll = line;
                                self.transcript.follow = false;
                            }
                            self.draw()?;
                            return Ok(Action::Handled);
                        }
                        return Ok(Action::Handled);
                    }
                    KeyCode::Char('f') if ctrl => {
                        self.search = Some(String::new());
                        self.match_count = 0;
                    }
                    KeyCode::Char('g' | 't') if ctrl => return Ok(Action::CloseAndPass),
                    KeyCode::Home if ctrl => self.transcript.top(),
                    KeyCode::PageUp => self.transcript.step(-(area.height as isize), area.height),
                    KeyCode::PageDown => self.transcript.step(area.height as isize, area.height),
                    KeyCode::Char('b') if ctrl => {
                        self.transcript.step(-(area.height as isize), area.height)
                    }
                    KeyCode::Char('f') if ctrl => {
                        self.transcript.step(area.height as isize, area.height)
                    }
                    _ => return Ok(Action::PassThrough),
                }
                draw = true;
            }
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.transcript.step(-3, area.height);
                    draw = true;
                }
                MouseEventKind::ScrollDown => {
                    self.transcript.step(3, area.height);
                    draw = true;
                }
                _ => return Ok(Action::Handled),
            },
            Event::Paste(_) => return Ok(Action::PassThrough),
            _ => return Ok(Action::Handled),
        }
        if draw {
            self.draw()?;
        }
        Ok(Action::Handled)
    }

    fn draw(&mut self) -> io::Result<()> {
        let hint = if let Some(query) = &self.search {
            format!(
                "/{}  · {} matches · Enter/↓ next · ↑ previous · Esc cancel",
                query, self.match_count
            )
        } else if let Some(index) = self.selected_tool_card_index() {
            let entry = &self.entries[index];
            let Some(card) = entry.tool_card.as_ref() else {
                unreachable!("selected tool card must have metadata")
            };
            let action = if self
                .expanded_cards
                .contains(&(entry.turn, card.call_id.clone()))
            {
                "Enter collapse"
            } else {
                "Enter expand"
            };
            format!(
                "Tool {} · {action} · PgUp/PgDn · End/Esc bottom",
                card.call_id
            )
        } else if self.transcript.follow {
            "PgUp/Ctrl+B ↑ · PgDn/Ctrl+F ↓ · End/Esc bottom · Wheel".to_string()
        } else if self.unread == 0 {
            "End / Esc back to bottom".to_string()
        } else {
            format!("End / Esc bottom · {} new updates", self.unread)
        };
        self.transcript.set_inline_hint(Some(hint));
        self.terminal.draw(|frame| {
            self.transcript.draw(frame, &[], "Conversation");
        })?;
        crate::surface::restore_composer_cursor();
        crate::surface::reader_drawn();
        Ok(())
    }

    fn visible_tool_card_index(&self) -> Option<usize> {
        let area = body_rect();
        let first = self.transcript.layout.entry_at_line(self.transcript.scroll);
        if self
            .entries
            .get(first)
            .is_some_and(|entry| entry.tool_card.is_some())
        {
            return Some(first);
        }
        let end = self.transcript.scroll.saturating_add(area.height as usize);
        (first..self.entries.len()).find(|candidate| {
            self.entries[*candidate].tool_card.is_some()
                && self
                    .transcript
                    .layout
                    .line_for_entry(*candidate)
                    .is_some_and(|line| (self.transcript.scroll..end).contains(&line))
        })
    }

    fn selected_tool_card_index(&self) -> Option<usize> {
        self.selected_card
            .as_ref()
            .and_then(|(turn, call_id)| {
                self.entries.iter().position(|entry| {
                    entry.turn == *turn
                        && entry
                            .tool_card
                            .as_ref()
                            .is_some_and(|card| card.call_id == *call_id)
                })
            })
            .or_else(|| self.visible_tool_card_index())
    }

    fn align_selected_card_end(&mut self, height: u16) {
        let Some(index) = self.selected_tool_card_index() else {
            return;
        };
        let end = self
            .transcript
            .layout
            .line_for_entry(index + 1)
            .unwrap_or_else(|| self.transcript.layout.total_lines());
        let last = end.saturating_sub(1);
        let page = height.max(1) as usize;
        let max_scroll = self.transcript.layout.total_lines().saturating_sub(page);
        self.transcript.scroll = last.saturating_sub(page.saturating_sub(1)).min(max_scroll);
        self.transcript.follow = false;
    }

    fn search_matches(&mut self, query: &str) {
        if !query.is_empty() {
            let needle = query.to_lowercase();
            self.selected_card = None;
            for entry in &self.entries {
                if let Some(card) = &entry.tool_card
                    && (card.command.to_lowercase().contains(&needle)
                        || card.output.to_lowercase().contains(&needle))
                {
                    self.expanded_cards
                        .insert((entry.turn, card.call_id.clone()));
                    if self.selected_card.is_none() {
                        self.selected_card = Some((entry.turn, card.call_id.clone()));
                    }
                }
            }
            self.transcript
                .replace_entries(render_entries(&self.entries, &self.expanded_cards));
        }
        self.match_count = self.transcript.search(query);
        self.align_selected_card_end(body_rect().height);
    }
}

fn render_entries(entries: &[Entry], expanded: &HashSet<(usize, String)>) -> Vec<Entry> {
    entries
        .iter()
        .cloned()
        .map(|mut entry| {
            if let Some(card) = &entry.tool_card {
                let is_expanded = expanded.contains(&(entry.turn, card.call_id.clone()));
                entry.text = crate::session::tool_card_text(card, is_expanded);
            }
            entry
        })
        .collect()
}

fn body_rect() -> Rect {
    let (x, y, width, height) = crate::surface::body_rect();
    Rect::new(x, y, width.max(1), height.max(1))
}

fn make_terminal(rect: Rect) -> io::Result<Tui> {
    Terminal::with_options(
        CrosstermBackend::new(TerminalWriter(io::stdout())),
        TerminalOptions {
            viewport: Viewport::Fixed(rect),
        },
    )
}

fn make_clean_terminal(rect: Rect) -> io::Result<Tui> {
    crate::surface::clear_body_rect()?;
    make_terminal(rect)
}
