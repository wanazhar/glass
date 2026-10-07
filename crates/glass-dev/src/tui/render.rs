use super::command;
use super::editor::EditorMode;
use super::file_view;
use super::overlay::{self, ActiveOverlay};
use super::pi_commands;
use super::state::{DevSurface, DevTuiState, EditorExitPrompt, ResponsiveClass, safe_browser_url};
use glass_browser::browser::{NATIVE_BEFOREUNLOAD_MESSAGE, NATIVE_DIALOG_TEXT_LIMIT_BYTES};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap,
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
                } else if trimmed.starts_with('×')
                    || (trimmed.contains("failed")
                        && !trimmed.contains("0 failed")
                        && !trimmed.starts_with("failed 0"))
                {
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
        && !line.starts_with(' ')
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

fn header_height() -> u16 {
    2
}

fn composer_visible_lines(state: &DevTuiState) -> u16 {
    if overlay::active_overlay(state) != Some(ActiveOverlay::Composer) {
        return 0;
    }
    let lines = state.composer_input.split('\n').count().max(1);
    (lines as u16).clamp(1, 6)
}

pub(super) fn footer_height(state: &DevTuiState) -> u16 {
    if overlay::active_overlay(state) == Some(ActiveOverlay::Composer) {
        composer_visible_lines(state).saturating_add(2)
    } else {
        3
    }
}

fn shell_rows(state: &DevTuiState, area: Rect) -> [Rect; 3] {
    let rows = match state.responsive_class(area.width, area.height) {
        ResponsiveClass::Desktop | ResponsiveClass::Compact => Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_height()),
                Constraint::Min(8),
                Constraint::Length(footer_height(state)),
            ])
            .split(area),
        ResponsiveClass::Phone => Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_height()),
                Constraint::Min(5),
                Constraint::Length(footer_height(state)),
            ])
            .split(area),
    };
    [rows[0], rows[1], rows[2]]
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ScreenGeometry {
    pub header: Rect,
    pub body: Rect,
    pub footer: Rect,
    pub navigation: Option<Rect>,
    pub surface: Rect,
    pub context: Option<Rect>,
}

pub(crate) fn screen_geometry(state: &DevTuiState, area: Rect) -> ScreenGeometry {
    let [header, body, footer] = shell_rows(state, area);
    match state.responsive_class(area.width, area.height) {
        ResponsiveClass::Desktop => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(24),
                    Constraint::Percentage(55),
                    Constraint::Min(30),
                ])
                .split(body);
            ScreenGeometry {
                header,
                body,
                footer,
                navigation: Some(columns[0]),
                surface: columns[1],
                context: Some(columns[2]),
            }
        }
        ResponsiveClass::Compact => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(22), Constraint::Min(36)])
                .split(body);
            ScreenGeometry {
                header,
                body,
                footer,
                navigation: Some(columns[0]),
                surface: columns[1],
                context: None,
            }
        }
        ResponsiveClass::Phone => ScreenGeometry {
            header,
            body,
            footer,
            navigation: None,
            surface: body,
            context: None,
        },
    }
}

fn surface_area(state: &DevTuiState, area: Rect) -> Rect {
    screen_geometry(state, area).surface
}

fn navigation_surfaces(state: &DevTuiState) -> Vec<DevSurface> {
    if state.surface == DevSurface::Trust {
        std::iter::once(DevSurface::Trust)
            .chain(DevSurface::PRIMARY)
            .collect()
    } else {
        DevSurface::PRIMARY.to_vec()
    }
}

fn navigation_block() -> Block<'static> {
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
        .padding(Padding::horizontal(1))
}

