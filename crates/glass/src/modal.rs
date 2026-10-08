//! Reusable modal flows, sharing the application's single input reader.
use crate::input::KeyReader;
use glass_tui::{
    Edit, Editor,
    crossterm::event::{Event, KeyCode, KeyModifiers},
};
use std::io::Write;
use std::time::Duration;

pub struct Screen {
    managed: bool,
}
impl Screen {
    pub fn new() -> Self {
        let mut out = std::io::stdout();
        let managed = crate::surface::managed();
        if managed {
            crate::surface::overlay(true).ok();
            let _ = write!(out, "\x1b[r\x1b[2J\x1b[H\x1b[?25l");
        } else {
            let _ = write!(out, "\x1b[?1049h\x1b[?25l");
        }
        let _ = out.flush();
        Self { managed }
    }
    pub fn draw(&self, title: &str, lines: &[String], footer: &str, offset: usize) {
        let (w, h) = glass_tui::crossterm::terminal::size().unwrap_or((80, 24));
        let w = usize::from(w.max(1));
        let h = usize::from(h.max(1));
        let framed = w >= 24 && h >= 12;
        let width = if framed {
            w.saturating_sub(6).clamp(1, 100)
        } else {
            w
        };
        let capacity = if framed { h.saturating_sub(4) } else { h };
        let left = if framed {
            w.saturating_sub(width + 4) / 2 + 1
        } else {
            1
        };
        let mut rows = vec![title.to_string()];
        if capacity > 10 {
            rows.push(String::new());
        }
        rows.extend(
            lines
                .iter()
                .skip(offset)
                .take(capacity.saturating_sub(rows.len() + 1))
                .cloned(),
        );
        if rows.len() < capacity {
            rows.push(footer.into());
        }
        let top = if framed {
            h.saturating_sub(rows.len() + 2) / 2 + 1
        } else {
            1
        };
        let mut out = glass_tui::theme::TerminalWriter(std::io::stdout().lock());
        let _ = write!(out, "\x1b[?2026h\x1b[2J\x1b[H\x1b[?25l");
        if framed {
            let _ = write!(
                out,
                "\x1b[{top};{left}H{}╭{}╮\x1b[0m",
                glass_tui::theme::ansi(glass_tui::theme::Tone::Border),
                "─".repeat(width + 2)
            );
        }
        for (i, line) in rows.iter().enumerate().take(capacity) {
            let clean = glass_tui::text::clip(line, width);
            let padding = width.saturating_sub(glass_tui::text::width(&clean));
            let row = top + i + usize::from(framed);
            let selected = line.starts_with("› ");
            let style = if i == 0 {
                glass_tui::theme::accent()
            } else if selected {
                glass_tui::theme::selected()
            } else if let Some(frame) = line
                .chars()
                .next()
                .and_then(|glyph| ['◐', '◓', '◑', '◒'].iter().position(|c| *c == glyph))
            {
                glass_tui::theme::activity_color(frame)
            } else if i == rows.len() - 1 {
                glass_tui::theme::ansi(glass_tui::theme::Tone::Muted)
            } else {
                glass_tui::theme::ansi(glass_tui::theme::Tone::Body)
            };
            let _ = write!(out, "\x1b[{row};{left}H{style}");
            if framed {
                let _ = write!(out, "│ {}{} │", clean, " ".repeat(padding));
            } else {
                let _ = write!(out, "{clean}");
            }
            let _ = write!(out, "\x1b[0m");
        }
        if framed {
            let _ = write!(
                out,
                "\x1b[{};{left}H{}╰{}╯\x1b[0m",
                top + rows.len() + 1,
                glass_tui::theme::ansi(glass_tui::theme::Tone::Border),
                "─".repeat(width + 2)
            );
        }
        let _ = write!(out, "\x1b[?2026l");
        let _ = out.flush();
    }
}
impl Drop for Screen {
    fn drop(&mut self) {
        if self.managed {
            crate::surface::overlay(false).ok();
            let _ = write!(std::io::stdout(), "\x1b[?25h");
        } else {
            let _ = write!(std::io::stdout(), "\x1b[?1049l\x1b[?25h");
        }
        let _ = std::io::stdout().flush();
    }
}
fn cancelled(code: KeyCode, mods: KeyModifiers) -> bool {
    code == KeyCode::Esc
        || (mods.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('c' | 'd')))
}
pub fn choose(keys: &KeyReader, title: &str, note: &str, items: &[String]) -> Option<usize> {
    choose_search(keys, title, note, items, items)
}

