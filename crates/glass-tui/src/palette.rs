//! A short-lived command modal. The alternate screen preserves terminal history.
use crate::{Edit, Editor};
use crossterm::event::{KeyCode, KeyEvent};
use std::io::{self, Write};

#[derive(Clone, Debug)]
pub struct Command {
    pub name: String,
    pub hint: String,
    pub description: String,
    pub required_args: bool,
}

pub struct Palette {
    commands: Vec<Command>,
    active: bool,
    selected: usize,
    saved_draft: Option<String>,
    dispatched_draft: Option<String>,
    owns_screen: bool,
}

impl Palette {
    pub fn new(commands: Vec<Command>) -> Self {
        Self {
            commands,
            active: false,
            selected: 0,
            saved_draft: None,
            dispatched_draft: None,
            owns_screen: false,
        }
    }
    pub fn take_draft(&mut self) -> Option<String> {
        self.dispatched_draft.take()
    }
    pub fn active(&self) -> bool {
        self.active
    }
    pub fn open(&mut self, editor: &mut Editor) -> io::Result<()> {
        self.open_with_screen(editor, true)
    }
    /// Open inside an application-managed screen without nesting an alternate buffer.
    pub fn open_managed(&mut self, editor: &mut Editor) -> io::Result<()> {
        self.open_with_screen(editor, false)
    }
    fn open_with_screen(&mut self, editor: &mut Editor, own_screen: bool) -> io::Result<()> {
        if self.active {
            return Ok(());
        }
        self.saved_draft =
            if editor.text().starts_with('/') && !editor.text().contains(char::is_whitespace) {
                None
            } else {
                let draft = editor.text().to_string();
                editor.set_text("/");
                Some(draft)
            };
        self.selected = 0;
        self.active = true;
        self.owns_screen = own_screen;
        if own_screen {
            // Standalone use gets a disposable screen; managed apps already own one.
            write!(io::stdout(), "\x1b[?1049h\x1b[?25l")?;
            io::stdout().flush()?;
        }
        Ok(())
    }
    pub fn close(&mut self) {
        if self.active {
            self.active = false;
            if self.owns_screen {
                let _ = write!(io::stdout(), "\x1b[?1049l\x1b[?25h");
                let _ = io::stdout().flush();
            }
            self.owns_screen = false;
        }
        self.saved_draft = None;
    }
    pub fn cancel(&mut self, editor: &mut Editor) {
        if let Some(draft) = self.saved_draft.take() {
            editor.set_text(&draft);
        } else if self.active && editor.text().starts_with('/') {
            editor.clear();
        }
        self.close();
    }
    fn matches(&self, query: &str) -> Vec<&Command> {
        let query = query
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        // Exact and prefix matches outrank bare substrings, so one
        // command whose name merely contains another's cannot shadow
        // it: `/review` must select `review`, not `code-review`.
        // The sort is stable, so catalogue order holds within a rank.
        let mut matches: Vec<(u8, &Command)> = self
            .commands
            .iter()
            .filter_map(|c| {
                let name = c.name.to_ascii_lowercase();
                let rank = if name == query {
                    0
                } else if name.starts_with(&query) {
                    1
                } else if name.contains(&query) {
                    2
                } else {
                    return None;
                };
                Some((rank, c))
            })
            .collect();
        matches.sort_by_key(|(rank, _)| *rank);
        matches.into_iter().map(|(_, c)| c).collect()
    }
    pub fn changed(&mut self, editor: &Editor) {
        self.selected = 0;
        if !editor.text().starts_with('/') {
            self.close();
        }
    }
    /// Returns a submitted command only when Enter selects a command without required arguments.
    pub fn key(&mut self, editor: &mut Editor, key: KeyEvent) -> Option<String> {
        match key.code {
            KeyCode::Esc => self.cancel(editor),
            KeyCode::Up | KeyCode::Down => {
                let len = self.matches(editor.text()).len();
                if len > 0 {
                    self.selected = if key.code == KeyCode::Up {
                        (self.selected + len - 1) % len
                    } else {
                        (self.selected + 1) % len
                    };
                }
            }
            KeyCode::Enter | KeyCode::Tab => {
                if editor.text().contains(char::is_whitespace) && key.code == KeyCode::Enter {
                    self.dispatched_draft = self.saved_draft.take();
                    self.close();
                    if let Edit::Submit(text) = editor.key(key) {
                        return Some(text);
                    }
                } else {
                    let command = self
                        .matches(editor.text())
                        .get(self.selected)
                        .cloned()
                        .cloned()?;
                    let args = key.code == KeyCode::Tab && !command.hint.is_empty();
                    editor.set_text(&format!("/{}{}", command.name, if args { " " } else { "" }));
                    if key.code == KeyCode::Enter {
                        self.dispatched_draft = self.saved_draft.take();
                        self.close();
                        if let Edit::Submit(text) = editor.key(key) {
                            return Some(text);
                        }
                    }
                }
            }
            _ => {
                let before = editor.text().to_string();
                editor.key(key);
                if before != editor.text() {
                    self.changed(editor);
                }
            }
        }
        None
    }
    pub fn render(&self, query: &str, width: u16, height: u16) {
        let mut out = io::stdout().lock();
        let _ = out.write_all(self.frame(query, width, height).as_bytes());
        let _ = out.flush();
    }
    fn frame(&self, query: &str, width: u16, height: u16) -> String {
        let width = width.max(1) as usize;
        let height = height.max(1) as usize;
        let matches = self.matches(query);
        let framed = width >= 24 && height >= 12;
        let compact = height < 14;
        let header = if compact { 2 } else { 3 };
        let visible = height
            .saturating_sub(header + 1 + usize::from(framed) * 2)
            .clamp(1, 8);
        let start = self.selected.saturating_sub(visible - 1);
        let mut lines = vec!["Commands".to_string(), format!("❯ {query}")];
        if !compact {
            lines.push(String::new());
        }
        if matches.is_empty() {
            lines.push("No matching commands".into());
        }
        for (i, command) in matches.iter().enumerate().skip(start).take(visible) {
            lines.push(format!(
                "{} /{} {} — {}",
                if i == self.selected { "›" } else { " " },
                command.name,
                command.hint,
                command.description
            ));
        }
        lines.push(
            if compact {
                "↑↓ Enter · Esc back"
            } else {
                "↑↓ select · Enter choose · Tab complete · Esc close"
            }
            .into(),
        );
        let inner = width.saturating_sub(8).clamp(1, 76);

        let box_width = if framed { inner + 4 } else { width };
        let count = lines.len() + if framed { 2 } else { 0 };
        let top = if framed {
            height.saturating_sub(count) / 2 + 1
        } else {
            1
        };
        let left = if framed {
            width.saturating_sub(box_width) / 2 + 1
        } else {
            1
        };
        let mut out = String::from("\x1b[?2026h\x1b[2J\x1b[H\x1b[?25l");
        let mut row = top;
        if framed {
            out.push_str(&format!(
                "\x1b[{row};{left}H{}╭{}╮",
                crate::theme::ansi(crate::theme::Tone::Border),
                "─".repeat(inner + 2)
            ));
            row += 1;
        }
        for (i, line) in lines
            .iter()
            .enumerate()
            .take(height.saturating_sub(row - 1 + usize::from(framed)))
        {
            let selected = i >= header
                && !matches.is_empty()
                && i - header + start == self.selected
                && i - header < visible;
            let limit = if framed { inner } else { width };
            let plain = crate::text::clip(line, limit);
            let text = if framed {
                format!(
                    "│ {}{} │",
                    plain,
                    " ".repeat(inner.saturating_sub(crate::text::width(&plain)))
                )
            } else {
                plain
            };
            out.push_str(&format!(
                "\x1b[{row};{left}H{}{}\x1b[0m",
                if selected {
                    crate::theme::selected()
                } else if i == 0 {
                    format!("{}\x1b[1m", crate::theme::accent())
                } else {
                    crate::theme::ansi(crate::theme::Tone::Body)
                },
                text
            ));
            row += 1;
        }
        if framed && row <= height {
            out.push_str(&format!(
                "\x1b[{row};{left}H{}╰{}╯",
                crate::theme::ansi(crate::theme::Tone::Border),
                "─".repeat(inner + 2)
            ));
        }
        out.push_str("\x1b[?2026l");
        crate::theme::neutralize(&out)
    }
}
impl Drop for Palette {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn palette() -> Palette {
        Palette::new(vec![
            Command {
                name: "help".into(),
                hint: "".into(),
                description: "Help".into(),
                required_args: false,
            },
            Command {
                name: "model".into(),
                hint: "<name>".into(),
                description: "Switch model".into(),
                required_args: true,
            },
            Command {
                name: "mine".into(),
                hint: "".into(),
                description: "Extension command".into(),
                required_args: false,
            },
        ])
    }
    #[test]
    fn filtering_includes_extension_commands() {
        let p = palette();
        assert_eq!(p.matches("/mi")[0].name, "mine");
        assert!(p.matches("/xyz").is_empty());
    }
    #[test]
    fn exact_and_prefix_matches_outrank_substrings() {
        // `review` and `code-review` both contain `review`; the
        // exact match must win, in both directions.
        let mut p = Palette::new(vec![
            Command {
                name: "code-review".into(),
                hint: "[max]".into(),
                description: "Review the diff".into(),
                required_args: false,
            },
            Command {
                name: "review".into(),
                hint: "".into(),
                description: "Task review".into(),
                required_args: false,
            },
        ]);
        assert_eq!(p.matches("/review")[0].name, "review");
        assert_eq!(p.matches("/code")[0].name, "code-review");
        assert_eq!(p.matches("/code-r")[0].name, "code-review");
        let mut e = Editor::new();
        e.set_text("/review");
        assert_eq!(
            p.key(&mut e, KeyCode::Enter.into()).as_deref(),
            Some("/review")
        );
    }
    #[test]
    fn enter_submits_command_to_its_guided_flow() {
        let mut p = palette();
        let mut e = Editor::new();
        e.set_text("/mod");
        assert_eq!(
            p.key(&mut e, KeyCode::Enter.into()).as_deref(),
            Some("/model")
        );
        assert_eq!(e.text(), "");
    }
    #[test]
    fn selection_can_submit_and_navigation_wraps() {
        let mut p = palette();
        let mut e = Editor::new();
        e.set_text("/");
        p.key(&mut e, KeyCode::Up.into());
        assert_eq!(p.selected, 2);
        assert_eq!(
            p.key(&mut e, KeyCode::Enter.into()).as_deref(),
            Some("/mine")
        );
    }
    #[test]
    fn empty_results_do_not_submit_a_prompt() {
        let mut p = palette();
        let mut e = Editor::new();
        e.set_text("/xyz");
        assert!(p.key(&mut e, KeyCode::Enter.into()).is_none());
        assert_eq!(e.text(), "/xyz");
    }
    #[test]
    fn escape_clears_a_slash_query() {
        let mut palette = Palette::new(vec![]);
        let mut editor = Editor::new();
        editor.set_text("/help");
        palette.open_managed(&mut editor).unwrap();
        palette.key(&mut editor, KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE));
        assert!(!palette.active());
        assert_eq!(editor.text(), "");
    }

    #[test]
    fn cancel_preserves_a_noncommand_draft() {
        let mut p = palette();
        p.saved_draft = Some("unfinished prompt".into());
        let mut e = Editor::new();
        e.set_text("/hel");
        p.cancel(&mut e);
        assert_eq!(e.text(), "unfinished prompt");
    }
    #[test]
    fn frame_handles_small_terminals_without_losing_the_query() {
        let p = palette();
        let frame = p.frame("/mi", 20, 8);
        assert!(frame.contains("/mi"));
        for w in [1, 8, 20, 80] {
            for h in [1, 3, 8, 24] {
                assert!(p.frame("/", w, h).ends_with("\x1b[?2026l"));
            }
        }
    }
}