pub(crate) fn navigation_surface_at(
    state: &DevTuiState,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<DevSurface> {
    let navigation = screen_geometry(state, area).navigation?;
    let items = navigation_surfaces(state);
    let inner = navigation_block().inner(navigation);
    let index = row.checked_sub(inner.y)? as usize;
    (column >= inner.x
        && column < inner.x.saturating_add(inner.width)
        && row < inner.y.saturating_add(inner.height))
    .then(|| items.get(index).copied())
    .flatten()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CommandMenuGeometry {
    pub list_area: Rect,
    pub list_inner: Rect,
    pub details_area: Rect,
    pub item_count: usize,
    pub scroll_offset: usize,
}

fn command_menu_regions(area: Rect) -> [Rect; 2] {
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
    [rows[0], rows[1]]
}

fn command_menu_list_block(surface: DevSurface) -> Block<'static> {
    Block::default()
        .title(format!(" ACTIONS · {} ", surface.label()))
        .title_style(
            Style::default()
                .fg(ACCENT_BRIGHT)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(PANEL_BORDER))
        .padding(Padding::horizontal(1))
}

fn command_menu_scroll_offset(selected: usize, item_count: usize, visible_rows: usize) -> usize {
    if item_count == 0 || visible_rows == 0 {
        return 0;
    }
    selected
        .min(item_count - 1)
        .saturating_add(1)
        .saturating_sub(visible_rows)
}

fn command_menu_geometry(state: &DevTuiState, area: Rect) -> CommandMenuGeometry {
    let [list_area, details_area] = command_menu_regions(area);
    let list_inner = command_menu_list_block(state.surface).inner(list_area);
    let item_count = state.surface_actions().len() + 2;
    let scroll_offset = command_menu_scroll_offset(
        state.menu_selection,
        item_count,
        usize::from(list_inner.height),
    );
    CommandMenuGeometry {
        list_area,
        list_inner,
        details_area,
        item_count,
        scroll_offset,
    }
}

pub(super) fn command_menu_geometry_for_screen(state: &DevTuiState) -> CommandMenuGeometry {
    let frame_area = Rect::new(
        0,
        0,
        state.terminal_width.max(1),
        state.terminal_height.max(1),
    );
    command_menu_geometry(state, surface_area(state, frame_area))
}

pub(super) fn command_menu_hit_index(state: &DevTuiState, column: u16, row: u16) -> Option<usize> {
    let geometry = command_menu_geometry_for_screen(state);
    let inner = geometry.list_inner;
    if column < inner.x
        || column >= inner.x.saturating_add(inner.width)
        || row < inner.y
        || row >= inner.y.saturating_add(inner.height)
    {
        return None;
    }
    let visible_row = usize::from(row - inner.y);
    let item = geometry.scroll_offset.checked_add(visible_row)?;
    (item < geometry.item_count).then_some(item)
}

fn git_header_label(state: &DevTuiState) -> Option<String> {
    if state.git_branch.is_empty() {
        return None;
    }
    Some(if state.git_dirty {
        format!("{}*", state.git_branch)
    } else {
        state.git_branch.clone()
    })
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
/// Return the inner cell area reserved for a Kitty browser frame.
///
/// The raw Kitty image is emitted after Ratatui draws, so this geometry must
/// stay identical to the App surface's visual panel layout.
pub fn browser_visual_area(state: &DevTuiState, area: Rect) -> Option<Rect> {
    if state.surface != DevSurface::App
        || !state.browser_visual_live
        || overlay::active_overlay(state).is_some_and(ActiveOverlay::occludes_surface)
    {
        return None;
    }

    let visual = app_surface_geometry(state, surface_area(state, area)).visual;
    let inner = visual_pixel_area(visual);
    (inner.width > 0 && inner.height > 0).then_some(inner)
}

fn visual_pixel_area(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

fn render_native_browser_dialog(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let Some(dialog) = state.browser_dialog.as_ref() else {
        return;
    };
    let is_prompt = dialog.pending.dialog.dialog_type == "prompt";
    let is_beforeunload = dialog.pending.dialog.dialog_type == "beforeunload";
    let compact = matches!(
        state.responsive_class(area.width, area.height),
        ResponsiveClass::Phone
    );
    let (kind, accent) = match dialog.pending.dialog.dialog_type.as_str() {
        "alert" => ("ALERT", WARNING),
        "confirm" => ("CONFIRM", ACCENT_BRIGHT),
        "prompt" => ("PROMPT", ACCENT_BRIGHT),
        "beforeunload" => ("LEAVE PAGE?", WARNING),
        _ => ("DIALOG", WARNING),
    };
    let width = area.width.min(82);
    let height = area.height.min(16);
    let modal = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    let title = if is_beforeunload {
        format!(" {kind} · page paused ")
    } else {
        format!(" JAVASCRIPT {kind} · page paused ")
    };
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(accent).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .style(Style::default().fg(TEXT).bg(PANEL_BACKGROUND))
        .padding(Padding::horizontal(1));
    let inner = block.inner(modal);
    frame.render_widget(Clear, modal);
    frame.render_widget(block, modal);

    let mut constraints = vec![
        Constraint::Min(1),
        Constraint::Length(if compact { 2 } else { 1 }),
    ];
    if is_prompt {
        constraints.push(Constraint::Length(1));
    }
    if dialog.error.is_some() {
        constraints.push(Constraint::Length(1));
    }
    constraints.extend([Constraint::Length(1), Constraint::Length(1)]);
    if is_prompt {
        constraints.push(Constraint::Length(1));
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);
    let mut row = 0;
    frame.render_widget(
        Paragraph::new(if is_beforeunload {
            NATIVE_BEFOREUNLOAD_MESSAGE
        } else {
            dialog.pending.dialog.message.as_str()
        })
        .style(Style::default().fg(TEXT))
        .wrap(Wrap { trim: false }),
        rows[row],
    );
    row += 1;
    let page_url = safe_browser_url(&dialog.pending.dialog.url)
        .unwrap_or_else(|| "local or unavailable page".into());
    let page = if compact {
        format!("Page: {page_url}")
    } else {
        compact_line(&format!("Page: {page_url}"), inner.width)
    };
    frame.render_widget(
        Paragraph::new(page)
            .style(Style::default().fg(MUTED))
            .wrap(Wrap { trim: false }),
        rows[row],
    );
    row += 1;
    if is_prompt {
        let cursor = dialog
            .prompt_cursor
            .min(dialog.prompt_input.chars().count());
        let before = dialog.prompt_input.chars().take(cursor).collect::<String>();
        let after = dialog.prompt_input.chars().skip(cursor).collect::<String>();
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("› ", Style::default().fg(ACCENT_BRIGHT)),
                Span::raw(before),
                Span::styled(
                    "▏",
                    Style::default()
                        .fg(Color::Black)
                        .bg(ACCENT_BRIGHT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(after),
            ]))
            .style(Style::default().fg(TEXT).bg(PANEL_INSET)),
            rows[row],
        );
        row += 1;
    }
    if let Some(error) = dialog.error.as_deref() {
        frame.render_widget(
            Paragraph::new(compact_line(&format!("Error: {error}"), inner.width))
                .style(Style::default().fg(ERROR)),
            rows[row],
        );
        row += 1;
    }
    let (first_hint, second_hint) = match (dialog.pending.dialog.dialog_type.as_str(), compact) {
        ("alert", true) => ("Enter/Esc acknowledge", "Ctrl-C quit"),
        ("alert", false) => (
            "Enter acknowledge · Esc acknowledge",
            "Ctrl-C opens quit confirmation",
        ),
        ("confirm", true) => ("Y/Enter yes · N/Esc no", "Ctrl-C quit"),
        ("confirm", false) => (
            "Y/Enter accept · N/Esc dismiss",
            "Ctrl-C opens quit confirmation",
        ),
        ("prompt", true) => (
            "Enter submit · Esc cancel",
            "←/→ edit · Home/End · Ctrl-C quit",
        ),
        ("prompt", false) => (
            "Enter submit · Esc dismiss",
            "Type to edit · ←/→ · Home/End · Ctrl-C quit",
        ),
        ("beforeunload", true) => ("Enter/Y leave · Esc/N stay", "Ctrl-C quit"),
        ("beforeunload", false) => (
            "Enter/Y leave · Esc/N stay",
            "Ctrl-C opens quit confirmation",
        ),
        _ => (
            "Enter continue · Esc dismiss",
            "Ctrl-C opens quit confirmation",
        ),
    };
    frame.render_widget(
        Paragraph::new(first_hint).style(Style::default().fg(MUTED)),
        rows[row],
    );
    row += 1;
    frame.render_widget(
        Paragraph::new(second_hint).style(Style::default().fg(MUTED)),
        rows[row],
    );
    if is_prompt {
        row += 1;
        frame.render_widget(
            Paragraph::new(format!(
                "Response: {}/{} UTF-8 bytes",
                dialog.prompt_input.len(),
                NATIVE_DIALOG_TEXT_LIMIT_BYTES
            ))
            .style(Style::default().fg(MUTED)),
            rows[row],
        );
    }
}

pub fn render(frame: &mut Frame<'_>, state: &DevTuiState) {
    let area = frame.area();
    let active = overlay::active_overlay(state);
    match active {
        Some(ActiveOverlay::QuitConfirmation) => {
            render_quit_confirmation(frame, state, area);
            return;
        }
        Some(ActiveOverlay::EditorExitPrompt) => {
            if state.code_edit_mode {
                render_fullscreen_editor(frame, state, area);
            } else if let Some(prompt) = state.editor_exit_prompt {
                render_editor_exit_prompt(frame, area, prompt);
            }
            return;
        }
        Some(ActiveOverlay::BrowserDialog) => {
            render_native_browser_dialog(frame, state, area);
            return;
        }
        Some(ActiveOverlay::Help) => {
            render_help(frame, state, area);
            return;
        }
        Some(ActiveOverlay::FullscreenEditor) => {
            render_fullscreen_editor(frame, state, area);
            return;
        }
        _ => {}
    }
    let editor_background = state.code_edit_mode
        && matches!(
            active,
            Some(
                ActiveOverlay::FilePicker | ActiveOverlay::SessionPicker | ActiveOverlay::Composer
            )
        );
    if editor_background {
        render_fullscreen_editor(frame, state, area);
    } else if state.factory_split
        && active == Some(ActiveOverlay::Composer)
        && !state.focused_editor_path.is_empty()
        && matches!(state.surface, DevSurface::Agent | DevSurface::Code)
        && !matches!(
            state.responsive_class(area.width, area.height),
            ResponsiveClass::Phone
        )
    {
        render_factory_home(frame, state, area);
        return;
    } else {
        match state.responsive_class(area.width, area.height) {
            ResponsiveClass::Desktop => render_desktop(frame, state, area),
            ResponsiveClass::Compact => render_compact(frame, state, area),
            ResponsiveClass::Phone => render_phone(frame, state, area),
        }
    }
    match active {
        Some(ActiveOverlay::CommandPalette) => render_command_palette(frame, state, area),
        Some(ActiveOverlay::PiSlashCommand) => render_pi_command_palette(frame, state, area),
        Some(ActiveOverlay::FilePicker) => render_file_picker(frame, state, area),
        Some(ActiveOverlay::SessionPicker) => render_session_picker(frame, state, area),
        _ => {}
    }
}
fn render_fullscreen_editor(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let rows = fullscreen_editor_rows(state, area);
    let content = state.focused_editor_content.as_str();
    let line_count = content.split('\n').count().max(1);
    let dirty = if state.focused_editor_dirty {
        ("● UNSAVED", WARNING)
    } else {
        ("○ saved", SUCCESS)
    };
    let header = vec![
        Line::from(vec![
            Span::styled(
                " GLASS DEV · EDITOR ",
                Style::default()
                    .fg(Color::Black)
                    .bg(ACCENT_BRIGHT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {} ", compact_path(&state.focused_editor_path, area.width)),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(dirty.0, Style::default().fg(dirty.1)),
            Span::raw("  "),
            Span::styled(
                format!(" {} ", state.editor_engine.mode.label()),
                Style::default()
                    .fg(Color::Black)
                    .bg(
                        if matches!(state.editor_engine.mode, super::editor::EditorMode::Insert) {
                            SUCCESS
                        } else {
                            ACCENT
                        },
                    )
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(Span::styled(
            compact_line(
                &format!(
                    "Ln {} · Col {} · {}/{} · {} · hunks {} · {}",
                    state.focused_editor_line,
                    state.focused_editor_column,
                    state.focused_editor_line,
                    line_count,
                    file_view::classify(&state.focused_editor_path).label(),
                    state.editor_engine.hunks.len(),
                    [
                        super::editor::GutterMark::Lsp,
                        super::editor::GutterMark::Git,
                        super::editor::GutterMark::Agent,
                        super::editor::GutterMark::Page,
                        super::editor::GutterMark::Proof,
                        super::editor::GutterMark::Comment,
                    ]
                    .into_iter()
                    .map(super::editor::GutterMark::glyph)
                    .collect::<String>(),
                ),
                area.width.saturating_sub(2),
            ),
            Style::default().fg(MUTED),
        )),
    ];
    frame.render_widget(
        Paragraph::new(header).style(Style::default().bg(PANEL_BACKGROUND)),
        rows[0],
    );

    let editor_block = surface_block(" SOURCE ", ACCENT_BRIGHT);
    let editor_inner = editor_block.inner(rows[1]);
    let marks = editor_mark_glyphs(state);
    let notes = state.editor_source_notes();
    let decorations = file_view::EditorDecorations {
        marks: &marks,
        inlays: &notes,
        extra_selections: &state.editor_engine.extra_selections,
    };
    let wrapped_cursor = if state.editor_soft_wrap {
        let mut wrapped = file_view::render_editable_source_wrapped(
            &state.focused_editor_path,
            content,
            state.focused_editor_line,
            state.focused_editor_column,
            state.focused_editor_selection.as_ref(),
            editor_inner.width.max(1),
            &decorations,
        );
        if let Some(ghost) = state.visible_ghost_text() {
            file_view::insert_ghost_at_cursor(
                &mut wrapped,
                ghost,
                Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
            );
        }
        let cursor = wrapped.cursor;
        frame.render_widget(
            Paragraph::new(wrapped.text)
                .style(Style::default().fg(TEXT).bg(PANEL_INSET))
                .scroll((state.editor_scroll_line.min(u16::MAX as usize) as u16, 0))
                .block(editor_block),
            rows[1],
        );
        cursor
    } else {
        let mut text = file_view::render_editable_source(
            &state.focused_editor_path,
            content,
            state.focused_editor_line,
            state.focused_editor_column,
            state.focused_editor_selection.as_ref(),
            &decorations,
        );
        if let Some(ghost) = state.visible_ghost_text() {
            let index = state.focused_editor_line.saturating_sub(1) as usize;
            let source_length = content
                .split('\n')
                .nth(index)
                .unwrap_or_default()
                .chars()
                .count();
            file_view::insert_ghost_at_source_cursor(
                &mut text,
                index,
                state.focused_editor_column.saturating_sub(1) as usize,
                source_length,
                ghost,
                Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
            );
        }
        frame.render_widget(
            Paragraph::new(text)
                .style(Style::default().fg(TEXT).bg(PANEL_INSET))
                .scroll((
                    state.editor_scroll_line.min(u16::MAX as usize) as u16,
                    state.editor_scroll_column.min(u16::MAX as usize) as u16,
                ))
                .block(editor_block),
            rows[1],
        );
        None
    };

    let insert = matches!(
        state.editor_engine.mode,
        EditorMode::Insert | EditorMode::Select
    );
    let (editor_help, exit_help) = if area.width < 40 {
        (
            "i edit · Ctrl-S save",
            if insert {
                "Esc normal"
            } else {
                "Esc back · Ctrl-C quit"
            },
        )
    } else if insert {
        ("Type to edit · Ctrl-S save · Alt-A ask", "Esc normal")
    } else if area.width < 70 {
        (
            "Arrows/hjkl move · i edit · Ctrl-S save",
            "Esc back · Ctrl-C quit",
        )
    } else {
        (
            "Arrows/hjkl move · i edit · Ctrl-S save · Alt-A ask",
            "Esc back · unsaved changes ask first",
        )
    };
    if overlay::active_overlay(state) == Some(ActiveOverlay::Composer) {
        render_status(frame, state, rows[2]);
    } else {
        let footer = vec![
            Line::from(Span::styled(
                compact_line(&state.status, area.width.saturating_sub(2)),
                status_style(state),
            )),
            Line::from(Span::styled(editor_help, Style::default().fg(MUTED))),
            Line::from(Span::styled(
                exit_help,
                Style::default()
                    .fg(ACCENT_BRIGHT)
                    .add_modifier(Modifier::BOLD),
            )),
        ];
        frame.render_widget(
            Paragraph::new(footer)
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

    if let Some(prompt) = state.editor_exit_prompt {
        render_editor_exit_prompt(frame, area, prompt);
        return;
    }
    if !state.focused_editor_path.is_empty() && editor_inner.width > 0 && editor_inner.height > 0 {
        let (x, y) = if let Some(cursor) = wrapped_cursor {
            let row = cursor.row.saturating_sub(state.editor_scroll_line);
            (
                editor_inner
                    .x
                    .saturating_add(cursor.column.min(u16::MAX as usize) as u16),
                editor_inner.y.saturating_add(
                    row.min(usize::from(editor_inner.height.saturating_sub(1))) as u16,
                ),
            )
        } else {
            let gutter_width = line_count.to_string().len().max(3);
            let line = state.focused_editor_line.saturating_sub(1) as usize;
            let row = line.saturating_sub(state.editor_scroll_line);
            let column = state.focused_editor_column.saturating_sub(1) as usize;
            (
                editor_inner.x.saturating_add(
                    (gutter_width + 4)
                        .saturating_add(column)
                        .saturating_sub(state.editor_scroll_column)
                        .min(u16::MAX as usize) as u16,
                ),
                editor_inner.y.saturating_add(
                    row.min(usize::from(editor_inner.height.saturating_sub(1))) as u16,
                ),
            )
        };
        if x < editor_inner.x.saturating_add(editor_inner.width)
            && y < editor_inner.y.saturating_add(editor_inner.height)
        {
            frame.set_cursor_position((x, y));
        }
    }
    if let Some(overlay) = &state.editor_engine.overlay {
        let modal = Rect {
            x: area.x + 2,
            y: area.y + area.height.saturating_sub(10).max(3),
            width: area.width.saturating_sub(4).min(80),
            height: 8,
        };
        frame.render_widget(Clear, modal);
        frame.render_widget(
            Paragraph::new(compact_multiline(overlay, modal.width.saturating_sub(2)))
                .wrap(Wrap { trim: false })
                .block(surface_block(" NOTE ", ACCENT)),
            modal,
        );
    }
}

pub(super) fn fullscreen_editor_rows(state: &DevTuiState, area: Rect) -> [Rect; 3] {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(4),
            Constraint::Length(
                if overlay::active_overlay(state) == Some(ActiveOverlay::Composer) {
                    footer_height(state).max(4)
                } else {
                    4
                },
            ),
        ])
        .split(area);
    [rows[0], rows[1], rows[2]]
}

pub(super) fn pointer_footer_area(state: &DevTuiState, area: Rect) -> Rect {
    if state.code_edit_mode && overlay::active_overlay(state) == Some(ActiveOverlay::Composer) {
        fullscreen_editor_rows(state, area)[2]
    } else {
        screen_geometry(state, area).footer
    }
}

fn render_editor_exit_prompt(
    frame: &mut Frame<'_>,
    area: Rect,
    prompt: super::state::EditorExitPrompt,
) {
    let unsaved = matches!(prompt, super::state::EditorExitPrompt::Unsaved);
    let width = area.width.saturating_sub(4).min(78);
    let height = area
        .height
        .saturating_sub(4)
        .min(if unsaved { 10 } else { 7 });
    let modal = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    let content = if unsaved {
        "UNSAVED CHANGES\n\nS  save and leave editor\nD  discard and leave editor\nQ  discard changes and quit Glass\nEsc stay in editor"
    } else {
        "LEAVE THIS FILE?\n\nEnter / Q  leave the editor\nEsc          stay in the editor"
    };
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Paragraph::new(panel_text(content))
            .style(Style::default().fg(TEXT).bg(PANEL_BACKGROUND))
            .block(
                Block::default()
                    .title(if unsaved {
                        " EXIT · unsaved changes "
                    } else {
                        " EXIT EDITOR · confirm "
                    })
                    .title_style(
                        Style::default()
                            .fg(if unsaved { WARNING } else { ACCENT_BRIGHT })
                            .add_modifier(Modifier::BOLD),
                    )
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(if unsaved { WARNING } else { ACCENT }))
                    .bg(PANEL_BACKGROUND)
                    .padding(Padding::horizontal(1)),
            )
            .wrap(Wrap { trim: false }),
        modal,
    );
}
fn render_quit_confirmation(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
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
        Paragraph::new(panel_text(
            if state.editor_exit_prompt == Some(EditorExitPrompt::Unsaved) {
                "UNSAVED EDITOR CHANGES\n\nEnter discard and quit · Esc keep editing"
            } else {
                "QUIT?\n\nEnter quit · Esc stay"
            },
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

fn render_file_picker(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let width = area.width.saturating_sub(4).min(104);
    let height = area.height.saturating_sub(6).max(5).min(area.height);
    let modal = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + 1,
        width,
        height,
    };
    let block = Block::default()
        .title(" OPEN FILE · Ctrl-P · type to filter · Enter open · Esc close ")
        .title_style(
            Style::default()
                .fg(ACCENT_BRIGHT)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .bg(PANEL_BACKGROUND)
        .padding(Padding::horizontal(1));
    let inner = block.inner(modal);
    let matches = state.file_picker_matches();
    let mut lines = vec![palette_fixed_line(
        if state.file_picker_query.is_empty() {
            format!("FILTER · {} files", state.files.len())
        } else {
            format!(
                "FILTER · {}/{}  {}",
                matches.len(),
                state.files.len(),
                state.file_picker_query
            )
        },
        inner.width,
        Style::default()
            .fg(ACCENT_BRIGHT)
            .add_modifier(Modifier::BOLD),
    )];
    if matches.is_empty() {
        lines.push(palette_fixed_line(
            "No matching files",
            inner.width,
            Style::default().fg(MUTED),
        ));
    } else {
        let visible = inner.height.saturating_sub(1) as usize;
        let start = state
            .file_picker_selection
            .saturating_sub(visible.saturating_sub(1));
        for (offset, index) in matches.into_iter().enumerate().skip(start).take(visible) {
            let selected = offset == state.file_picker_selection;
            let path = state.files.get(index).map(String::as_str).unwrap_or("");
            let marker = if selected { "▸ " } else { "  " };
            let style = if selected {
                Style::default()
                    .fg(ACCENT_BRIGHT)
                    .bg(ACTIVE_BACKGROUND)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT).bg(PANEL_BACKGROUND)
            };
            lines.push(Line::from(Span::styled(
                palette_row_text(&format!("{marker}{path}"), inner.width),
                style,
            )));
        }
    }
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(TEXT).bg(PANEL_BACKGROUND))
            .block(block),
        modal,
    );
}

fn render_session_picker(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let width = area.width.saturating_sub(4).min(104);
    let height = area.height.saturating_sub(6).max(5).min(area.height);
    let modal = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + 1,
        width,
        height,
    };
    let block = Block::default()
        .title(" PI SESSIONS · Enter switch · Esc close ")
        .title_style(
            Style::default()
                .fg(ACCENT_BRIGHT)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .bg(PANEL_BACKGROUND)
        .padding(Padding::horizontal(1));
    let inner = block.inner(modal);
    let mut lines = vec![palette_fixed_line(
        format!("SESSIONS · {}", state.session_picker_items.len()),
        inner.width,
        Style::default()
            .fg(ACCENT_BRIGHT)
            .add_modifier(Modifier::BOLD),
    )];
    if state.session_picker_items.is_empty() {
        lines.push(palette_fixed_line(
            "No persisted Pi sessions",
            inner.width,
            Style::default().fg(MUTED),
        ));
    } else {
        let visible = inner.height.saturating_sub(1) as usize;
        let start = state
            .session_picker_selection
            .saturating_sub(visible.saturating_sub(1));
        for (index, item) in state
            .session_picker_items
            .iter()
            .enumerate()
            .skip(start)
            .take(visible)
        {
            let selected = index == state.session_picker_selection;
            let marker = if selected { "▸ " } else { "  " };
            let style = if selected {
                Style::default()
                    .fg(ACCENT_BRIGHT)
                    .bg(ACTIVE_BACKGROUND)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT).bg(PANEL_BACKGROUND)
            };
            lines.push(palette_fixed_line(
                format!("{marker}{} · {}", item.label, item.path),
                inner.width,
                style,
            ));
        }
    }
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(TEXT).bg(PANEL_BACKGROUND))
            .block(block),
        modal,
    );
}

fn render_command_palette(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let width = area.width.saturating_sub(4).min(104);
    let height = area.height.saturating_sub(6).max(5).min(area.height);
    let modal = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + 1,
        width,
        height,
    };
    let address = navigation_value(state);
    let title = match address {
        Some(_) => " NAVIGATE · URL OR DOMAIN · Enter submit · Esc close ".to_string(),
        None => format!(
            " COMMAND PALETTE · {} · ↑↓ select · Enter run · Esc close ",
            state.surface.label()
        ),
    };
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(ACCENT_BRIGHT)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .bg(PANEL_BACKGROUND)
        .padding(Padding::horizontal(1));
    let inner = block.inner(modal);
    let is_navigation = address.is_some();
    let mut lines = match address {
        Some(address) => vec![
            palette_fixed_line(
                "ENTER A URL OR DOMAIN",
                inner.width,
                Style::default()
                    .fg(ACCENT_BRIGHT)
                    .add_modifier(Modifier::BOLD),
            ),
            palette_fixed_line(
                "https:// is optional · Enter navigates · Esc cancels",
                inner.width,
                Style::default().fg(MUTED),
            ),
            palette_fixed_line(
                format!("Address: {address}"),
                inner.width,
                Style::default().fg(TEXT),
            ),
        ],
        None => vec![
            palette_fixed_line(
                "SELECT AN ACTION",
                inner.width,
                Style::default()
                    .fg(ACCENT_BRIGHT)
                    .add_modifier(Modifier::BOLD),
            ),
            palette_fixed_line(
                "Command search · ↑/↓ select · Enter run · Esc close · type only to filter (optional)",
                inner.width,
                Style::default().fg(MUTED),
            ),
        ],
    };
    if !state.command_input.is_empty() {
        lines.push(palette_fixed_line(
            format!("Filter: {}", state.command_input),
            inner.width,
            Style::default().fg(MUTED),
        ));
    }
    lines.push(palette_fixed_line(
        "",
        inner.width,
        Style::default().bg(PANEL_BACKGROUND),
    ));
    let action_offset = lines.len();
    let indices = state.palette_action_indices();
    if indices.is_empty() {
        let message = if is_navigation {
            "Ready · Enter runs this navigation · Esc cancels"
        } else if !state.command_input.trim().is_empty() {
            "No matching action · Enter runs the typed command · Ctrl-U clears"
        } else {
            "No matching actions · type a command or Esc closes"
        };
        lines.push(palette_fixed_line(
            message,
            inner.width,
            Style::default().fg(MUTED),
        ));
    } else {
        for (visible_index, action_index) in indices.into_iter().enumerate() {
            let action = &state.surface_actions()[action_index];
            lines.push(palette_action_line(
                action,
                visible_index == state.palette_selection,
                inner.width,
            ));
        }
    }
    let line_count = lines.len();
    let visible_lines = usize::from(inner.height);
    let max_scroll = line_count.saturating_sub(visible_lines);
    let selected_line = action_offset.saturating_add(state.palette_selection);
    let selected_scroll = selected_line.saturating_sub(visible_lines.saturating_sub(1));
    let scroll = usize::from(state.palette_scroll)
        .max(selected_scroll)
        .min(max_scroll) as u16;
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .scroll((scroll, 0))
            .block(block),
        modal,
    );
}