/// Search recorded content separately from the concise displayed labels.
pub fn choose_search(
    keys: &KeyReader,
    title: &str,
    note: &str,
    items: &[String],
    searchable: &[String],
) -> Option<usize> {
    let mut screen = Screen::new();
    let mut query = Editor::new();
    let mut selected = 0usize;
    loop {
        let needle = query.text().to_lowercase();
        let matches: Vec<_> = items
            .iter()
            .enumerate()
            .filter(|(i, s)| {
                searchable
                    .get(*i)
                    .unwrap_or(s)
                    .to_lowercase()
                    .contains(&needle)
            })
            .collect();
        selected = selected.min(matches.len().saturating_sub(1));
        let (w, h) = glass_tui::crossterm::terminal::size().unwrap_or((80, 24));
        let compact = h < 14;
        let framed = w >= 24 && h >= 12;
        let overhead = if compact { 3 } else { 8 } + if framed { 4 } else { 0 };
        let visible = usize::from(h).saturating_sub(overhead).max(1);
        let start = selected.saturating_sub(visible - 1);
        let mut lines = if compact {
            vec![format!("Find: {}", query.text())]
        } else {
            vec![
                note.into(),
                format!("Search: {}", query.text()),
                String::new(),
            ]
        };
        lines.extend(
            matches
                .iter()
                .enumerate()
                .skip(start)
                .take(visible)
                .map(|(i, (_, s))| format!("{} {s}", if i == selected { "›" } else { " " })),
        );
        if matches.is_empty() {
            lines.push("No matches. Change your search.".into());
        }
        if !compact {
            lines.push(format!("{} choices", matches.len()));
        }
        screen.draw(
            title,
            &lines,
            if compact {
                "Enter · Esc · F1 info"
            } else {
                "↑↓ select · Enter choose · Esc back · F1 details"
            },
            0,
        );
        match keys.next_blocking()? {
            Event::Key(k) if cancelled(k.code, k.modifiers) => return None,
            Event::Key(k) => match k.code {
                KeyCode::Up if !matches.is_empty() => {
                    selected = (selected + matches.len() - 1) % matches.len()
                }
                KeyCode::Down if !matches.is_empty() => selected = (selected + 1) % matches.len(),
                KeyCode::F(1) | KeyCode::Right => {
                    if let Some((_, item)) = matches.get(selected) {
                        drop(screen);
                        info(keys, "Selection details", item);
                        screen = Screen::new();
                    }
                }
                KeyCode::Enter => {
                    if let Some((i, _)) = matches.get(selected) {
                        return Some(*i);
                    }
                }
                _ => {
                    query.key(k);
                    selected = 0;
                }
            },
            Event::Paste(s) => {
                query.insert_str(&s.replace(['\r', '\n'], ""));
                selected = 0;
            }
            _ => {}
        }
    }
}
pub fn field(
    keys: &KeyReader,
    title: &str,
    label: &str,
    initial: &str,
    secret: bool,
) -> Option<String> {
    validated_field(keys, title, label, initial, secret, |_| Ok(()))
}
pub fn validated_field(
    keys: &KeyReader,
    title: &str,
    label: &str,
    initial: &str,
    secret: bool,
    validate: impl Fn(&str) -> Result<(), String>,
) -> Option<String> {
    let screen = Screen::new();
    let mut editor = Editor::new();
    editor.set_text(initial);
    let mut error = String::new();
    loop {
        let shown = if secret {
            "•".repeat(editor.text().chars().count())
        } else {
            editor.text().into()
        };
        let width = usize::from(
            glass_tui::crossterm::terminal::size()
                .unwrap_or((80, 24))
                .0
                .saturating_sub(10),
        )
        .max(1);
        let caret = if secret {
            editor.text()[..editor.cursor()].chars().count()
        } else {
            editor.cursor()
        };
        let mut shown = shown.to_string();
        // Masked text has multibyte bullets, so convert the character offset to bytes.
        let caret = if secret {
            shown
                .char_indices()
                .nth(caret)
                .map_or(shown.len(), |(i, _)| i)
        } else {
            caret
        };
        shown.insert(caret, '│');
        let before = glass_tui::text::width(&shown[..caret]);
        let skip = before.saturating_sub(width.saturating_sub(1));
        let mut used = 0;
        let tail = shown
            .chars()
            .skip_while(|c| {
                let w = glass_tui::text::width(&c.to_string());
                let skip_char = used + w <= skip;
                used += w;
                skip_char
            })
            .collect::<String>();
        let shown = glass_tui::text::clip(&tail, width);
        screen.draw(
            title,
            &[
                label.into(),
                String::new(),
                format!("❯ {shown}"),
                error.clone(),
            ],
            "Enter continue · Esc back",
            0,
        );
        match keys.next_blocking()? {
            Event::Key(k) if cancelled(k.code, k.modifiers) => return None,
            Event::Key(k) => {
                if let Edit::Submit(s) = editor.key(k) {
                    let value = s.trim();
                    match if value.is_empty() {
                        Err("Enter a value.".into())
                    } else {
                        validate(value)
                    } {
                        Ok(()) => return Some(value.into()),
                        Err(e) => {
                            error = e;
                            editor.set_text(&s);
                        }
                    }
                }
            }
            Event::Paste(s) => editor.insert_str(&s.replace(['\r', '\n'], "")),
            _ => {}
        }
    }
}
pub fn info(keys: &KeyReader, title: &str, text: &str) {
    let screen = Screen::new();
    let mut offset = 0usize;
    loop {
        let (w, h) = glass_tui::crossterm::terminal::size().unwrap_or((80, 24));
        let framed = w >= 24 && h >= 12;
        let width = if framed {
            usize::from(w.saturating_sub(6)).clamp(1, 100)
        } else {
            usize::from(w.max(1))
        };
        let capacity = usize::from(if framed { h.saturating_sub(4) } else { h });
        let page = capacity
            .saturating_sub(2 + usize::from(capacity > 10))
            .max(1);
        let lines = glass_tui::text::wrap_words(&glass_tui::markdown::sanitize(text), width);
        offset = offset.min(lines.len().saturating_sub(page));
        screen.draw(
            title,
            &lines,
            "↑↓ scroll · PgUp/PgDn page · Enter/Esc close",
            offset,
        );
        match keys.next_blocking() {
            Some(Event::Key(k)) => match k.code {
                KeyCode::Enter | KeyCode::Esc => return,
                _ if cancelled(k.code, k.modifiers) => return,
                KeyCode::Down => offset = (offset + 1).min(lines.len().saturating_sub(1)),
                KeyCode::Up => offset = offset.saturating_sub(1),
                KeyCode::PageDown => offset = (offset + page).min(lines.len().saturating_sub(1)),
                KeyCode::PageUp => offset = offset.saturating_sub(page),
                KeyCode::Home => offset = 0,
                KeyCode::End => offset = lines.len().saturating_sub(page),
                _ => {}
            },
            None => return,
            _ => {}
        }
    }
}
pub fn work<T: Send + 'static>(
    keys: &KeyReader,
    title: &str,
    task: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Option<Result<T, String>> {
    let screen = Screen::new();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(task());
    });
    let mut tick = 0;
    let started = std::time::Instant::now();
    loop {
        match rx.try_recv() {
            Ok(result) => return Some(result),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Some(Err("connection worker stopped".into()));
            }
            _ => {}
        }
        screen.draw(
            title,
            &[format!(
                "{} Connecting · {:.1}s",
                glass_tui::theme::frame(tick),
                started.elapsed().as_secs_f32()
            )],
            "Esc cancel",
            0,
        );
        tick += 1;
        if let Some(Event::Key(k)) = keys.next_timeout(Duration::from_millis(125))
            && cancelled(k.code, k.modifiers)
        {
            return None;
        }
    }
}

