use super::command;
use super::state::{DevSurface, DevTuiState, ResponsiveClass};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Padding, Paragraph, Row,
    Table, Wrap,
};

// GitHub/Druk-inspired neutrals keep the data readable while the accent colors
// reserve meaning for focus, progress, trust, and failure.
const ACCENT: Color = Color::Rgb(88, 166, 255);
const ACCENT_BRIGHT: Color = Color::Rgb(121, 192, 255);
const PANEL_BORDER: Color = Color::Rgb(48, 54, 61);
const PANEL_BACKGROUND: Color = Color::Rgb(22, 27, 34);
const PANEL_INSET: Color = Color::Rgb(13, 17, 23);
const ACTIVE_BACKGROUND: Color = Color::Rgb(31, 50, 72);
const TEXT: Color = Color::Rgb(230, 237, 243);
const MUTED: Color = Color::Rgb(139, 148, 158);
const SUCCESS: Color = Color::Rgb(126, 231, 135);
const WARNING: Color = Color::Rgb(210, 153, 34);
const ERROR: Color = Color::Rgb(255, 123, 114);
const PURPLE: Color = Color::Rgb(210, 168, 255);
fn panel_text(content: &str) -> Text<'static> {
    Text::from(
        content
            .lines()
            .map(|line| {
                let trimmed = line.trim();
                let heading = is_panel_heading(trimmed);
                let style = if heading {
                    Style::default()
                        .fg(ACCENT_BRIGHT)
                        .add_modifier(Modifier::BOLD)
                } else if trimmed.starts_with("Keys:")
                    || trimmed.starts_with("Use ")
                    || trimmed.starts_with("Composer:")
                    || trimmed.starts_with("Try ")
                {
                    Style::default().fg(MUTED)
                } else if trimmed.starts_with('✓') {
                    Style::default().fg(SUCCESS)
                } else if trimmed.starts_with('×') || trimmed.contains("failed") {
                    Style::default().fg(ERROR)
                } else {
                    Style::default().fg(TEXT)
                };
                if heading {
                    Line::from(vec![
                        Span::styled("▎ ", Style::default().fg(ACCENT)),
                        Span::styled(line.to_string(), style),
                    ])
                } else {
                    Line::from(Span::styled(line.to_string(), style))
                }
            })
            .collect::<Vec<_>>(),
    )
}

fn is_panel_heading(line: &str) -> bool {
    !line.is_empty()
        && line
            .chars()
            .any(|character| character.is_ascii_alphabetic())
        && line.chars().all(|character| {
            character.is_ascii_uppercase()
                || character.is_ascii_digit()
                || matches!(character, ' ' | '/' | '-' | '·' | ':' | '(' | ')')
        })
        && !line.starts_with("Keys:")
}

fn status_glyph(state: &DevTuiState) -> (&'static str, Color) {
    let status = state.status.to_ascii_lowercase();
    if status.contains("error")
        || status.contains("failed")
        || status.contains("denied")
        || status.contains("cancelled")
    {
        ("×", ERROR)
    } else if status.contains("trust")
        || status.contains("confirm")
        || status.contains("approval")
        || status.contains("required")
    {
        ("!", WARNING)
    } else if status.contains("working")
        || status.contains("loading")
        || status.contains("running")
        || status.contains("sending")
    {
        ("◐", ACCENT_BRIGHT)
    } else {
        ("●", SUCCESS)
    }
}

fn status_line(state: &DevTuiState, width: u16) -> Line<'static> {
    let (glyph, color) = status_glyph(state);
    Line::from(vec![
        Span::styled(
            format!(" {glyph} "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            compact_line(&state.status, width.saturating_sub(3)),
            status_style(state),
        ),
    ])
}

fn context_next_actions(surface: DevSurface) -> &'static str {
    match surface {
        DevSurface::Trust => "inspect config\ntrust once / project",
        DevSurface::Agent => "i chat · a launch\n: routes",
        DevSurface::Code => "Enter open · i edit\nCtrl-S save",
        DevSurface::App => "n address · T targets\nv visual · Enter attach selected",
        DevSurface::Terminal => "s start suite · a launch\n: routes",
        DevSurface::Tasks => "a launch\n: deps · evidence · verify",
        DevSurface::Git => "d diff · a launch\n: stage · commit · branch",
        DevSurface::Debug => "a launch\n: debugger · tests",
        DevSurface::More => "a launch\n: routes",
    }
}

pub fn render(frame: &mut Frame<'_>, state: &DevTuiState) {
    let area = frame.area();
    if state.quit_confirmation {
        render_quit_confirmation(frame, area);
        return;
    }
    if state.help_open {
        render_help(frame, state, area);
        return;
    }
    match state.responsive_class(area.width, area.height) {
        ResponsiveClass::Desktop => render_desktop(frame, state, area),
        ResponsiveClass::Compact => render_compact(frame, state, area),
        ResponsiveClass::Phone => render_phone(frame, state, area),
    }
}
fn render_quit_confirmation(frame: &mut Frame<'_>, area: Rect) {
    let width = area.width.saturating_sub(4).min(64);
    let height = area.height.saturating_sub(4).min(9);
    let modal = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Paragraph::new(panel_text(
            "QUIT GLASS DEV?\n\nReturn to the shell and close this TUI?\n\n[ Enter / Y ] Quit\n[ Esc / N ] Stay",
        ))
        .style(Style::default().fg(TEXT).bg(PANEL_BACKGROUND))
        .block(
            Block::default()
                .title(" EXIT · confirm ")
                .title_style(Style::default().fg(WARNING).add_modifier(Modifier::BOLD))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(WARNING))
                .bg(PANEL_BACKGROUND)
                .padding(Padding::horizontal(1)),
        )
        .wrap(Wrap { trim: false }),
        modal,
    );
}

fn render_help(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let group = command::command_group_for(state.surface);
    let content = format!(
        "COMMAND CENTER · {}\n\nLAUNCH\n  a        open guided launchers for this surface\n  :        search every route by root command\n  Tab      complete the highlighted route\n  ?        this guide · Esc closes overlays\n\nNAVIGATION\n  1–7      switch surfaces (phone uses 1–5)\n  j/k ↑/↓  move and scroll\n  PgUp/Dn  page content · Home/End bounds\n\nAPP\n  n        navigate · t type · v live view\n  Alt-←/→  browser back/forward · Ctrl-R reload\n  H/G      hand control to human / return to Glass\n\nAGENT\n  i        compose a message\n  s/l      setup Pi / sign in\n  Ctrl-D   toggle steer/follow-up mode\n  Ctrl-X   abort the selected session\n\nCODE\n  Enter    open selected file · i edit\n  [/]]     switch open buffers · Ctrl-S save\n\nGIT / DEV\n  current route group: {}\n  example: `{}`",
        state.surface.label(),
        group.roots.join(" · "),
        group.example,
    );
    frame.render_widget(
        Paragraph::new(panel_text(&content))
            .style(Style::default().fg(TEXT))
            .scroll((state.help_scroll, 0))
            .block(
                Block::default()
                    .title(" HELP · j/k scroll · ?/Esc close ")
                    .title_style(
                        Style::default()
                            .fg(ACCENT_BRIGHT)
                            .add_modifier(Modifier::BOLD),
                    )
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(PANEL_BORDER))
                    .bg(PANEL_BACKGROUND)
                    .padding(Padding::horizontal(1)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}
fn render_desktop(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(area);
    render_header(frame, state, rows[0], "desktop");
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(24),
            Constraint::Percentage(55),
            Constraint::Min(30),
        ])
        .split(rows[1]);
    render_navigation(frame, state, columns[0]);
    render_surface(frame, state, columns[1]);
    render_context(frame, state, columns[2]);
    render_status(frame, state, rows[2]);
}

fn render_compact(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(4),
        ])
        .split(area);
    render_header(frame, state, rows[0], "compact");
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(22), Constraint::Min(36)])
        .split(rows[1]);
    render_navigation(frame, state, columns[0]);
    render_surface(frame, state, columns[1]);
    render_status(frame, state, rows[2]);
}

fn render_phone(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(if state.composer_mode { 5 } else { 4 }),
        ])
        .split(area);
    render_header(frame, state, rows[0], "phone cockpit");
    render_surface(frame, state, rows[1]);
    let footer_lines = if state.composer_mode {
        vec![
            Line::from(input_spans(
                " > ",
                Style::default()
                    .fg(ACCENT_BRIGHT)
                    .add_modifier(Modifier::BOLD),
                &state.composer_input,
                state.composer_cursor,
                rows[2].width.saturating_sub(6),
            )),
            status_line(state, rows[2].width.saturating_sub(2)),
            Line::from(Span::styled(
                "Enter send · Ctrl-D toggle steer/follow-up",
                Style::default().fg(MUTED),
            )),
            Line::from(Span::styled(
                "Esc cancel · ? help",
                Style::default().fg(MUTED),
            )),
        ]
    } else if state.command_mode {
        vec![
            Line::from(input_spans(
                " : ",
                Style::default()
                    .fg(ACCENT_BRIGHT)
                    .add_modifier(Modifier::BOLD),
                &state.command_input,
                state.command_cursor,
                rows[2].width.saturating_sub(6),
            )),
            status_line(state, rows[2].width.saturating_sub(2)),
            Line::from(Span::styled(
                "Tab complete · Enter run · Esc close",
                Style::default().fg(MUTED),
            )),
        ]
    } else if state.surface == DevSurface::Trust {
        vec![
            status_line(state, rows[2].width.saturating_sub(2)),
            Line::from(Span::styled(
                "I inspect · O untrusted · 1 once · T project",
                Style::default().fg(MUTED),
            )),
            Line::from(Span::styled(
                "a launch · ? help",
                Style::default().fg(MUTED),
            )),
        ]
    } else if state.surface == DevSurface::Agent {
        vec![
            status_line(state, rows[2].width.saturating_sub(2)),
            Line::from(Span::styled(
                "s setup · l login · i chat · a launch",
                Style::default().fg(MUTED),
            )),
            Line::from(Span::styled(
                "? help · j/k scroll",
                Style::default().fg(MUTED),
            )),
        ]
    } else if state.surface == DevSurface::Terminal {
        vec![
            status_line(state, rows[2].width.saturating_sub(2)),
            Line::from(Span::styled(
                "s start suite · a launch · : routes · ? help",
                Style::default().fg(MUTED),
            )),
        ]
    } else {
        vec![
            status_line(state, rows[2].width.saturating_sub(2)),
            Line::from(Span::styled(
                "1 Agent · 2 Code · 3 App · 4 Tasks",
                Style::default().fg(MUTED),
            )),
            Line::from(Span::styled(
                "5 More · a launch · : routes · ? help",
                Style::default().fg(MUTED),
            )),
        ]
    };
    frame.render_widget(
        Paragraph::new(footer_lines)
            .style(Style::default().bg(PANEL_BACKGROUND))
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(PANEL_BORDER))
                    .padding(Padding::horizontal(1)),
            )
            .wrap(Wrap { trim: true }),
        rows[2],
    );
}

