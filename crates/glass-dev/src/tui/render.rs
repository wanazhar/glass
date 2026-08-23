use super::command;
use super::file_view;
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
const USER_BUBBLE_BG: Color = Color::Rgb(20, 48, 78);
const AGENT_BUBBLE_BG: Color = Color::Rgb(18, 60, 34);
const SYSTEM_BUBBLE_BG: Color = Color::Rgb(45, 49, 57);
const ALERT_BUBBLE_BG: Color = Color::Rgb(80, 54, 14);
const ERROR_BUBBLE_BG: Color = Color::Rgb(80, 28, 27);
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
    let height = area.height.saturating_sub(4).min(7);
    let modal = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Paragraph::new(panel_text("QUIT?\n\nEnter quit · Esc stay"))
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
    let content = "KEYS\n  ←/→      surface\n  ↑/↓      move or scroll\n  Enter    open / run / chat\n  Tab      next surface\n  Esc      back\n  ?        help\n\nWORKSPACE\n  Enter    describe a task\n  a        workspace actions\n  :        command palette\n  s/l      setup / login\n\nAGENT\n  i        compose a prompt\n  :review  review current changes\n  :harness list\n           list installed harnesses\n  :harness start NAME\n           launch a harness in this terminal\n\nAPP (OPTIONAL)\n  n        address\n  t        type\n  v        live view\n  Alt-←/→  back / forward\n  Ctrl-R   reload\n\nEDITOR\n  Enter    open\n  i        edit\n  Alt-A    ask Pi with the focused buffer\n  Ctrl-S   save\n  Ctrl-Z/Y undo / redo\n\nGIT\n  ↑/↓      changed file\n  Enter    diff\n  Esc      close";
    frame.render_widget(
        Paragraph::new(panel_text(content))
            .style(Style::default().fg(TEXT))
            .scroll((state.help_scroll, 0))
            .block(
                Block::default()
                    .title(" HELP · ↑↓ scroll · ?/Esc ")
                    .title_style(
                        Style::default()
                            .fg(ACCENT_BRIGHT)
                            .add_modifier(Modifier::BOLD),
                    )
                    .borders(Borders::ALL)
                    .border_type(BorderType::Plain)
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
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(if state.composer_mode || state.command_mode {
                3
            } else {
                2
            }),
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
            Constraint::Length(2),
            Constraint::Min(8),
            Constraint::Length(3),
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
    let footer_height = if state.composer_mode || state.command_mode {
        4
    } else {
        3
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(footer_height),
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
                "Enter send · Ctrl-D mode · Esc",
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
                "Tab complete · Enter run · Esc",
                Style::default().fg(MUTED),
            )),
        ]
    } else {
        vec![
            status_line(state, rows[2].width.saturating_sub(2)),
            Line::from(Span::styled(
                "←→ surface · ↑↓ move · Enter open · ?",
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

fn render_header(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect, _mode: &str) {
    let brand = Span::styled(
        " GLASS DEV ",
        Style::default()
            .fg(Color::Black)
            .bg(ACCENT)
            .add_modifier(Modifier::BOLD),
    );
    let surface_label = if state.surface == DevSurface::Agent {
        "Agent · WORKSPACE".to_string()
    } else {
        state.surface.label().to_string()
    };
    let surface = Span::styled(
        format!(" {surface_label} "),
        Style::default()
            .fg(ACCENT_BRIGHT)
            .add_modifier(Modifier::BOLD),
    );
    let trust = Span::styled(
        format!("{} ", state.snapshot_trust_label),
        Style::default().fg(if state.snapshot_trust_label == "untrusted" {
            WARNING
        } else {
            MUTED
        }),
    );
    let path = Span::styled(
        compact_path(&state.snapshot_root, area.width.saturating_sub(24)),
        Style::default().fg(MUTED),
    );
    let lines = if area.width < 104 {
        vec![
            Line::from(vec![brand.clone(), surface]),
            Line::from(vec![
                Span::raw(" "),
                path,
                Span::styled(" · ", Style::default().fg(PANEL_BORDER)),
                trust,
            ]),
        ]
    } else {
        vec![Line::from(vec![
            brand,
            Span::raw("  "),
            surface,
            path,
            Span::styled(" · ", Style::default().fg(PANEL_BORDER)),
            trust,
        ])]
    };

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

fn render_navigation(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let items = DevSurface::PRIMARY
        .into_iter()
        .map(|surface| {
            let selected = surface == state.surface;
            let style = if selected {
                Style::default()
                    .fg(ACCENT_BRIGHT)
                    .bg(ACTIVE_BACKGROUND)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    if selected { " › " } else { "   " },
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(surface.label(), style),
            ]))
            .style(style)
        })
        .collect::<Vec<_>>();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(2), Constraint::Length(2)])
        .split(area);
    frame.render_widget(
        List::new(items)
            .style(Style::default().bg(PANEL_BACKGROUND))
            .block(
                Block::default()
                    .title(" SURFACES ")
                    .title_style(
                        Style::default()
                            .fg(ACCENT_BRIGHT)
                            .add_modifier(Modifier::BOLD),
                    )
                    .borders(Borders::ALL)
                    .border_type(BorderType::Plain)
                    .border_style(Style::default().fg(PANEL_BORDER))
                    .padding(Padding::horizontal(1)),
            ),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new("←→ move · Enter open")
            .style(Style::default().fg(MUTED).bg(PANEL_BACKGROUND))
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(PANEL_BORDER))
                    .padding(Padding::horizontal(1)),
            ),
        rows[1],
    );
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
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(PANEL_BORDER))
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

