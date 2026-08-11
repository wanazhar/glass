//! Focused browser-only terminal workspace.
//!
//! The complete development cockpit is owned by `glass-dev`. This module keeps
//! the independently installable browser product responsive and structured
//! first without importing project, process, agent, or debugger contracts.

use crossterm::event::{
    self, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, Event, KeyCode, KeyEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use std::io::{self, IsTerminal};
use std::time::Duration;

use crate::browser::session::{BrowserResult, BrowserSession, SessionOptions};
use crate::cli::args::Cli;

const PHONE_MAX_COLUMNS: u16 = 72;
const COMPACT_MAX_COLUMNS: u16 = 109;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkspaceMode {
    #[default]
    Browser,
    Semantic,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponsiveClass {
    Phone,
    Compact,
    Desktop,
}

impl ResponsiveClass {
    const fn for_width(width: u16) -> Self {
        if width <= PHONE_MAX_COLUMNS {
            Self::Phone
        } else if width <= COMPACT_MAX_COLUMNS {
            Self::Compact
        } else {
            Self::Desktop
        }
    }
}

struct BrowserTui {
    mode: WorkspaceMode,
    command: String,
    status: String,
    page: String,
    session: Option<BrowserSession>,
}

impl BrowserTui {
    fn new() -> Self {
        Self {
            mode: WorkspaceMode::Browser,
            command: String::new(),
            status: "Ready · structured observation is the default".into(),
            page: "No browser session. Enter `navigate https://example.com`.".into(),
            session: None,
        }
    }

    async fn submit(&mut self, cli: &Cli) -> BrowserResult<bool> {
        let command = std::mem::take(&mut self.command);
        let command = command.trim();
        if command.is_empty() {
            return Ok(false);
        }
        if matches!(command, "quit" | "exit" | "q") {
            return Ok(true);
        }
        if command == "help" {
            self.mode = WorkspaceMode::Help;
            self.status = "Browser TUI command reference".into();
            return Ok(false);
        }
        if command == "semantic" {
            self.mode = WorkspaceMode::Semantic;
            return self.observe().await.map(|_| false);
        }
        if command == "observe" {
            self.mode = WorkspaceMode::Browser;
            return self.observe().await.map(|_| false);
        }
        if command == "screenshot" {
            self.status =
                "Screenshots are explicit: use `glass screenshot PATH` outside the TUI".into();
            return Ok(false);
        }
        if let Some(url) = command.strip_prefix("navigate ") {
            self.ensure_session(cli).await?;
            let page = self
                .session
                .as_ref()
                .expect("session initialized")
                .navigate(url.trim())
                .await?;
            self.page = format!("{}\n{}", page.title, page.url);
            self.status = "Navigation complete · run `observe` for structured evidence".into();
            return Ok(false);
        }
        self.status = format!("Unknown command `{command}` · enter `help`");
        Ok(false)
    }

    async fn ensure_session(&mut self, cli: &Cli) -> BrowserResult<()> {
        if self.session.is_none() {
            let options = SessionOptions {
                port: cli.port,
                chrome_path: cli.chrome_path.clone(),
                profile: cli.profile.clone(),
                incognito: cli.incognito,
                attach: cli.attach,
                target_id: cli.target_id.clone(),
                frame_id: cli.frame_id.clone(),
                headed: cli.headed,
                interaction_mode: cli.interaction,
                audit: cli.audit,
                policy: None,
            };
            self.session = Some(BrowserSession::start(&options).await?);
        }
        Ok(())
    }

    async fn observe(&mut self) -> BrowserResult<()> {
        let Some(session) = self.session.as_ref() else {
            self.status = "Navigate first; observation never starts Chrome implicitly".into();
            return Ok(());
        };
        let observation = session.observe().await?;
        self.page = serde_json::to_string_pretty(&observation)?;
        self.status = format!(
            "Structured observation · revision {}",
            observation.accessibility.revision
        );
        Ok(())
    }

    async fn close(&mut self) -> BrowserResult<()> {
        if let Some(session) = self.session.take() {
            session.close().await?;
        }
        Ok(())
    }
}

pub async fn run_tui(cli: &Cli) -> BrowserResult<()> {
    run_tui_for_product(cli, false).await
}

/// Run the focused browser TUI.
///
/// The argument remains for source compatibility with callers from before the
/// product split; requesting development mode now fails explicitly.
pub async fn run_tui_for_product(cli: &Cli, development_enabled: bool) -> BrowserResult<()> {
    if development_enabled {
        return Err(
            "development TUI surfaces belong to the `glass` binary from `glass-dev`".into(),
        );
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("browser TUI requires an interactive terminal".into());
    }
    let mut terminal = TerminalGuard::enter()?;
    let mut app = BrowserTui::new();
    let result: BrowserResult<()> = loop {
        terminal.terminal.draw(|frame| draw(frame, &app))?;
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('c')
                    if key
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::CONTROL) =>
                {
                    break Ok(());
                }
                KeyCode::Char('q') => break Ok(()),
                KeyCode::Esc => app.command.clear(),
                KeyCode::Enter => match app.submit(cli).await {
                    Ok(true) => break Ok(()),
                    Ok(false) => {}
                    Err(error) => app.status = format!("Command failed: {error}"),
                },
                KeyCode::Backspace => {
                    app.command.pop();
                }
                KeyCode::Char(character) => app.command.push(character),
                _ => {}
            }
        }
    };
    let close = app.close().await;
    result?;
    close
}