fn render_header(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect, mode: &str) {
    let brand = Span::styled(
        " GLASS DEV ",
        Style::default()
            .fg(Color::Black)
            .bg(ACCENT)
            .add_modifier(Modifier::BOLD),
    );
    let surface = Span::styled(
        format!(" {} ", state.surface.label()),
        Style::default()
            .fg(ACCENT_BRIGHT)
            .add_modifier(Modifier::BOLD),
    );
    let product = Span::styled(
        format!(" mode:{} ", state.product_mode().label()),
        Style::default().fg(SUCCESS),
    );
    let trust = Span::styled(
        format!(" trust:{} ", state.snapshot_trust_label),
        Style::default().fg(if state.snapshot_trust_label == "untrusted" {
            WARNING
        } else {
            MUTED
        }),
    );
    let activity = Span::styled(
        format!(" {} ", activity_summary(state)),
        Style::default().fg(MUTED),
    );
    let path = Span::styled(
        compact_path(&state.snapshot_root, area.width.saturating_sub(30)),
        Style::default().fg(MUTED),
    );
    let mut lines = if area.width < 72 {
        vec![
            Line::from(vec![brand.clone(), surface]),
            Line::from(vec![
                Span::raw(" "),
                path,
                Span::styled(" · ", Style::default().fg(PANEL_BORDER)),
                trust,
            ]),
        ]
    } else if area.width < 104 {
        vec![
            Line::from(vec![
                brand.clone(),
                surface,
                product,
                trust,
                Span::styled(format!(" {} ", mode), Style::default().fg(MUTED)),
            ]),
            Line::from(vec![
                Span::raw(" "),
                path,
                Span::styled(" · ", Style::default().fg(PANEL_BORDER)),
                activity,
            ]),
        ]
    } else {
        vec![Line::from(vec![
            brand,
            Span::raw("  "),
            path,
            surface,
            product,
            trust,
            Span::styled(format!(" {} ", mode), Style::default().fg(MUTED)),
            activity,
        ])]
    };
    if area.height >= 2 {
        lines.push(Line::from(Span::styled(
            "─".repeat(usize::from(area.width)),
            Style::default().fg(PANEL_BORDER),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(PANEL_BACKGROUND)),
        area,
    );
}

fn compact_path(path: &str, width: u16) -> String {
    let width = usize::from(width.max(8));
    let chars = path.chars().collect::<Vec<_>>();
    if chars.len() <= width {
        return path.to_string();
    }
    let suffix = chars[chars.len().saturating_sub(width.saturating_sub(2))..]
        .iter()
        .collect::<String>();
    format!("…{suffix}")
}
fn compact_line(text: &str, width: u16) -> String {
    let width = usize::from(width);
    if width == 0 {
        return String::new();
    }
    let chars = text.chars().collect::<Vec<_>>();
    if chars.len() <= width {
        return text.to_string();
    }
    if width == 1 {
        return "…".into();
    }
    let mut compact = chars[..width - 1].iter().collect::<String>();
    compact.push('…');
    compact
}

fn surface_hint(surface: DevSurface) -> &'static str {
    match surface {
        DevSurface::Trust => "authority",
        DevSurface::Agent => "chat",
        DevSurface::Code => "files",
        DevSurface::App => "browser",
        DevSurface::Terminal => "processes",
        DevSurface::Tasks => "verify",
        DevSurface::Git => "changes",
        DevSurface::Debug => "tests",
        DevSurface::More => "services",
    }
}

fn render_navigation(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let rows = if area.height >= 17 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(10), Constraint::Length(5)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(10), Constraint::Length(0)])
            .split(area)
    };
    let items = DevSurface::PRIMARY
        .into_iter()
        .enumerate()
        .map(|(index, surface)| {
            let key = index + 1;
            let selected = surface == state.surface;
            let style = if selected {
                Style::default()
                    .fg(ACCENT_BRIGHT)
                    .bg(ACTIVE_BACKGROUND)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            };
            let mut row = vec![
                Span::styled(
                    if selected { " › " } else { "   " },
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {key} "),
                    Style::default()
                        .fg(if selected { Color::Black } else { ACCENT })
                        .bg(if selected { ACCENT } else { PANEL_BACKGROUND })
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(surface.label(), style),
            ];
            if area.width >= 30 {
                row.push(Span::styled(
                    format!("  {}", surface_hint(surface)),
                    Style::default().fg(MUTED),
                ));
            }
            ListItem::new(Line::from(row)).style(style)
        });
    frame.render_widget(
        List::new(items)
            .style(Style::default().bg(PANEL_BACKGROUND))
            .block(
                Block::default()
                    .title(" SURFACES · 1–7 ")
                    .title_style(
                        Style::default()
                            .fg(ACCENT_BRIGHT)
                            .add_modifier(Modifier::BOLD),
                    )
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(PANEL_BORDER))
                    .padding(Padding::horizontal(1)),
            ),
        rows[0],
    );
    if rows[1].height > 0 {
        let quick_keys = if area.width <= 22 {
            "a launch\n: routes\nq quit · ? help"
        } else {
            "a launch · : routes · q quit\nm More · ? help\nj/k move · scroll"
        };
        frame.render_widget(
            Paragraph::new(panel_text(quick_keys))
                .style(Style::default().bg(PANEL_BACKGROUND))
                .block(
                    Block::default()
                        .title(" QUICK KEYS ")
                        .title_style(Style::default().fg(MUTED))
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(PANEL_BORDER))
                        .padding(Padding::horizontal(1)),
                )
                .wrap(Wrap { trim: true }),
            rows[1],
        );
    }
}

fn surface_block(title: impl Into<String>, title_color: Color) -> Block<'static> {
    Block::default()
        .title(title.into())
        .title_style(
            Style::default()
                .fg(title_color)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(PANEL_BORDER))
        .bg(PANEL_BACKGROUND)
        .padding(Padding::horizontal(1))
}