fn render_pi_command_palette(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let width = area.width.saturating_sub(4).min(112);
    let height = area.height.saturating_sub(6).max(5).min(area.height);
    let modal = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + 1,
        width,
        height,
    };
    let block = Block::default()
        .title(" PI COMMANDS · resident AgentSession · Esc close ")
        .title_style(Style::default().fg(PURPLE).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(PURPLE))
        .bg(PANEL_BACKGROUND)
        .padding(Padding::horizontal(1));
    let inner = block.inner(modal);
    let mut lines = vec![
        palette_fixed_line(
            "NATIVE PI SLASH COMMANDS",
            inner.width,
            Style::default().fg(PURPLE).add_modifier(Modifier::BOLD),
        ),
        palette_fixed_line(
            "Official SDK commands · extension commands can also be typed directly",
            inner.width,
            Style::default().fg(MUTED),
        ),
    ];
    if !state.pi_command_input.is_empty() {
        lines.push(palette_fixed_line(
            format!(
                "Command: /{}",
                state.pi_command_input.trim_start_matches('/')
            ),
            inner.width,
            Style::default().fg(TEXT),
        ));
    }
    lines.push(palette_fixed_line(
        "",
        inner.width,
        Style::default().bg(PANEL_BACKGROUND),
    ));
    let command_offset = lines.len();
    let indices = state.pi_command_indices();
    if indices.is_empty() {
        lines.push(palette_fixed_line(
            "No built-in match · Enter still sends a typed extension command",
            inner.width,
            Style::default().fg(MUTED),
        ));
    } else {
        for (visible_index, command_index) in indices.into_iter().enumerate() {
            let command = &pi_commands::BUILTIN_PI_COMMANDS[command_index];
            let hint = command.argument_hint.unwrap_or("");
            let invocation = if hint.is_empty() {
                format!("/{}", command.name)
            } else {
                format!("/{} {}", command.name, hint)
            };
            let content = format!(
                "{}{:<28} · {}",
                if visible_index == state.pi_command_selection {
                    "▸ "
                } else {
                    "  "
                },
                invocation,
                command.description
            );
            let style = if visible_index == state.pi_command_selection {
                Style::default()
                    .fg(ACCENT_BRIGHT)
                    .bg(ACTIVE_BACKGROUND)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT).bg(PANEL_BACKGROUND)
            };
            lines.push(palette_fixed_line(content, inner.width, style));
        }
    }
    let line_count = lines.len();
    let visible_lines = usize::from(inner.height);
    let max_scroll = line_count.saturating_sub(visible_lines);
    let selected_line = command_offset.saturating_add(state.pi_command_selection);
    let selected_scroll = selected_line.saturating_sub(visible_lines.saturating_sub(1));
    let scroll = usize::from(state.pi_command_scroll)
        .max(selected_scroll)
        .min(max_scroll) as u16;
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .scroll((scroll, 0))
            .block(block),
        modal,
    );
}

#[cfg(test)]
fn command_palette_content(state: &DevTuiState) -> String {
    let address = navigation_value(state);
    let is_navigation = address.is_some();
    let indices = state.palette_action_indices();
    let mut lines = match address {
        Some(address) => vec![
            "ENTER A URL OR DOMAIN".to_string(),
            "https:// is optional · Enter navigates · Esc cancels".to_string(),
            format!("Address: {address}"),
        ],
        None => vec![
            "SELECT AN ACTION".to_string(),
            "Command search · ↑/↓ select · Enter run · Esc close · type only to filter (optional)"
                .to_string(),
        ],
    };
    lines.push(String::new());
    if indices.is_empty() {
        lines.push(if is_navigation {
            "Ready · Enter runs this navigation · Esc cancels".into()
        } else if !state.command_input.trim().is_empty() {
            "No matching action · Enter runs the typed command · Ctrl-U clears".into()
        } else {
            "No matching actions · type a command or Esc closes".into()
        });
    } else {
        for (visible_index, action_index) in indices.into_iter().enumerate() {
            let action = &state.surface_actions()[action_index];
            let marker = if visible_index == state.palette_selection {
                "▸"
            } else {
                " "
            };
            lines.push(format!(
                "{marker} {:<26} · {}",
                action.label,
                palette_action_hint(action.command)
            ));
        }
    }
    lines.join("\n")
}
const BROWSER_NAVIGATE_PREFIX: &str = "browser navigate ";

fn navigation_value(state: &DevTuiState) -> Option<&str> {
    state.command_input.strip_prefix(BROWSER_NAVIGATE_PREFIX)
}

fn navigation_cursor(state: &DevTuiState, address: &str) -> usize {
    state
        .command_cursor
        .saturating_sub(BROWSER_NAVIGATE_PREFIX.len())
        .min(address.len())
}
fn palette_fixed_line(content: impl Into<String>, width: u16, style: Style) -> Line<'static> {
    let content = content.into();
    let mut padded_content = String::with_capacity(content.len() + 1);
    padded_content.push(' ');
    padded_content.push_str(&content);
    Line::from(Span::styled(
        palette_row_text(&padded_content, width),
        style,
    ))
}

fn palette_action_line(
    action: &command::SurfaceAction,
    selected: bool,
    width: u16,
) -> Line<'static> {
    let marker = if selected { "▸ " } else { "  " };
    let content = format!(
        "{marker}{:<26} · {}",
        action.label,
        palette_action_hint(action.command)
    );
    let style = if selected {
        Style::default()
            .fg(ACCENT_BRIGHT)
            .bg(ACTIVE_BACKGROUND)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT).bg(PANEL_BACKGROUND)
    };
    Line::from(Span::styled(palette_row_text(&content, width), style))
}

fn palette_row_text(content: &str, width: u16) -> String {
    let mut row = compact_line(content, width);
    let padding = usize::from(width).saturating_sub(row.chars().count());
    if padding > 0 {
        row.push_str(&" ".repeat(padding));
    }
    row
}

fn palette_action_hint(command: &str) -> String {
    let mut prefix = Vec::new();
    let mut has_arguments = false;
    for token in command.split_whitespace() {
        if token
            .chars()
            .all(|character| character.is_ascii_uppercase() || character == '_')
        {
            has_arguments = true;
            break;
        }
        prefix.push(token);
    }
    let mut hint = prefix.join(" ");
    if has_arguments {
        if !hint.is_empty() {
            hint.push(' ');
        }
        hint.push('…');
    }
    hint
}

fn help_content(surface: DevSurface) -> String {
    super::bindings::curriculum_help(surface)
}

fn render_help(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let content = help_content(state.surface);
    frame.render_widget(
        Paragraph::new(panel_text(&content))
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
fn render_factory_home(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height()),
            Constraint::Min(8),
            Constraint::Length(footer_height(state)),
        ])
        .split(area);
    render_header(frame, state, rows[0], "factory");
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Min(36)])
        .split(rows[1]);
    if state.surface == DevSurface::Code {
        render_agent_conversation_panel(
            frame,
            state,
            "Ask from Code · Ctrl-L dock · comments stay on this buffer",
            columns[0],
        );
    } else {
        render_surface(frame, state, columns[0]);
    }
    let marks = editor_mark_glyphs(state);
    let notes = state.editor_source_notes();
    let decorations = file_view::EditorDecorations {
        marks: &marks,
        inlays: &notes,
        extra_selections: &state.editor_engine.extra_selections,
    };
    let source = file_view::render_editable_source(
        &state.focused_editor_path,
        if state.focused_editor_content.is_empty() {
            "No buffer · Ctrl-P open file"
        } else {
            &state.focused_editor_content
        },
        state.focused_editor_line,
        state.focused_editor_column,
        state.focused_editor_selection.as_ref(),
        &decorations,
    );
    frame.render_widget(
        Paragraph::new(source)
            .style(Style::default().fg(TEXT).bg(PANEL_INSET))
            .block(surface_block(
                format!(" SOURCE · {} ", state.editor_engine.mode.label()),
                ACCENT_BRIGHT,
            )),
        columns[1],
    );
    render_status(frame, state, rows[2]);
}

fn render_desktop(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let geometry = screen_geometry(state, area);
    render_header(frame, state, geometry.header, "desktop");
    if let Some(navigation) = geometry.navigation {
        render_navigation(frame, state, navigation);
    }
    render_surface(frame, state, geometry.surface);
    if let Some(context) = geometry.context {
        render_context(frame, state, context);
    }
    render_status(frame, state, geometry.footer);
}

fn render_compact(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let geometry = screen_geometry(state, area);
    render_header(frame, state, geometry.header, "compact");
    if let Some(navigation) = geometry.navigation {
        render_navigation(frame, state, navigation);
    }
    render_surface(frame, state, geometry.surface);
    render_status(frame, state, geometry.footer);
}
fn render_phone(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let geometry = screen_geometry(state, area);
    render_header(frame, state, geometry.header, "phone cockpit");
    render_surface(frame, state, geometry.body);
    let active = overlay::active_overlay(state);
    let footer_lines = if active == Some(ActiveOverlay::Composer) {
        let mut lines = composer_input_lines(state, geometry.footer.width.saturating_sub(6));
        lines.push(status_line(state, geometry.footer.width.saturating_sub(2)));
        lines
    } else if active == Some(ActiveOverlay::FilePicker) {
        vec![
            Line::from(input_spans(
                " file: ",
                Style::default()
                    .fg(ACCENT_BRIGHT)
                    .add_modifier(Modifier::BOLD),
                &state.file_picker_query,
                state.file_picker_cursor,
                geometry.footer.width.saturating_sub(8),
            )),
            status_line(state, geometry.footer.width.saturating_sub(2)),
        ]
    } else if active == Some(ActiveOverlay::PiSlashCommand) {
        let mut spans = input_spans(
            "/ ",
            Style::default().fg(PURPLE).add_modifier(Modifier::BOLD),
            &state.pi_command_input,
            state.pi_command_cursor,
            geometry.footer.width.saturating_sub(8),
        );
        let hint = if state.pi_command_input.trim().is_empty() {
            "  ↑↓ select · Tab complete · Enter run · Esc cancel"
        } else {
            "  Enter runs native command · Esc cancel"
        };
        spans.push(Span::styled(hint, Style::default().fg(MUTED)));
        vec![Line::from(spans)]
    } else if active == Some(ActiveOverlay::CommandPalette) {
        let (prefix, input, cursor) = match navigation_value(state) {
            Some(address) => (" URL: ", address, navigation_cursor(state, address)),
            None => (" : ", state.command_input.as_str(), state.command_cursor),
        };
        vec![
            Line::from(input_spans(
                prefix,
                Style::default()
                    .fg(ACCENT_BRIGHT)
                    .add_modifier(Modifier::BOLD),
                input,
                cursor,
                geometry.footer.width.saturating_sub(6),
            )),
            status_line(state, geometry.footer.width.saturating_sub(2)),
        ]
    } else {
        vec![
            Line::from(Span::styled(
                compact_line(
                    "Tab surfaces · 1-5 jump · a actions · ? help",
                    geometry.footer.width.saturating_sub(2),
                ),
                Style::default().fg(MUTED),
            )),
            status_line(state, geometry.footer.width.saturating_sub(2)),
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
        geometry.footer,
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
    let mode_label = if state.yolo_mode {
        "YOLO · confirmations off"
    } else {
        "guarded"
    };
    let git = git_header_label(state);
    let trust = Span::styled(
        format!("{} · {mode_label} ", state.snapshot_trust_label),
        Style::default().fg(
            if state.snapshot_trust_label == "untrusted" || state.yolo_mode {
                WARNING
            } else {
                MUTED
            },
        ),
    );
    let path = Span::styled(
        compact_path(&state.snapshot_root, area.width.saturating_sub(32)),
        Style::default().fg(MUTED),
    );
    let git_span = git.map(|branch| {
        Span::styled(
            format!("{branch} · "),
            Style::default().fg(if state.git_dirty { WARNING } else { SUCCESS }),
        )
    });
    let mut meta = vec![Span::raw(" ")];
    if let Some(git_span) = git_span.clone() {
        meta.push(git_span);
    }
    meta.push(path.clone());
    meta.push(Span::styled(" · ", Style::default().fg(PANEL_BORDER)));
    meta.push(trust.clone());
    let lines = if area.width < 104 {
        vec![Line::from(vec![brand.clone(), surface]), Line::from(meta)]
    } else {
        let mut line = vec![brand, Span::raw("  "), surface];
        if let Some(git_span) = git_span {
            line.push(Span::raw(" "));
            line.push(git_span);
        }
        line.push(path);
        line.push(Span::styled(" · ", Style::default().fg(PANEL_BORDER)));
        line.push(trust);
        vec![Line::from(line)]
    };

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(PANEL_BACKGROUND)),
        area,
    );
}