/// Cancellable local worker. Cancellation does not undo a provider's remote work.
pub fn task<T: Send + 'static>(
    keys: &KeyReader,
    title: &str,
    task: impl FnOnce(
        std::sync::Arc<std::sync::atomic::AtomicBool>,
        std::sync::Arc<std::sync::Mutex<String>>,
    ) -> Result<T, String>
    + Send
    + 'static,
) -> Option<Result<T, String>> {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    let cancel = Arc::new(AtomicBool::new(false));
    struct Cancel(Arc<AtomicBool>);
    impl Drop for Cancel {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let _cancel = Cancel(cancel.clone());
    let status = Arc::new(Mutex::new("Working…".to_string()));
    let worker_status = status.clone();
    let screen = Screen::new();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(task(cancel, worker_status));
    });
    let mut tick = 0;
    let started = std::time::Instant::now();
    loop {
        match rx.try_recv() {
            Ok(result) => return Some(result),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Some(Err("Worker stopped unexpectedly".into()));
            }
            _ => {}
        }
        let text = status
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|_| "Working…".into());
        screen.draw(
            title,
            &[format!(
                "{} {text} · {:.1}s",
                glass_tui::theme::frame(tick),
                started.elapsed().as_secs_f32()
            )],
            "Esc cancel",
            0,
        );
        tick += 1;
        if let Some(Event::Key(k)) = keys.next_timeout(Duration::from_millis(125))
            && cancelled(k.code, k.modifiers)
        {
            return None;
        }
    }
}