fn render_panel(
    frame: &mut Frame<'_>,
    area: Rect,
    title: impl Into<String>,
    content: impl Into<String>,
    title_color: Color,
) {
    if area.width < 2 || area.height < 2 {
        return;
    }
    frame.render_widget(
        Paragraph::new(panel_text(&content.into()))
            .style(Style::default().fg(TEXT).bg(PANEL_BACKGROUND))
            .block(surface_block(title, title_color))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_scrollable_panel(
    frame: &mut Frame<'_>,
    area: Rect,
    title: impl Into<String>,
    content: impl Into<String>,
    title_color: Color,
    scroll: u16,
) {
    if area.width < 2 || area.height < 2 {
        return;
    }
    frame.render_widget(
        Paragraph::new(panel_text(&content.into()))
            .style(Style::default().fg(TEXT).bg(PANEL_BACKGROUND))
            .scroll((scroll, 0))
            .block(surface_block(title, title_color))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn stack_for_phone(state: &DevTuiState, area: Rect) -> bool {
    area.width < 84
        || matches!(
            state.responsive_class(area.width, area.height),
            ResponsiveClass::Phone
        )
}

fn compact_browser_copy(state: &DevTuiState, area: Rect) -> bool {
    area.width < 72
        || matches!(
            state.responsive_class(area.width, area.height),
            ResponsiveClass::Phone
        )
}

fn status_color(line: &str) -> Color {
    let lower = line.to_ascii_lowercase();
    if lower.starts_with('✓') || lower.contains("ready") || lower.contains("connected") {
        SUCCESS
    } else if lower.starts_with('×') || lower.contains("failed") || lower.contains("error") {
        ERROR
    } else if lower.starts_with('!') || lower.contains("blocked") || lower.contains("required") {
        WARNING
    } else if lower.starts_with('●') || lower.contains("running") || lower.contains("working") {
        ACCENT_BRIGHT
    } else {
        TEXT
    }
}

fn render_status_list(
    frame: &mut Frame<'_>,
    area: Rect,
    title: impl Into<String>,
    content: &str,
    empty: &str,
) {
    let content = if content.trim().is_empty() {
        empty
    } else {
        content
    };
    let lines = content
        .lines()
        .map(|line| {
            Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(status_color(line)),
            ))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(TEXT).bg(PANEL_BACKGROUND))
            .block(surface_block(title, ACCENT_BRIGHT))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn activity_summary(state: &DevTuiState) -> String {
    let mut activities = Vec::new();
    if state.agent_send_job.is_some() {
        activities.push("AGENT sending");
    }
    if state.running_tool_job.is_some() {
        activities.push("TOOL running");
    }
    if state.browser_observe_pending {
        activities.push("BROWSER observing");
    }
    if state.pending_confirmation.is_some() {
        activities.push("CONFIRMATION waiting");
    }
    if state.pending_agent_approval.is_some() {
        activities.push("AGENT approval");
    }
    if activities.is_empty() {
        "idle".into()
    } else {
        activities.join(" · ")
    }
}

fn agent_progress(state: &DevTuiState) -> String {
    let phase = if state.pending_agent_approval.is_some() {
        "approval required"
    } else if state.agent_send_job.is_some() {
        "sending prompt"
    } else if state.running_tool_job.is_some() {
        "Glass tool running"
    } else if state.composer_mode {
        "drafting prompt"
    } else if state.agent_readiness.starts_with("✓ Ready") {
        "ready for a prompt"
    } else {
        "setup required"
    };
    let queued = state
        .pending_chat_messages
        .iter()
        .filter(|message| matches!(message.state, super::state::ChatMessageState::Sending))
        .count();
    format!(
        "AGENT PROGRESS\nphase  {phase}\nqueue  {queued} prompt(s) · cursor {}\nactivity  {}",
        state.conversation_cursor,
        activity_summary(state)
    )
}

fn browser_progress(
    state: &DevTuiState,
    browser: &glass_browser::browser_workspace::BrowserWorkspaceState,
) -> String {
    let phase = if browser.loading {
        "loading page"
    } else if state.browser_observe_pending {
        "observing semantic page"
    } else if state.browser_visual_live {
        "live presentation"
    } else if browser.selected().is_some() {
        "ready for guarded action"
    } else {
        "select a target"
    };
    format!(
        "BROWSER PROGRESS  {phase} · target {} · rev {}",
        browser
            .selected()
            .map(|entity| entity.name.as_str())
            .unwrap_or("none"),
        browser
            .browser_revision
            .map_or_else(|| "—".into(), |revision| revision.to_string())
    )
}

fn render_trust_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(4)])
        .split(area);
    let summary = format!(
        "TRUST REQUIRED\n{} configuration item(s) need attention\n\nCurrent authority: {}\n\n[I] Inspect  [O] Open untrusted\n[1] Trust once  [T] Trust project",
        state
            .snapshot_trust_inspection
            .iter()
            .filter(|item| item.trust_required)
            .count(),
        state.snapshot_trust_label,
    );
    render_panel(
        frame,
        rows[0],
        " WORKSPACE TRUST · required ",
        summary,
        WARNING,
    );
    let inspection = super::projection::trust_items(&state.snapshot_trust_inspection);
    render_panel(
        frame,
        rows[1],
        " CONFIGURATION BY AUTHORITY / RISK ",
        format!(
            "{}\n\nNEXT ACTIONS\nInspect first; trust only the scope this project needs.",
            inspection
        ),
        ACCENT_BRIGHT,
    );
}

fn compact_agent_header(state: &DevTuiState, width: u16) -> String {
    let readiness = state
        .agent_readiness
        .lines()
        .next()
        .unwrap_or("Pi runtime unavailable");
    let fields = readiness.split(" · ").collect::<Vec<_>>();
    let runtime_state = fields.first().copied().unwrap_or("Pi runtime unavailable");
    let runtime_details = fields
        .iter()
        .skip(1)
        .copied()
        .collect::<Vec<_>>()
        .join(" · ");
    let runtime_details = if runtime_details.is_empty() {
        "runtime details unavailable".to_string()
    } else {
        runtime_details
    };
    let app_summary = state
        .browser_chat_header()
        .split(" · ")
        .take(3)
        .collect::<Vec<_>>()
        .join(" · ");
    let app_summary = format!("{app_summary} · {}", activity_summary(state));
    format!(
        "{}\n{}\n{}",
        compact_line(&format!("PI RUNTIME  {runtime_state}"), width),
        compact_line(&runtime_details, width),
        compact_line(&app_summary, width),
    )
}

fn render_agent_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let compact = matches!(
        state.responsive_class(area.width, area.height),
        ResponsiveClass::Phone | ResponsiveClass::Compact
    );
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if compact {
            [Constraint::Length(6), Constraint::Min(4)]
        } else {
            [Constraint::Length(5), Constraint::Min(6)]
        })
        .split(area);
    let header = compact_agent_header(state, rows[0].width.saturating_sub(4));
    render_panel(
        frame,
        rows[0],
        " AGENT · resident session ",
        header,
        ACCENT_BRIGHT,
    );

    let conversation = state.conversation_view();
    let landing = if conversation.starts_with("No conversation yet.") {
        if state.agent_readiness.starts_with("✓ Ready") {
            if compact {
                "START HERE\n[i] Ask Glass Agent\n[a] Launchers · [:] Routes\n\nYour prompt stays inside this workspace.".into()
            } else {
                "START HERE\n\n◆ [i] Ask Glass Agent\n◆ [a] Open launchers\n◆ [:] Search routes\n\nYour first prompt stays inside this workspace.".into()
            }
        } else {
            "SETUP REQUIRED\n\n◆ [s] Install/repair Pi runtime\n◆ [u] Refresh pinned runtime\n◆ [l] Sign in after setup\n◆ [i] Chat when Pi is ready".into()
        }
    } else {
        format!("TRANSCRIPT\n{conversation}")
    };
    if compact || area.width < 84 {
        render_scrollable_panel(
            frame,
            rows[1],
            " CONVERSATION · i compose ",
            format!(
                "{}\n\n{}\n\nNEXT ACTIONS\n{}\n\nKeys: Enter sends · Ctrl-D toggles steer/follow-up · PgUp/PgDn scroll · Esc closes",
                landing,
                agent_progress(state),
                context_next_actions(DevSurface::Agent)
            ),
            ACCENT_BRIGHT,
            state.current_scroll(),
        );
        return;
    }
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Min(24)])
        .split(rows[1]);
    render_scrollable_panel(
        frame,
        columns[0],
        " CONVERSATION · i compose ",
        format!(
            "{}\n\nKeys: Enter sends · Ctrl-D toggles steer/follow-up · Ctrl-X abort · PgUp/PgDn scroll · Esc closes",
            landing
        ),
        ACCENT_BRIGHT,
        state.current_scroll(),
    );
    let sidebar = format!(
        "FIRST RUN\n{}\n\n{}\n\nAGENT READINESS\n{}\n\nNEXT ACTIONS\n◆ [i] compose\n◆ [a] launchers\n◆ [:] routes\n◆ [s] setup · [l] sign in",
        if state.agent_readiness.starts_with("✓ Ready") {
            "Ready to ask"
        } else {
            "Setup required"
        },
        agent_progress(state),
        state.agent_readiness,
    );
    render_panel(frame, columns[1], " SESSION ", sidebar, PURPLE);
}
fn render_file_tree(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let items = if state.files.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No files discovered yet",
            Style::default().fg(MUTED),
        )))]
    } else {
        state
            .files
            .iter()
            .enumerate()
            .take(128)
            .map(|(index, path)| {
                let selected = index == state.selected_file;
                let focused = path == &state.focused_editor_path;
                let marker = if selected {
                    "›"
                } else if focused && state.focused_editor_dirty {
                    "●"
                } else {
                    " "
                };
                let icon = if path.ends_with('/') {
                    "▾ "
                } else if focused {
                    "▸ "
                } else {
                    "· "
                };
                let style = if selected {
                    Style::default()
                        .fg(ACCENT_BRIGHT)
                        .bg(ACTIVE_BACKGROUND)
                        .add_modifier(Modifier::BOLD)
                } else if focused && state.focused_editor_dirty {
                    Style::default().fg(WARNING)
                } else {
                    Style::default().fg(TEXT)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{marker} "), Style::default().fg(ACCENT)),
                    Span::styled(icon, Style::default().fg(MUTED)),
                    Span::styled(path.clone(), style),
                ]))
                .style(style)
            })
            .collect::<Vec<_>>()
    };
    let selected = state
        .files
        .get(state.selected_file)
        .map(|path| compact_path(path, area.width.saturating_sub(18)))
        .unwrap_or_else(|| "none".into());
    frame.render_widget(
        List::new(items)
            .style(Style::default().bg(PANEL_BACKGROUND))
            .block(surface_block(
                format!(" FILES · {} selected · j/k · Enter ", selected),
                ACCENT_BRIGHT,
            )),
        area,
    );
}

fn render_code_editor(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let title = if state.focused_editor_path.is_empty() {
        " EDITOR · select a file "
    } else if state.focused_editor_dirty {
        " EDITOR · unsaved "
    } else {
        " EDITOR · saved "
    };
    let content = if state.editor.trim().is_empty() {
        "No file open.\n\nSelect a file and press Enter.\nPress i to edit; Ctrl-S saves.".into()
    } else {
        state.editor.clone()
    };
    frame.render_widget(
        Paragraph::new(panel_text(&content))
            .style(Style::default().fg(TEXT).bg(PANEL_INSET))
            .scroll((state.current_scroll(), 0))
            .block(surface_block(title, ACCENT_BRIGHT))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_code_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    if stack_for_phone(state, area) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(area);
        render_file_tree(frame, state, rows[0]);
        render_code_editor(frame, state, rows[1]);
        return;
    }
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(28),
            Constraint::Min(36),
            Constraint::Length(28),
        ])
        .split(area);
    render_file_tree(frame, state, columns[0]);
    render_code_editor(frame, state, columns[1]);
    render_panel(
        frame,
        columns[2],
        " DIAGNOSTICS · lsp ",
        format!(
            "{}\n\nNEXT ACTIONS\nEnter open · i edit · Ctrl-S save\n:editor search QUERY\n:lsp diagnostics",
            if state.lsp.trim().is_empty() {
                "No diagnostics yet"
            } else {
                &state.lsp
            }
        ),
        WARNING,
    );
}

fn render_browser_visual(
    frame: &mut Frame<'_>,
    state: &DevTuiState,
    area: Rect,
    browser: &glass_browser::browser_workspace::BrowserWorkspaceState,
    compact: bool,
) {
    if state.browser_visual_live {
        if let Some(pane) = state.browser_pane.as_ref() {
            draw_ansi_pane(frame, area, pane);
            return;
        }
        if matches!(
            browser.presentation,
            glass_browser::browser_workspace::BrowserPresentationPath::Herdr
        ) {
            render_panel(
                frame,
                area,
                " VISUAL PLANE · Herdr pane ",
                "HERDR PANE\n\nLive browser frames are owned by the Herdr graphics pane.\n\nThe semantic inspector remains synchronized here while the pane renders pixels.\n\n[v] stop live view",
                SUCCESS,
            );
            return;
        }
    }
    let reason = browser
        .presentation_reason
        .as_deref()
        .unwrap_or("Request a visual drawer with v after the browser is connected.");
    let content = if compact {
        format!(
            "{}\n\n{}\n\n[n] address · [Enter] attach · [v] visual\nSemantic inspection remains available.",
            browser_progress(state, browser),
            reason,
        )
    } else {
        format!(
            "START HERE\n\n{}\n\nDIAGNOSTIC\n{}\n\nNEXT ACTIONS\n[n] address · [Enter] attach · [v] request visual\nSemantic inspection remains available while pixels are unavailable.",
            browser_progress(state, browser),
            reason,
        )
    };
    render_panel(
        frame,
        area,
        format!(" VISUAL PLANE · {} ", browser.presentation_label()),
        content,
        WARNING,
    );
}