fn stack_for_phone(state: &DevTuiState, area: Rect) -> bool {
    area.width < 84
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
    if queued == 0 {
        phase.into()
    } else {
        format!("{phase} · {queued} queued")
    }
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
        "target {} · rev {} · {phase}",
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
        .constraints([Constraint::Length(5), Constraint::Min(4)])
        .split(area);
    let pending = state
        .snapshot_trust_inspection
        .iter()
        .filter(|item| item.trust_required)
        .count();
    render_panel(
        frame,
        rows[0],
        " TRUST ",
        format!(
            "{pending} pending · {}\nI inspect · O open\n1 trust once · T trust project",
            state.snapshot_trust_label
        ),
        WARNING,
    );
    let inspection = super::projection::trust_items(&state.snapshot_trust_inspection);
    render_panel(
        frame,
        rows[1],
        " CONFIG ",
        format!("{}\nInspect before trust.", inspection),
        ACCENT_BRIGHT,
    );
}

fn compact_agent_header(state: &DevTuiState, width: u16) -> String {
    let readiness = state
        .agent_readiness
        .lines()
        .next()
        .unwrap_or("Pi unavailable");
    let runtime_state = readiness.split(" · ").next().unwrap_or("Pi unavailable");
    let app_summary = state
        .browser_chat_header()
        .split(" · ")
        .take(2)
        .collect::<Vec<_>>()
        .join(" · ");
    let app_summary = app_summary
        .strip_prefix("APP · ")
        .map_or(app_summary.clone(), |summary| {
            format!("APP (optional) · {summary}")
        });
    format!(
        "{}\n{}",
        compact_line(runtime_state, width),
        compact_line(&app_summary, width)
    )
}
#[derive(Clone, Copy)]
enum ConversationBubbleKind {
    User,
    Agent,
    System,
    Alert,
    Error,
}

fn conversation_bubble_style(kind: ConversationBubbleKind) -> (Color, Color, Color) {
    match kind {
        ConversationBubbleKind::User => (ACCENT_BRIGHT, USER_BUBBLE_BG, TEXT),
        ConversationBubbleKind::Agent => (SUCCESS, AGENT_BUBBLE_BG, TEXT),
        ConversationBubbleKind::System => (MUTED, SYSTEM_BUBBLE_BG, TEXT),
        ConversationBubbleKind::Alert => (WARNING, ALERT_BUBBLE_BG, TEXT),
        ConversationBubbleKind::Error => (ERROR, ERROR_BUBBLE_BG, ERROR),
    }
}

fn wrap_bubble_line(line: &str, width: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }
    line.chars()
        .collect::<Vec<_>>()
        .chunks(width.max(1))
        .map(|chunk| chunk.iter().collect())
        .collect()
}

fn conversation_bubble_lines(content: &str, width: u16) -> Vec<Line<'static>> {
    let available = usize::from(width).max(24);
    let mut lines = Vec::new();
    for (index, block) in content.split("\n\n").enumerate() {
        if index > 0 {
            lines.push(Line::from(""));
        }
        let mut block_lines = block.lines();
        let first = block_lines.next().unwrap_or_default().trim();
        let (kind, label, body) = match first {
            "YOU" => (
                ConversationBubbleKind::User,
                "YOU",
                block_lines.collect::<Vec<_>>().join("\n"),
            ),
            "GLASS AGENT" => (
                ConversationBubbleKind::Agent,
                "GLASS AGENT",
                block_lines.collect::<Vec<_>>().join("\n"),
            ),
            "SYSTEM" => (
                ConversationBubbleKind::System,
                "SYSTEM",
                block_lines.collect::<Vec<_>>().join("\n"),
            ),
            "ALERT" => (
                ConversationBubbleKind::Alert,
                "ALERT",
                block_lines.collect::<Vec<_>>().join("\n"),
            ),
            "ERROR" => (
                ConversationBubbleKind::Error,
                "ERROR",
                block_lines.collect::<Vec<_>>().join("\n"),
            ),
            _ => (ConversationBubbleKind::System, "SYSTEM", block.to_string()),
        };
        let (border, background, text_color) = conversation_bubble_style(kind);
        let bubble_width = if matches!(kind, ConversationBubbleKind::User) {
            available.saturating_sub(8).max(18).min(available)
        } else {
            available
        };
        let inner_width = bubble_width.saturating_sub(4).max(8);
        let indent = if matches!(kind, ConversationBubbleKind::User) {
            available.saturating_sub(bubble_width)
        } else {
            0
        };
        let label_width = label.chars().count();
        let rule = "─".repeat(inner_width.saturating_sub(label_width + 1));
        let prefix = " ".repeat(indent);
        let top = format!("{prefix}╭─ {label} {rule}╮");
        lines.push(Line::from(Span::styled(
            top,
            Style::default()
                .fg(border)
                .bg(background)
                .add_modifier(Modifier::BOLD),
        )));
        let body_lines = body
            .lines()
            .flat_map(|line| wrap_bubble_line(line, inner_width))
            .collect::<Vec<_>>();
        for line in if body_lines.is_empty() {
            vec![String::new()]
        } else {
            body_lines
        } {
            let padded = format!("{line:<inner_width$}");
            lines.push(Line::from(Span::styled(
                format!("{prefix}│ {padded} │"),
                Style::default().fg(text_color).bg(background),
            )));
        }
        lines.push(Line::from(Span::styled(
            format!("{prefix}╰{}╯", "─".repeat(inner_width + 2)),
            Style::default().fg(border).bg(background),
        )));
    }
    lines
}

