use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use std::io;

use crate::browser::profile::ProfileManager;
use crate::browser::session::{BrowserResult, BrowserSession, PageContext, SessionOptions};
use crate::cli::args::Cli;

pub struct App {
    url: String,
    title: String,
    thoughts: Vec<String>,
    page_content: Vec<String>,
    page_scroll: u16,
    input: String,
    cursor_pos: usize,
    should_quit: bool,
    error_msg: Option<String>,
    status: String,
}

impl App {
    fn new() -> Self {
        Self {
            url: String::new(),
            title: "Glass — Browser Agent".to_string(),
            thoughts: vec![
                "Glass started.".to_string(),
                "Waiting for instructions...".to_string(),
            ],
            page_content: vec!["No page loaded.".to_string()],
            page_scroll: 0,
            input: String::new(),
            cursor_pos: 0,
            should_quit: false,
            error_msg: None,
            status: "Ready".to_string(),
        }
    }

    fn add_thought(&mut self, message: impl Into<String>) {
        self.thoughts.push(message.into());
        if self.thoughts.len() > 100 {
            self.thoughts.remove(0);
        }
    }

    fn set_error(&mut self, message: impl Into<String>) {
        self.error_msg = Some(message.into());
    }

    fn clear_error(&mut self) {
        self.error_msg = None;
    }

    fn cursor_byte_index(&self) -> usize {
        self.input
            .char_indices()
            .nth(self.cursor_pos)
            .map(|(index, _)| index)
            .unwrap_or(self.input.len())
    }

    fn insert_char(&mut self, character: char) {
        let index = self.cursor_byte_index();
        self.input.insert(index, character);
        self.cursor_pos += 1;
    }

    fn remove_before_cursor(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        let end = self.cursor_byte_index();
        let start = self
            .input
            .char_indices()
            .nth(self.cursor_pos - 1)
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.input.drain(start..end);
        self.cursor_pos -= 1;
    }

    fn remove_at_cursor(&mut self) {
        let start = self.cursor_byte_index();
        let end = self
            .input
            .char_indices()
            .nth(self.cursor_pos + 1)
            .map(|(index, _)| index)
            .unwrap_or(self.input.len());
        if start < end {
            self.input.drain(start..end);
        }
    }
}

fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let header = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);

    let title = Paragraph::new(app.title.as_str())
        .block(Block::default().borders(Borders::ALL).title("Glass"))
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(title, header[0]);

    let url = Paragraph::new(app.url.as_str())
        .block(Block::default().borders(Borders::ALL).title("URL"))
        .style(Style::default().fg(Color::Yellow));
    frame.render_widget(url, header[1]);

    let content = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(chunks[1]);

    let thoughts = app
        .thoughts
        .iter()
        .map(|thought| ListItem::new(Line::from(thought.as_str())))
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(thoughts)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Agent Thoughts"),
            )
            .style(Style::default().fg(Color::Green)),
        content[0],
    );

    let page = app.page_content.join("\n");
    frame.render_widget(
        Paragraph::new(page)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Structured Observation"),
            )
            .scroll((app.page_scroll, 0))
            .wrap(Wrap { trim: true }),
        content[1],
    );

    frame.render_widget(
        Paragraph::new(app.input.as_str())
            .block(Block::default().borders(Borders::ALL).title("Command")),
        chunks[2],
    );
    frame.render_widget(
        Paragraph::new(format!(
            " {}   PgUp/PgDn: observation   q: quit   Enter: execute   Esc: close error/quit   {}",
            app.status,
            app.input.chars().count()
        ))
        .style(Style::default().fg(Color::DarkGray)),
        chunks[3],
    );

    if let Some(error) = &app.error_msg {
        let area = frame.area();
        let popup = Rect {
            x: area.width / 6,
            y: area.height / 2 - 2,
            width: area.width * 2 / 3,
            height: 5,
        };
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new(error.as_str())
                .block(Block::default().borders(Borders::ALL).title("Error"))
                .style(Style::default().fg(Color::Red))
                .wrap(Wrap { trim: true }),
            popup,
        );
    }
}