fn render_browser_inspector(
    frame: &mut Frame<'_>,
    state: &DevTuiState,
    area: Rect,
    browser: &glass_browser::browser_workspace::BrowserWorkspaceState,
) {
    let entities = if browser.entities.is_empty() {
        "No semantic entities".into()
    } else {
        browser
            .entities
            .iter()
            .enumerate()
            .take(18)
            .map(|(index, entity)| {
                format!(
                    "{} {} · {}",
                    if Some(index) == browser.selected_entity {
                        "›"
                    } else {
                        " "
                    },
                    entity.name,
                    entity.role
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    render_panel(
        frame,
        area,
        " INSPECTOR · j/k select ",
        format!(
            "PAGE\n{}\n{}\n\nCONNECTION\n{} · rev {}\n\nOWNER\n{} · focus {}\n\nENTITIES\n{}\n\n{}\n\nWORKFLOW\n{}",
            browser.title,
            compact_path(&browser.url, area.width.saturating_sub(6)),
            browser.connection_label(),
            browser
                .browser_revision
                .map_or_else(|| "—".into(), |revision| revision.to_string()),
            browser.input_owner_label(),
            browser.focus_label(),
            entities,
            state.browser_detail,
            browser.workflow,
        ),
        ACCENT_BRIGHT,
    );
}

fn render_app_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let browser = state.browser_workspace.state();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(8),
            Constraint::Length(4),
        ])
        .split(area);
    let address = if browser.url.is_empty() {
        "No page · press n to enter an address".into()
    } else {
        compact_path(&browser.url, rows[0].width.saturating_sub(28))
    };
    let toolbar = format!(
        "{}  {}  {}  ·  {}\n{}\n[Alt-←/→] navigate · [Ctrl-R] reload · [n] address · [v] visual",
        if browser.loading {
            "◐ loading"
        } else {
            "● ready"
        },
        browser.connection_label(),
        address,
        activity_summary(state),
        browser_progress(state, browser),
    );
    render_panel(
        frame,
        rows[0],
        " BROWSER · visual + semantic ",
        toolbar,
        ACCENT_BRIGHT,
    );

    if stack_for_phone(state, rows[1]) {
        render_browser_visual(
            frame,
            state,
            rows[1],
            browser,
            compact_browser_copy(state, rows[1]),
        );
    } else {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(68), Constraint::Min(30)])
            .split(rows[1]);
        render_browser_visual(frame, state, columns[0], browser, false);
        render_browser_inspector(frame, state, columns[1], browser);
    }
    render_panel(
        frame,
        rows[2],
        " WORKFLOW · guarded actions ",
        format!(
            "{}\nNEXT ACTIONS  [n] address · [t] type · [v] visual · [H/G] handoff · : routes",
            state.workflow
        ),
        PURPLE,
    );
}

fn split_process_line(line: &str) -> (String, String, String) {
    let mut parts = line.splitn(2, ' ');
    let marker = parts.next().unwrap_or(" ");
    let rest = parts.next().unwrap_or("");
    let (name, detail) = rest.split_once(" · ").unwrap_or((rest, ""));
    (marker.to_string(), name.to_string(), detail.to_string())
}
fn status_line_count(content: &str) -> usize {
    content
        .lines()
        .filter(|line| {
            matches!(
                line.chars().next(),
                Some('●') | Some('○') | Some('×') | Some('!') | Some('✓')
            )
        })
        .count()
}

fn render_terminal_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(7),
            Constraint::Length(3),
        ])
        .split(area);
    let process_lines = state
        .processes
        .lines()
        .filter(|line| {
            line.starts_with('●')
                || line.starts_with('○')
                || line.starts_with('×')
                || line.starts_with('!')
        })
        .collect::<Vec<_>>();
    let process_count = process_lines.len();
    let healthy_count = process_lines
        .iter()
        .filter(|line| line.starts_with('●'))
        .count();
    let failed_count = process_lines
        .iter()
        .filter(|line| line.starts_with('×') || line.starts_with('!'))
        .count();
    render_panel(
        frame,
        rows[0],
        " TERMINALS · managed processes ",
        format!(
            "MANAGED TERMINALS   {process_count} process(es) · {healthy_count} healthy · {failed_count} attention\nBACKGROUND-SAFE PTYs · activity {}",
            activity_summary(state)
        ),
        ACCENT_BRIGHT,
    );
    if process_lines.is_empty() {
        render_panel(
            frame,
            rows[1],
            " PROCESS TABLE · s start · a launch ",
            "No managed terminals.\n\n[s] Start the detected suite\n[a] Open launchers\n:process start · :process logs · :process stop",
            ACCENT_BRIGHT,
        );
    } else {
        let table_rows = process_lines
            .into_iter()
            .map(|line| {
                let (marker, name, detail) = split_process_line(line);
                Row::new(vec![
                    Cell::from(marker.clone()).style(Style::default().fg(status_color(&marker))),
                    Cell::from(name),
                    Cell::from(detail).style(Style::default().fg(MUTED)),
                ])
            })
            .collect::<Vec<_>>();
        let table_columns = if stack_for_phone(state, area) {
            vec![
                Constraint::Length(3),
                Constraint::Length(14),
                Constraint::Min(10),
            ]
        } else {
            vec![
                Constraint::Length(3),
                Constraint::Percentage(32),
                Constraint::Min(24),
            ]
        };
        let table_header = if stack_for_phone(state, area) {
            vec!["", "PROCESS", "HEALTH · PID"]
        } else {
            vec!["", "PROCESS", "HEALTH · PID · COMMAND"]
        };
        frame.render_widget(
            Table::new(table_rows, table_columns)
                .header(
                    Row::new(table_header)
                        .style(Style::default().fg(MUTED).add_modifier(Modifier::BOLD)),
                )
                .column_spacing(1)
                .block(surface_block(
                    " PROCESS TABLE · s start · a launch ",
                    ACCENT_BRIGHT,
                )),
            rows[1],
        );
    }
    render_panel(
        frame,
        rows[2],
        " NEXT ACTIONS ",
        "s start detected suite · a open launchers · :process start · :process logs · :process stop",
        WARNING,
    );
}

fn render_task_list(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let items = if state.tasks.trim().is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No tasks · :task create TITLE PROMPT",
            Style::default().fg(MUTED),
        )))]
    } else {
        state
            .tasks
            .split("\n\n")
            .take(24)
            .map(|task| {
                let first = task.lines().next().unwrap_or(task);
                let style = Style::default().fg(status_color(first));
                ListItem::new(panel_text(task)).style(style)
            })
            .collect::<Vec<_>>()
    };
    let task_count = status_line_count(&state.tasks);
    frame.render_widget(
        List::new(items)
            .style(Style::default().bg(PANEL_BACKGROUND))
            .block(surface_block(
                format!(" TASK DAG · {task_count} task(s) · status + verification "),
                ACCENT_BRIGHT,
            )),
        area,
    );
}

fn render_tasks_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let rows = if stack_for_phone(state, area) {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(66), Constraint::Percentage(34)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Min(24)])
            .split(area)
    };
    render_task_list(frame, state, rows[0]);
    let running = state
        .tasks
        .lines()
        .filter(|line| line.starts_with("●"))
        .count();
    let queued = state
        .tasks
        .lines()
        .filter(|line| line.starts_with("○"))
        .count();
    let failed = state
        .tasks
        .lines()
        .filter(|line| line.starts_with("×"))
        .count();
    render_panel(
        frame,
        rows[1],
        " TASK SUMMARY ",
        format!(
            "TASKS\n{running} running · {queued} queued · {failed} failed\n\nFLOW\nlaunch → verify → evidence\n\nNEXT ACTIONS\n[a] task launchers\n:task create\n:task cancel\n:task retry\n\nEvidence and verification stay attached to each task.",
        ),
        PURPLE,
    );
}

fn diff_text(content: &str) -> Text<'static> {
    Text::from(
        content
            .lines()
            .map(|line| {
                let color = if line.starts_with('+') && !line.starts_with("+++") {
                    SUCCESS
                } else if line.starts_with('-') && !line.starts_with("---") {
                    ERROR
                } else if line.starts_with("@@") {
                    ACCENT_BRIGHT
                } else {
                    TEXT
                };
                Line::from(Span::styled(line.to_string(), Style::default().fg(color)))
            })
            .collect::<Vec<_>>(),
    )
}

fn render_git_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    if state.git_diff_open {
        frame.render_widget(
            Paragraph::new(diff_text(&state.git_diff))
                .style(Style::default().bg(PANEL_INSET))
                .scroll((state.current_scroll(), 0))
                .block(surface_block(
                    " DIFF · Esc closes · PgUp/PgDn scroll ",
                    ACCENT_BRIGHT,
                ))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let branch = state.git.lines().next().unwrap_or("branch unavailable");
    let change_count = state
        .git
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("M ")
                || trimmed.starts_with("A ")
                || trimmed.starts_with("D ")
                || trimmed.starts_with("R ")
                || trimmed.starts_with("??")
                || trimmed.starts_with("✓")
        })
        .count();
    let columns = if stack_for_phone(state, area) {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(38), Constraint::Min(34)])
            .split(area)
    };
    render_status_list(
        frame,
        columns[0],
        format!(" CHANGES · {change_count} item(s) · d diff "),
        &state.git,
        "No changes detected",
    );
    render_panel(
        frame,
        columns[1],
        " SOURCE CONTROL ",
        format!(
            "GIT WORKSPACE\n{}\n\nCHANGES  {change_count} item(s)\n\nNEXT ACTIONS\n[d] open inline diff\n[a] launchers\n:git stage · :git commit · :git branches\n\nChanged lines use green/red markers in the diff.",
            branch
        ),
        PURPLE,
    );
}

fn render_debug_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let rows = if stack_for_phone(state, area) {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(56), Constraint::Min(28)])
            .split(area)
    };
    let session_count = status_line_count(&state.debugger);
    let test_status = state
        .tests
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("No test runs");
    render_status_list(
        frame,
        rows[0],
        format!(" DEBUG SESSIONS · {session_count} · a launch "),
        &state.debugger,
        "No debugger sessions",
    );
    render_panel(
        frame,
        rows[1],
        " TEST LAB ",
        format!(
            "TEST RESULTS\n{test_status}\n\nDEBUG FLOW\nlaunch → attach → inspect\n\nNEXT ACTIONS\n[a] debugger/test launchers\n:debug start\n:test run\n:test results",
        ),
        WARNING,
    );
}

fn render_more_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(7)])
        .split(area);
    let narrow = area.width < 60
        || matches!(
            state.responsive_class(area.width, area.height),
            ResponsiveClass::Phone
        );
    let kernel_count = state
        .kernels
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    render_panel(
        frame,
        rows[0],
        " SERVICES · a launch ",
        format!(
            "WORKSPACE  {} skills · {} tools · {kernel_count} kernels\nACTIVITY  {}\n{}",
            state.snapshot_skills_count,
            state.snapshot_tools_count,
            activity_summary(state),
            agent_progress(state)
        ),
        ACCENT_BRIGHT,
    );
    let columns = if narrow {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(38),
                Constraint::Percentage(31),
                Constraint::Percentage(31),
            ])
            .split(rows[1])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(34),
                Constraint::Percentage(33),
                Constraint::Min(24),
            ])
            .split(rows[1])
    };
    render_panel(
        frame,
        columns[0],
        " PI READINESS ",
        if narrow {
            format!(
                "{}\n{}\n{kernel_count} kernels",
                state
                    .agent_readiness
                    .lines()
                    .next()
                    .unwrap_or("Pi unavailable"),
                agent_progress(state)
            )
        } else {
            format!(
                "{}\n\n{}\n\nKERNELS\n{}",
                state.agent_readiness,
                agent_progress(state),
                state.kernels
            )
        },
        PURPLE,
    );
    render_panel(
        frame,
        columns[1],
        " EXPERIMENTS ",
        if narrow {
            format!(
                "{}\n{}",
                state.experiments.lines().next().unwrap_or("No experiments"),
                state.replay.lines().next().unwrap_or("No replay")
            )
        } else {
            format!(
                "{}\n\nREPLAY / OPERATIONS\n{}",
                state.experiments, state.replay
            )
        },
        ACCENT_BRIGHT,
    );
    render_panel(
        frame,
        columns[2],
        " ROUTES ",
        if narrow {
            String::from("a launch\n:workspace\n:experiment create\n:kernel start\n:replay")
        } else {
            String::from(
                "NEXT ACTIONS\n\na service launch\n:workspace\n:experiment create\n:kernel start\n:replay\n\nUse the command center for complete routes.",
            )
        },
        WARNING,
    );
}