fn editor_mark_glyphs(state: &DevTuiState) -> Vec<(u32, char)> {
    state
        .editor_gutter_marks()
        .into_iter()
        .map(|(line, mark)| (line, mark.glyph()))
        .collect()
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
fn compact_multiline(text: &str, width: u16) -> String {
    text.lines()
        .map(|line| compact_line(line, width))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_navigation(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let items = navigation_surfaces(state)
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
    frame.render_widget(
        List::new(items)
            .style(Style::default().bg(PANEL_BACKGROUND))
            .block(navigation_block()),
        area,
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

pub(super) fn panel_content_area(area: Rect) -> Rect {
    surface_block("", ACCENT_BRIGHT).inner(area)
}

#[derive(Debug, Clone, Copy)]
struct IndexedListGeometry {
    content: Rect,
    item_count: usize,
    first_item: usize,
    selected: Option<usize>,
}

fn indexed_list_geometry(area: Rect, item_count: usize, selected: usize) -> IndexedListGeometry {
    let content = panel_content_area(area);
    let selected = (selected < item_count).then_some(selected);
    let visible_rows = usize::from(content.height);
    let first_item = match selected {
        Some(selected) if visible_rows > 0 => {
            selected.saturating_add(1).saturating_sub(visible_rows)
        }
        _ => 0,
    };
    IndexedListGeometry {
        content,
        item_count,
        first_item,
        selected,
    }
}

impl IndexedListGeometry {
    fn hit(&self, column: u16, row: u16) -> Option<usize> {
        if self.item_count == 0
            || self.content.width == 0
            || self.content.height == 0
            || column < self.content.x
            || column >= self.content.right()
            || row < self.content.y
            || row >= self.content.bottom()
        {
            return None;
        }
        let index = self
            .first_item
            .saturating_add(usize::from(row - self.content.y));
        (index < self.item_count).then_some(index)
    }

    fn apply_to_list_state(self, state: &mut ListState) {
        *state.offset_mut() = self.first_item;
        state.select(self.selected);
    }
}

fn indexed_list_hit(
    area: Rect,
    item_count: usize,
    selected: usize,
    column: u16,
    row: u16,
) -> Option<usize> {
    indexed_list_geometry(area, item_count, selected).hit(column, row)
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

fn render_scrolled_panel(
    frame: &mut Frame<'_>,
    area: Rect,
    title: impl Into<String>,
    content: impl Into<String>,
    title_color: Color,
    scroll: usize,
) {
    if area.width < 2 || area.height < 2 {
        return;
    }
    frame.render_widget(
        Paragraph::new(panel_text(&content.into()))
            .style(Style::default().fg(TEXT).bg(PANEL_BACKGROUND))
            .scroll((scroll.min(u16::MAX as usize) as u16, 0))
            .block(surface_block(title, title_color))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn stack_for_phone(state: &DevTuiState, area: Rect) -> bool {
    match state.responsive_class(
        state.terminal_width.max(area.width),
        state.terminal_height.max(area.height),
    ) {
        ResponsiveClass::Phone => true,
        ResponsiveClass::Compact => area.width < 70,
        ResponsiveClass::Desktop => false,
    }
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
            "{} pending review · {}\nI inspect · O open\n1 trust once · T trust project",
            pending, state.snapshot_trust_label
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
    let chrome = state.agent_chrome_line();
    let mode = state.composer_run_mode.label();
    let second = if chrome.is_empty() {
        format!("{mode} · Ctrl-L · {app_summary}")
    } else {
        format!("{mode} · {chrome} · {app_summary}")
    };
    format!(
        "{}\n{}",
        compact_line(runtime_state, width),
        compact_line(&second, width)
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

fn conversation_entry_lines(
    entries: &[super::projection::ConversationEntry],
    selected: usize,
    expanded: bool,
    width: u16,
) -> Vec<Line<'static>> {
    let mut rendered = String::new();
    for (index, entry) in entries.iter().enumerate() {
        if !rendered.is_empty() {
            rendered.push_str("\n\n");
        }
        let mut block = if !expanded && entry.tool_name.is_some() {
            let heading = match entry.kind {
                super::projection::ConversationKind::Alert => "ALERT",
                super::projection::ConversationKind::Error => "ERROR",
                _ => "SYSTEM",
            };
            format!("{heading}\n{}", entry.card_line())
        } else {
            entry.render()
        };
        if index == selected {
            block = format!("▸ {block}");
        }
        rendered.push_str(&block);
    }
    conversation_bubble_lines(&rendered, width)
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
        let (selected_bubble, first) = first
            .strip_prefix("▸ ")
            .map(|rest| (true, rest))
            .unwrap_or((false, first));
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
        let label = if selected_bubble {
            format!("▸ {label}")
        } else {
            label.to_string()
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
    let entries = state.conversation_entries_view();
    let mut lines = if entries.is_empty() {
        panel_text(landing).lines
    } else {
        conversation_entry_lines(
            &entries,
            state.transcript_selection,
            state.transcript_expanded,
            width,
        )
    };
    if !state.composer_mode {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Enter chat · j/k bubbles · f fork · r rewind · e last",
            Style::default().fg(MUTED),
        )));
    } else {
        let chips = state.composer_context_chips();
        if !chips.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("Context · {chips}"),
                Style::default().fg(MUTED),
            )));
        }
        if state.composer_steer {
            lines.push(Line::from(Span::styled(
                "Steer · Ctrl-D follow-up · Ctrl-X abort",
                Style::default().fg(WARNING),
            )));
        }
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
            "START HERE\nDescribe a coding task.\nEnter to chat or type a message.\n/ Pi commands · : Glass actions\nGlass inspects, edits, runs, verifies.\nBrowser opens only for UI work.".into()
        } else {
            "SETUP\nPress :actions.\nChoose Setup Pi runtime · Enter to install.\nChoose Authenticate if installed.\nThen Enter or type to chat.".into()
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
    let workspace_content = if !state.snapshot_ready {
        "Loading workspace snapshot…\nGit, checks, and service counts will appear after refresh."
            .to_string()
    } else {
        let branch = state.git.lines().next().unwrap_or("branch unavailable");
        let branch = branch.strip_prefix("branch ").unwrap_or(branch);
        let check = state
            .tests
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("No checks run yet");
        format!(
            "BRANCH {branch}\n{} changed · rev {}\nGITHUB {}\n{}\n{}\n{}",
            state.git_entries.len(),
            state.snapshot_project_revision,
            state.github.summary(),
            activity_summary(state),
            check,
            state
                .session_todos
                .items
                .iter()
                .take(3)
                .map(|item| format!("{} {}", item.status.label(), item.title))
                .collect::<Vec<_>>()
                .join(" · ")
        )
    };
    render_panel(
        frame,
        rows[0],
        " WORKSPACE ",
        workspace_content,
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
    let items = if state.files.is_empty() && !state.snapshot_ready {
        vec![ListItem::new(Line::from(Span::styled(
            "Loading workspace snapshot…",
            Style::default().fg(MUTED),
        )))]
    } else if state.files.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No project files found · create or open a file",
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
                let git = state
                    .git_entries
                    .iter()
                    .find(|entry| entry.path == *path || path.ends_with(&entry.path));
                let icon = if path.ends_with('/') {
                    "▾ ".into()
                } else if let Some(entry) = git {
                    if entry.untracked {
                        "?? ".into()
                    } else {
                        format!("{}{} ", entry.index_status, entry.worktree_status)
                    }
                } else if focused {
                    "▸ ".into()
                } else {
                    "  ".into()
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
                    Span::styled(compact_path(path, area.width.saturating_sub(9)), style),
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
    let list_geometry =
        indexed_list_geometry(area, state.files.len().min(128), state.selected_file);
    let mut list_state = ListState::default();
    list_geometry.apply_to_list_state(&mut list_state);
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
        " EDITOR ".to_string()
    } else {
        format!(
            " PREVIEW · Ln {} Col {} ",
            state.focused_editor_line, state.focused_editor_column,
        )
    };
    let content = if state.editor.trim().is_empty() {
        "No file open\n↑/↓ select a file\nEnter full-screen\ni edits"
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

#[derive(Debug, Clone, Copy)]
struct CodeSurfaceGeometry {
    files: Rect,
    editor: Rect,
    collaboration: Option<Rect>,
    lsp: Option<Rect>,
}

fn code_surface_geometry(state: &DevTuiState, area: Rect) -> CodeSurfaceGeometry {
    if stack_for_phone(state, area) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(28),
                Constraint::Percentage(52),
                Constraint::Percentage(20),
            ])
            .split(area);
        return CodeSurfaceGeometry {
            files: rows[0],
            editor: rows[1],
            collaboration: Some(rows[2]),
            lsp: None,
        };
    }
    if area.width < 96 {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(24), Constraint::Min(28)])
            .split(area);
        return CodeSurfaceGeometry {
            files: columns[0],
            editor: columns[1],
            collaboration: None,
            lsp: None,
        };
    }
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(28),
            Constraint::Min(36),
            Constraint::Length(28),
        ])
        .split(area);
    let side = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(54), Constraint::Percentage(46)])
        .split(columns[2]);
    CodeSurfaceGeometry {
        files: columns[0],
        editor: columns[1],
        collaboration: Some(side[0]),
        lsp: Some(side[1]),
    }
}

pub(super) fn code_file_list_area(state: &DevTuiState, area: Rect) -> Rect {
    code_surface_geometry(state, area).files
}

pub(super) fn file_hit_at(state: &DevTuiState, area: Rect, column: u16, row: u16) -> Option<usize> {
    indexed_list_hit(
        code_file_list_area(state, area),
        state.files.len().min(128),
        state.selected_file,
        column,
        row,
    )
}

fn render_code_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let geometry = code_surface_geometry(state, area);
    render_file_tree(frame, state, geometry.files);
    render_code_editor(frame, state, geometry.editor);
    if let Some(collaboration) = geometry.collaboration {
        render_editor_collaboration(frame, state, collaboration);
    }
    if let Some(lsp) = geometry.lsp {
        render_editor_lsp(frame, state, lsp);
    }
}

fn render_browser_visual(
    frame: &mut Frame<'_>,
    state: &DevTuiState,
    area: Rect,
    browser: &glass_browser::browser_workspace::BrowserWorkspaceState,
) {
    if browser
        .presentation_reason
        .as_deref()
        .is_some_and(|reason| {
            reason.starts_with("Live view paused") || reason.starts_with("Live view resuming")
        })
    {
        let reason = browser.presentation_reason.as_deref().unwrap_or_default();
        render_panel(
            frame,
            area,
            " LIVE VIEW · PAUSED ",
            format!("{}\n{}", browser_progress(state, browser), reason),
            WARNING,
        );
        return;
    }
    if state.browser_visual_live {
        if let Some(pane) = state.browser_pane.as_ref() {
            draw_ansi_pane(frame, area, pane);
            return;
        }
        if matches!(
            browser.presentation,
            glass_browser::browser_workspace::BrowserPresentationPath::Herdr
                | glass_browser::browser_workspace::BrowserPresentationPath::Kitty
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
    let content = browser_inspector_content(state, area, browser);
    render_panel(frame, area, " INSPECTOR ", content.rendered, ACCENT_BRIGHT);
}

struct BrowserInspectorContent {
    prefix: String,
    entity_lines: Vec<String>,
    rendered: String,
}

fn browser_inspector_content(
    state: &DevTuiState,
    area: Rect,
    browser: &glass_browser::browser_workspace::BrowserWorkspaceState,
) -> BrowserInspectorContent {
    let entity_lines = browser
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
        .collect::<Vec<_>>();
    let prefix = format!(
        "{}\n{}\n\n{} · rev {}\n{} · focus {}\n\n",
        browser.title,
        compact_path(&browser.url, area.width.saturating_sub(6)),
        browser.connection_label(),
        browser
            .browser_revision
            .map_or_else(|| "—".into(), |revision| revision.to_string()),
        browser.input_owner_label(),
        browser.focus_label(),
    );
    let entities = if entity_lines.is_empty() {
        "No semantic entities".into()
    } else {
        entity_lines.join("\n")
    };
    BrowserInspectorContent {
        rendered: format!("{prefix}{entities}\n\n{}", state.browser_detail),
        prefix,
        entity_lines,
    }
}

#[derive(Debug, Clone, Copy)]
struct AppSurfaceGeometry {
    toolbar: Rect,
    visual: Rect,
    inspector: Option<Rect>,
    workflow: Rect,
}

fn app_surface_geometry(state: &DevTuiState, area: Rect) -> AppSurfaceGeometry {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(4),
        ])
        .split(area);
    if stack_for_phone(state, rows[1]) {
        return AppSurfaceGeometry {
            toolbar: rows[0],
            visual: rows[1],
            inspector: None,
            workflow: rows[2],
        };
    }
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Min(30)])
        .split(rows[1]);
    AppSurfaceGeometry {
        toolbar: rows[0],
        visual: columns[0],
        inspector: Some(columns[1]),
        workflow: rows[2],
    }
}

pub(super) fn browser_inspector_area(state: &DevTuiState, area: Rect) -> Option<Rect> {
    app_surface_geometry(state, area).inspector
}

pub(super) fn browser_entity_at(
    state: &DevTuiState,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<usize> {
    let browser = state.browser_workspace.state();
    let inspector = browser_inspector_area(state, area)?;
    if browser.entities.is_empty() || inspector.width < 2 || inspector.height < 2 {
        return None;
    }
    let inner = panel_content_area(inspector);
    if inner.width == 0 || inner.height == 0 || column < inner.x || column >= inner.right() {
        return None;
    }
    let content = browser_inspector_content(state, inspector, browser);
    let prefix_lines = panel_text(&content.prefix).lines;
    if prefix_lines
        .iter()
        .any(|line| line.width() > usize::from(inner.width))
    {
        return None;
    }
    let mut item_row = inner
        .y
        .saturating_add(prefix_lines.len().min(u16::MAX as usize) as u16);
    for (index, entity) in content.entity_lines.iter().enumerate() {
        let lines = panel_text(entity).lines;
        if lines.len() != 1 || lines[0].width() > usize::from(inner.width) {
            return None;
        }
        if row == item_row && row < inner.bottom() {
            return Some(index);
        }
        item_row = item_row.saturating_add(1);
    }
    None
}

fn render_app_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let browser = state.browser_workspace.state();
    let geometry = app_surface_geometry(state, area);
    let address = if browser.url.is_empty() {
        "No page yet · :browser navigate URL".into()
    } else {
        compact_path(&browser.url, geometry.toolbar.width.saturating_sub(18))
    };
    let toolbar = format!(
        "{} · {}\n{}",
        if browser.loading { "loading" } else { "ready" },
        browser.connection_label(),
        address,
    );
    render_panel(frame, geometry.toolbar, " BROWSER ", toolbar, ACCENT_BRIGHT);

    render_browser_visual(frame, state, geometry.visual, browser);
    if let Some(inspector) = geometry.inspector {
        render_browser_inspector(frame, state, inspector, browser);
    }
    render_panel(
        frame,
        geometry.workflow,
        " WORKFLOW ",
        &state.workflow,
        PURPLE,
    );
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

#[derive(Debug, Clone, Copy)]
struct TerminalSurfaceGeometry {
    summary: Rect,
    processes: Rect,
    logs: Rect,
}

fn terminal_surface_geometry(area: Rect) -> TerminalSurfaceGeometry {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Percentage(48),
            Constraint::Min(6),
        ])
        .split(area);
    TerminalSurfaceGeometry {
        summary: rows[0],
        processes: rows[1],
        logs: rows[2],
    }
}

pub(super) fn terminal_process_list_area(area: Rect) -> Rect {
    terminal_surface_geometry(area).processes
}

pub(super) fn process_hit_at(
    state: &DevTuiState,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<usize> {
    let item_count = if state.snapshot_ready {
        state.process_entries.len()
    } else {
        0
    };
    indexed_list_hit(
        terminal_process_list_area(area),
        item_count,
        state.selected_process,
        column,
        row,
    )
}

fn render_terminal_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let geometry = terminal_surface_geometry(area);
    let process_count = state.process_entries.len();
    let healthy_count = state
        .process_entries
        .iter()
        .filter(|entry| matches!(entry.health, crate::development::ProcessHealth::Healthy))
        .count();
    let process_count_label = if state.snapshot_ready {
        process_count.to_string()
    } else {
        "…".into()
    };
    let healthy_count_label = if state.snapshot_ready {
        healthy_count.to_string()
    } else {
        "…".into()
    };
    let selected = if !state.snapshot_ready {
        "waiting for workspace snapshot".into()
    } else {
        state
            .selected_process_entry()
            .map(|entry| format!("selected {}", entry.name))
            .unwrap_or_else(|| "j/k choose a process".into())
    };
    render_panel(
        frame,
        geometry.summary,
        " TERMINAL ",
        format!("{process_count_label} processes · {healthy_count_label} healthy · {selected}"),
        ACCENT_BRIGHT,
    );
    if !state.snapshot_ready {
        render_panel(
            frame,
            geometry.processes,
            " PROCESSES ",
            "Loading workspace snapshot…",
            ACCENT_BRIGHT,
        );
    } else if state.process_entries.is_empty() {
        let empty_state = if state.processes.trim().is_empty()
            || state.processes.contains("No managed terminals")
        {
            "No managed processes yet\ns starts the detected suite · a opens actions\n:process start dev runs a custom command".to_string()
        } else {
            format!("No managed processes yet\n{}", state.processes.trim())
        };
        render_panel(
            frame,
            geometry.processes,
            " PROCESSES ",
            empty_state,
            ACCENT_BRIGHT,
        );
    } else {
        let items = state
            .process_entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let selected = index == state.selected_process;
                let marker = if selected { "›" } else { " " };
                let health = match entry.health {
                    crate::development::ProcessHealth::Healthy => "●",
                    crate::development::ProcessHealth::Failed => "×",
                    _ => "○",
                };
                let pid = entry
                    .pid
                    .map(|pid| pid.to_string())
                    .unwrap_or_else(|| "—".into());
                let detail = entry.url.clone().unwrap_or_else(|| entry.command.clone());
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
                        format!("{marker} {health} "),
                        Style::default().fg(status_color(health)),
                    ),
                    Span::styled(
                        format!(
                            "{} · {} · pid {pid} · {detail}",
                            entry.name,
                            entry.health.label()
                        ),
                        style,
                    ),
                ]))
                .style(style)
            })
            .collect::<Vec<_>>();
        let list_geometry = indexed_list_geometry(
            geometry.processes,
            state.process_entries.len(),
            state.selected_process,
        );
        let mut list_state = ListState::default();
        list_geometry.apply_to_list_state(&mut list_state);
        frame.render_stateful_widget(
            List::new(items)
                .style(Style::default().bg(PANEL_BACKGROUND))
                .block(surface_block(
                    format!(" PROCESSES · {} ", process_count),
                    ACCENT_BRIGHT,
                )),
            geometry.processes,
            &mut list_state,
        );
    }
    let logs = if state.process_logs.trim().is_empty() {
        "Logs follow the selected process · Enter refreshes · Space restart · u App · x stop"
            .to_string()
    } else {
        state.process_logs.clone()
    };
    render_panel(frame, geometry.logs, " LOGS ", logs, PURPLE);
}

