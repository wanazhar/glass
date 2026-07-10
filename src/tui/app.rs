use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;


use crate::cli::main::Cli;

/// Application state for the TUI.
pub struct App {
    /// Current URL being displayed
    pub url: String,
    /// Page title
    pub title: String,
    /// Agent thought log (left panel)
    pub thoughts: Vec<String>,
    /// Page content / accessibility tree (right panel)
    pub page_content: Vec<String>,
    /// User input buffer
    pub input: String,
    /// Input cursor position
    pub cursor_pos: usize,
    /// Whether the app should quit
    pub should_quit: bool,
    /// Error message to display
    pub error_msg: Option<String>,
    /// Status message
    pub status: String,
}

impl App {
    pub fn new() -> Self {
        Self {
            url: String::new(),
            title: "Glass — Browser Agent".to_string(),
            thoughts: vec![
                "Glass started.".to_string(),
                "Waiting for instructions...".to_string(),
            ],
            page_content: vec!["No page loaded.".to_string()],
            input: String::new(),
            cursor_pos: 0,
            should_quit: false,
            error_msg: None,
            status: "Ready".to_string(),
        }
    }

    pub fn add_thought(&mut self, msg: &str) {
        self.thoughts.push(msg.to_string());
        // Keep last 100 thoughts
        if self.thoughts.len() > 100 {
            self.thoughts.remove(0);
        }
    }

    pub fn set_error(&mut self, msg: &str) {
        self.error_msg = Some(msg.to_string());
    }

    pub fn clear_error(&mut self) {
        self.error_msg = None;
    }
}