fn render_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    if state.menu_open {
        let search_index = state.surface_actions().len();
        let quit_index = state.quit_menu_index();
        let items = state
            .surface_actions()
            .iter()
            .enumerate()
            .map(|(index, action)| {
                let selected = index == state.menu_selection;
                let launch = if action.key == ":" {
                    format!(":{}", action.command)
                } else {
                    action.command.to_string()
                };
                let item_style = if selected {
                    Style::default()
                        .fg(ACCENT_BRIGHT)
                        .bg(ACTIVE_BACKGROUND)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TEXT)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        if selected { "› " } else { "  " },
                        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("[{}] ", action.key),
                        Style::default()
                            .fg(if selected { Color::Black } else { ACCENT })
                            .bg(if selected { ACCENT } else { PANEL_BACKGROUND })
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(action.label, item_style),
                    Span::styled(format!(" · {launch}"), Style::default().fg(MUTED)),
                ]))
                .style(item_style)
            })
            .chain(std::iter::once({
                let selected = state.menu_selection == search_index;
                let item_style = if selected {
                    Style::default()
                        .fg(ACCENT_BRIGHT)
                        .bg(ACTIVE_BACKGROUND)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TEXT)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        if selected { "› " } else { "  " },
                        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "[:] ",
                        Style::default()
                            .fg(if selected { Color::Black } else { ACCENT })
                            .bg(if selected { ACCENT } else { PANEL_BACKGROUND })
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("Search all commands", item_style),
                    Span::styled(" · type a route and Tab", Style::default().fg(MUTED)),
                ]))
                .style(item_style)
            }))
            .chain(std::iter::once({
                let selected = state.menu_selection == quit_index;
                let item_style = if selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(WARNING)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(WARNING)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        if selected { "› " } else { "  " },
                        Style::default().fg(WARNING).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "[q] ",
                        Style::default()
                            .fg(if selected { Color::Black } else { WARNING })
                            .bg(if selected { WARNING } else { PANEL_BACKGROUND })
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("Quit Glass Dev", item_style),
                    Span::styled(" · ask before exiting", Style::default().fg(MUTED)),
                ]))
                .style(item_style)
            }))
            .collect::<Vec<_>>();
        let selected_description = if state.menu_selection == search_index {
            "Search every Glass route by root command."
        } else if state.menu_selection == quit_index {
            "Close the TUI after an explicit quit confirmation."
        } else {
            state
                .surface_actions()
                .get(state.menu_selection)
                .map(|action| action.description)
                .unwrap_or("Choose an action.")
        };
        let group = command::command_group_for(state.surface);
        let rows = if area.height >= 11 {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(5), Constraint::Length(5)])
                .split(area)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(0)])
                .split(area)
        };
        let mut menu_state = ListState::default();
        menu_state.select(Some(state.menu_selection));
        frame.render_stateful_widget(
            List::new(items)
                .style(Style::default().bg(PANEL_BACKGROUND))
                .block(
                    Block::default()
                        .title(format!(" COMMAND CENTER · {} ", state.surface.label()))
                        .title_style(
                            Style::default()
                                .fg(ACCENT_BRIGHT)
                                .add_modifier(Modifier::BOLD),
                        )
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(PANEL_BORDER))
                        .padding(Padding::horizontal(1)),
                ),
            rows[0],
            &mut menu_state,
        );
        if rows[1].height > 0 {
            frame.render_widget(
                Paragraph::new(panel_text(&format!(
                    "{}\n\nROUTES\n{} · {}\n\nTry `{}` · j/k select · Enter run · Esc close",
                    selected_description,
                    group.label,
                    group.roots.join(" · "),
                    group.example,
                )))
                .style(Style::default().fg(TEXT))
                .block(
                    Block::default()
                        .title(" ACTION DETAILS ")
                        .title_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(PANEL_BORDER))
                        .bg(PANEL_BACKGROUND)
                        .padding(Padding::horizontal(1)),
                )
                .wrap(Wrap { trim: false }),
                rows[1],
            );
        }
        return;
    }
    if state.browser_target_picker {
        frame.render_widget(
            Paragraph::new(panel_text(&state.browser_target_picker_view()))
                .style(Style::default().fg(TEXT))
                .block(
                    Block::default()
                        .title(" BROWSER TARGETS · select page ")
                        .title_style(
                            Style::default()
                                .fg(ACCENT_BRIGHT)
                                .add_modifier(Modifier::BOLD),
                        )
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(ACCENT))
                        .bg(PANEL_BACKGROUND)
                        .padding(Padding::horizontal(1)),
                )
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    if let Some(offer) = state.browser_recovery.as_ref() {
        let actions = offer
            .actions()
            .iter()
            .map(|(key, description)| format!("[{key}] {description}"))
            .collect::<Vec<_>>()
            .join("\n");
        frame.render_widget(
            Paragraph::new(panel_text(&format!(
                "BROWSER RECOVERY\n\nport {}\n{}\n\n{}\n\n{}",
                offer.port,
                offer.guidance(),
                offer.reason,
                actions
            )))
            .style(Style::default().fg(TEXT))
            .block(
                Block::default()
                    .title(" RECOVERY · browser ")
                    .title_style(Style::default().fg(WARNING).add_modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(WARNING))
                    .bg(PANEL_BACKGROUND)
                    .padding(Padding::horizontal(1)),
            )
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    if let Some(pending) = state.pending_agent_approval.as_ref() {
        frame.render_widget(
            Paragraph::new(panel_text(&format!(
                "PI TOOL APPROVAL\n\nAgent: {}\nTool: {}\nArguments: {}\n\n[Y / Enter] Approve once\n[N / Esc] Deny\n\nGlass will return the decision to the resident agent.",
                pending.agent_id,
                pending.tool_name,
                pending.arguments
            )))
            .style(Style::default().fg(TEXT))
            .block(
                Block::default()
                    .title(" APPROVAL · resident agent ")
                    .title_style(Style::default().fg(WARNING).add_modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(WARNING))
                    .bg(PANEL_BACKGROUND)
                    .padding(Padding::horizontal(1)),
            )
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    if let Some(pending) = state.pending_confirmation.as_ref() {
        frame.render_widget(
            Paragraph::new(panel_text(&format!(
                "CONFIRM ONE MUTATION\n\n{}\n\n[Y / Enter] Approve once\n[N / Esc] Deny\n\nThe frozen call cannot authorize a retry or changed arguments.",
                pending.summary
            )))
            .style(Style::default().fg(TEXT))
            .block(
                Block::default()
                    .title(" CONFIRMATION · one use ")
                    .title_style(Style::default().fg(WARNING).add_modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(WARNING))
                    .bg(PANEL_BACKGROUND)
                    .padding(Padding::horizontal(1)),
            )
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    match state.surface {
        DevSurface::Trust => render_trust_surface(frame, state, area),
        DevSurface::Agent => render_agent_surface(frame, state, area),
        DevSurface::Code => render_code_surface(frame, state, area),
        DevSurface::App => render_app_surface(frame, state, area),
        DevSurface::Terminal => render_terminal_surface(frame, state, area),
        DevSurface::Tasks => render_tasks_surface(frame, state, area),
        DevSurface::Git => render_git_surface(frame, state, area),
        DevSurface::Debug => render_debug_surface(frame, state, area),
        DevSurface::More => render_more_surface(frame, state, area),
    }
}

/// Paint an ANSI half-block browser frame into a bounded visual plane.
fn draw_ansi_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    pane: &glass_browser::tui::live_view::AnsiPane,
) {
    if pane.columns == 0 || pane.rows == 0 || area.width < 2 || area.height < 2 {
        return;
    }
    let frame_block = surface_block(" VISUAL PLANE · live pixels ", ACCENT_BRIGHT);
    frame.render_widget(frame_block, area);
    let inner = Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    for (row_index, row_cells) in pane.cells.chunks(pane.columns as usize).enumerate() {
        let y = inner.y + row_index as u16;
        if y >= inner.bottom() {
            break;
        }
        for (column_index, cell) in row_cells.iter().enumerate() {
            let x = inner.x + column_index as u16;
            if x >= inner.right() {
                break;
            }
            if let Some(target) = frame.buffer_mut().cell_mut((x, y)) {
                target.set_char('▀');
                target.set_fg(Color::Rgb(cell.top.red, cell.top.green, cell.top.blue));
                target.set_bg(Color::Rgb(
                    cell.bottom.red,
                    cell.bottom.green,
                    cell.bottom.blue,
                ));
            }
        }
    }
}
fn render_context(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let authority = format!(
        "\n\nAUTHORITY\n{} · project rev {}\nmutations require revision + confirmation\n\nNEXT\n{}",
        state.snapshot_trust_label.as_str(),
        state.snapshot_project_revision,
        context_next_actions(state.surface)
    );
    let content = match state.surface {
        DevSurface::Trust => format!(
            "TRUST DECISION\n{} configuration item(s) need attention{}",
            state
                .snapshot_trust_inspection
                .iter()
                .filter(|item| item.trust_required)
                .count(),
            authority
        ),
        DevSurface::Agent => {
            let working = state
                .agents
                .lines()
                .any(|line| line.to_ascii_lowercase().contains("working"));
            let model = state
                .agents
                .lines()
                .find_map(|line| line.split("model ").nth(1).map(str::to_string));
            if state.selected_agent.is_some() || state.agents.lines().count() > 1 {
                format!(
                    "ACTIVE AGENT\n{} · model {} · events in Inspect{}{}",
                    if working { "● working" } else { "○ idle" },
                    model.as_deref().unwrap_or("default"),
                    if state.browser_workspace.state().selected().is_some() {
                        "\napp entity attached as context"
                    } else {
                        ""
                    },
                    authority
                )
            } else {
                format!(
                    "AGENT READINESS\n{}{}",
                    state.agent_readiness.lines().next().unwrap_or(""),
                    authority
                )
            }
        }
        DevSurface::Code => {
            let selected = (!state.focused_editor_path.is_empty()).then(|| {
                (
                    state.focused_editor_path.clone(),
                    state.focused_editor_dirty,
                    state.focused_editor_line,
                )
            });
            match selected {
                Some((path, dirty, line)) => format!(
                    "EDITING\n{}{} · line {}\n\nLINKED APP\n{}{}",
                    if dirty { "● " } else { "○ " },
                    path,
                    line,
                    state
                        .browser_workspace
                        .state()
                        .selected()
                        .map(|entity| format!("{} · {}", entity.name, entity.reference))
                        .unwrap_or_else(|| "No current source/runtime link".into()),
                    authority
                ),
                None => format!(
                    "FILES\n{} project file(s) listed{}",
                    state.files.len(),
                    authority
                ),
            }
        }
        DevSurface::App => {
            let browser = state.browser_workspace.state();
            format!(
                "SELECTED APP ENTITY\n{}\n\nBROWSER\n{} · rev {}\nVISUAL {} · input {}\nFOCUS {}\n{}\n{}",
                browser
                    .selected()
                    .map(|entity| format!(
                        "◆ {} · {} · {}",
                        entity.name, entity.role, entity.reference
                    ))
                    .unwrap_or_else(|| "No semantic entity selected".into()),
                browser.connection_label(),
                browser
                    .browser_revision
                    .map_or_else(|| "—".into(), |revision| revision.to_string()),
                browser.presentation_label(),
                browser.input_owner_label(),
                browser.focus_label(),
                browser
                    .presentation_reason
                    .as_deref()
                    .unwrap_or("visual plane available"),
                authority
            )
        }
        DevSurface::Terminal => format!(
            "PROCESSES\n{}{}",
            state
                .processes
                .lines()
                .next()
                .unwrap_or("No process selected"),
            authority
        ),
        DevSurface::Tasks => {
            let running = state
                .tasks
                .lines()
                .filter(|line| line.starts_with("●"))
                .count();
            format!("TASKS\n{running} running{}", authority)
        }
        DevSurface::Git => format!(
            "SOURCE CONTROL\n{}{}",
            state.git.lines().next().unwrap_or("No change selected"),
            authority
        ),
        DevSurface::Debug => format!(
            "DEBUG\n{}{}",
            state.debugger.lines().next().unwrap_or("No frame selected"),
            authority
        ),
        DevSurface::More => format!(
            "PROJECT SERVICES\n{} skills · {} custom tools{}",
            state.snapshot_skills_count, state.snapshot_tools_count, authority
        ),
    };
    frame.render_widget(
        Paragraph::new(panel_text(&content))
            .style(Style::default().fg(TEXT).bg(PANEL_BACKGROUND))
            .block(
                Block::default()
                    .title(" LIVE CONTEXT ")
                    .title_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(PANEL_BORDER))
                    .bg(PANEL_BACKGROUND)
                    .padding(Padding::horizontal(1)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn input_spans(
    prefix: &str,
    prefix_style: Style,
    input: &str,
    cursor: usize,
    width: u16,
) -> Vec<Span<'static>> {
    let (before, after) = input_preview(input, cursor, width);
    vec![
        Span::styled(prefix.to_string(), prefix_style),
        Span::styled(before, Style::default().fg(TEXT)),
        Span::styled("▌", Style::default().fg(ACCENT_BRIGHT)),
        Span::styled(after, Style::default().fg(TEXT)),
    ]
}

fn input_preview(input: &str, cursor: usize, width: u16) -> (String, String) {
    let chars = input.chars().collect::<Vec<_>>();
    let cursor = input
        .char_indices()
        .take_while(|(index, _)| *index < cursor.min(input.len()))
        .count();
    let max_input = usize::from(width.max(8)).saturating_sub(1);
    if chars.len() <= max_input {
        return (
            chars[..cursor].iter().collect(),
            chars[cursor..].iter().collect(),
        );
    }

    let window = max_input.saturating_sub(2);
    let mut before_capacity = window / 2;
    let mut after_capacity = window - before_capacity;
    let before_available = cursor;
    let after_available = chars.len().saturating_sub(cursor);
    if before_available < before_capacity {
        after_capacity += before_capacity - before_available;
        before_capacity = before_available;
    }
    if after_available < after_capacity {
        before_capacity =
            (before_capacity + after_capacity - after_available).min(before_available);
        after_capacity = after_available;
    }

    let start = cursor.saturating_sub(before_capacity);
    let end = (cursor + after_capacity).min(chars.len());
    let mut before = String::new();
    if start > 0 {
        before.push('…');
    }
    before.extend(chars[start..cursor].iter());
    let mut after = chars[cursor..end].iter().collect::<String>();
    if end < chars.len() {
        after.push('…');
    }
    (before, after)
}

fn status_style(state: &DevTuiState) -> Style {
    let status = state.status.to_ascii_lowercase();
    if status.contains("error")
        || status.contains("failed")
        || status.contains("denied")
        || status.contains("cancelled")
    {
        Style::default().fg(ERROR)
    } else if status.contains("trust")
        || status.contains("confirm")
        || status.contains("approval")
        || status.contains("required")
    {
        Style::default().fg(WARNING)
    } else if state.composer_mode || state.command_mode {
        Style::default().fg(ACCENT_BRIGHT)
    } else {
        Style::default().fg(SUCCESS)
    }
}

fn render_status(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let (glyph, glyph_color) = status_glyph(state);
    let lines = if state.composer_mode {
        vec![
            Line::from(input_spans(
                "> ",
                Style::default()
                    .fg(ACCENT_BRIGHT)
                    .add_modifier(Modifier::BOLD),
                &state.composer_input,
                state.composer_cursor,
                area.width.saturating_sub(5),
            )),
            Line::from(vec![
                Span::styled(
                    format!(" {glyph} "),
                    Style::default()
                        .fg(glyph_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(state.status.clone(), status_style(state)),
                Span::styled(
                    " · Enter sends · Ctrl-D toggles steer/follow-up · Esc closes",
                    Style::default().fg(MUTED),
                ),
            ]),
        ]
    } else if state.command_mode {
        let group = command::command_group_for(state.surface);
        let suggestions = state.palette_matches().join(" · ");
        let error = state
            .palette_error
            .as_deref()
            .map(|error| format!(" · {error}"))
            .unwrap_or_default();
        let first_line = if state.command_input.trim().is_empty() {
            {
                let mut spans = input_spans(
                    ": ",
                    Style::default().fg(ACCENT_BRIGHT),
                    &state.command_input,
                    state.command_cursor,
                    area.width.saturating_sub(8),
                );
                spans.push(Span::styled(
                    format!("  {}", state.status),
                    status_style(state),
                ));
                spans.push(Span::styled(
                    format!(" · try `{}`", group.example),
                    Style::default().fg(MUTED),
                ));
                Line::from(spans)
            }
        } else {
            {
                let mut spans = input_spans(
                    ": ",
                    Style::default().fg(ACCENT_BRIGHT),
                    &state.command_input,
                    state.command_cursor,
                    area.width.saturating_sub(8),
                );
                spans.push(Span::styled(
                    format!("  [{suggestions}]"),
                    Style::default().fg(MUTED),
                ));
                Line::from(spans)
            }
        };
        vec![
            first_line,
            Line::from(Span::styled(
                format!(
                    "{glyph} Tab completes roots · ↑/↓ history · Enter runs · Esc closes{error}"
                ),
                Style::default().fg(MUTED),
            )),
        ]
    } else {
        let mut line = vec![
            Span::styled(
                format!(" {glyph} "),
                Style::default()
                    .fg(glyph_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(state.status.clone(), status_style(state)),
        ];
        if state.refresh_latency_ms >= 200 {
            line.push(Span::styled(
                format!(" · refresh {}ms", state.refresh_latency_ms),
                Style::default().fg(MUTED),
            ));
        }
        let activity = activity_summary(state);
        if activity != "idle" {
            line.push(Span::styled(
                format!(" · {activity}"),
                Style::default().fg(ACCENT_BRIGHT),
            ));
        }
        vec![Line::from(line)]
    };
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(PANEL_BACKGROUND))
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(PANEL_BORDER))
                    .padding(Padding::horizontal(1)),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use glass_browser::browser_workspace::BrowserWorkspaceIntent;
    use glass_browser::cli::args::TuiLayout;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn state(layout: TuiLayout) -> DevTuiState {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("glass-dev-tui-{}-{sequence}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .unwrap();
        DevTuiState::open(root, layout).unwrap()
    }

    fn rendered(state: &DevTuiState, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, state)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    #[test]
    fn desktop_and_phone_prioritize_resident_workspace_state() {
        for (width, height, layout) in [(140, 40, TuiLayout::Desktop), (48, 18, TuiLayout::Mobile)]
        {
            let mut state = state(layout);
            state.status = "ASYNC STATUS VISIBLE".into();
            let rendered = rendered(&state, width, height);
            assert!(rendered.contains("GLASS DEV"));
            if width < 72 {
                assert!(rendered.contains("ASYNC STATUS VISIBLE"));
            }
            assert!(rendered.contains("Agent"));
            assert!(rendered.contains("CONVERSATION"));
        }
    }

    #[test]
    fn agent_landing_and_status_markers_are_scannable() {
        let mut state = state(TuiLayout::Mobile);
        let output = rendered(&state, 48, 18);
        if state.agent_readiness.starts_with("✓ Ready") {
            assert!(output.contains("START HERE"));
            assert!(output.contains("Ask Glass Agent"));
        } else {
            assert!(output.contains("SETUP REQUIRED"));
            assert!(output.contains("Install/repair Pi runtime"));
        }
        assert!(output.contains("APP · Detached · no page · idle"));
        assert!(output.contains("● Ready"));
        assert!(output.contains("─"));

        state.status = "browser failed".into();
        assert!(rendered(&state, 48, 18).contains("× browser failed"));
    }

    #[test]
    fn quit_confirmation_renders_a_clear_modal() {
        let mut state = state(TuiLayout::Desktop);
        state.request_quit();
        let output = rendered(&state, 80, 24);
        assert!(output.contains("QUIT GLASS DEV?"));
        assert!(output.contains("Enter / Y"));
        assert!(output.contains("Esc / N"));
    }

    #[test]
    fn browser_header_omits_redundant_empty_page_title() {
        let state = state(TuiLayout::Desktop);
        let header = state.browser_chat_header();
        assert_eq!(header.matches("no page").count(), 1);
        assert!(!header.contains("No page · no page"));
    }

    #[test]
    fn empty_surface_counts_ignore_placeholder_copy() {
        let mut state = state(TuiLayout::Desktop);

        state.surface = DevSurface::Tasks;
        assert!(rendered(&state, 140, 40).contains("TASK DAG · 0 task(s)"));

        state.surface = DevSurface::Debug;
        assert!(rendered(&state, 140, 40).contains("DEBUG SESSIONS · 0"));

        state.surface = DevSurface::Terminal;
        let terminal = rendered(&state, 140, 40);
        assert!(terminal.contains("0 process(es)"));
        assert!(terminal.contains("[s] Start the detected suite"));
    }

    #[test]
    fn surface_empty_states_show_guided_next_actions() {
        let mut desktop = state(TuiLayout::Desktop);
        for (surface, marker) in [
            (DevSurface::App, "START HERE"),
            (DevSurface::Terminal, "NEXT ACTIONS"),
            (DevSurface::Tasks, "NEXT ACTIONS"),
            (DevSurface::Debug, "NEXT ACTIONS"),
            (DevSurface::More, "NEXT ACTIONS"),
        ] {
            desktop.surface = surface;
            assert!(
                rendered(&desktop, 140, 40).contains(marker),
                "{surface:?} should expose {marker}"
            );
        }

        let compact = state(TuiLayout::Desktop);
        assert!(rendered(&compact, 64, 24).contains("m More"));
    }

    #[test]
    fn progress_overlays_and_visual_fallback_diagnostics_are_visible() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Agent;
        let agent = rendered(&state, 140, 40);
        assert!(agent.contains("AGENT PROGRESS"));
        if state.agent_readiness.starts_with("✓ Ready") {
            assert!(agent.contains("ready for a prompt"));
        } else {
            assert!(agent.contains("setup required"));
        }
        state.surface = DevSurface::App;
        let fallback = rendered(&state, 140, 40);
        assert!(fallback.contains("BROWSER PROGRESS"));
        assert!(fallback.contains("DIAGNOSTIC"));

        state.browser_visual_live = true;
        state.browser_workspace.state_mut().presentation =
            glass_browser::browser_workspace::BrowserPresentationPath::Herdr;
        let herdr = rendered(&state, 140, 40);
        assert!(herdr.contains("HERDR PANE"));
        assert!(herdr.contains("semantic inspector"));
    }

    #[test]
    fn every_surface_exposes_a_distinct_workbench_hierarchy() {
        let mut state = state(TuiLayout::Desktop);
        for (surface, markers) in [
            (DevSurface::Trust, ["WORKSPACE TRUST", "CONFIGURATION"]),
            (DevSurface::Agent, ["CONVERSATION", "FIRST RUN"]),
            (DevSurface::Code, ["FILES", "EDITOR"]),
            (DevSurface::App, ["VISUAL PLANE", "INSPECTOR"]),
            (DevSurface::Terminal, ["PROCESS TABLE", "NEXT ACTIONS"]),
            (DevSurface::Tasks, ["TASK DAG", "TASK SUMMARY"]),
            (DevSurface::Git, ["CHANGES", "SOURCE CONTROL"]),
            (DevSurface::Debug, ["DEBUG SESSIONS", "TEST LAB"]),
            (DevSurface::More, ["PI READINESS", "NEXT ACTIONS"]),
        ] {
            state.surface = surface;
            let output = rendered(&state, 220, 40);
            for marker in markers {
                assert!(
                    output.contains(marker),
                    "{surface:?} should expose {marker}"
                );
            }
        }
    }

    #[test]
    fn help_scroll_and_git_diff_keep_small_cockpits_interactive() {
        let mut state = state(TuiLayout::Mobile);
        state.toggle_help();
        state.scroll_help(8);
        assert_eq!(state.help_scroll, 8);
        assert!(rendered(&state, 48, 18).contains("APP"));

        state.help_open = false;
        state.surface = DevSurface::Git;
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);
        state.queue_git_diff(&mut worker);
        assert!(state.git_diff_open);
        assert!(state.running_tool_job.is_some());
        assert!(state.git_diff.contains("Loading"));
        drop(worker);
    }

    #[test]
    fn phone_sizes_keep_agent_code_app_tasks_more_flows_reachable() {
        for (width, height) in [(48, 18), (64, 24), (80, 24)] {
            let mut state = state(TuiLayout::Mobile);
            for (key, expected) in [
                ('1', DevSurface::Agent),
                ('2', DevSurface::Code),
                ('3', DevSurface::App),
                ('4', DevSurface::Tasks),
                ('5', DevSurface::More),
            ] {
                state.handle_printable(key);
                assert_eq!(state.surface, expected);
                assert!(rendered(&state, width, height).contains(expected.label()));
            }
        }
    }

    #[test]
    fn composer_palette_focus_and_local_scroll_are_interactive_state() {
        let mut state = state(TuiLayout::Desktop);
        state.open_composer();
        state.insert_composer_text("hello");
        state.composer_backspace();
        assert_eq!(state.composer_input, "hell");
        state.close_composer();

        state.open_palette();
        assert!(rendered(&state, 120, 32).contains("▌"));
        assert!(rendered(&state, 120, 32).contains("try `agent prompt TEXT`"));
        state.insert_palette_text("browser");
        state.move_palette_cursor(false);
        state.palette_backspace();
        state.insert_palette_char('e');
        assert!(state.command_cursor < state.command_input.len());
        assert!(state.palette_matches().contains(&"browser"));

        state.close_palette();
        state.command_history = vec!["agent status".into(), "browser observe".into()];
        state.open_palette();
        state.navigate_palette_history(true);
        assert_eq!(state.command_input, "browser observe");
        state.navigate_palette_history(true);
        assert_eq!(state.command_input, "agent status");
        state.navigate_palette_history(false);
        assert_eq!(state.command_input, "browser observe");
        state.command_input = "brw".into();
        state.command_cursor = state.command_input.len();
        state.complete_palette();
        assert_eq!(state.command_input, "browser");

        state.close_palette();
        let surface = state.surface;
        state.scroll_surface(3);
        assert_eq!(state.surface, surface);
        assert_eq!(state.current_scroll(), 3);
    }

    #[test]
    fn surface_action_menu_is_discoverable_and_runs_or_prefills() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Agent;
        state.open_menu();
        assert!(!state.surface_actions().is_empty());
        state.move_menu_selection(3);
        state.run_menu_action();
        // `agent setup login` carries an argument, so the palette opens prefilled.
        assert!(state.command_mode);
        assert!(state.command_input.starts_with("agent setup login"));
        state.close_palette();

        state.surface = DevSurface::App;
        state.open_menu();
        state.run_menu_action();
        // `browser start` has no argument requirement in the menu, palette prefilled.
        assert!(state.command_input.starts_with("browser start"));
        state.close_palette();
        state.open_menu();
        state.menu_selection = 2;
        state.run_menu_action();
        assert_eq!(state.command_input, "browser navigate ");

        state.surface = DevSurface::Terminal;
        state.open_menu();
        state.run_menu_action();
        assert!(state.status.contains("No development command detected"));
        state.open_menu();
        state.menu_selection = 1;
        state.run_menu_action();
        assert_eq!(state.command_input, "process start dev ");

        state.surface = DevSurface::More;
        state.open_menu();
        state.run_menu_action();
        assert!(state.status.contains("No development command detected"));
    }

    #[test]
    fn redesigned_action_menu_keeps_selection_and_context_visible() {
        let mut desktop = state(TuiLayout::Desktop);
        desktop.surface = DevSurface::Agent;
        desktop.open_menu();
        let output = rendered(&desktop, 118, 32);
        assert!(output.contains("COMMAND CENTER · Agent"));
        assert!(output.contains("[i]"));
        assert!(output.contains("ACTION DETAILS"));
        assert!(output.contains("ask the resident Glass Agent"));

        let mut mobile = state(TuiLayout::Mobile);
        mobile.surface = DevSurface::Agent;
        mobile.open_menu();
        mobile.menu_selection = mobile.surface_actions().len();
        assert!(rendered(&mobile, 48, 18).contains("Search all commands"));
    }

    #[test]
    fn phone_layout_stacks_surface_panels_without_browser_inspector() {
        let mut state = state(TuiLayout::Mobile);
        state.surface = DevSurface::More;
        let more = rendered(&state, 160, 24);
        let readiness = more.find("PI READINESS").expect("readiness panel");
        let experiments = more.find("EXPERIMENTS").expect("experiments panel");
        let routes = more.find("ROUTES").expect("routes panel");
        assert!(readiness < experiments && experiments < routes);

        state.surface = DevSurface::App;
        let app = rendered(&state, 160, 24);
        assert!(app.contains("VISUAL PLANE"));
        assert!(!app.contains("INSPECTOR"));
    }

    #[test]
    fn phone_composer_preserves_status_for_long_drafts() {
        let mut state = state(TuiLayout::Mobile);
        state.surface = DevSurface::Agent;
        state.open_composer();
        state.insert_composer_text(
            "Inspect the workspace and summarize the three most important development commands",
        );

        let output = rendered(&state, 48, 18);
        assert!(output.contains("…"));
        assert!(output.contains("development commands"));
        assert!(output.contains("Agent composer"));
        assert!(output.contains("Enter send"));
        assert!(output.contains("▌"));
    }

    #[test]
    fn long_input_preview_keeps_cursor_context_visible() {
        let input = "0123456789abcdefghij";
        let (before, after) = input_preview(input, 10, 12);
        assert!(before.starts_with('…'));
        assert!(before.contains('9'));
        assert!(after.starts_with('a'));
        assert!(after.ends_with('…'));
        assert!(before.chars().count() + after.chars().count() < 12);

        let (before, after) = input_preview(input, input.len(), 12);
        assert!(before.starts_with('…'));
        assert!(before.ends_with('j'));
        assert!(after.is_empty());
    }

    #[test]
    fn composer_ctrl_d_mode_toggle_is_visible_and_stateful() {
        let mut state = state(TuiLayout::Mobile);
        state.surface = DevSurface::Agent;
        state.open_composer();
        assert!(!state.composer_steer);
        state.toggle_composer_steer();
        assert!(state.composer_steer);
        assert!(state.status.contains("Steer mode"));
        assert!(rendered(&state, 118, 32).contains("toggle steer/follow-up"));
        state.toggle_composer_steer();
        assert!(!state.composer_steer);
        assert!(state.status.contains("Follow-up mode"));
    }
    #[test]
    fn agent_transcript_scroll_follows_surface_navigation() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Agent;
        state.agent_conversation = (1..=20)
            .map(|line| format!("TRANSCRIPT_LINE_{line:02}"))
            .collect::<Vec<_>>()
            .join("\n");
        let top = rendered(&state, 118, 20);
        assert!(top.contains("TRANSCRIPT_LINE_01"));
        state.scroll_surface(8);
        let scrolled = rendered(&state, 118, 20);
        assert!(!scrolled.contains("TRANSCRIPT_LINE_01"));
        assert!(scrolled.contains("TRANSCRIPT_LINE_10"));
    }
    #[test]
    fn read_only_palette_tools_are_queued_off_the_ui_thread() {
        let mut state = state(TuiLayout::Desktop);
        let result = super::super::command::execute(&mut state, "browser state").unwrap();
        assert!(result.contains("queued"));
        assert!(!result.contains('{'));
        assert!(state.queued_tool_request.is_some());
    }

    #[test]
    fn browser_semantic_actions_are_confirmed_before_cdp_execution() {
        let mut state = state(TuiLayout::Desktop);
        state
            .browser_workspace
            .connected(true, Some("127.0.0.1:9222".into()), Some(4));
        state.queue_browser_intent(BrowserWorkspaceIntent::Back);
        assert!(state.pending_confirmation.is_some());
        assert!(state.status.contains("Enter approves once"));
    }

    #[test]
    fn mutating_palette_action_requires_frozen_one_use_confirmation() {
        let mut state = state(TuiLayout::Desktop);
        super::super::command::execute(&mut state, "browser start").unwrap();
        let pending = state.pending_confirmation.as_ref().unwrap();
        assert_eq!(pending.call.name, "glass.browser.start");
        assert!(rendered(&state, 120, 32).contains("CONFIRM ONE MUTATION"));
        state.deny_confirmation();
        assert!(state.pending_confirmation.is_none());
        assert!(state.status.contains("Denied"));
    }

    #[test]
    fn code_surface_opens_edits_navigates_undoes_redoes_and_saves() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Code;
        state.selected_file = state
            .files
            .iter()
            .position(|path| path == "Cargo.toml")
            .unwrap();
        state.open_selected_file();
        state.enter_code_edit();
        state.edit_code_key(
            crossterm::event::KeyCode::Char('#'),
            crossterm::event::KeyModifiers::NONE,
        );
        assert!(
            state
                .workspace
                .lock()
                .unwrap()
                .project()
                .buffer("Cargo.toml")
                .unwrap()
                .dirty
        );
        state.edit_code_key(
            crossterm::event::KeyCode::Char('z'),
            crossterm::event::KeyModifiers::CONTROL,
        );
        assert!(
            !state
                .workspace
                .lock()
                .unwrap()
                .project()
                .buffer("Cargo.toml")
                .unwrap()
                .content
                .starts_with('#')
        );
        state.edit_code_key(
            crossterm::event::KeyCode::Char('y'),
            crossterm::event::KeyModifiers::CONTROL,
        );
        state.edit_code_key(
            crossterm::event::KeyCode::Char('s'),
            crossterm::event::KeyModifiers::CONTROL,
        );
        assert!(
            std::fs::read_to_string(state.workspace.lock().unwrap().root().join("Cargo.toml"),)
                .unwrap()
                .starts_with('#')
        );
    }

    #[test]
    fn desktop_compact_and_phone_can_drill_into_every_required_surface() {
        for (width, height, layout) in [
            (140, 40, TuiLayout::Desktop),
            (90, 28, TuiLayout::Compact),
            (64, 24, TuiLayout::Mobile),
        ] {
            let mut state = state(layout);
            for surface in DevSurface::ALL {
                state.surface = surface;
                let output = rendered(&state, width, height);
                assert!(
                    output.contains(surface.label()),
                    "{} layout did not expose {}",
                    match layout {
                        TuiLayout::Desktop => "desktop",
                        TuiLayout::Compact => "compact",
                        TuiLayout::Mobile => "phone",
                        TuiLayout::Auto => "auto",
                    },
                    surface.label()
                );
            }
        }
    }

    #[test]
    fn desktop_flow_opens_actions_menu_composes_and_pages_without_clipping() {
        let mut state = state(TuiLayout::Desktop);

        // Discoverability: the action menu opens from navigation mode and
        // renders its entries inside the surface pane at desktop size.
        state.surface = DevSurface::Agent;
        state.open_menu();
        let output = rendered(&state, 118, 32);
        assert!(output.contains("COMMAND CENTER · Agent"));
        assert!(output.contains("Compose message"));
        state.move_menu_selection(1);
        state.run_menu_action();
        assert!(state.command_mode);
        assert!(state.command_input.starts_with("agent setup"));
        state.close_palette();

        state.open_menu();
        state.menu_selection = state.surface_actions().len();
        let launcher_output = rendered(&state, 118, 32);
        assert!(launcher_output.contains("Search all commands"));
        state.run_menu_action();
        assert!(state.command_mode);
        assert!(state.command_input.is_empty());
        state.close_palette();

        state.open_menu();
        state.menu_selection = state.quit_menu_index();
        let quit_output = rendered(&state, 118, 32);
        assert!(quit_output.contains("Quit Glass Dev"));
        state.run_menu_action();
        assert!(state.quit_confirmation);
        assert!(!state.quit);

        // Composer editing keys behave like a real input line.
        state.surface = DevSurface::Agent;
        state.open_composer();
        state.insert_composer_text("fix the login bug");
        // Ctrl-W deletes the word before the cursor (at end: drops "bug").
        state.delete_composer_word();
        assert_eq!(state.composer_input, "fix the login");
        state.move_composer_cursor(false);
        state.composer_backspace();
        assert_eq!(state.composer_input, "fix the logn");
        state.close_composer();

        // Long content pages with PageDown/Home/End instead of being clipped.
        state.surface = DevSurface::More;
        state.scroll_surface(40);
        assert_eq!(state.current_scroll(), 40);
        state.scroll_home();
        assert_eq!(state.current_scroll(), 0);
        state.scroll_surface(-5);
        assert_eq!(state.current_scroll(), 0, "scroll is bounded at zero");
    }

    #[test]
    fn app_recovery_sheet_offers_choices_after_collision() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::App;
        state.note_browser_failure(
            "glass.browser.start",
            "CDP port 9222 is already occupied; use --attach to connect to that Chrome endpoint or choose another --port",
        );
        let output = rendered(&state, 118, 32);
        assert!(output.contains("BROWSER RECOVERY"));
        assert!(output.contains("automatic free port"));
        // Actions are reachable without leaving the TUI and launch off-thread.
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);
        state.accept_browser_recovery(1, &mut worker);
        assert!(state.browser_recovery.is_none());
        assert!(state.running_tool_job.is_some());
        drop(worker);
    }

    #[test]
    fn executable_project_configuration_prompts_on_phone_before_activation() {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "glass-dev-tui-trust-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("glass.toml"),
            "[tools.probe]\ndescription='probe'\ncommand='echo unsafe'\n",
        )
        .unwrap();
        let mut state = DevTuiState::open(&root, TuiLayout::Mobile).unwrap();
        assert_eq!(state.surface, DevSurface::Trust);
        let rendered = rendered(&state, 64, 24);
        assert!(rendered.contains("WORKSPACE TRUST"));
        assert!(rendered.contains("Open untrusted"));
        state.handle_printable('O');
        assert_eq!(
            state.workspace.lock().unwrap().trust(),
            crate::WorkspaceTrust::Untrusted
        );
        assert_eq!(state.surface, DevSurface::Agent);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn agent_onboarding_actions_are_available_without_leaving_the_tui() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Agent;
        state.request_agent_setup();
        assert_eq!(state.surface, DevSurface::Agent);
        assert_eq!(
            state
                .pending_confirmation
                .as_ref()
                .map(|pending| pending.call.name.as_str()),
            Some("glass.agent.setup")
        );
        assert_eq!(
            state
                .pending_confirmation
                .as_ref()
                .and_then(|pending| pending.call.arguments.get("login"))
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        state.deny_confirmation();
        state.request_agent_update();
        assert_eq!(
            state
                .pending_confirmation
                .as_ref()
                .and_then(|pending| pending.call.arguments.get("update"))
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert!(
            state
                .pending_confirmation
                .as_ref()
                .is_some_and(|pending| pending.summary.contains("Refresh"))
        );
        state.deny_confirmation();
        state.request_agent_login().expect("queue Pi login");
        assert!(state.agent_login_requested);
        assert!(state.status.contains("exit Pi to return"));
    }

    #[test]
    fn composer_keeps_message_and_routes_to_trust_when_workspace_is_untrusted() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Agent;
        state.snapshot_trust_label = "untrusted".into();
        state.open_composer();
        state.insert_composer_text("inspect this workspace");
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);
        state.submit_composer(&mut worker);
        assert_eq!(state.surface, DevSurface::Trust);
        assert!(!state.composer_mode);
        assert_eq!(state.composer_input, "inspect this workspace");
        drop(worker);
    }
    #[test]
    fn agent_chat_keeps_optimistic_message_and_composer_visible() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Agent;
        state.agent_conversation = "GLASS AGENT\nprevious answer".into();
        state
            .pending_chat_messages
            .push(super::super::state::PendingChatMessage {
                text: "follow up while you work".into(),
                state: super::super::state::ChatMessageState::Sending,
                job_id: Some(7),
                error: None,
            });
        state.composer_mode = true;
        let output = rendered(&state, 120, 32);
        assert!(output.contains("GLASS AGENT"));
        assert!(output.contains("YOU"));
        assert!(output.contains("follow up while you work"));
        assert!(output.contains("sending"));
        assert!(output.contains("Enter sends"));
    }

    #[test]
    fn agent_chat_send_result_has_success_and_retry_states() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Agent;
        state.composer_mode = true;
        state.agent_send_job = Some(7);
        state
            .pending_chat_messages
            .push(super::super::state::PendingChatMessage {
                text: "inspect the failing test".into(),
                state: super::super::state::ChatMessageState::Sending,
                job_id: Some(7),
                error: None,
            });
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 7,
            tool: "glass.agent.send".into(),
            result: Ok(serde_json::json!({"queued": true, "agentId": "agent-0001"})),
        });
        assert!(state.agent_send_job.is_none());
        assert_eq!(
            state.pending_chat_messages[0].state,
            super::super::state::ChatMessageState::Sent
        );
        assert!(state.status.contains("Sent to agent-0001"));
        state.agent_send_job = Some(9);
        state
            .pending_chat_messages
            .push(super::super::state::PendingChatMessage {
                text: "retry after the agent stopped".into(),
                state: super::super::state::ChatMessageState::Sending,
                job_id: Some(9),
                error: None,
            });
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 9,
            tool: "glass.agent.send".into(),
            result: Ok(serde_json::json!({
                "queued": true,
                "agentId": "agent-0001",
                "restarted": true
            })),
        });
        assert!(state.status.contains("Restarted agent-0001"));

        state.agent_send_job = Some(8);
        state
            .pending_chat_messages
            .push(super::super::state::PendingChatMessage {
                text: "retry this request".into(),
                state: super::super::state::ChatMessageState::Sending,
                job_id: Some(8),
                error: None,
            });
        state.composer_input.clear();
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 8,
            tool: "glass.agent.send".into(),
            result: Err("Pi session stopped".into()),
        });
        assert!(state.composer_mode);
        assert_eq!(state.composer_input, "retry this request");
        assert_eq!(
            state.pending_chat_messages[2].state,
            super::super::state::ChatMessageState::Failed
        );
        assert!(state.status.contains("edit and retry"));
    }
}