pub async fn run_tui(cli: &Cli) -> BrowserResult<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new();

    let options = SessionOptions {
        port: cli.port,
        chrome_path: cli.chrome_path.clone(),
        profile: cli.profile.clone(),
        incognito: cli.incognito,
        headed: cli.headed,
        interaction_mode: cli.interaction,
    };
    let session = match BrowserSession::start(&options).await {
        Ok(session) => {
            app.status = format!("Connected on port {}", cli.port);
            app.add_thought("Connected to Chrome.");
            if let Err(error) = refresh_observation(&session, &mut app, false).await {
                app.add_thought(format!("Initial observation failed: {error}"));
            }
            Some(session)
        }
        Err(error) => {
            app.status = "Browser unavailable".to_string();
            app.add_thought(format!("Browser startup failed: {error}"));
            app.set_error(error.to_string());
            None
        }
    };

    while !app.should_quit {
        terminal.draw(|frame| draw(frame, &app))?;
        if !event::poll(std::time::Duration::from_millis(100))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match key.code {
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.should_quit = true
            }
            KeyCode::Char('q') if app.input.is_empty() => app.should_quit = true,
            KeyCode::Esc => {
                if app.error_msg.is_some() {
                    app.clear_error();
                } else {
                    app.should_quit = true;
                }
            }
            KeyCode::Enter => {
                if !app.input.trim().is_empty() {
                    let command = app.input.clone();
                    app.input.clear();
                    app.cursor_pos = 0;
                    app.add_thought(format!("> {command}"));
                    if let Err(error) = execute_command(session.as_ref(), &command, &mut app).await
                    {
                        app.set_error(error.to_string());
                        app.add_thought(format!("Error: {error}"));
                    }
                }
            }
            KeyCode::Backspace => app.remove_before_cursor(),
            KeyCode::Delete => app.remove_at_cursor(),
            KeyCode::Left => app.cursor_pos = app.cursor_pos.saturating_sub(1),
            KeyCode::Right => app.cursor_pos = (app.cursor_pos + 1).min(app.input.chars().count()),
            KeyCode::Home => app.cursor_pos = 0,
            KeyCode::End => app.cursor_pos = app.input.chars().count(),
            KeyCode::PageUp => app.page_scroll = app.page_scroll.saturating_sub(10),
            KeyCode::PageDown => app.page_scroll = app.page_scroll.saturating_add(10),
            KeyCode::Char(character) => app.insert_char(character),
            _ => {}
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    if let Some(session) = session {
        session.close().await?;
    }
    Ok(())
}

async fn execute_command(
    session: Option<&BrowserSession>,
    command: &str,
    app: &mut App,
) -> BrowserResult<()> {
    let lower = command.trim().to_lowercase();
    if lower == "help" {
        app.add_thought("navigate URL | click TARGET | type TEXT | screenshot [FILE]");
        app.add_thought("observe | text | dom | scroll DX DY | profiles | JavaScript");
        return Ok(());
    }
    if lower == "profiles" {
        let profiles = ProfileManager::new().list_profiles()?;
        if profiles.is_empty() {
            app.add_thought("No saved profiles.");
        } else {
            for profile in profiles {
                app.add_thought(format!("  - {profile}"));
            }
        }
        return Ok(());
    }

    let session = session.ok_or("browser session is unavailable")?;
    if let Some(url) = command
        .strip_prefix("navigate ")
        .or_else(|| command.strip_prefix("go "))
    {
        session.navigate(url.trim()).await?;
        refresh_observation(session, app, true).await?;
        app.add_thought(format!("Page loaded: {}", app.title));
    } else if lower.starts_with("screenshot") {
        let output = command
            .split_once(char::is_whitespace)
            .map(|(_, value)| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or("screenshot.png");
        std::fs::write(output, session.screenshot_png().await?)?;
        app.add_thought(format!("Screenshot saved to {output}"));
    } else if lower == "text" || lower == "content" {
        app.page_content = session.text().await?.lines().map(String::from).collect();
        app.page_scroll = 0;
        app.add_thought("Page content refreshed.");
    } else if lower == "dom" || lower == "snapshot" {
        app.page_content = session
            .snapshot()
            .await?
            .format()
            .lines()
            .map(String::from)
            .collect();
        app.page_scroll = 0;
        app.add_thought("Accessibility snapshot refreshed.");
    } else if lower == "observe" || lower == "context" {
        refresh_observation(session, app, false).await?;
        app.add_thought("DOM and accessibility context refreshed without a screenshot.");
    } else if let Some(rest) = command.strip_prefix("click ") {
        let target = session.click(rest.trim()).await?;
        refresh_observation(session, app, true).await?;
        app.add_thought(format!("Clicked {target}"));
    } else if let Some(rest) = command.strip_prefix("type ") {
        session.type_text(rest, None).await?;
        refresh_observation(session, app, true).await?;
        app.add_thought(format!("Typed {} characters.", rest.chars().count()));
    } else if let Some(rest) = command.strip_prefix("scroll ") {
        let mut values = rest
            .split_whitespace()
            .filter_map(|value| value.parse().ok());
        session
            .scroll(values.next().unwrap_or(0.0), values.next().unwrap_or(600.0))
            .await?;
        refresh_observation(session, app, true).await?;
        app.add_thought("Scrolled.");
    } else {
        let result = session.evaluate(command).await?;
        refresh_observation(session, app, true).await?;
        app.add_thought(format!("Result: {result}"));
    }
    Ok(())
}

async fn refresh_observation(
    session: &BrowserSession,
    app: &mut App,
    fresh: bool,
) -> BrowserResult<()> {
    let context = if fresh {
        session.observe_fresh().await?
    } else {
        session.observe().await?
    };
    apply_observation(app, &context)?;
    Ok(())
}

fn apply_observation(app: &mut App, context: &PageContext) -> BrowserResult<()> {
    app.url.clone_from(&context.page.url);
    app.title = format!("Glass — {}", context.page.title);
    app.page_content = serde_json::to_string_pretty(context)?
        .lines()
        .map(String::from)
        .collect();
    app.page_scroll = 0;
    Ok(())
}