/// Draw the TUI.
fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(10),   // Content
            Constraint::Length(3), // Input
            Constraint::Length(1), // Footer
        ])
        .split(f.area());

    // Header
    let header = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);

    let title_block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));

    let title = Paragraph::new(Text::from(vec![
        Line::from(Span::styled(
            &app.title,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
    ]))
    .block(title_block);
    f.render_widget(title, header[0]);

    let url_block = Block::default()
        .borders(Borders::ALL)
        .title("URL");

    let url = Paragraph::new(Text::from(vec![
        Line::from(Span::styled(
            &app.url,
            Style::default().fg(Color::Yellow),
        )),
    ]))
    .block(url_block);
    f.render_widget(url, header[1]);

    // Content area: left (thoughts) + right (page content)
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    // Left panel: Agent thoughts
    let thoughts_block = Block::default()
        .borders(Borders::ALL)
        .title("Agent Thoughts");

    let thought_items: Vec<ListItem> = app
        .thoughts
        .iter()
        .map(|t| ListItem::new(Line::from(Span::styled(t.as_str(), Style::default().fg(Color::Green)))))
        .collect();

    let thoughts_list = List::new(thought_items).block(thoughts_block);
    f.render_widget(thoughts_list, content_chunks[0]);

    // Right panel: Page content
    let content_block = Block::default()
        .borders(Borders::ALL)
        .title("Page Content");

    let content_lines: Vec<Line> = app
        .page_content
        .iter()
        .map(|l| Line::from(Span::styled(l.as_str(), Style::default().fg(Color::White))))
        .collect();

    let content_text = Text::from(content_lines);
    let content_paragraph = Paragraph::new(content_text)
        .block(content_block)
        .wrap(Wrap { trim: true });
    f.render_widget(content_paragraph, content_chunks[1]);

    // Input area
    let input_block = Block::default()
        .borders(Borders::ALL)
        .title("Command");

    let input = Paragraph::new(Text::from(vec![
        Line::from(Span::styled(
            &app.input,
            Style::default().fg(Color::White),
        )),
    ]))
    .block(input_block);
    f.render_widget(input, chunks[2]);

    // Footer
    let footer = Paragraph::new(Text::from(vec![
        Line::from(Span::styled(
            " q:Quit  Enter:Execute  Tab:Switch Panel  ↑↓:History",
            Style::default().fg(Color::DarkGray),
        )),
    ]));
    f.render_widget(footer, chunks[3]);

    // Error overlay
    if let Some(err) = &app.error_msg {
        let area = f.area();
        let popup_area = Rect {
            x: area.width / 4,
            y: area.height / 2 - 2,
            width: area.width / 2,
            height: 5,
        };

        f.render_widget(Clear, popup_area);
        let error_block = Block::default()
            .borders(Borders::ALL)
            .title("Error")
            .style(Style::default().fg(Color::Red));

        let error_text = Paragraph::new(Text::from(vec![
            Line::from(Span::styled(err.as_str(), Style::default().fg(Color::Red))),
        ]))
        .block(error_block);

        f.render_widget(error_text, popup_area);
    }
}

/// Run the TUI application.
pub async fn run_tui(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    // Try to connect to Chrome
    let chrome_path = cli
        .chrome_path
        .clone()
        .or_else(|| crate::browser::chrome::detect_chrome());

    if let Some(path) = chrome_path {
        if crate::browser::chrome::check_chrome_health(cli.port).await {
            app.add_thought("Connected to Chrome.");
            app.status = format!("Connected (port {})", cli.port);
        } else {
            app.add_thought("Chrome not running. Launching...");
            let profile_dir = if cli.incognito {
                None
            } else {
                let manager = crate::browser::profile::ProfileManager::new();
                Some(manager.chrome_data_dir(&cli.profile))
            };
            match crate::browser::chrome::launch_chrome(&path, cli.port, profile_dir.as_deref()).await
            {
                Ok(_) => {
                    app.add_thought("Chrome launched successfully.");
                    app.status = format!("Chrome running (port {})", cli.port);
                }
                Err(e) => {
                    app.set_error(&format!("Failed to launch Chrome: {e}"));
                    app.add_thought(&format!("Chrome launch failed: {e}"));
                }
            }
        }
    } else {
        app.add_thought("Chrome not found. Install with: glass install-chromium");
        app.set_error("Chrome not found. Run 'glass install-chromium' first.");
    }

    // Main loop
    loop {
        terminal.draw(|f| draw(f, &app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                            app.should_quit = true;
                        }
                        KeyCode::Char('q') if app.input.is_empty() => {
                            app.should_quit = true;
                        }
                        KeyCode::Esc => {
                            if app.error_msg.is_some() {
                                app.clear_error();
                            } else {
                                app.should_quit = true;
                            }
                        }
                        KeyCode::Enter => {
                            if !app.input.is_empty() {
                                let cmd = app.input.clone();
                                app.add_thought(&format!("> {cmd}"));
                                app.input.clear();
                                app.cursor_pos = 0;

                                // Execute command
                                match execute_command(cli, &cmd, &mut app).await {
                                    Ok(_) => {}
                                    Err(e) => {
                                        app.set_error(&e.to_string());
                                        app.add_thought(&format!("Error: {e}"));
                                    }
                                }
                            }
                        }
                        KeyCode::Backspace => {
                            if app.cursor_pos > 0 {
                                app.cursor_pos -= 1;
                                app.input.remove(app.cursor_pos);
                            }
                        }
                        KeyCode::Delete => {
                            if app.cursor_pos < app.input.len() {
                                app.input.remove(app.cursor_pos);
                            }
                        }
                        KeyCode::Left => {
                            if app.cursor_pos > 0 {
                                app.cursor_pos -= 1;
                            }
                        }
                        KeyCode::Right => {
                            if app.cursor_pos < app.input.len() {
                                app.cursor_pos += 1;
                            }
                        }
                        KeyCode::Home => {
                            app.cursor_pos = 0;
                        }
                        KeyCode::End => {
                            app.cursor_pos = app.input.len();
                        }
                        KeyCode::Char(c) => {
                            app.input.insert(app.cursor_pos, c);
                            app.cursor_pos += 1;
                        }
                        _ => {}
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

/// Execute a TUI command.
async fn execute_command(
    cli: &Cli,
    cmd: &str,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    let cmd_lower = cmd.to_lowercase();

    if cmd_lower.starts_with("navigate ") || cmd_lower.starts_with("go ") {
        let url = if cmd_lower.starts_with("navigate ") {
            &cmd[9..]
        } else {
            &cmd[3..]
        };
        let url = url.trim();
        let url = if !url.starts_with("http://") && !url.starts_with("https://") {
            format!("https://{url}")
        } else {
            url.to_string()
        };

        let ws_url = crate::browser::chrome::get_ws_url(cli.port).await?;
        let mut cdp = crate::browser::cdp::CdpClient::connect(&ws_url).await?;
        cdp.enable_page().await?;

        app.add_thought(&format!("Navigating to {url}"));
        cdp.navigate(&url).await?;
        app.url = url.clone();

        // Wait for page load
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Get page title
        let result = cdp.evaluate("document.title").await?;
        if let Some(title) = result["result"]["value"].as_str() {
            app.title = format!("Glass — {title}");
        }

        // Get page content
        let content = cdp.evaluate("document.body.innerText.substring(0, 2000)").await?;
        if let Some(text) = content["result"]["value"].as_str() {
            app.page_content = text.lines().map(String::from).collect();
        }

        app.add_thought(&format!("Page loaded: {}", app.title));
        cdp.close().await;
    } else if cmd_lower == "screenshot" {
        let ws_url = crate::browser::chrome::get_ws_url(cli.port).await?;
        let mut cdp = crate::browser::cdp::CdpClient::connect(&ws_url).await?;

        let data = cdp.screenshot("png").await?;
        let decoded = base64_decode_simple(&data)?;
        std::fs::write("screenshot.png", &decoded)?;
        app.add_thought("Screenshot saved to screenshot.png");
        cdp.close().await;
    } else if cmd_lower == "content" || cmd_lower == "text" {
        let ws_url = crate::browser::chrome::get_ws_url(cli.port).await?;
        let mut cdp = crate::browser::cdp::CdpClient::connect(&ws_url).await?;

        let content = cdp.evaluate("document.body.innerText.substring(0, 5000)").await?;
        if let Some(text) = content["result"]["value"].as_str() {
            app.page_content = text.lines().map(String::from).collect();
        }
        app.add_thought("Page content refreshed.");
        cdp.close().await;
    } else if cmd_lower == "help" {
        app.add_thought("Commands:");
        app.add_thought("  navigate <url> — Navigate to URL");
        app.add_thought("  screenshot — Save screenshot");
        app.add_thought("  content — Refresh page text");
        app.add_thought("  profiles — List saved profiles");
        app.add_thought("  help — Show this help");
        app.add_thought("  q — Quit");
    } else if cmd_lower == "profiles" {
        let manager = crate::browser::profile::ProfileManager::new();
        let profiles = manager.list_profiles()?;
        if profiles.is_empty() {
            app.add_thought("No profiles found.");
        } else {
            for name in &profiles {
                app.add_thought(&format!("  - {name}"));
            }
        }
    } else {
        // Try to evaluate as JS
        let ws_url = crate::browser::chrome::get_ws_url(cli.port).await?;
        let mut cdp = crate::browser::cdp::CdpClient::connect(&ws_url).await?;

        match cdp.evaluate(cmd).await {
            Ok(result) => {
                app.add_thought(&format!("Result: {result}"));
            }
            Err(e) => {
                app.add_thought(&format!("Error: {e}"));
            }
        }
        cdp.close().await;
    }

    Ok(())
}

fn base64_decode_simple(data: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut map: std::collections::HashMap<u8, u8> =
        CHARS.iter().enumerate().map(|(i, &c)| (c, i as u8)).collect();
    map.insert(b'=', 0);

    let data = data.trim_end_matches('=');
    let mut result = Vec::with_capacity(data.len() * 3 / 4);

    for chunk in data.as_bytes().chunks(4) {
        let mut buf = [0u8; 4];
        for (i, &b) in chunk.iter().enumerate() {
            buf[i] = map.get(&b).copied().ok_or("Invalid base64 character")?;
        }

        let b0 = buf[0] as u32;
        let b1 = buf[1] as u32;
        let b2 = buf[2] as u32;

        result.push(((b0 << 2) | (b1 >> 4)) as u8);
        if chunk.len() > 2 {
            result.push(((b1 << 4) | (b2 >> 2)) as u8);
        }
        if chunk.len() > 3 {
            result.push(((b2 << 6) | (buf[3] as u32)) as u8);
        }
    }

    Ok(result)
}