fn agent_conversation_text(state: &DevTuiState, landing: &str, width: u16) -> Text<'static> {
    let conversation = state.conversation_view();
    let mut lines = if conversation.starts_with("No conversation yet.") {
        panel_text(landing).lines
    } else {
        conversation_bubble_lines(&conversation, width)
    };
    if !state.composer_mode {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Enter to chat",
            Style::default().fg(MUTED),
        )));
    }
    Text::from(lines)
}
fn render_agent_conversation_panel(
    frame: &mut Frame<'_>,
    state: &DevTuiState,
    landing: &str,
    area: Rect,
) {
    if area.width < 2 || area.height < 2 {
        return;
    }
    frame.render_widget(
        Paragraph::new(agent_conversation_text(
            state,
            landing,
            area.width.saturating_sub(4),
        ))
        .style(Style::default().fg(TEXT).bg(PANEL_BACKGROUND))
        .scroll((state.current_scroll(), 0))
        .block(surface_block(" CONVERSATION ", ACCENT_BRIGHT))
        .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_agent_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let compact = matches!(
        state.responsive_class(area.width, area.height),
        ResponsiveClass::Phone | ResponsiveClass::Compact
    );
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if compact {
            [Constraint::Length(4), Constraint::Min(5)]
        } else {
            [Constraint::Length(4), Constraint::Min(6)]
        })
        .split(area);
    let header = compact_agent_header(state, rows[0].width.saturating_sub(4));
    render_panel(frame, rows[0], " CODING AGENT ", header, ACCENT_BRIGHT);

    let conversation = state.conversation_view();
    let landing = if conversation.starts_with("No conversation yet.") {
        if state.agent_readiness.starts_with("✓ Ready") {
            "START HERE\nDescribe a coding task.\nEnter to chat · Glass inspects, edits, runs, and verifies.\nBrowser opens only for UI work.\n\nNeed another tool? Press a for actions, or use :harness list.".into()
        } else {
            "SETUP\nChoose Setup Pi runtime or Login from Actions.\nThen return here to describe a coding task.\n\nInstalled coding harnesses remain available from :harness list.".into()
        }
    } else {
        conversation
    };
    if compact || area.width < 84 {
        render_agent_conversation_panel(frame, state, &landing, rows[1]);
        return;
    }
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Min(24)])
        .split(rows[1]);
    render_agent_conversation_panel(frame, state, &landing, columns[0]);
    let installed_harnesses = state
        .harnesses
        .lines()
        .filter(|line| line.starts_with("●"))
        .count();
    let known_harnesses = state
        .harnesses
        .lines()
        .filter(|line| line.starts_with("●") || line.starts_with("○"))
        .count();
    let sidebar = format!(
        "TASK LOOP\n{}\n\nHARNESS BRIDGE\n{installed_harnesses}/{known_harnesses} detected\n:harness list\n:harness start NAME",
        agent_progress(state)
    );
    render_panel(frame, columns[1], " SESSION ", sidebar, PURPLE);
}
fn render_agent_workspace_context(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(8),
            Constraint::Length(8),
        ])
        .split(area);
    let branch = state.git.lines().next().unwrap_or("branch unavailable");
    let branch = branch.strip_prefix("branch ").unwrap_or(branch);
    let check = state
        .tests
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("No checks run yet");
    render_panel(
        frame,
        rows[0],
        " WORKSPACE ",
        format!(
            "BRANCH {branch}\n{} changed · rev {}\nGITHUB {}\n{}\n{}",
            state.git_entries.len(),
            state.snapshot_project_revision,
            state.github.summary(),
            activity_summary(state),
            check
        ),
        ACCENT_BRIGHT,
    );
    if state.git_diff_open {
        render_git_diff_panel(frame, state, rows[1]);
    } else {
        render_git_file_list(frame, state, rows[1]);
    }
    let browser = state.browser_workspace.state();
    render_browser_visual(frame, state, rows[2], browser);
}
fn render_file_tree(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let items = if state.files.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No files",
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
    let mut list_state = ListState::default();
    if !state.files.is_empty() {
        list_state.select(Some(state.selected_file));
    }
    frame.render_stateful_widget(
        List::new(items)
            .style(Style::default().bg(PANEL_BACKGROUND))
            .block(surface_block(
                format!(" FILES · {} ", selected),
                ACCENT_BRIGHT,
            )),
        area,
        &mut list_state,
    );
}