fn render_task_list(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let task_count = status_line_count(&state.tasks);
    let task_count_label = if state.snapshot_ready {
        task_count.to_string()
    } else {
        "…".into()
    };
    let guided_empty =
        state.tasks.trim().is_empty() || state.tasks.trim_start().starts_with("No tasks.");
    let items = if !state.snapshot_ready {
        vec![ListItem::new(panel_text("Loading workspace snapshot…"))]
    } else if guided_empty {
        vec![ListItem::new(panel_text(
            "No tasks yet.\na opens actions · :task create TITLE PROMPT",
        ))]
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
    frame.render_widget(
        List::new(items)
            .style(Style::default().bg(PANEL_BACKGROUND))
            .block(surface_block(
                format!(" TASKS · {task_count_label} "),
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
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(rows[0]);
    render_todo_list(frame, state, split[0]);
    render_task_list(frame, state, split[1]);
    let summary = if !state.snapshot_ready {
        "Loading workspace snapshot…\nTask and todo counts appear after refresh.".into()
    } else {
        let (running, queued, failed) = task_counts(state);
        let wake = state
            .last_crew_wake
            .as_deref()
            .and_then(|wake| wake.lines().next())
            .unwrap_or("no crew wake");
        format!("{running} running\n{queued} queued\n{failed} failed\n{wake}")
    };
    render_panel(frame, rows[1], " SUMMARY ", summary, PURPLE);
}

fn task_counts(state: &DevTuiState) -> (usize, usize, usize) {
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
    (running, queued, failed)
}

fn render_todo_list(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let items = if !state.snapshot_ready {
        vec![ListItem::new(panel_text("Loading workspace snapshot…"))]
    } else if state.session_todos.items.is_empty() {
        vec![ListItem::new(panel_text(
            "No session todos\nPlan accept or glass.todo.write",
        ))]
    } else {
        state
            .session_todos
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let selected = index == state.selected_todo;
                let mark = match item.status {
                    crate::TodoStatus::Done => "✓",
                    crate::TodoStatus::Active => "●",
                    crate::TodoStatus::Pending => "○",
                };
                let marker = if selected { "›" } else { " " };
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
                        format!("{marker} {mark} "),
                        Style::default().fg(status_color(mark)),
                    ),
                    Span::styled(format!("{}  {}", item.id, item.title), style),
                ]))
                .style(style)
            })
            .collect()
    };
    let mut list_state = ListState::default();
    if !state.session_todos.items.is_empty() {
        list_state.select(Some(state.selected_todo));
    }
    frame.render_stateful_widget(
        List::new(items)
            .style(Style::default().bg(PANEL_BACKGROUND))
            .block(surface_block(" TODOS ", ACCENT_BRIGHT)),
        area,
        &mut list_state,
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
    let items = if !state.snapshot_ready {
        vec![ListItem::new(Line::from(Span::styled(
            "Loading workspace snapshot…",
            Style::default().fg(MUTED),
        )))]
    } else if state.git_entries.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No changed files · working tree clean",
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
                        compact_path(&path, area.width.saturating_sub(9)),
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
    let item_count = if state.snapshot_ready {
        state.git_entries.len()
    } else {
        0
    };
    let list_geometry = indexed_list_geometry(area, item_count, state.selected_git_file);
    let mut list_state = ListState::default();
    list_geometry.apply_to_list_state(&mut list_state);
    frame.render_stateful_widget(
        List::new(items)
            .style(Style::default().bg(PANEL_BACKGROUND))
            .block(surface_block(
                if !state.snapshot_ready {
                    " CHANGES · … ".to_string()
                } else {
                    let mut title = format!(" CHANGES · {} ", state.git_entries.len());
                    let conflicts = state.git_conflicts.len();
                    let unstaged = state
                        .git_entries
                        .iter()
                        .filter(|entry| entry.worktree_status != ' ' && !entry.untracked)
                        .count();
                    let untracked = state
                        .git_entries
                        .iter()
                        .filter(|entry| entry.untracked)
                        .count();
                    if conflicts > 0 {
                        title.push_str(&format!("· {conflicts}! "));
                    }
                    if unstaged > 0 {
                        title.push_str(&format!("· {unstaged}M "));
                    }
                    if untracked > 0 {
                        title.push_str(&format!("· {untracked}? "));
                    }
                    title
                },
                ACCENT_BRIGHT,
            )),
        area,
        &mut list_state,
    );
}

fn render_git_diff_panel(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    if !state.snapshot_ready {
        render_panel(frame, area, " DIFF ", "Loading workspace snapshot…", PURPLE);
        return;
    }
    if state.git_diff_open {
        if state.git_diff.starts_with("REVIEW") {
            render_panel(frame, area, " REVIEW ", &state.git_diff, ACCENT_BRIGHT);
            return;
        }
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
            "j/k choose a file\nEnter/d diff · Space stage · c commit · o open",
            PURPLE,
        );
        return;
    };
    render_panel(
        frame,
        area,
        " DIFF ",
        format!("{path}\nEnter/d loads the diff · Space stages"),
        PURPLE,
    );
}

#[derive(Debug, Clone, Copy)]
struct GitSurfaceGeometry {
    summary: Rect,
    files: Rect,
    diff: Rect,
}

fn git_surface_geometry(state: &DevTuiState, area: Rect) -> GitSurfaceGeometry {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(5)])
        .split(area);
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
    GitSurfaceGeometry {
        summary: rows[0],
        files: columns[0],
        diff: columns[1],
    }
}

pub(super) fn git_file_list_area(state: &DevTuiState, area: Rect) -> Rect {
    git_surface_geometry(state, area).files
}

pub(super) fn git_file_hit_at(
    state: &DevTuiState,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<usize> {
    let item_count = if state.snapshot_ready {
        state.git_entries.len()
    } else {
        0
    };
    indexed_list_hit(
        git_file_list_area(state, area),
        item_count,
        state.selected_git_file,
        column,
        row,
    )
}

fn render_git_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let geometry = git_surface_geometry(state, area);
    let branch = if state.snapshot_ready {
        state.git_branch.as_str()
    } else {
        "loading…"
    };
    let change_count = state.git_entries.len();
    let change_count_label = if state.snapshot_ready {
        change_count.to_string()
    } else {
        "…".into()
    };
    let review = if state.snapshot_ready {
        state
            .github_review
            .lines()
            .next()
            .unwrap_or("no GitHub review")
    } else {
        "workspace snapshot pending"
    };
    let conflicts = if state.git_conflicts.is_empty() {
        String::new()
    } else {
        format!(" · {} conflict(s)", state.git_conflicts.len())
    };
    render_panel(
        frame,
        geometry.summary,
        format!(
            " GIT · {branch} ↑{} ↓{} ",
            state.git_ahead, state.git_behind
        ),
        format!(
            "{change_count_label} changed{conflicts} · {}\n{review}",
            if state.snapshot_ready {
                state
                    .selected_git_entry()
                    .map(|entry| format!("selected {}", entry.path))
                    .unwrap_or_else(|| "j/k choose · c commit · o open · x discard".into())
            } else {
                "waiting for workspace snapshot".into()
            },
        ),
        PURPLE,
    );
    render_git_file_list(frame, state, geometry.files);
    render_git_diff_panel(frame, state, geometry.diff);
}

fn render_debug_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let geometry = debug_surface_geometry(state, area);
    let session_count = state.debug_sessions.len();
    if session_count == 0 {
        render_panel(
            frame,
            geometry.sessions,
            if state.snapshot_ready {
                " DEBUG · 0 ".to_string()
            } else {
                " DEBUG · … ".to_string()
            },
            if state.snapshot_ready {
                "No debugger sessions\n:debug start NAME COMMAND\na opens actions".to_string()
            } else {
                "Loading workspace snapshot…".to_string()
            },
            ACCENT_BRIGHT,
        );
        render_panel(
            frame,
            geometry.variables,
            " VARIABLES ",
            "Starts after a session is selected",
            ACCENT_BRIGHT,
        );
        render_status_list(
            frame,
            geometry.tests,
            " TESTS ",
            &state.tests,
            "No test runs",
        );
        return;
    }
    render_debug_list(
        frame,
        geometry.sessions,
        format!(" DEBUG · {session_count} "),
        &debug_session_lines(state),
        state.debug_pane == super::state::DebugPane::Sessions,
    );
    render_debug_list(
        frame,
        geometry.threads.expect("debug thread panel geometry"),
        " THREADS ",
        &debug_thread_lines(state),
        state.debug_pane == super::state::DebugPane::Threads,
    );
    render_debug_list(
        frame,
        geometry.frames.expect("debug frame panel geometry"),
        " FRAMES ",
        &debug_frame_lines(state),
        state.debug_pane == super::state::DebugPane::Frames,
    );
    render_panel(
        frame,
        geometry.variables,
        " VARIABLES ",
        debug_variable_lines(state).join("\n"),
        ACCENT_BRIGHT,
    );
    render_status_list(
        frame,
        geometry.tests,
        " TESTS ",
        &state.tests,
        "No test runs",
    );
}

fn debug_variable_lines(state: &DevTuiState) -> Vec<String> {
    if let Some(notice) = &state.debug_variables_notice {
        vec![notice.clone()]
    } else if state.debug_variables.is_empty() {
        if state.debug_scopes.is_empty() {
            vec!["Enter a frame to load scopes".into()]
        } else {
            state
                .debug_scopes
                .iter()
                .map(|scope| format!("◆ {}", scope.name))
                .collect()
        }
    } else {
        state
            .debug_variables
            .iter()
            .take(24)
            .map(|variable| format!("{} = {}", variable.name, variable.value))
            .collect()
    }
}

fn debug_session_lines(state: &DevTuiState) -> Vec<String> {
    if state.debug_sessions.is_empty() {
        return vec![
            "No debugger sessions".into(),
            ":debug start NAME COMMAND".into(),
        ];
    }
    state
        .debug_sessions
        .iter()
        .enumerate()
        .map(|(index, session)| {
            let marker = if index == state.selected_debug_session {
                "›"
            } else {
                " "
            };
            format!(
                "{marker} {} · {} · pid {} · {} bp · {} watch",
                session.name,
                session.state.label(),
                session.pid,
                session.breakpoints,
                session.watches
            )
        })
        .collect()
}

fn debug_thread_lines(state: &DevTuiState) -> Vec<String> {
    if state.debug_threads.is_empty() {
        return vec!["Enter a session to load threads".into()];
    }
    state
        .debug_threads
        .iter()
        .enumerate()
        .map(|(index, thread)| {
            let marker = if index == state.selected_debug_thread {
                "›"
            } else {
                " "
            };
            format!("{marker} {} · id {}", thread.name, thread.id)
        })
        .collect()
}

fn debug_frame_lines(state: &DevTuiState) -> Vec<String> {
    if state.debug_frames.is_empty() {
        return vec!["Enter a thread to load frames".into()];
    }
    state
        .debug_frames
        .iter()
        .enumerate()
        .map(|(index, frame)| {
            let marker = if index == state.selected_debug_frame {
                "›"
            } else {
                " "
            };
            format!(
                "{marker} {}{}",
                frame.name,
                frame
                    .path
                    .as_deref()
                    .map(|path| {
                        format!(
                            " · {path}{}",
                            frame
                                .line
                                .map(|line| format!(":{line}"))
                                .unwrap_or_default()
                        )
                    })
                    .unwrap_or_default()
            )
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct DebugSurfaceGeometry {
    sessions: Rect,
    threads: Option<Rect>,
    frames: Option<Rect>,
    variables: Rect,
    tests: Rect,
}

fn debug_surface_geometry(state: &DevTuiState, area: Rect) -> DebugSurfaceGeometry {
    if state.debug_sessions.is_empty() {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(8), Constraint::Min(6)])
            .split(area);
        let bottom = if stack_for_phone(state, area) {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[1])
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(58), Constraint::Min(18)])
                .split(rows[1])
        };
        return DebugSurfaceGeometry {
            sessions: rows[0],
            threads: None,
            frames: None,
            variables: bottom[0],
            tests: bottom[1],
        };
    }
    if stack_for_phone(state, area) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(22),
                Constraint::Percentage(18),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
            ])
            .split(area);
        return DebugSurfaceGeometry {
            sessions: rows[0],
            threads: Some(rows[1]),
            frames: Some(rows[2]),
            variables: rows[3],
            tests: rows[4],
        };
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(72), Constraint::Min(6)])
        .split(area);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(28),
            Constraint::Percentage(28),
            Constraint::Min(24),
        ])
        .split(rows[0]);
    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Min(18)])
        .split(rows[1]);
    DebugSurfaceGeometry {
        sessions: columns[0],
        threads: Some(columns[1]),
        frames: Some(columns[2]),
        variables: bottom[0],
        tests: bottom[1],
    }
}

pub(super) fn debug_session_area(state: &DevTuiState, area: Rect) -> Rect {
    debug_surface_geometry(state, area).sessions
}

pub(super) fn debug_session_hit_at(
    state: &DevTuiState,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<usize> {
    if state.debug_sessions.is_empty() {
        return None;
    }
    let panel = debug_session_area(state, area);
    let inner = panel_content_area(panel);
    if inner.width == 0 || inner.height == 0 || column < inner.x || column >= inner.right() {
        return None;
    }
    let rendered_lines = panel_text(&debug_session_lines(state).join("\n")).lines;
    if rendered_lines
        .iter()
        .any(|line| line.width() > usize::from(inner.width))
    {
        return None;
    }
    let index = usize::from(row.checked_sub(inner.y)?);
    (row < inner.bottom() && index < state.debug_sessions.len()).then_some(index)
}

fn render_debug_list(
    frame: &mut Frame<'_>,
    area: Rect,
    title: impl Into<String>,
    lines: &[String],
    focused: bool,
) {
    let color = if focused { ACCENT_BRIGHT } else { MUTED };
    render_panel(frame, area, title, lines.join("\n"), color);
}

fn more_phone_layout(state: &DevTuiState, area: Rect) -> bool {
    matches!(
        state.responsive_class(area.width, area.height),
        ResponsiveClass::Phone
    ) && area.width < 80
}

#[derive(Debug, Clone, Copy)]
struct MoreSurfaceGeometry {
    phone: bool,
    narrow: bool,
    summary: Rect,
    pi: Rect,
    experiments: Option<Rect>,
    routes: Rect,
}

fn more_surface_geometry(state: &DevTuiState, area: Rect) -> MoreSurfaceGeometry {
    let phone = more_phone_layout(state, area);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(if phone { 3 } else { 5 }),
            Constraint::Min(5),
        ])
        .split(area);
    if phone {
        let columns = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(7)])
            .split(rows[1]);
        MoreSurfaceGeometry {
            phone,
            narrow: true,
            summary: rows[0],
            pi: columns[0],
            experiments: None,
            routes: columns[1],
        }
    } else {
        let narrow = area.width < 60;
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
        MoreSurfaceGeometry {
            phone,
            narrow,
            summary: rows[0],
            pi: columns[0],
            experiments: Some(columns[1]),
            routes: columns[2],
        }
    }
}

