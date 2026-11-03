//! A short-lived command modal. The alternate screen preserves terminal history.
use crate::{Edit, Editor};
use crossterm::event::{KeyCode, KeyEvent};
use std::io::{self, Write};

#[derive(Clone, Debug, Default)]
pub struct Command {
    pub name: String,
    pub hint: String,
    pub description: String,
    pub required_args: bool,
    pub label: String,
    pub aliases: Vec<String>,
    pub keywords: String,
    pub primary: bool,
}

pub struct Palette {
    commands: Vec<Command>,
    active: bool,
    selected: usize,
    saved_draft: Option<String>,
    dispatched_draft: Option<String>,
    owns_screen: bool,
    parents: Vec<(String, usize)>,
}

#[derive(Clone)]
pub struct Bookmark {
    query: String,
    selected: usize,
    parents: Vec<(String, usize)>,
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
            parents: Vec::new(),
        }
    }
    pub fn selected_details(&self, query: &str) -> Option<String> {
        let matches = self.matches(query);
        let command = matches.get(self.selected.min(matches.len().saturating_sub(1)))?;
        Some(format!(
            "/{} {}\n\n{}\n\nAliases: {}",
            command.name,
            command.hint,
            command.description,
            command.aliases.join(", ")
        ))
    }
    pub fn take_draft(&mut self) -> Option<String> {
        self.dispatched_draft.take()
    }
    pub fn bookmark(&self, editor: &Editor) -> Bookmark {
        Bookmark {
            query: editor.text().to_string(),
            selected: self.selected,
            parents: self.parents.clone(),
        }
    }
    pub fn restore_managed(&mut self, editor: &mut Editor, bookmark: Bookmark) -> io::Result<()> {
        self.open_managed(editor)?;
        editor.set_text(&bookmark.query);
        self.selected = bookmark.selected;
        self.parents = bookmark.parents;
        Ok(())
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
        self.parents.clear();
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
    fn has_children(&self, name: &str) -> bool {
        self.commands.iter().any(|command| {
            command
                .name
                .rsplit_once(' ')
                .is_some_and(|(parent, _)| parent == name)
        })
    }
    fn namespace(&self, query: &str) -> Option<(String, String)> {
        let raw = query.trim_start_matches('/');
        self.commands
            .iter()
            .filter(|command| self.has_children(&command.name))
            .flat_map(|command| {
                std::iter::once(&command.name)
                    .chain(command.aliases.iter())
                    .filter_map(move |name| {
                        raw.strip_prefix(&format!("{name} "))
                            .map(|search| (command.name.clone(), search.to_owned()))
                    })
            })
            .max_by_key(|(name, _)| name.len())
    }
    fn back(&mut self, editor: &mut Editor) {
        if let Some((query, selected)) = self.parents.pop() {
            editor.set_text(&query);
            self.selected = selected;
        } else if let Some((parent, _)) = self.namespace(editor.text()) {
            editor.set_text(&format!("/{parent}"));
            self.selected = 0;
        } else {
            self.cancel(editor);
        }
    }
    fn matches(&self, query: &str) -> Vec<&Command> {
        let namespace = self.namespace(query);
        let query = namespace
            .as_ref()
            .map(|(_, search)| search.as_str())
            .unwrap_or_else(|| query.trim_start_matches('/'))
            .trim()
            .to_ascii_lowercase();
        let rooted = self.commands.iter().any(|c| c.primary);
        let mut matches: Vec<(u8, &Command)> = self
            .commands
            .iter()
            .filter_map(|c| {
                let parent = c.name.rsplit_once(' ').map(|(parent, _)| parent);
                if parent != namespace.as_ref().map(|(name, _)| name.as_str()) {
                    return None;
                }
                if namespace.is_none() && query.is_empty() && rooted && !c.primary {
                    return None;
                }
                let name = namespace
                    .as_ref()
                    .and_then(|(parent, _)| c.name.strip_prefix(&format!("{parent} ")))
                    .unwrap_or(&c.name)
                    .to_ascii_lowercase();
                let aliases = c
                    .aliases
                    .iter()
                    .map(|a| a.to_ascii_lowercase())
                    .collect::<Vec<_>>();
                let haystack = format!(
                    "{name} {} {} {} {}",
                    aliases.join(" "),
                    c.label,
                    c.description,
                    c.keywords
                )
                .to_ascii_lowercase();
                let rank = if name == query || aliases.iter().any(|a| a == &query) {
                    0
                } else if name.starts_with(&query) || aliases.iter().any(|a| a.starts_with(&query))
                {
                    1
                } else if query == "*"
                    || query.split_whitespace().all(|word| haystack.contains(word))
                {
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
            KeyCode::Esc => self.back(editor),
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
                let selected = self
                    .matches(editor.text())
                    .get(self.selected)
                    .cloned()
                    .cloned();
                if let Some(command) = selected
                    .as_ref()
                    .filter(|command| self.has_children(&command.name))
                {
                    self.parents.push((editor.text().to_owned(), self.selected));
                    editor.set_text(&format!("/{} ", command.name));
                    self.selected = 0;
                    return None;
                }
                let first = editor
                    .text()
                    .trim_start_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or("");
                let exact_with_args = editor.text().contains(char::is_whitespace)
                    && self
                        .namespace(editor.text())
                        .is_none_or(|(_, search)| !search.trim().is_empty())
                    && self.commands.iter().any(|c| {
                        (!c.hint.is_empty() || c.required_args)
                            && (c.name == first || c.aliases.iter().any(|a| a == first))
                    });
                let exact_with_args = exact_with_args
                    && (self.namespace(editor.text()).is_none()
                        || self
                            .commands
                            .iter()
                            .any(|c| c.name == editor.text().trim_start_matches('/').trim())
                        || self.matches(editor.text()).is_empty());
                if exact_with_args && key.code == KeyCode::Enter {
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
                    let query = editor.text().trim_start_matches('/').trim();
                    let dispatch = command
                        .aliases
                        .iter()
                        .find(|alias| alias.as_str() == query)
                        .map(String::as_str)
                        .unwrap_or(&command.name);
                    editor.set_text(&format!("/{}{}", dispatch, if args { " " } else { "" }));
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
        let hint_row = usize::from(rooted_hint(width, height));
        let visible = height
            .saturating_sub(header + 1 + hint_row + usize::from(framed) * 2)
            .clamp(1, 8);
        let start = self.selected.saturating_sub(visible - 1);
        let rooted = self.commands.iter().any(|c| c.primary);
        let namespace = self.namespace(query);
        let mut lines = vec![
            if let Some((parent, _)) = &namespace {
                if width < 40 {
                    format!("{parent} · {}/{}", self.selected + 1, matches.len())
                } else {
                    format!(
                        "Commands › {parent} · {}/{}",
                        self.selected + 1,
                        matches.len()
                    )
                }
            } else if rooted && width >= 40 {
                "Commands · search or * browse all".into()
            } else {
                "Commands".to_string()
            },
            format!("❯ {query}"),
        ];
        if !compact {
            lines.push(String::new());
        }
        if matches.is_empty() {
            lines.push("No matching commands".into());
        }
        for (i, command) in matches.iter().enumerate().skip(start).take(visible) {
            let label = if command.label.is_empty() {
                &command.name
            } else {
                &command.label
            };
            let branch = if self.has_children(&command.name) {
                " ›"
            } else {
                ""
            };
            let route = command.name.rsplit(' ').next().unwrap_or(&command.name);
            let suffix = if width >= 65 && !command.label.is_empty() && !command.primary {
                format!("  /{route}")
            } else {
                String::new()
            };
            lines.push(format!(
                "{} {label}{branch}{suffix}",
                if i == self.selected { "›" } else { " " }
            ));
        }
        if hint_row > 0 {
            lines.push(
                matches
                    .get(self.selected)
                    .map(|c| c.description.clone())
                    .unwrap_or_else(|| "Type * to browse all".into()),
            );
        }
        let back = if namespace.is_some() || !self.parents.is_empty() {
            "back"
        } else {
            "close"
        };
        lines.push(
            if (if framed {
                width.saturating_sub(8).clamp(1, 76)
            } else {
                width
            }) < 18
            {
                format!("Esc {back}")
            } else if width < 50 {
                format!("Enter · Esc {back}")
            } else {
                format!("↑↓ select · Enter choose · F1 details · Esc {back}")
            },
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
fn rooted_hint(_width: usize, height: usize) -> bool {
    height >= 8
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
                ..Default::default()
            },
            Command {
                name: "model".into(),
                hint: "<name>".into(),
                description: "Switch model".into(),
                required_args: true,
                ..Default::default()
            },
            Command {
                name: "mine".into(),
                hint: "".into(),
                description: "Extension command".into(),
                required_args: false,
                ..Default::default()
            },
        ])
    }
    fn hierarchy() -> Palette {
        Palette::new(vec![
            Command {
                name: "agents".into(),
                aliases: vec!["teammates".into()],
                hint: "[action]".into(),
                ..Default::default()
            },
            Command {
                name: "agents view".into(),
                label: "Conversations".into(),
                ..Default::default()
            },
            Command {
                name: "agents status".into(),
                label: "Workflow status".into(),
                ..Default::default()
            },
            Command {
                name: "loop".into(),
                hint: "[interval] [task]".into(),
                ..Default::default()
            },
            Command {
                name: "loop list".into(),
                ..Default::default()
            },
        ])
    }
    fn press(p: &mut Palette, e: &mut Editor, code: KeyCode) -> Option<String> {
        p.key(e, KeyEvent::new(code, crossterm::event::KeyModifiers::NONE))
    }
    #[test]
    fn submenu_back_restores_selection_and_original_draft() {
        let mut p = hierarchy();
        let mut e = Editor::new();
        e.set_text("unfinished message");
        p.open_managed(&mut e).unwrap();
        e.set_text("/agents");
        assert_eq!(press(&mut p, &mut e, KeyCode::Enter), None);
        assert_eq!(e.text(), "/agents ");
        assert_eq!(
            p.matches(e.text())
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            ["agents view", "agents status"]
        );
        press(&mut p, &mut e, KeyCode::Down);
        let bookmark = p.bookmark(&e);
        assert_eq!(
            press(&mut p, &mut e, KeyCode::Enter).as_deref(),
            Some("/agents status")
        );
        e.set_text(&p.take_draft().unwrap());
        p.restore_managed(&mut e, bookmark).unwrap();
        assert_eq!(p.selected, 1);
        press(&mut p, &mut e, KeyCode::Esc);
        assert_eq!(e.text(), "/agents");
        press(&mut p, &mut e, KeyCode::Esc);
        assert_eq!(e.text(), "unfinished message");
        assert!(!p.active());
    }
    #[test]
    fn namespaces_support_aliases_search_and_legacy_arguments() {
        let mut p = hierarchy();
        let mut e = Editor::new();
        e.set_text("/teammates");
        p.open_managed(&mut e).unwrap();
        press(&mut p, &mut e, KeyCode::Tab);
        assert_eq!(e.text(), "/agents ");
        e.set_text("/agents stat");
        assert_eq!(
            press(&mut p, &mut e, KeyCode::Enter).as_deref(),
            Some("/agents status")
        );
        e.set_text("/loop");
        p.open_managed(&mut e).unwrap();
        e.set_text("/loop 30m review changes");
        assert_eq!(
            press(&mut p, &mut e, KeyCode::Enter).as_deref(),
            Some("/loop 30m review changes")
        );
        assert!(!p.matches("/*").iter().any(|c| c.name.contains(' ')));
    }
    #[test]
    fn submenu_frame_is_readable_at_phone_and_desktop_sizes() {
        let p = hierarchy();
        for (w, h) in [(20, 8), (40, 24), (80, 30)] {
            let frame = p.frame("/agents ", w, h);
            assert!(frame.contains("Esc back"), "{frame}");
            assert!(frame.contains("Conversations"), "{frame}");
        }
    }
    #[test]
    fn roots_intent_aliases_and_argument_dispatch() {
        let mut p = Palette::new(vec![
            Command {
                name: "work".into(),
                label: "Work".into(),
                primary: true,
                ..Default::default()
            },
            Command {
                name: "outputs".into(),
                aliases: vec!["details".into()],
                keywords: "copy code".into(),
                ..Default::default()
            },
            Command {
                name: "compact".into(),
                hint: "[focus]".into(),
                aliases: vec!["compaction".into()],
                ..Default::default()
            },
        ]);
        assert_eq!(p.matches("/").len(), 1);
        assert_eq!(p.matches("/copy code")[0].name, "outputs");
        assert_eq!(p.matches("/*").len(), 3);
        let mut editor = Editor::new();
        editor.set_text("/details");
        p.open_managed(&mut editor).unwrap();
        assert_eq!(
            p.key(
                &mut editor,
                KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE)
            )
            .as_deref(),
            Some("/details")
        );
        editor.set_text("/compaction preserve task");
        // Explicit arguments reach the original command unchanged.
        assert_eq!(
            p.key(
                &mut editor,
                KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE)
            )
            .as_deref(),
            Some("/compaction preserve task")
        );
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
                ..Default::default()
            },
            Command {
                name: "review".into(),
                hint: "".into(),
                description: "Task review".into(),
                required_args: false,
                ..Default::default()
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
        palette.key(
            &mut editor,
            KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE),
        );
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
    fn root_footer_says_close_at_mobile_and_desktop_sizes() {
        let p = palette();
        for (width, height) in [(20, 8), (24, 12), (40, 24), (80, 30)] {
            let frame = p.frame("/", width, height);
            assert!(frame.contains("Esc close"), "{width}x{height}: {frame}");
            assert!(!frame.contains("Esc back"));
        }
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