fn render_code_editor(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let title = if state.focused_editor_path.is_empty() {
        " EDITOR · choose a file ".to_string()
    } else {
        let dirty = if state.focused_editor_dirty {
            "unsaved"
        } else {
            "saved"
        };
        format!(
            " EDITOR · {dirty} · Ln {} Col {} · {} ",
            state.focused_editor_line,
            state.focused_editor_column,
            state.editor_collaboration_summary()
        )
    };
    let content = if state.editor.trim().is_empty() {
        "No file open · choose a file from the list"
    } else {
        state.editor.as_str()
    };
    frame.render_widget(
        Paragraph::new(file_view::render_editor(
            &state.focused_editor_path,
            content,
            state.focused_editor_selection.as_ref(),
        ))
        .style(Style::default().bg(PANEL_INSET))
        .scroll((state.current_scroll(), 0))
        .block(surface_block(title, ACCENT_BRIGHT))
        .wrap(Wrap { trim: false }),
        area,
    );
}
fn render_editor_collaboration(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let width = area.width.saturating_sub(4);
    let open_comments = state
        .editor_comments
        .iter()
        .filter(|comment| comment.state == crate::development::EditorCommentState::Open)
        .count();
    let pending_proposals = state
        .editor_proposals
        .iter()
        .filter(|proposal| proposal.state == crate::development::EditorProposalState::Pending)
        .count();
    let mut lines = vec![
        format!("comments {open_comments} open · proposals {pending_proposals} pending",),
        format!("checkpoints {}", state.editor_checkpoints.len()),
    ];
    if !state.focused_editor_path.is_empty() {
        lines.push(format!(
            "FILE {}",
            compact_path(&state.focused_editor_path, width.saturating_sub(5)),
        ));
        lines.push(format!(
            "CURSOR {}:{}{}",
            state.focused_editor_line,
            state.focused_editor_column,
            if state.focused_editor_dirty {
                " · unsaved"
            } else {
                ""
            }
        ));
        if let Some(selection) = state
            .focused_editor_selection
            .as_ref()
            .filter(|selection| !selection.is_empty())
        {
            let (start, end) = selection.ordered();
            lines.push(format!(
                "SELECT {}:{}–{}:{}",
                start.line, start.column, end.line, end.column
            ));
        }
    }
    if open_comments > 0 {
        lines.push("COMMENTS".into());
        lines.extend(
            state
                .editor_comments
                .iter()
                .filter(|comment| comment.state == crate::development::EditorCommentState::Open)
                .take(3)
                .map(|comment| {
                    compact_line(
                        &format!(
                            "L{}-{} {}",
                            comment.start_line, comment.end_line, comment.text
                        ),
                        width,
                    )
                }),
        );
    }
    if pending_proposals > 0 {
        lines.push("PROPOSALS".into());
        lines.extend(
            state
                .editor_proposals
                .iter()
                .filter(|proposal| {
                    proposal.state == crate::development::EditorProposalState::Pending
                })
                .take(3)
                .map(|proposal| {
                    compact_line(&format!("{} · {}", proposal.id, proposal.summary), width)
                }),
        );
    }
    render_panel(frame, area, " REVIEW ", lines.join("\n"), ACCENT_BRIGHT);
}

fn render_editor_lsp(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    render_panel(
        frame,
        area,
        " LSP ",
        if state.lsp.trim().is_empty() {
            "No diagnostics".into()
        } else {
            state.lsp.clone()
        },
        WARNING,
    );
}

fn render_code_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    if stack_for_phone(state, area) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(28),
                Constraint::Percentage(52),
                Constraint::Percentage(20),
            ])
            .split(area);
        render_file_tree(frame, state, rows[0]);
        render_code_editor(frame, state, rows[1]);
        render_editor_collaboration(frame, state, rows[2]);
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
    let side = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(54), Constraint::Percentage(46)])
        .split(columns[2]);
    render_editor_collaboration(frame, state, side[0]);
    render_editor_lsp(frame, state, side[1]);
}

fn render_browser_visual(
    frame: &mut Frame<'_>,
    state: &DevTuiState,
    area: Rect,
    browser: &glass_browser::browser_workspace::BrowserWorkspaceState,
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
                " LIVE VIEW ",
                "Live browser view is active",
                SUCCESS,
            );
            return;
        }
    }
    let reason = browser
        .presentation_reason
        .as_deref()
        .unwrap_or("Connect a browser to inspect the page.");
    let content = format!("{}\n{}", browser_progress(state, browser), reason);
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
                    "{} [{}] {} · {}",
                    if Some(index) == browser.selected_entity {
                        "›"
                    } else {
                        " "
                    },
                    index + 1,
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
        " INSPECTOR ",
        format!(
            "{}\n{}\n\n{} · rev {}\n{} · focus {}\n\n{}\n\n{}",
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
        ),
        ACCENT_BRIGHT,
    );
}

fn render_app_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let browser = state.browser_workspace.state();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(2),
        ])
        .split(area);
    let address = if browser.url.is_empty() {
        "No page".into()
    } else {
        compact_path(&browser.url, rows[0].width.saturating_sub(18))
    };
    let toolbar = format!(
        "{} · {}\n{}",
        if browser.loading { "loading" } else { "ready" },
        browser.connection_label(),
        address,
    );
    render_panel(frame, rows[0], " BROWSER ", toolbar, ACCENT_BRIGHT);

    if stack_for_phone(state, rows[1]) {
        render_browser_visual(frame, state, rows[1], browser);
    } else {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(68), Constraint::Min(30)])
            .split(rows[1]);
        render_browser_visual(frame, state, columns[0], browser);
        render_browser_inspector(frame, state, columns[1], browser);
    }
    render_panel(frame, rows[2], " WORKFLOW ", &state.workflow, PURPLE);
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
        .constraints([Constraint::Length(3), Constraint::Min(7)])
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
        " TERMINAL ",
        format!("{process_count} processes · {healthy_count} healthy · {failed_count} attention"),
        ACCENT_BRIGHT,
    );
    if process_lines.is_empty() {
        render_panel(
            frame,
            rows[1],
            " PROCESSES ",
            "No managed processes yet",
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
                .block(surface_block(" PROCESSES ", ACCENT_BRIGHT)),
            rows[1],
        );
    }
}