fn more_route_scroll(selected: usize, viewport_height: usize) -> usize {
    let viewport_height = viewport_height.max(1);
    selected.saturating_sub(viewport_height.saturating_sub(1))
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MoreRouteGeometry {
    pub panel: Rect,
    pub content: Rect,
    pub scroll_offset: usize,
}

fn more_route_geometry(state: &DevTuiState, panel: Rect) -> MoreRouteGeometry {
    let content = panel_content_area(panel);
    let scroll_offset = more_route_scroll(state.selected_more, usize::from(content.height));
    MoreRouteGeometry {
        panel,
        content,
        scroll_offset,
    }
}

pub(super) fn more_route_hit_geometry(state: &DevTuiState, screen: Rect) -> MoreRouteGeometry {
    let surface = surface_area(state, screen);
    more_route_geometry(state, more_surface_geometry(state, surface).routes)
}

pub(crate) fn more_route_at(state: &DevTuiState, column: u16, row: u16) -> Option<usize> {
    if state.surface != DevSurface::More {
        return None;
    }
    let area = Rect::new(
        0,
        0,
        state.terminal_width.max(1),
        state.terminal_height.max(1),
    );
    let route = more_route_hit_geometry(state, area);
    if column < route.content.x
        || column >= route.content.right()
        || row < route.content.y
        || row >= route.content.bottom()
    {
        return None;
    }
    let index = usize::from(row - route.content.y).saturating_add(route.scroll_offset);
    (index < DevTuiState::MORE_ROUTES.len()).then_some(index)
}

fn render_more_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let geometry = more_surface_geometry(state, area);
    let route_geometry = more_route_geometry(state, geometry.routes);
    let phone = geometry.phone;
    let narrow = geometry.narrow;
    let kernel_count = status_line_count(&state.kernels);
    let latest_result = state
        .more_result
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim);
    let summary = if !state.snapshot_ready {
        "Loading workspace snapshot…".to_string()
    } else if phone {
        latest_result.map_or_else(
            || {
                format!(
                    "{} skills · {} tools · {kernel_count} kernels",
                    state.snapshot_skills_count, state.snapshot_tools_count
                )
            },
            |result| format!("LAST {result}"),
        )
    } else {
        format!(
            "{} skills · {} tools · {kernel_count} kernels\n{}\nCOCKPIT {}",
            state.snapshot_skills_count,
            state.snapshot_tools_count,
            latest_result
                .map(|result| format!("LAST {result}"))
                .unwrap_or_else(|| activity_summary(state)),
            state.private_cockpit_status(),
        )
    };
    let summary = compact_multiline(&summary, geometry.summary.width.saturating_sub(4));
    render_panel(
        frame,
        geometry.summary,
        " SERVICES ",
        summary,
        ACCENT_BRIGHT,
    );
    if phone {
        let pi_content = compact_multiline(
            &if state.snapshot_ready {
                format!(
                    "{}\n{}\n{}\n{}",
                    state
                        .agent_readiness
                        .lines()
                        .next()
                        .unwrap_or("Pi unavailable"),
                    agent_progress(state),
                    state.experiments.lines().next().unwrap_or("No experiments"),
                    state.replay.lines().next().unwrap_or("No replay"),
                )
            } else {
                "Loading workspace snapshot…".to_string()
            },
            geometry.pi.width.saturating_sub(4),
        );
        render_panel(frame, geometry.pi, " PI · WORKSPACE ", pi_content, PURPLE);
        let route_content = compact_multiline(
            &more_route_lines(state).join("\n"),
            route_geometry.content.width,
        );
        let route_title = format!(
            " ROUTES · {}/{} ",
            state.selected_more + 1,
            DevTuiState::MORE_ROUTES.len()
        );
        render_scrolled_panel(
            frame,
            route_geometry.panel,
            route_title,
            route_content,
            WARNING,
            route_geometry.scroll_offset,
        );
        return;
    }
    let pi_content = compact_multiline(
        &if state.snapshot_ready {
            format!(
                "{}\n{}\n{kernel_count} kernels",
                state
                    .agent_readiness
                    .lines()
                    .next()
                    .unwrap_or("Pi unavailable"),
                agent_progress(state),
            )
        } else {
            "Loading workspace snapshot…".to_string()
        },
        geometry.pi.width.saturating_sub(4),
    );
    render_panel(frame, geometry.pi, " PI ", pi_content, PURPLE);
    let experiments_content = compact_multiline(
        &more_experiment_content(state),
        geometry
            .experiments
            .expect("non-phone More layout has an experiments panel")
            .width
            .saturating_sub(4),
    );
    let experiments_area = geometry
        .experiments
        .expect("non-phone More layout has an experiments panel");
    render_panel(
        frame,
        experiments_area,
        " EXPERIMENTS ",
        experiments_content,
        ACCENT_BRIGHT,
    );
    let installed_harnesses = state
        .harnesses
        .lines()
        .filter(|line| line.starts_with("●"))
        .take(4)
        .collect::<Vec<_>>()
        .join("\n");
    let mut routes = more_route_lines(state);
    if !narrow {
        routes.push(String::new());
        routes.push("Harness catalog".into());
        routes.push(if installed_harnesses.is_empty() {
            "none detected".into()
        } else {
            installed_harnesses
        });
    }
    let routes_content = compact_multiline(&routes.join("\n"), route_geometry.content.width);
    render_scrolled_panel(
        frame,
        route_geometry.panel,
        " ROUTES ",
        routes_content,
        WARNING,
        route_geometry.scroll_offset,
    );
}

fn more_experiment_content(state: &DevTuiState) -> String {
    let mut lines = state
        .experiments
        .lines()
        .take(6)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push("No experiments".into());
    }
    if let Some(recommended) = state
        .experiment_comparison
        .as_ref()
        .and_then(|comparison| comparison.recommended.as_deref())
    {
        lines.push(format!("recommended {recommended}"));
    }
    lines.push(format!(
        "replay {}",
        state.replay.lines().next().unwrap_or("idle")
    ));
    lines.join("\n")
}

fn more_route_lines(state: &DevTuiState) -> Vec<String> {
    crate::tui::state::DevTuiState::MORE_ROUTES
        .iter()
        .enumerate()
        .map(|(index, route)| {
            let marker = if index == state.selected_more {
                "›"
            } else {
                " "
            };
            format!("{marker} {route}")
        })
        .collect()
}

