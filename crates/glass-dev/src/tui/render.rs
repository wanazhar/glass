use super::state::{DevSurface, DevTuiState, ResponsiveClass};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

pub fn render(frame: &mut Frame<'_>, state: &DevTuiState) {
    let area = frame.area();
    match state.responsive_class(area.width, area.height) {
        ResponsiveClass::Desktop => render_desktop(frame, state, area),
        ResponsiveClass::Compact => render_compact(frame, state, area),
        ResponsiveClass::Phone => render_phone(frame, state, area),
    }
}

fn render_desktop(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(area);
    render_header(frame, state, rows[0], "desktop");
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(22),
            Constraint::Percentage(52),
            Constraint::Min(28),
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
            Constraint::Length(2),
        ])
        .split(area);
    render_header(frame, state, rows[0], "compact");
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(20), Constraint::Min(36)])
        .split(rows[1]);
    render_navigation(frame, state, columns[0]);
    render_surface(frame, state, columns[1]);
    render_status(frame, state, rows[2]);
}

fn render_phone(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(area);
    render_header(frame, state, rows[0], "phone cockpit");
    render_surface(frame, state, rows[1]);
    let help = if state.command_mode {
        format!(":{}", state.command_input)
    } else {
        format!("{} · : actions · j/k views · q quit", state.surface.label())
    };
    frame.render_widget(
        Paragraph::new(help)
            .block(Block::default().borders(Borders::TOP))
            .wrap(Wrap { trim: true }),
        rows[2],
    );
}

fn render_header(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect, mode: &str) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " GLASS DEV ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                " {} · {} · {}",
                state.workspace.root().display(),
                state.surface.label(),
                mode
            )),
        ])),
        area,
    );
}

fn render_navigation(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let items = DevSurface::ALL.into_iter().map(|surface| {
        let marker = if surface == state.surface {
            "◆"
        } else {
            "○"
        };
        let style = if surface == state.surface {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        ListItem::new(format!("{marker} {}", surface.label())).style(style)
    });
    frame.render_widget(
        List::new(items).block(Block::default().title(" Workspace ").borders(Borders::ALL)),
        area,
    );
}

fn render_surface(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let content = match state.surface {
        DevSurface::Dashboard => format!(
            "AGENTS\n{}\n\nPROCESSES\n{}\n\nTESTS\n{}",
            state.agents, state.processes, state.tests
        ),
        DevSurface::Editor => format!(
            "SHARED BUFFERS\n{}\n\nActions: :editor open PATH · :editor replace PATH OLD NEW · :editor save PATH",
            state.editor
        ),
        DevSurface::Agent | DevSurface::Agents => state.agents.clone(),
        DevSurface::Processes => state.processes.clone(),
        DevSurface::Debugger => state.debugger.clone(),
        DevSurface::Git => state.git.clone(),
        DevSurface::Tests => state.tests.clone(),
        DevSurface::Experiments => state
            .experiment_comparison
            .as_ref()
            .and_then(|comparison| serde_json::to_string_pretty(comparison).ok())
            .map(|comparison| {
                format!(
                    "EXPERIMENTS\n{}\n\nEVIDENCE RANKING\n{}",
                    state.experiments, comparison
                )
            })
            .unwrap_or_else(|| state.experiments.clone()),
        DevSurface::Graph => format!(
            "CAUSAL DEVELOPMENT GRAPH\n\n{} replay events recorded\nUse governed tools to create source/runtime/test/debug/browser evidence paths.",
            state
                .workspace
                .intelligence()
                .replay(0, 4096)
                .map(|events| events.len())
                .unwrap_or(0)
        ),
        DevSurface::Replay => state.replay.clone(),
        DevSurface::Browser => format!(
            "AUTHORITATIVE BROWSER\n{}\n\nLATEST EVIDENCE\n{}\n\nActions: :browser start|stop|observe|targets|navigate|click|type",
            state.browser, state.browser_detail
        ),
    };
    frame.render_widget(
        Paragraph::new(content)
            .block(
                Block::default()
                    .title(format!(" {} ", state.surface.label()))
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_context(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let content = format!(
        "KERNELS\n{}\n\nDEBUGGERS\n{}\n\nAUTHORITY\ngeneration {}\nproject revision {}\nmutations require actor + revision + confirmation",
        state.kernels,
        state.debugger,
        state.workspace.generation(),
        state.workspace.project().revision()
    );
    frame.render_widget(
        Paragraph::new(content)
            .block(
                Block::default()
                    .title(" Live context ")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_status(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let content = if state.command_mode {
        format!(":{}", state.command_input)
    } else {
        state.status.clone()
    };
    frame.render_widget(
        Paragraph::new(content).style(Style::default().fg(Color::Yellow)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn desktop_and_phone_render_agent_native_surfaces() {
        for (width, height, layout) in [(140, 40, TuiLayout::Desktop), (48, 18, TuiLayout::Mobile)]
        {
            let state = state(layout);
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal.draw(|frame| render(frame, &state)).unwrap();
            let rendered = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(rendered.contains("GLASS DEV"));
            assert!(rendered.contains("Dashboard"));
            assert!(rendered.contains("AGENTS"));
        }
    }
}