fn render_task_list(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let items = if state.tasks.trim().is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No tasks",
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
                format!(" TASKS · {task_count} "),
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
        " SUMMARY ",
        format!("{running} running · {queued} queued · {failed} failed"),
        PURPLE,
    );
}

fn git_entry_status(entry: &crate::git::GitStatusEntry) -> String {
    if entry.untracked {
        "??".into()
    } else {
        format!("{}{}", entry.index_status, entry.worktree_status)
    }
}

fn git_entry_status_color(entry: &crate::git::GitStatusEntry) -> Color {
    if entry.untracked {
        WARNING
    } else if entry.index_status == 'U' || entry.worktree_status == 'U' {
        ERROR
    } else if entry.index_status != ' ' && entry.worktree_status != ' ' {
        PURPLE
    } else if entry.index_status != ' ' {
        SUCCESS
    } else if entry.worktree_status != ' ' {
        ERROR
    } else {
        MUTED
    }
}

fn render_git_file_list(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let items = if state.git_entries.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No changed files",
            Style::default().fg(MUTED),
        )))]
    } else {
        state
            .git_entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let selected = index == state.selected_git_file;
                let status = git_entry_status(entry);
                let marker = if selected { "›" } else { " " };
                let path = entry
                    .original_path
                    .as_deref()
                    .map(|original| format!("{} ← {}", entry.path, original))
                    .unwrap_or_else(|| entry.path.clone());
                let line = Line::from(vec![
                    Span::styled(
                        format!("{marker} "),
                        Style::default()
                            .fg(if selected { ACCENT_BRIGHT } else { MUTED })
                            .add_modifier(if selected {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ),
                    Span::styled(
                        format!("{status:<2} "),
                        Style::default()
                            .fg(git_entry_status_color(entry))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        path,
                        Style::default()
                            .fg(if selected { TEXT } else { MUTED })
                            .add_modifier(if selected {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ),
                ]);
                ListItem::new(line).style(if selected {
                    Style::default().bg(ACTIVE_BACKGROUND)
                } else {
                    Style::default()
                })
            })
            .collect::<Vec<_>>()
    };
    let mut list_state = ListState::default();
    if !state.git_entries.is_empty() {
        list_state.select(Some(state.selected_git_file));
    }
    frame.render_stateful_widget(
        List::new(items)
            .style(Style::default().bg(PANEL_BACKGROUND))
            .block(surface_block(
                format!(" CHANGES · {} ", state.git_entries.len()),
                ACCENT_BRIGHT,
            )),
        area,
        &mut list_state,
    );
}

fn render_git_diff_panel(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    if state.git_diff_open {
        let path = state.git_diff_path.as_deref().unwrap_or("WORKTREE");
        frame.render_widget(
            Paragraph::new(file_view::render_diff(path, &state.git_diff))
                .style(Style::default().bg(PANEL_INSET))
                .scroll((state.current_scroll(), 0))
                .block(surface_block(format!(" DIFF · {path} "), ACCENT_BRIGHT))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let Some(path) = state.selected_git_entry().map(|entry| entry.path.as_str()) else {
        render_panel(
            frame,
            area,
            " DIFF ",
            "Choose a file\nEnter actions",
            PURPLE,
        );
        return;
    };
    render_panel(
        frame,
        area,
        " DIFF ",
        format!("{path}\nEnter actions"),
        PURPLE,
    );
}

fn render_git_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let branch = state.git.lines().next().unwrap_or("branch unavailable");
    let change_count = state.git_entries.len();
    let review = state
        .github_review
        .lines()
        .take(2)
        .collect::<Vec<_>>()
        .join(" · ");
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(5)])
        .split(area);
    render_panel(
        frame,
        rows[0],
        format!(" GIT · {branch} "),
        format!(
            "{change_count} changed · {}\nGH {}",
            state
                .selected_git_entry()
                .map(|entry| format!("selected {}", entry.path))
                .unwrap_or_else(|| "↑/↓ choose a file".into()),
            review
        ),
        PURPLE,
    );
    let columns = if stack_for_phone(state, rows[1]) {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(rows[1])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(38), Constraint::Min(40)])
            .split(rows[1])
    };
    render_git_file_list(frame, state, columns[0]);
    render_git_diff_panel(frame, state, columns[1]);
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
        format!(" DEBUG · {session_count} "),
        &state.debugger,
        "No debugger sessions",
    );
    render_panel(frame, rows[1], " TESTS ", test_status, WARNING);
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
        " SERVICES ",
        format!(
            "{} skills · {} tools · {kernel_count} kernels\n{}\nCOCKPIT {}",
            state.snapshot_skills_count,
            state.snapshot_tools_count,
            activity_summary(state),
            state.private_cockpit_status(),
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
        " PI ",
        format!(
            "{}\n{}\n{kernel_count} kernels",
            state
                .agent_readiness
                .lines()
                .next()
                .unwrap_or("Pi unavailable"),
            agent_progress(state),
        ),
        PURPLE,
    );
    render_panel(
        frame,
        columns[1],
        " EXPERIMENTS ",
        format!(
            "{}\n{}",
            state.experiments.lines().next().unwrap_or("No experiments"),
            state.replay.lines().next().unwrap_or("No replay"),
        ),
        ACCENT_BRIGHT,
    );
    let installed_harnesses = state
        .harnesses
        .lines()
        .filter(|line| line.starts_with("●"))
        .take(4)
        .collect::<Vec<_>>()
        .join("\n");
    render_panel(
        frame,
        columns[2],
        " ROUTES ",
        format!(
            "workspace · experiments · kernels · replay\n\nHARNESS CATALOG\n{}\n\n:harness list\n:harness start NAME",
            if installed_harnesses.is_empty() {
                "none detected"
            } else {
                &installed_harnesses
            }
        ),
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
                    Span::styled(action.label, item_style),
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
                    Span::styled("Search commands", item_style),
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
                    Span::styled("Quit", item_style),
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
                        .title(format!(" ACTIONS · {} ", state.surface.label()))
                        .title_style(
                            Style::default()
                                .fg(ACCENT_BRIGHT)
                                .add_modifier(Modifier::BOLD),
                        )
                        .borders(Borders::ALL)
                        .border_type(BorderType::Plain)
                        .border_style(Style::default().fg(PANEL_BORDER))
                        .padding(Padding::horizontal(1)),
                ),
            rows[0],
            &mut menu_state,
        );
        if rows[1].height > 0 {
            frame.render_widget(
                Paragraph::new(panel_text(&format!(
                    "{}\n{}\nEnter run · Esc close",
                    selected_description, group.example,
                )))
                .style(Style::default().fg(TEXT))
                .block(
                    Block::default()
                        .title(" DETAILS ")
                        .title_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
                        .borders(Borders::ALL)
                        .border_type(BorderType::Plain)
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
                        .title(" TARGETS ")
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
                "RECOVER\nport {}\n{}\n{}\n{}",
                offer.port,
                offer.guidance(),
                offer.reason,
                actions
            )))
            .style(Style::default().fg(TEXT))
            .block(
                Block::default()
                    .title(" RECOVERY ")
                    .title_style(Style::default().fg(WARNING).add_modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .border_type(BorderType::Plain)
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
                "APPROVE TOOL\n{}\n{}\n{}\nEnter/Y approve · Esc/N deny",
                pending.agent_id, pending.tool_name, pending.arguments
            )))
            .style(Style::default().fg(TEXT))
            .block(
                Block::default()
                    .title(" APPROVAL ")
                    .title_style(Style::default().fg(WARNING).add_modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .border_type(BorderType::Plain)
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
                "CONFIRM\n{}\nEnter/Y approve · Esc/N deny",
                pending.summary
            )))
            .style(Style::default().fg(TEXT))
            .block(
                Block::default()
                    .title(" CONFIRM ")
                    .title_style(Style::default().fg(WARNING).add_modifier(Modifier::BOLD))
                    .borders(Borders::ALL)
                    .border_type(BorderType::Plain)
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
    if state.surface == DevSurface::Agent {
        render_agent_workspace_context(frame, state, area);
        return;
    }
    let (label, content) = match state.surface {
        DevSurface::Trust => {
            let pending = state
                .snapshot_trust_inspection
                .iter()
                .filter(|item| item.trust_required)
                .count();
            (
                "TRUST",
                format!(
                    "{pending} pending\n{} · rev {}",
                    state.snapshot_trust_label, state.snapshot_project_revision
                ),
            )
        }
        DevSurface::Agent => ("WORKSPACE", "Use the workspace".into()),
        DevSurface::Code => {
            let content = if state.focused_editor_path.is_empty() {
                format!("{} files", state.files.len())
            } else {
                format!(
                    "{}{}\nline {}",
                    if state.focused_editor_dirty {
                        "● "
                    } else {
                        ""
                    },
                    state.focused_editor_path,
                    state.focused_editor_line
                )
            };
            ("CODE", content)
        }
        DevSurface::App => {
            let browser = state.browser_workspace.state();
            let selected = browser.selected().map(|entity| {
                let index = browser
                    .entities
                    .iter()
                    .position(|candidate| candidate.reference == entity.reference)
                    .map_or(0, |index| index + 1);
                if index == 0 {
                    format!("{} · {}", entity.name, entity.role)
                } else {
                    format!("[{index}] {} · {}", entity.name, entity.role)
                }
            });
            (
                "BROWSER",
                format!(
                    "{}\n{}\n{}",
                    selected.unwrap_or_else(|| "no selection".into()),
                    browser.connection_label(),
                    browser.focus_label()
                ),
            )
        }
        DevSurface::Terminal => (
            "TERMINAL",
            state
                .processes
                .lines()
                .next()
                .unwrap_or("no processes")
                .to_string(),
        ),
        DevSurface::Tasks => {
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
            (
                "TASKS",
                format!("{running} running · {queued} queued · {failed} failed"),
            )
        }
        DevSurface::Git => (
            "GIT",
            state
                .selected_git_entry()
                .map(|entry| entry.path.clone())
                .unwrap_or_else(|| "no changed files".into()),
        ),
        DevSurface::Debug => (
            "DEBUG",
            state
                .debugger
                .lines()
                .next()
                .unwrap_or("no sessions")
                .to_string(),
        ),
        DevSurface::More => (
            "SERVICES",
            format!(
                "{} skills · {} tools · {} kernels",
                state.snapshot_skills_count,
                state.snapshot_tools_count,
                state
                    .kernels
                    .lines()
                    .filter(|line| !line.is_empty())
                    .count()
            ),
        ),
    };
    frame.render_widget(
        Paragraph::new(panel_text(&content))
            .style(Style::default().fg(TEXT).bg(PANEL_BACKGROUND))
            .block(surface_block(format!(" CONTEXT · {label} "), ACCENT))
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
                    " · Enter send · Ctrl-D mode · Esc",
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
                format!("{glyph} Tab roots · ↑/↓ history · Enter run · Esc{error}"),
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
        let mut lines = vec![Line::from(line)];
        if area.width < 104 {
            lines.push(Line::from(Span::styled(
                "←→ surface · ↑↓ move · Enter open",
                Style::default().fg(MUTED),
            )));
        }
        lines
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
            assert!(output.contains("Enter to chat"));
        } else {
            assert!(output.contains("SETUP"));
            assert!(output.contains("Enter to install"));
        }
        assert!(output.contains("APP (optional)"));
        assert!(output.contains("● Ready"));
        assert!(output.contains("Enter"));

        state.status = "browser failed".into();
        assert!(rendered(&state, 48, 18).contains("× browser failed"));
    }

    #[test]
    fn agent_surface_exposes_workspace_context() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Agent;
        state.git = "branch main".into();
        state.git_entries = vec![crate::git::GitStatusEntry {
            path: "src/lib.rs".into(),
            original_path: None,
            index_status: ' ',
            worktree_status: 'M',
            untracked: false,
        }];
        let output = rendered(&state, 140, 40);
        assert!(output.contains("Agent · WORKSPACE"));
        assert!(!output.contains("CODING WORKSPACE"));
        assert!(output.contains("BRANCH main"));
        assert!(output.contains("GITHUB no GitHub origin"));
        assert!(output.contains("CHANGES · 1"));
        assert!(output.contains("src/lib.rs"));
        assert!(output.contains("VISUAL PLANE"));
        if state.agent_readiness.starts_with("✓ Ready") {
            assert!(output.contains("Describe a coding task"));
            assert!(output.contains("Browser opens only for UI work"));
        }

        state.git_diff_open = true;
        state.git_diff_path = Some("src/lib.rs".into());
        state.git_diff = "@@ -1 +1 @@\n-old\n+added".into();
        let diff_output = rendered(&state, 140, 40);
        assert!(diff_output.contains("DIFF"));
        assert!(diff_output.contains("+added"));
    }

    #[test]
    fn agent_transcript_uses_distinct_role_bubbles() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Agent;
        state.agent_conversation = [
            "YOU\nInspect the failing test.",
            "GLASS AGENT\nI found the failing assertion.",
            "SYSTEM\nworkspace revision 4",
            "ALERT\napproval required",
            "ERROR\ncommand failed",
        ]
        .join("\n\n");

        let output = rendered(&state, 140, 40);
        for marker in ["YOU", "GLASS AGENT", "SYSTEM", "ALERT", "ERROR"] {
            assert!(output.contains(marker), "missing role marker {marker}");
        }
        assert!(output.contains("╭"));
        assert!(output.contains("CONVERSATION"));
    }

    #[test]
    fn quit_confirmation_renders_a_clear_modal() {
        let mut state = state(TuiLayout::Desktop);
        state.request_quit();
        let output = rendered(&state, 80, 24);
        assert!(output.contains("QUIT?"));
        assert!(output.contains("Enter quit"));
        assert!(output.contains("Esc stay"));
    }

    #[test]
    fn browser_header_omits_redundant_empty_page_title() {
        let state = state(TuiLayout::Desktop);
        let header = state.browser_chat_header();
        assert_eq!(header.matches("no page").count(), 1);
        assert!(!header.contains("No page · no page"));
    }

    #[test]
    fn browser_inspector_uses_compact_local_refs() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::App;
        state.browser_workspace.replace_entities(
            7,
            vec![glass_browser::browser_workspace::BrowserWorkspaceEntity {
                reference: "r7:b42".into(),
                role: "button".into(),
                name: "Save".into(),
                actionable: true,
                revision: 7,
            }],
        );
        let output = rendered(&state, 140, 40);
        assert!(output.contains("[1] Save · button"));
        assert!(!output.contains("r7:b42"));
    }

    #[test]
    fn empty_surface_counts_ignore_placeholder_copy() {
        let mut state = state(TuiLayout::Desktop);

        state.surface = DevSurface::Tasks;
        assert!(rendered(&state, 140, 40).contains("TASKS · 0"));

        state.surface = DevSurface::Debug;
        assert!(rendered(&state, 140, 40).contains("DEBUG · 0"));

        state.surface = DevSurface::Terminal;
        let terminal = rendered(&state, 140, 40);
        assert!(terminal.contains("0 processes"));
        assert!(terminal.contains("No managed processes yet"));
    }

    #[test]
    fn surface_empty_states_show_guided_next_actions() {
        let mut desktop = state(TuiLayout::Desktop);
        for (surface, marker) in [
            (DevSurface::App, "VISUAL PLANE"),
            (DevSurface::Terminal, "PROCESSES"),
            (DevSurface::Tasks, "SUMMARY"),
            (DevSurface::Debug, "TESTS"),
            (DevSurface::More, "ROUTES"),
        ] {
            desktop.surface = surface;
            assert!(
                rendered(&desktop, 140, 40).contains(marker),
                "{surface:?} should expose {marker}"
            );
        }

        let compact = state(TuiLayout::Desktop);
        let compact_output = rendered(&compact, 64, 24);
        assert!(compact_output.contains("SURFACES"));
        assert!(compact_output.contains("←→ move · Enter open"));
    }

    #[test]
    fn progress_overlays_and_visual_fallback_diagnostics_are_visible() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Agent;
        let agent = rendered(&state, 140, 40);
        assert!(agent.contains("idle"));
        state.surface = DevSurface::App;
        let fallback = rendered(&state, 140, 40);
        assert!(fallback.contains("target"));
        assert!(fallback.contains("Connect a browser") || fallback.contains("visual"));

        state.browser_visual_live = true;
        state.browser_workspace.state_mut().presentation =
            glass_browser::browser_workspace::BrowserPresentationPath::Herdr;
        let herdr = rendered(&state, 140, 40);
        assert!(herdr.contains("Live browser view is active"));
    }

    #[test]
    fn every_surface_exposes_a_distinct_workbench_hierarchy() {
        let mut state = state(TuiLayout::Desktop);

        for (surface, markers) in [
            (DevSurface::Trust, ["TRUST", "CONFIG"]),
            (DevSurface::Agent, ["CONVERSATION", "START HERE"]),
            (DevSurface::Code, ["FILES", "EDITOR"]),
            (DevSurface::App, ["VISUAL PLANE", "INSPECTOR"]),
            (DevSurface::Terminal, ["TERMINAL", "PROCESSES"]),
            (DevSurface::Tasks, ["TASKS", "SUMMARY"]),
            (DevSurface::Git, ["CHANGES", "DIFF"]),
            (DevSurface::Debug, ["DEBUG", "TESTS"]),
            (DevSurface::More, ["PI", "ROUTES"]),
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
    fn git_surface_selects_files_before_loading_focused_diff() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Git;
        state.git = "branch main\nworking tree dirty".into();
        state.git_entries = vec![
            crate::git::GitStatusEntry {
                path: "src/main.rs".into(),
                original_path: None,
                index_status: ' ',
                worktree_status: 'M',
                untracked: false,
            },
            crate::git::GitStatusEntry {
                path: "tests/agent.rs".into(),
                original_path: None,
                index_status: '?',
                worktree_status: '?',
                untracked: true,
            },
        ];

        let output = rendered(&state, 140, 40);
        assert!(output.contains("CHANGES · 2"));
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("tests/agent.rs"));
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("Enter actions"));

        state.move_git_selection(1);
        assert_eq!(
            state.selected_git_entry().map(|entry| entry.path.as_str()),
            Some("tests/agent.rs")
        );
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
        assert!(output.contains("ACTIONS · Agent"));
        assert!(output.contains("Compose message"));
        assert!(!output.contains("[i]"));
        assert!(output.contains("DETAILS"));
        assert!(output.contains("ask the resident Glass Agent"));

        let mut mobile = state(TuiLayout::Mobile);
        mobile.surface = DevSurface::Agent;
        mobile.open_menu();
        mobile.menu_selection = mobile.surface_actions().len();
        assert!(rendered(&mobile, 48, 18).contains("Search commands"));
    }

    #[test]
    fn phone_layout_stacks_surface_panels_without_browser_inspector() {
        let mut state = state(TuiLayout::Mobile);
        state.surface = DevSurface::More;
        let more = rendered(&state, 160, 24);
        let readiness = more.find("PI").expect("readiness panel");
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
        assert!(output.contains("AGENT"));
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
        let output = rendered(&state, 118, 32);
        assert!(output.contains("Ctrl-D mode"));
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
        assert!(rendered(&state, 120, 32).contains("CONFIRM"));
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
        assert!(output.contains("ACTIONS · Agent"));
        assert!(output.contains("Compose message"));
        state.move_menu_selection(1);
        state.run_menu_action();
        assert!(state.command_mode);
        assert!(state.command_input.starts_with("agent setup"));
        state.close_palette();

        state.open_menu();
        state.menu_selection = state.surface_actions().len();
        let launcher_output = rendered(&state, 118, 32);
        assert!(launcher_output.contains("Search commands"));
        state.run_menu_action();
        assert!(state.command_mode);
        assert!(state.command_input.is_empty());
        state.close_palette();

        state.open_menu();
        state.menu_selection = state.quit_menu_index();
        let quit_output = rendered(&state, 118, 32);
        assert!(quit_output.contains("Quit"));
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
        assert!(output.contains("RECOVER"));
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
        assert!(rendered.contains("TRUST"));
        assert!(rendered.contains("O open"));
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
        assert!(output.contains("Enter send"));
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
    #[test]
    fn code_surface_renders_source_content_instead_of_flat_panel_text() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Code;
        state.focused_editor_path = "src/main.rs".into();
        state.editor =
            "○ src/main.rs · cursor 1:1 · actor local · 1 lines\n▶  1 │ fn main() { return 42; }"
                .into();
        let output = rendered(&state, 120, 32);
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("fn main()"));
        assert!(output.contains("return 42"));
    }

    #[test]
    fn code_surface_renders_mermaid_preview_for_raw_diagram_output() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Code;
        state.focused_editor_path = "docs/flow.mmd".into();
        state.editor = "flowchart LR\nstart[Start] --> done{Done}".into();
        let output = rendered(&state, 120, 32);
        assert!(output.contains("DIAGRAM LR"));
        assert!(output.contains("[Start]"));
        assert!(output.contains("──▶"));
    }

    #[test]
    fn git_surface_renders_syntax_aware_diff_content() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Git;
        state.git_diff_open = true;
        state.git_diff_path = Some("src/lib.rs".into());
        state.git_diff = "@@ -1 +1 @@\n-fn old() {}\n+fn new() {}".into();
        let output = rendered(&state, 120, 32);
        assert!(output.contains("fn old()"));
        assert!(output.contains("fn new()"));
        assert!(output.contains("DIFF"));
    }
}
