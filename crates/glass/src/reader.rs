//! Inline conversation reader for the owned chat body region.
use glass_tui::{
    Entry, Transcript,
    crossterm::event::{Event, KeyCode, KeyModifiers, MouseEventKind},
    ratatui::{Terminal, TerminalOptions, Viewport, backend::CrosstermBackend, layout::Rect},
    theme::TerminalWriter,
};
use std::collections::{HashMap, HashSet};
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
    loaded_spills: LoadedSpills,
}

impl Reader {
    pub fn new(entries: Vec<Entry>, page_up: bool) -> io::Result<Self> {
        let rect = body_rect();
        let terminal = make_clean_terminal(rect)?;
        let loaded_spills = HashMap::new();
        let mut transcript =
            Transcript::new(render_entries(&entries, &HashSet::new(), &loaded_spills));
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
            loaded_spills,
        };
        reader.draw()?;
        Ok(reader)
    }

    pub fn update(&mut self, entries: Vec<Entry>) -> io::Result<()> {
        let following = self.transcript.follow;
        self.entries = entries;
        if !self.transcript.replace_entries(render_entries(
            &self.entries,
            &self.expanded_cards,
            &self.loaded_spills,
        )) {
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
                                &self.loaded_spills,
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
                    KeyCode::Char('l') => {
                        if let Some(index) = self.selected_tool_card_index()
                            && let Some(card) = self.entries[index].tool_card.as_ref()
                            && let Some(path) = card.spill.as_deref()
                        {
                            let key = (self.entries[index].turn, card.call_id.clone());
                            self.loaded_spills
                                .entry(key.clone())
                                .or_insert_with(|| crate::polish::load_spill(path));
                            self.selected_card = Some(key.clone());
                            self.expanded_cards.insert(key);
                            self.transcript.replace_entries(render_entries(
                                &self.entries,
                                &self.expanded_cards,
                                &self.loaded_spills,
                            ));
                            self.align_selected_card_end(area.height);
                            self.draw()?;
                            return Ok(Action::Handled);
                        }
                        return Ok(Action::PassThrough);
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
            let loaded_spill = self.loaded_spills.get(&(entry.turn, card.call_id.clone()));
            if card.spill.is_some() && loaded_spill.is_none() {
                "L load full output (64 KiB cap)".to_string()
            } else if let Some(loaded) = loaded_spill {
                let status = match loaded {
                    Ok((_, true)) => "spill loaded 64 KiB",
                    Ok((_, false)) => "spill loaded",
                    Err(_) => "spill load failed",
                };
                format!("Tool {} · {status} · {action}", card.call_id)
            } else {
                format!(
                    "Tool {} · {action} · PgUp/PgDn · End/Esc bottom",
                    card.call_id,
                )
            }
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
        // Transcript reserves one row for its inline controls hint.
        let page = height.saturating_sub(1).max(1) as usize;
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
                        || card.output.to_lowercase().contains(&needle)
                        || self
                            .loaded_spills
                            .get(&(entry.turn, card.call_id.clone()))
                            .and_then(|spill| spill.as_ref().ok())
                            .is_some_and(|(text, _)| text.to_lowercase().contains(&needle)))
                {
                    self.expanded_cards
                        .insert((entry.turn, card.call_id.clone()));
                    if self.selected_card.is_none() {
                        self.selected_card = Some((entry.turn, card.call_id.clone()));
                    }
                }
            }
            self.transcript.replace_entries(render_entries(
                &self.entries,
                &self.expanded_cards,
                &self.loaded_spills,
            ));
        }
        self.match_count = self.transcript.search(query);
        self.align_selected_card_end(body_rect().height);
    }
}

type LoadedSpills = HashMap<(usize, String), Result<(String, bool), String>>;

fn render_entries(
    entries: &[Entry],
    expanded: &HashSet<(usize, String)>,
    loaded_spills: &LoadedSpills,
) -> Vec<Entry> {
    entries
        .iter()
        .cloned()
        .map(|mut entry| {
            if let Some(card) = &entry.tool_card {
                let key = (entry.turn, card.call_id.clone());
                let is_expanded = expanded.contains(&key);
                let mut rendered = card.clone();
                match loaded_spills.get(&key) {
                    Some(Ok((text, truncated))) => {
                        rendered.output.push_str("\n[loaded spill continuation]\n");
                        rendered.output.push_str(text);
                        if *truncated {
                            rendered
                                .output
                                .push_str("\n[spill display capped at 64 KiB]");
                        }
                        rendered.spill = None;
                        rendered.truncated = false;
                    }
                    Some(Err(error)) => {
                        rendered
                            .output
                            .push_str(&format!("\n[spill load failed: {error}]"));
                    }
                    None => {}
                }
                entry.text = crate::session::tool_card_text(&rendered, is_expanded);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spill_content_is_absent_until_exact_card_is_explicitly_loaded() {
        let entries = vec![Entry {
            role: glass_tui::Role::Bash,
            text: String::new(),
            turn: 7,
            tool_card: Some(glass_tui::ToolCard {
                call_id: "exact-call".into(),
                command: "echo output".into(),
                output: "inline preview".into(),
                status: "exit 0".into(),
                elapsed_ms: None,
                spill: Some("spill.log".into()),
                truncated: true,
            }),
        }];
        let expanded = HashSet::from([(7, "exact-call".to_string())]);
        let unloaded = render_entries(&entries, &expanded, &HashMap::new());
        assert!(!unloaded[0].text.contains("SPILL_ONLY_SEARCH_MARKER"));
        assert!(unloaded[0].text.contains("spill available"));

        let loaded = HashMap::from([(
            (7, "exact-call".to_string()),
            Ok(("spill-only-search-marker".to_string(), false)),
        )]);
        let rendered = render_entries(&entries, &expanded, &loaded);
        assert!(rendered[0].text.contains("spill-only-search-marker"));
        assert!(rendered[0].text.contains("loaded spill continuation"));
    }

    #[test]
    fn spill_search_text_is_visible_only_after_a_successful_explicit_load() {
        let entries = vec![Entry {
            role: glass_tui::Role::Bash,
            text: String::new(),
            turn: 2,
            tool_card: Some(glass_tui::ToolCard {
                call_id: "call-2".into(),
                command: "echo inline".into(),
                output: "inline output".into(),
                status: "exit 0".into(),
                elapsed_ms: None,
                spill: Some("spill.log".into()),
                truncated: true,
            }),
        }];
        let unloaded: HashMap<(usize, String), Result<(String, bool), String>> = HashMap::new();
        let query = "spill-only-search-marker";
        assert!(
            !entries[0]
                .tool_card
                .as_ref()
                .unwrap()
                .output
                .to_lowercase()
                .contains(query)
        );
        assert!(!unloaded.contains_key(&(2, "call-2".to_string())));
        assert!(
            !render_entries(&entries, &HashSet::new(), &unloaded)[0]
                .text
                .to_lowercase()
                .contains(query)
        );
        let loaded = HashMap::from([(
            (2, "call-2".to_string()),
            Ok(("spill-only-search-marker".to_string(), false)),
        )]);
        let rows = render_entries(
            &entries,
            &HashSet::from([(2, "call-2".to_string())]),
            &loaded,
        );
        assert!(rows[0].text.to_lowercase().contains(query));
    }
}