pub fn markdown(keys: &KeyReader, title: &str, text: &str) {
    use glass_tui::ratatui::{
        Terminal,
        backend::CrosstermBackend,
        layout::{Constraint, Direction, Layout},
        widgets::Paragraph,
    };
    let _screen = Screen::new();
    let Ok(mut terminal) = Terminal::new(CrosstermBackend::new(glass_tui::theme::TerminalWriter(
        std::io::stdout(),
    ))) else {
        return;
    };
    let mut offset = 0usize;
    loop {
        let mut count = 0usize;
        let mut height = 1usize;
        if terminal
            .draw(|frame| {
                let areas = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(1),
                        Constraint::Min(1),
                        Constraint::Length(1),
                    ])
                    .split(frame.area());
                let lines = glass_tui::markdown::lines(text, areas[1].width as usize);
                count = lines.len();
                height = areas[1].height.max(1) as usize;
                offset = offset.min(count.saturating_sub(height));
                frame.render_widget(
                    Paragraph::new(glass_tui::text::clip(title, areas[0].width as usize)),
                    areas[0],
                );
                frame.render_widget(
                    Paragraph::new(
                        lines
                            .into_iter()
                            .skip(offset)
                            .take(height)
                            .collect::<Vec<_>>(),
                    ),
                    areas[1],
                );
                frame.render_widget(
                    Paragraph::new(glass_tui::text::clip(
                        "↑↓ scroll · PgUp/PgDn · Esc close",
                        areas[2].width as usize,
                    )),
                    areas[2],
                );
            })
            .is_err()
        {
            return;
        }
        match keys.next_blocking() {
            Some(Event::Key(k)) if cancelled(k.code, k.modifiers) => return,
            Some(Event::Key(k)) => match k.code {
                KeyCode::Up => offset = offset.saturating_sub(1),
                KeyCode::Down => offset = (offset + 1).min(count.saturating_sub(height)),
                KeyCode::PageUp => offset = offset.saturating_sub(height),
                KeyCode::PageDown => offset = (offset + height).min(count.saturating_sub(height)),
                KeyCode::Home => offset = 0,
                KeyCode::End => offset = count.saturating_sub(height),
                _ => {}
            },
            None => return,
            _ => {}
        }
    }
}