fn render_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let active = overlay::active_overlay(state);
    if active == Some(ActiveOverlay::CommandCenterMenu) {
        let search_index = state.surface_actions().len();
        let quit_index = state.quit_menu_index();
        let items = state
            .surface_actions()
            .iter()
            .enumerate()
            .map(|(index, action)| {
                let selected = index == state.menu_selection;
                let unavailable = !state.surface_action_available(*action);
                let item_style = if unavailable {
                    Style::default().fg(MUTED).bg(if selected {
                        ACTIVE_BACKGROUND
                    } else {
                        PANEL_BACKGROUND
                    })
                } else if selected {
                    Style::default()
                        .fg(ACCENT_BRIGHT)
                        .bg(ACTIVE_BACKGROUND)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TEXT)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        if unavailable {
                            "× "
                        } else if selected {
                            "› "
                        } else {
                            "  "
                        },
                        Style::default()
                            .fg(if unavailable { MUTED } else { ACCENT })
                            .add_modifier(Modifier::BOLD),
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
            "Search every Glass route by root command.".to_string()
        } else if state.menu_selection == quit_index {
            "Close the TUI after an explicit quit confirmation.".to_string()
        } else {
            state
                .surface_actions()
                .get(state.menu_selection)
                .map(|action| {
                    state
                        .surface_action_unavailable_reason(*action)
                        .map(|reason| format!("Unavailable · {reason}"))
                        .unwrap_or_else(|| action.description.to_string())
                })
                .unwrap_or_else(|| "Choose an action.".to_string())
        };
        let selected_available = state
            .surface_actions()
            .get(state.menu_selection)
            .is_none_or(|action| state.surface_action_available(*action));
        let run_hint = if selected_available {
            "Enter run · Esc close"
        } else {
            "Unavailable · Esc close"
        };
        let example = command::palette_example(state.surface);
        let geometry = command_menu_geometry(state, area);
        let mut menu_state = ListState::default()
            .with_offset(geometry.scroll_offset)
            .with_selected(Some(state.menu_selection));
        frame.render_stateful_widget(
            List::new(items)
                .style(Style::default().bg(PANEL_BACKGROUND))
                .block(command_menu_list_block(state.surface)),
            geometry.list_area,
            &mut menu_state,
        );
        if geometry.details_area.height > 0 {
            frame.render_widget(
                Paragraph::new(panel_text(&format!(
                    "{}\n{}\n{}",
                    selected_description, example, run_hint,
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
                geometry.details_area,
            );
        }
        return;
    }
    if active == Some(ActiveOverlay::BrowserTargetPicker) {
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
    if let Some(offer) = state
        .browser_recovery
        .as_ref()
        .filter(|_| active == Some(ActiveOverlay::BrowserRecovery))
    {
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
    if let Some(pending) = state
        .pending_agent_approval
        .as_ref()
        .filter(|_| active == Some(ActiveOverlay::AgentApproval))
    {
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
    if let Some(pending) = state
        .pending_confirmation
        .as_ref()
        .filter(|_| active == Some(ActiveOverlay::MutationConfirmation))
    {
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
    let inner = visual_pixel_area(area);
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
fn last_conversation_preview(state: &DevTuiState) -> String {
    state
        .conversation_entries_view()
        .into_iter()
        .rev()
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|entry| {
            let label = match entry.kind {
                super::projection::ConversationKind::User => "YOU",
                super::projection::ConversationKind::Assistant => "AGENT",
                super::projection::ConversationKind::Alert => "ALERT",
                super::projection::ConversationKind::Error => "ERR",
                super::projection::ConversationKind::System => "SYS",
            };
            format!("{label} {}", entry.text.lines().next().unwrap_or_default())
        })
        .collect::<Vec<_>>()
        .join("\n")
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
            let chat = if state.composer_mode {
                last_conversation_preview(state)
            } else {
                String::new()
            };
            let content = if state.focused_editor_path.is_empty() {
                format!("{} files\nCtrl-L ask", state.files.len())
            } else {
                format!(
                    "{}{}\nline {}\nCtrl-L ask{}",
                    if state.focused_editor_dirty {
                        "● "
                    } else {
                        ""
                    },
                    state.focused_editor_path,
                    state.focused_editor_line,
                    if chat.is_empty() {
                        String::new()
                    } else {
                        format!("\n{chat}")
                    }
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
                    "{}\n{}\n{}\nCtrl-L ask · C comment",
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
            let (running, queued, failed) = task_counts(state);
            (
                "TASKS",
                format!("{running} running\n{queued} queued\n{failed} failed"),
            )
        }
        DevSurface::Git => (
            "GIT",
            state
                .selected_git_entry()
                .map(|entry| entry.path.clone())
                .unwrap_or_else(|| "no changed files".into()),
        ),
        DevSurface::More => (
            "SERVICES",
            format!(
                "{} skills\n{} tools\n{} kernels",
                state.snapshot_skills_count,
                state.snapshot_tools_count,
                status_line_count(&state.kernels)
            ),
        ),
        DevSurface::Debug => (
            "DEBUG",
            state
                .selected_debug_session()
                .map(|session| format!("{} · {}", session.name, session.state.label()))
                .unwrap_or_else(|| "no session · a starts".into()),
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
    } else if matches!(
        overlay::active_overlay(state),
        Some(
            ActiveOverlay::Composer | ActiveOverlay::PiSlashCommand | ActiveOverlay::CommandPalette
        )
    ) {
        Style::default().fg(ACCENT_BRIGHT)
    } else {
        Style::default().fg(SUCCESS)
    }
}

fn composer_input_lines(state: &DevTuiState, width: u16) -> Vec<Line<'static>> {
    let prefix_style = Style::default()
        .fg(ACCENT_BRIGHT)
        .add_modifier(Modifier::BOLD);
    let input = &state.composer_input;
    let cursor = state.composer_cursor.min(input.len());
    let mut start = 0;
    let mut lines = Vec::new();
    let mut cursor_line = 0;
    let mut cursor_column = cursor;
    for (index, part) in input.split('\n').enumerate() {
        let end = start + part.len();
        if cursor >= start && cursor <= end {
            cursor_line = index;
            cursor_column = cursor - start;
        }
        start = end + 1;
        lines.push(part);
    }
    if lines.is_empty() {
        lines.push("");
    }
    let visible = composer_visible_lines(state) as usize;
    let window_start = cursor_line.saturating_sub(visible.saturating_sub(1));
    lines
        .into_iter()
        .enumerate()
        .skip(window_start)
        .take(visible)
        .map(|(index, part)| {
            let prefix = if index == window_start {
                match state.composer_run_mode {
                    crate::AgentTurnMode::Ask => "? ",
                    crate::AgentTurnMode::Plan => "P ",
                    crate::AgentTurnMode::Agent => "> ",
                }
            } else {
                "  "
            };
            if index == cursor_line {
                Line::from(input_spans(
                    prefix,
                    prefix_style,
                    part,
                    cursor_column,
                    width,
                ))
            } else {
                Line::from(vec![
                    Span::styled(prefix.to_string(), prefix_style),
                    Span::styled(part.to_string(), Style::default().fg(TEXT)),
                ])
            }
        })
        .collect()
}

fn render_status(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let (glyph, glyph_color) = status_glyph(state);
    let active = overlay::active_overlay(state);
    let lines = if active == Some(ActiveOverlay::Composer) {
        let mut lines = composer_input_lines(state, area.width.saturating_sub(5));
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {glyph} "),
                Style::default()
                    .fg(glyph_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(state.status.clone(), status_style(state)),
        ]));
        lines
    } else if active == Some(ActiveOverlay::FilePicker) {
        vec![
            Line::from(input_spans(
                " file: ",
                Style::default()
                    .fg(ACCENT_BRIGHT)
                    .add_modifier(Modifier::BOLD),
                &state.file_picker_query,
                state.file_picker_cursor,
                area.width.saturating_sub(8),
            )),
            status_line(state, area.width.saturating_sub(2)),
        ]
    } else if active == Some(ActiveOverlay::PiSlashCommand) {
        let mut spans = input_spans(
            "/ ",
            Style::default().fg(PURPLE).add_modifier(Modifier::BOLD),
            &state.pi_command_input,
            state.pi_command_cursor,
            area.width.saturating_sub(8),
        );
        let hint = if state.pi_command_input.trim().is_empty() {
            "  ↑↓ select · Tab complete · Enter run · Esc cancel"
        } else {
            "  Enter runs native command · Esc cancel"
        };
        spans.push(Span::styled(hint, Style::default().fg(MUTED)));
        vec![Line::from(spans)]
    } else if active == Some(ActiveOverlay::CommandPalette) {
        let (prefix, input, cursor, hint) = match navigation_value(state) {
            Some(address) => (
                "URL: ",
                address,
                navigation_cursor(state, address),
                "  Enter navigate · Esc cancel",
            ),
            None => (
                ": ",
                state.command_input.as_str(),
                state.command_cursor,
                if state.command_input.trim().is_empty() {
                    "  ↑↓ select · Enter run · type only to filter"
                } else {
                    "  Enter runs typed command · Esc cancel"
                },
            ),
        };
        let mut spans = input_spans(
            prefix,
            Style::default().fg(ACCENT_BRIGHT),
            input,
            cursor,
            area.width.saturating_sub(8),
        );
        spans.push(Span::styled(hint, Style::default().fg(MUTED)));
        vec![Line::from(spans)]
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
        vec![
            Line::from(Span::styled(
                compact_line(
                    &super::bindings::dock_placeholder(state.surface, state.composer_run_mode),
                    area.width.saturating_sub(2),
                ),
                Style::default().fg(MUTED),
            )),
            Line::from(line),
        ]
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
    use crate::tui::state::BrowserDialogPrompt;
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
        let mut state = DevTuiState::open(root, layout).unwrap();
        // Render tests must not depend on whether the host has Pi installed.
        state.agent_readiness = "✓ Ready · test runtime".into();
        state.status = "Ready · describe a coding task".into();
        state
    }

    fn overlay_state() -> (DevTuiState, std::path::PathBuf) {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "glass-dev-tui-overlay-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let state = DevTuiState::open(&root, TuiLayout::Desktop).unwrap();
        (state, root)
    }

    fn trust_workspace_once(state: &mut DevTuiState) {
        state
            .ws_mut()
            .expect("workspace lock")
            .apply_local_trust_decision(crate::LocalTrustDecision::TrustOnce)
            .expect("trust temporary workspace");
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
    fn rendered_buffer(state: &DevTuiState, width: u16, height: u16) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, state)).unwrap();
        terminal.backend().buffer().clone()
    }

    #[test]
    fn file_picker_is_rendered_above_editor_and_pi_and_suppresses_lower_overlays() {
        let (mut state, root) = overlay_state();
        state.code_edit_mode = true;
        state.pi_command_mode = true;
        state.command_mode = true;
        state.file_picker_open = true;
        state.session_picker_open = true;

        let output = rendered(&state, 100, 30);
        assert!(output.contains("OPEN FILE"));
        assert!(!output.contains("PI COMMANDS"));
        assert!(!output.contains("PI SESSIONS"));
        assert!(!output.contains("COMMAND PALETTE"));
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn session_picker_is_rendered_above_editor_and_pi() {
        let (mut state, root) = overlay_state();
        state.code_edit_mode = true;
        state.pi_command_mode = true;
        state.session_picker_open = true;

        let output = rendered(&state, 100, 30);
        assert!(output.contains("PI SESSIONS"));
        assert!(!output.contains("PI COMMANDS"));
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn help_rendering_suppresses_a_lower_priority_command_center_menu() {
        let (mut state, root) = overlay_state();
        state.help_open = true;
        state.menu_open = true;

        let output = rendered(&state, 100, 30);
        assert!(output.contains("HELP ·"));
        assert!(!output.contains("ACTIONS ·"));
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn install_native_dialog(state: &mut DevTuiState, dialog_type: &str) {
        let prompt = dialog_type == "prompt";
        let prompt_input = if prompt { "Ada" } else { "" };
        state.browser_dialog = Some(BrowserDialogPrompt {
            pending: glass_browser::browser::NativePendingDialog {
                id: "native-dialog-render-test".into(),
                context_id: "native-context-render-test".into(),
                frame_id: "native-frame-render-test".into(),
                dialog: glass_browser::browser::session::PendingDialog {
                    dialog_type: dialog_type.into(),
                    message: "Confirm the checkout details before continuing".into(),
                    default_value: prompt.then(|| prompt_input.to_string()),
                    url: "https://example.test/checkout?token=secret#receipt".into(),
                },
            },
            prompt_input: prompt_input.into(),
            prompt_cursor: prompt_input.chars().count(),
            error: None,
        });
    }

    #[test]
    fn native_browser_dialog_overlay_covers_live_view_at_desktop_and_phone_sizes() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::App;
        state.browser_visual_live = true;
        install_native_dialog(&mut state, "confirm");

        let desktop = rendered(&state, 120, 32);
        assert!(desktop.contains("JAVASCRIPT CONFIRM"));
        assert!(desktop.contains("Confirm the checkout details"));
        assert!(desktop.contains("https://example.test/checkout"));
        assert!(!desktop.contains("secret"));
        assert!(browser_visual_area(&state, Rect::new(0, 0, 120, 32)).is_none());

        let phone = rendered(&state, 48, 18);
        assert!(phone.contains("JAVASCRIPT CONFIRM"));
        assert!(phone.contains("Confirm"));
        assert!(phone.contains("https://example.test/checkout"));
    }

    #[test]
    fn composer_keeps_live_browser_pixels_while_pickers_occlude_them() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::App;
        state.browser_visual_live = true;
        let area = Rect::new(0, 0, 120, 32);
        assert!(browser_visual_area(&state, area).is_some());

        state.composer_mode = true;
        assert_eq!(
            overlay::active_overlay(&state),
            Some(ActiveOverlay::Composer)
        );
        assert!(browser_visual_area(&state, area).is_some());

        state.file_picker_open = true;
        assert_eq!(
            overlay::active_overlay(&state),
            Some(ActiveOverlay::FilePicker)
        );
        assert!(browser_visual_area(&state, area).is_none());

        state.file_picker_open = false;
        state.session_picker_open = true;
        assert_eq!(
            overlay::active_overlay(&state),
            Some(ActiveOverlay::SessionPicker)
        );
        assert!(browser_visual_area(&state, area).is_none());
    }

    #[test]
    fn browser_visual_area_uses_the_rendered_workflow_aware_app_geometry() {
        let (mut state, root) = overlay_state();
        let area = Rect::new(0, 0, 120, 32);
        state.surface = DevSurface::App;
        state.browser_visual_live = true;

        let rendered = app_surface_geometry(&state, surface_area(&state, area));
        assert_eq!(
            browser_visual_area(&state, area),
            Some(visual_pixel_area(rendered.visual))
        );
        assert!(
            browser_visual_area(&state, area)
                .is_some_and(|visual| visual.bottom() <= rendered.workflow.y)
        );

        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn paused_visual_reason_overrides_selected_backend_active_placeholder() {
        let (mut state, root) = overlay_state();
        state.surface = DevSurface::App;
        state.browser_visual_live = true;
        let browser = state.browser_workspace.state_mut();
        browser.presentation = glass_browser::browser_workspace::BrowserPresentationPath::Kitty;
        browser.presentation_reason =
            Some("Live view paused while the browser pane is hidden".into());
        browser.frame_revision = None;

        let output = rendered(&state, 120, 32);
        assert!(output.contains("LIVE VIEW · PAUSED"));
        assert!(output.contains("Live view paused while the"));
        assert!(output.contains("browser pane is hidden"));
        assert!(!output.contains("Live browser view is active"));

        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_browser_prompt_renders_default_text_cursor_and_resolution_error() {
        let mut state = state(TuiLayout::Mobile);
        install_native_dialog(&mut state, "prompt");
        let dialog = state.browser_dialog.as_mut().unwrap();
        dialog.prompt_cursor = 1;
        dialog.error = Some("stale dialog identity".into());

        let output = rendered(&state, 48, 18);
        assert!(output.contains("JAVASCRIPT PROMPT"));
        assert!(output.contains("A▏da"));
        assert!(output.contains("stale dialog identity"));
        assert!(output.contains("UTF-8 bytes"));
    }

    #[test]
    fn native_beforeunload_overlay_uses_browser_copy_not_page_text() {
        let mut state = state(TuiLayout::Desktop);
        install_native_dialog(&mut state, "beforeunload");

        let output = rendered(&state, 120, 32);
        assert!(output.contains("LEAVE PAGE?"));
        assert!(output.contains(NATIVE_BEFOREUNLOAD_MESSAGE));
        assert!(output.contains("Enter/Y leave"));
        assert!(output.contains("Esc/N stay"));
        assert!(!output.contains("Confirm the checkout details"));
    }

    #[test]
    fn command_palette_keeps_bottom_selection_visible_and_highlighted() {
        let mut palette_state = state(TuiLayout::Mobile);
        palette_state.open_palette();
        let last = palette_state.surface_actions().len().saturating_sub(1);
        while palette_state.palette_selection < last {
            palette_state.move_palette_selection(1);
        }

        let buffer = rendered_buffer(&palette_state, 48, 18);
        let selected_row = (0..18)
            .find(|row| {
                let row_text = (0..48)
                    .map(|column| {
                        buffer
                            .cell((column, *row))
                            .expect("cell in buffer")
                            .symbol()
                    })
                    .collect::<String>();
                row_text.contains("Delegate to external harness")
                    && (0..48).any(|column| {
                        buffer
                            .cell((column, *row))
                            .expect("cell in buffer")
                            .symbol()
                            == "▸"
                    })
            })
            .expect("selected bottom palette row must remain visible");
        let row_text = (0..48)
            .map(|column| {
                buffer
                    .cell((column, selected_row))
                    .expect("cell in buffer")
                    .symbol()
            })
            .collect::<String>();
        assert!(row_text.contains("Delegate to external harness"));
        assert!(
            (0..48).any(|column| {
                buffer
                    .cell((column, selected_row))
                    .expect("cell in buffer")
                    .bg
                    == ACTIVE_BACKGROUND
            }),
            "selected palette row must use the active background"
        );
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
    fn setup_landing_explains_in_tui_recovery() {
        let mut state = state(TuiLayout::Mobile);
        state.surface = DevSurface::Agent;
        state.agent_readiness = "○ Needs setup".into();
        let output = rendered(&state, 48, 18);

        assert!(output.contains("SETUP"));
        assert!(output.contains("Press :actions"));
        assert!(output.contains("Enter to install"));
    }

    #[test]
    fn header_shows_dirty_git_branch_and_help_is_surface_local() {
        let mut state = state(TuiLayout::Desktop);
        state.git_branch = "main".into();
        state.git_dirty = true;
        let output = rendered(&state, 120, 32);
        assert!(output.contains("main*"));
        assert!(output.contains("GLASS DEV"));

        state.help_open = true;
        state.surface = DevSurface::Code;
        let help = rendered(&state, 100, 28);
        assert!(help.contains("DO THIS"));
        assert!(help.contains("CODE"));
        assert!(help.contains("Enter opens"));
        assert!(help.contains("Ctrl-L"));
        assert!(help.contains("KEYS"));
        assert!(help.contains("Tab surfaces"));
        assert!(help.contains("/todo"));
        assert!(help.contains("Agent · Code"));
        assert!(help.contains("Ctrl-O symbols"));
        assert!(!help.contains("Ctrl-O back"));
        assert!(!help.contains("▎   AGENT"));
        assert!(!help.contains("Alt-←/→"));
    }

    #[test]
    fn file_picker_overlay_lists_filtered_paths() {
        let mut state = state(TuiLayout::Desktop);
        state.files = vec!["src/lib.rs".into(), "src/main.rs".into()];
        state.open_file_picker();
        state.insert_file_picker_char('l');
        state.insert_file_picker_char('i');
        state.insert_file_picker_char('b');
        let output = rendered(&state, 100, 28);
        assert!(output.contains("OPEN FILE"));
        assert!(output.contains("src/lib.rs"));
        assert!(!output.contains("src/main.rs"));
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

        state.editor_exit_prompt = Some(EditorExitPrompt::Unsaved);
        state.quit_confirmation = true;
        let unsaved_output = rendered(&state, 80, 24);
        assert!(unsaved_output.contains("UNSAVED EDITOR CHANGES"));
        assert!(unsaved_output.contains("Enter discard and quit"));
        assert!(unsaved_output.contains("Esc keep editing"));
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
    fn async_startup_shows_loading_instead_of_empty_workspace_claims() {
        let mut state = state(TuiLayout::Desktop);
        state.snapshot_ready = false;
        state.surface = DevSurface::Agent;
        state.git.clear();
        state.git_entries.clear();
        state.tests.clear();

        let output = rendered(&state, 140, 40);
        assert!(output.contains("Loading workspace snapshot"));
        assert!(!output.contains("branch unavailable"));
        assert!(!output.contains("0 changed"));
    }

    #[test]
    fn action_menu_marks_unavailable_rows_and_rejects_execution() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Terminal;
        state.process_entries.clear();
        state.snapshot_ready = true;
        state.open_menu();
        state.menu_selection = 2;

        let output = rendered(&state, 118, 32);
        assert!(output.contains("× View logs"));
        assert!(output.contains("Unavailable"));

        state.run_menu_action();
        assert!(state.menu_open);
        assert!(state.status.contains("unavailable"));
    }

    #[test]
    fn phone_more_route_selection_reports_position_and_keeps_marker_visible() {
        let mut state = state(TuiLayout::Mobile);
        state.surface = DevSurface::More;
        state.selected_more = DevTuiState::MORE_ROUTES.len() - 1;

        let output = rendered(&state, 48, 18);
        assert!(output.contains("ROUTES · 5/5"));
        assert!(output.contains("› harness list"));
    }

    #[test]
    fn compact_more_surface_keeps_service_routes_reachable() {
        let mut state = state(TuiLayout::Mobile);
        state.surface = DevSurface::More;
        let output = rendered(&state, 48, 18);
        assert!(output.contains("0 kernels"));
        assert!(output.contains("PI"));
        assert!(output.contains("ROUTES"));
    }

    #[test]
    fn more_phone_surface_shows_every_route_and_the_latest_result() {
        let mut state = state(TuiLayout::Mobile);
        state.surface = DevSurface::More;
        let output = rendered(&state, 48, 18);
        for route in DevTuiState::MORE_ROUTES {
            assert!(output.contains(route), "missing phone route {route}");
        }

        state.more_result = "External harnesses · ● installed · ○ unavailable".into();
        let output = rendered(&state, 48, 18);
        assert!(output.contains("LAST External harnesses"));
    }

    #[test]
    fn panel_headings_ignore_title_case_catalog_labels() {
        assert!(is_panel_heading("START HERE"));
        assert!(is_panel_heading("HARNESS CATALOG"));
        assert!(!is_panel_heading("Harness catalog"));
        assert!(!is_panel_heading("  ROUTES"));
    }

    #[test]
    fn tasks_and_more_context_keep_counts_on_their_own_lines() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Tasks;
        let tasks = rendered(&state, 120, 32);
        assert!(tasks.contains("0 running"));
        assert!(tasks.contains("0 queued"));
        assert!(tasks.contains("0 failed"));
        assert!(!tasks.contains("0 running · 0 queued · 0"));

        state.surface = DevSurface::More;
        state.harnesses = "● claude    Claude Code\n● pi        Pi".into();
        let more = rendered(&state, 220, 40);
        assert!(more.contains("Harness catalog"));
        assert!(!more.contains("HARNESS CATALOG"));
        assert!(more.contains("0 skills"));
        assert!(more.contains("0 tools"));
    }

    #[test]
    fn empty_debug_workbench_uses_a_single_start_panel() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Debug;
        let output = rendered(&state, 140, 40);
        assert!(output.contains("DEBUG · 0"));
        assert!(output.contains("No debugger sessions"));
        assert!(output.contains(":debug start NAME COMMAND"));
        assert!(!output.contains("THREADS"));
        assert!(output.contains("VARIABLES"));
        assert!(output.contains("TESTS"));
    }

    #[test]
    fn guided_task_empty_state_keeps_creation_command_visible() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Tasks;
        let output = rendered(&state, 140, 40);
        assert!(output.contains("No tasks yet."));
        assert!(output.contains(":task create TITLE PROMPT"));
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
        let desktop_output = rendered(&compact, 140, 40);
        assert!(!desktop_output.contains("←→ move · Enter open"));
        assert!(!desktop_output.contains("n navigate · t type"));
        let compact_output = rendered(&compact, 64, 24);
        assert!(compact_output.contains("SURFACES"));
        assert!(!compact_output.contains("←→ move · Enter open"));
    }

    #[test]
    fn trust_prompt_is_highlighted_in_desktop_and_compact_navigation() {
        let mut desktop = state(TuiLayout::Desktop);
        desktop.surface = DevSurface::Trust;
        assert!(rendered(&desktop, 140, 40).contains("›  Trust"));

        let mut compact = state(TuiLayout::Compact);
        compact.surface = DevSurface::Trust;
        assert!(rendered(&compact, 64, 24).contains("›  Trust"));
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
    fn terminal_and_debug_surfaces_select_rows() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Terminal;
        state.process_entries = vec![
            crate::tui::state::ProcessRow {
                name: "dev".into(),
                command: "npm run dev".into(),
                pid: Some(42),
                health: crate::development::ProcessHealth::Healthy,
                url: Some("http://127.0.0.1:5173/".into()),
            },
            crate::tui::state::ProcessRow {
                name: "test".into(),
                command: "cargo test".into(),
                pid: Some(43),
                health: crate::development::ProcessHealth::Starting,
                url: None,
            },
        ];
        let terminal = rendered(&state, 140, 40);
        assert!(terminal.contains("PROCESSES · 2"));
        assert!(terminal.contains("dev"));
        state.move_process_selection(1);
        assert_eq!(
            state
                .selected_process_entry()
                .map(|entry| entry.name.as_str()),
            Some("test")
        );

        state.surface = DevSurface::Debug;
        state.debug_sessions = vec![crate::tui::state::DebugSessionRow {
            name: "lldb".into(),
            state: crate::debugger::DebugSessionState::Stopped,
            pid: 9,
            breakpoints: 1,
            watches: 0,
        }];
        state.debug_threads = vec![crate::tui::state::DebugThreadRow {
            id: 1,
            name: "main".into(),
        }];
        state.debug_frames = vec![crate::tui::state::DebugFrameRow {
            id: 12,
            name: "main".into(),
            path: Some("src/main.rs".into()),
            line: Some(40),
        }];
        let debug = rendered(&state, 140, 40);
        assert!(debug.contains("DEBUG · 1"));
        assert!(debug.contains("THREADS"));
        assert!(debug.contains("FRAMES"));
        assert!(debug.contains("src/main.rs:40"));
        assert!(debug.contains("VARIABLES"));
        state.debug_pane = crate::tui::state::DebugPane::Frames;
        state.move_debug_selection(0);
        assert_eq!(
            state.selected_debug_frame().and_then(|frame| frame.line),
            Some(40)
        );
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
        assert!(output.contains("Space stages"));
        assert!(!output.contains("0!"));

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
        let help = rendered(&state, 48, 18);
        assert!(help.contains("DO THIS"));
        assert!(help.contains("App"));
        state.scroll_help(16);
        assert!(state.help_scroll <= 16);

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
        assert!(rendered(&state, 120, 32).contains("SELECT AN ACTION"));
        assert!(rendered(&state, 120, 32).contains("type only to filter"));
        assert_eq!(state.palette_selection, 0);
        state.move_palette_selection(2);
        assert_eq!(
            state.selected_palette_action().map(|action| action.label),
            Some("Update Pi runtime")
        );
        state.insert_palette_text("browser");
        state.move_palette_cursor(false);
        state.palette_backspace();
        state.insert_palette_char('e');
        assert!(state.command_cursor < state.command_input.len());
        assert!(!state.palette_matches().contains(&"browser"));

        state.close_palette();
        state.command_history = vec![
            "editor proposals".into(),
            "agent status".into(),
            "browser observe".into(),
            "review".into(),
        ];
        state.open_palette();
        state.navigate_palette_history(true);
        assert_eq!(state.command_input, "review");
        state.navigate_palette_history(true);
        assert_eq!(state.command_input, "agent status");
        state.navigate_palette_history(false);
        assert_eq!(state.command_input, "review");
        state.command_input = "agnt".into();
        state.command_cursor = state.command_input.len();
        state.complete_palette();
        assert_eq!(state.command_input, "agent");

        state.close_palette();
        let surface = state.surface;
        state.scroll_surface(3);
        assert_eq!(state.surface, surface);
        assert_eq!(state.current_scroll(), 3);
    }

    #[test]
    fn command_palette_lists_arrow_selectable_surface_actions() {
        let mut palette_state = state(TuiLayout::Desktop);
        palette_state.open_palette();
        let first_page = rendered(&palette_state, 120, 32);
        for marker in [
            "COMMAND PALETTE",
            "SELECT AN ACTION",
            "↑↓ select",
            "Compose message",
            "agent setup",
        ] {
            assert!(
                first_page.contains(marker),
                "missing palette marker {marker}"
            );
        }

        let mut git_state = state(TuiLayout::Desktop);
        git_state.surface = DevSurface::Git;
        let git_content = command_palette_content(&git_state);
        assert!(git_content.contains("Stage all changes"));
        assert!(git_content.contains("git stage"));
        assert!(!git_content.contains("Open selected file"));
        assert!(!git_content.contains("COMMAND ROOTS"));

        palette_state.move_palette_selection(5);
        assert_eq!(palette_state.palette_selection, 5);
        assert!(rendered(&palette_state, 120, 32).contains("▸"));
        palette_state.scroll_palette(-1);
        assert_eq!(palette_state.palette_selection, 0);
    }

    #[test]
    fn surface_action_menu_is_discoverable_and_runs_or_prefills() {
        let mut state = state(TuiLayout::Desktop);
        state.surface = DevSurface::Agent;
        state.open_menu();
        assert!(!state.surface_actions().is_empty());
        state.move_menu_selection(3);
        state.run_menu_action();
        // No-input actions execute immediately; login hands control to the
        // outer loop instead of requiring a second Enter in the palette.
        assert!(!state.command_mode);
        assert!(state.agent_login_requested);

        state.surface = DevSurface::App;
        state.open_menu();
        state.run_menu_action();
        // Mutating no-input actions go directly to their confirmation guard.
        assert!(!state.command_mode);
        assert!(state.pending_confirmation.is_some());
        state.deny_confirmation();
        state.open_menu();
        state.menu_selection = 2;
        state.run_menu_action();
        assert_eq!(state.command_input, "browser navigate ");

        state.surface = DevSurface::Terminal;
        state.open_menu();
        state.menu_selection = 0;
        state.run_menu_action();
        assert!(state.menu_open);
        assert!(state.status.contains("unavailable"));
        state.close_menu();
        state.open_menu();
        state.menu_selection = 1;
        state.run_menu_action();
        assert_eq!(state.command_input, "process start dev ");

        state.surface = DevSurface::More;
        state.open_menu();
        state.menu_selection = 0;
        state.run_menu_action();
        assert!(state.menu_open);
        assert!(state.status.contains("unavailable"));
    }

    #[test]
    fn redesigned_action_menu_keeps_selection_and_context_visible() {
        let mut desktop = state(TuiLayout::Desktop);
        desktop.surface = DevSurface::Agent;
        desktop.open_menu();
        assert_eq!(desktop.status, "Command center · Agent");
        let output = rendered(&desktop, 118, 32);
        assert!(output.contains("ACTIONS · Agent"));
        assert!(output.contains("Compose message"));
        assert!(!output.contains("[i]"));
        assert!(output.contains("DETAILS"));
        assert!(output.contains("ask the resident Glass Agent"));

        let mut mobile = state(TuiLayout::Mobile);
        mobile.surface = DevSurface::Agent;
        mobile.open_menu();
        assert_eq!(mobile.status, "Command center · Agent");
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
        assert!(output.contains("Enter sends"));
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
        assert!(!output.contains("Ctrl-D mode"));
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
        trust_workspace_once(&mut state);
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
    fn selected_file_opens_as_full_screen_editor_with_cursor_and_exit_help() {
        let mut state = state(TuiLayout::Desktop);
        trust_workspace_once(&mut state);
        state.surface = DevSurface::Code;
        state.selected_file = state
            .files
            .iter()
            .position(|path| path == "Cargo.toml")
            .unwrap();
        state.open_selected_file_for_edit();
        assert!(state.code_edit_mode);
        assert_eq!(state.editor_engine.mode, EditorMode::Normal);
        let output = rendered(&state, 100, 30);
        assert!(output.contains("GLASS DEV · EDITOR"));
        assert!(output.contains("SOURCE"));
        assert!(output.contains("NORMAL"), "rendered output: {output:?}");
        assert!(output.contains("Esc back"), "rendered output: {output:?}");
        assert!(output.contains("Ctrl-S"));

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let cursor = terminal.backend().cursor_position();
        assert!(cursor.x > 0);
        assert!(cursor.y > 0);

        state.edit_code_key(
            crossterm::event::KeyCode::Char('i'),
            crossterm::event::KeyModifiers::NONE,
        );
        state.edit_code_key(
            crossterm::event::KeyCode::Char('#'),
            crossterm::event::KeyModifiers::NONE,
        );
        state.request_editor_exit();
        let prompt = rendered(&state, 100, 30);
        assert!(prompt.contains("UNSAVED CHANGES"));
        assert!(prompt.contains("discard changes and quit Glass"));
    }

    #[test]
    fn fullscreen_editor_keeps_projected_content_when_workspace_is_busy() {
        let mut state = state(TuiLayout::Desktop);
        trust_workspace_once(&mut state);
        state.surface = DevSurface::Code;
        state.selected_file = state
            .files
            .iter()
            .position(|path| path == "Cargo.toml")
            .unwrap();
        state.open_selected_file_for_edit();
        let expected = state.focused_editor_content.clone();
        let workspace = state.workspace.clone();
        let _workspace_guard = workspace.lock().expect("workspace lock");

        let output = rendered(&state, 100, 30);
        assert!(expected.contains("name='x'"));
        assert!(output.contains("name='x'"));
    }

    #[test]
    fn fullscreen_editor_cursor_tracks_active_line_without_wrapping() {
        let mut state = state(TuiLayout::Desktop);
        state.code_edit_mode = true;
        state.focused_editor_path = "CHANGELOG.md".into();
        state.focused_editor_content = "one\nthis is a deliberately long source line that should remain horizontally scrollable instead of wrapping across the editor viewport\nactive line".into();
        state.focused_editor_line = 3;
        state.focused_editor_column = 1;
        state.status = "EDITING".into();

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let active_row = (0..30)
            .find(|row| {
                (0..100)
                    .map(|column| {
                        terminal
                            .backend()
                            .buffer()
                            .cell((column, *row))
                            .expect("cell in editor buffer")
                            .symbol()
                    })
                    .collect::<String>()
                    .contains("active line")
            })
            .expect("active source line rendered");
        assert_eq!(
            terminal.backend().cursor_position().y,
            active_row,
            "terminal cursor must stay on the rendered active source line"
        );
    }

    #[test]
    fn fullscreen_editor_soft_wrap_keeps_cursor_on_wrapped_cell() {
        let mut state = state(TuiLayout::Mobile);
        state.code_edit_mode = true;
        state.editor_soft_wrap = true;
        state.focused_editor_path = "src/main.rs".into();
        state.focused_editor_content = format!("{}\n", "word ".repeat(80));
        state.focused_editor_line = 1;
        state.focused_editor_column = 180;
        state.status = "EDITING".into();
        state.set_terminal_size(32, 16);
        let narrow_output = rendered(&state, 32, 16);
        assert!(
            narrow_output.contains("Esc"),
            "narrow editor footer should keep Esc: {narrow_output:?}"
        );

        let backend = TestBackend::new(32, 16);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let cursor = terminal.backend().cursor_position();
        let cell = terminal
            .backend()
            .buffer()
            .cell((cursor.x, cursor.y))
            .expect("cursor cell in editor buffer");
        assert_eq!(cell.fg, Color::Black);
        assert_eq!(cell.bg, ACCENT_BRIGHT);
        assert!(state.editor_scroll_line > 0);
    }

    #[test]
    fn soft_wrapped_editor_renders_and_accepts_only_current_ghost_text() {
        let mut state = state(TuiLayout::Desktop);
        trust_workspace_once(&mut state);
        let root = std::path::PathBuf::from(&state.snapshot_root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/main.rs"),
            "fn main() { let first = 1; let second = 2; let third = 3; }\n",
        )
        .unwrap();
        state
            .ws_mut()
            .unwrap()
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .unwrap();
        state.refresh_editor_projection();
        state
            .ws_mut()
            .unwrap()
            .project_mut()
            .set_buffer_cursor("src/main.rs", 1, 18)
            .unwrap();
        state.refresh_editor_projection();
        state.enter_code_edit();
        let offset = crate::development::editor::text_position_offset(
            &state.focused_editor_content,
            crate::development::TextPosition {
                line: state.focused_editor_line,
                column: state.focused_editor_column,
            },
        )
        .unwrap();
        let revision = state.ws().unwrap().project().revision();
        state.editor_engine.ghost = Some(crate::tui::editor::GhostText {
            text: "visible-completion".into(),
            path: state.focused_editor_path.clone(),
            offset,
            revision,
        });
        state.editor_soft_wrap = true;

        let output = rendered(&state, 48, 24);
        assert!(output.contains("visible-completion"), "{output:?}");
        state.edit_code_key(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        );
        assert!(state.focused_editor_content.contains("visible-completion"));
    }

    #[test]
    fn mobile_code_view_wraps_long_preview_lines() {
        let mut state = state(TuiLayout::Mobile);
        state.surface = DevSurface::Code;
        state.focused_editor_path = "src/main.rs".into();
        state.editor = "a long preview line that should wrap on a phone terminal and keep its final marker visible".into();

        let output = rendered(&state, 48, 24);
        assert!(output.contains("final marker"));
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
        assert!(!state.command_mode);
        assert!(state.pending_confirmation.is_some());
        state.deny_confirmation();

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
                message_id: "chat-follow-up-7".into(),
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
        assert!(!output.contains("Enter send"));
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
                message_id: "chat-send-7".into(),
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
                message_id: "chat-send-9".into(),
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
                message_id: "chat-send-8".into(),
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
    fn agent_chat_status_clears_thinking_after_idle_snapshot() {
        let mut state = state(TuiLayout::Desktop);
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        state.surface = DevSurface::Agent;
        state.selected_agent = Some(agent_id.clone());
        state.status = "Sent to agent-0001 · Glass Agent is thinking…".into();
        state.agent_conversation = "YOU\nhello hello\n\nGLASS AGENT\nhello hello".into();
        state
            .pending_chat_messages
            .push(super::super::state::PendingChatMessage {
                message_id: "chat-hello-1".into(),
                text: "hello hello".into(),
                state: super::super::state::ChatMessageState::Sent,
                job_id: None,
                error: None,
            });

        state.apply_snapshot(&super::super::snapshot::DisplaySnapshot {
            agent_states: vec![(agent_id, crate::AgentStatus::Idle)],
            agent_conversation: state.agent_conversation.clone(),
            conversation_items: vec![super::super::projection::ConversationEntry {
                kind: super::super::projection::ConversationKind::User,
                text: "hello hello".into(),
                streaming: false,
                entry_id: Some("entry-hello-1".into()),
                client_message_id: Some("chat-hello-1".into()),
                tool_name: None,
            }],
            ..Default::default()
        });

        assert!(state.pending_chat_messages.is_empty());
        assert_eq!(state.status, "Glass Agent ready · response received");
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