fn draw(frame: &mut ratatui::Frame<'_>, app: &BrowserTui) {
    let area = frame.area();
    let class = ResponsiveClass::for_width(area.width);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(if class == ResponsiveClass::Phone {
                4
            } else {
                3
            }),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(area);
    let title = match class {
        ResponsiveClass::Phone => "GLASS BROWSER · PHONE",
        ResponsiveClass::Compact => "GLASS BROWSER · COMPACT",
        ResponsiveClass::Desktop => "GLASS BROWSER · DESKTOP",
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                title,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(app.status.as_str()),
        ])
        .block(Block::default().borders(Borders::ALL)),
        rows[0],
    );
    let content = if app.mode == WorkspaceMode::Help {
        "navigate URL  start/navigate browser\nobserve       structured accessibility evidence\nsemantic      structured semantic view\nscreenshot    explains explicit capture path\nquit          close owned browser and exit"
    } else {
        app.page.as_str()
    };
    draw_content(frame, rows[1], content, class);
    frame.render_widget(
        Paragraph::new(format!("> {}", app.command))
            .block(Block::default().title("COMMAND").borders(Borders::ALL)),
        rows[2],
    );
}

fn draw_content(frame: &mut ratatui::Frame<'_>, area: Rect, content: &str, class: ResponsiveClass) {
    let title = match class {
        ResponsiveClass::Phone => "Overview · pixels opt-in",
        ResponsiveClass::Compact => "Browser evidence",
        ResponsiveClass::Desktop => "Browser / Structured Observation",
    };
    frame.render_widget(
        Paragraph::new(content)
            .wrap(Wrap { trim: false })
            .block(Block::default().title(title).borders(Borders::ALL)),
        area,
    );
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableFocusChange,
            EnableBracketedPaste
        )?;
        Ok(Self {
            terminal: Terminal::new(CrosstermBackend::new(stdout))?,
        })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            DisableBracketedPaste,
            DisableFocusChange,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = self.terminal.show_cursor();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responsive_classes_preserve_phone_compact_and_desktop_boundaries() {
        assert_eq!(ResponsiveClass::for_width(40), ResponsiveClass::Phone);
        assert_eq!(ResponsiveClass::for_width(72), ResponsiveClass::Phone);
        assert_eq!(ResponsiveClass::for_width(73), ResponsiveClass::Compact);
        assert_eq!(ResponsiveClass::for_width(109), ResponsiveClass::Compact);
        assert_eq!(ResponsiveClass::for_width(110), ResponsiveClass::Desktop);
    }

    #[test]
    fn browser_tui_starts_structured_first_and_without_a_session() {
        let app = BrowserTui::new();
        assert!(app.status.contains("structured observation"));
        assert!(app.session.is_none());
        assert_eq!(app.mode, WorkspaceMode::Browser);
    }
}
