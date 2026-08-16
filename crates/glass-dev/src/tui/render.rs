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
    let help = if state.composer_mode {
        format!("> {}", state.composer_input)
    } else if state.command_mode {
        format!(":{}", state.command_input)
    } else {
        format!(
            "1 Agent  2 Code  3 App  4 Tasks  5 More · {}",
            state.surface.label()
        )
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
                " {} · {} · {} · {}",
                state.workspace.root().display(),
                state.surface.label(),
                state.product_mode().label(),
                mode
            )),
        ])),
        area,
    );
}

fn render_navigation(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let items = DevSurface::PRIMARY.into_iter().map(|surface| {
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
    if let Some(offer) = state.browser_recovery.as_ref() {
        let actions = offer
            .actions()
            .iter()
            .map(|(key, description)| format!("[{key}] {description}"))
            .collect::<Vec<_>>()
            .join("\n");
        frame.render_widget(
            Paragraph::new(format!(
                "BROWSER RECOVERY\n\n{}\n\n{}",
                offer.reason, actions
            ))
            .block(Block::default().title(" Recovery ").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    if let Some(pending) = state.pending_confirmation.as_ref() {
        frame.render_widget(
            Paragraph::new(format!(
                "CONFIRM ONE MUTATION\n\n{}\n\n[Y / Enter] Approve once\n[N / Esc] Deny\n\nThe frozen call cannot authorize a retry or changed arguments.",
                pending.summary
            ))
            .block(Block::default().title(" Confirmation ").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let content = match state.surface {
        DevSurface::Trust => {
            let inspection = super::projection::trust_items(&state.workspace.trust_inspection());
            format!(
                "WORKSPACE TRUST\n\nThis repository contains executable Glass settings.\nCurrent state: {}\n\n[I] Inspect configuration\n[O] Open untrusted\n[1] Trust once\n[T] Trust this project\n\nCONFIGURATION BY AUTHORITY / RISK\n{}",
                state.workspace.trust().label(),
                inspection
            )
        }
        DevSurface::Agent => format!(
            "CONVERSATION\n{}\n\n{}\n\nComposer: i compose · Enter sends · Ctrl-A abort · Ctrl-S steer · Esc returns to navigation",
            state.agent_readiness, state.agent_conversation
        ),
        DevSurface::Code => format!(
            "FILES\n{}\n\nEDITOR{}\n{}\n\nDIAGNOSTICS\n{}\n\nKeys: j/k select file · Enter open · i edit · arrows · Ctrl-S · Ctrl-Z/Y",
            state
                .files
                .iter()
                .enumerate()
                .take(24)
                .map(|(index, path)| format!(
                    "{} {}",
                    if index == state.selected_file {
                        "◆"
                    } else {
                        "○"
                    },
                    path
                ))
                .collect::<Vec<_>>()
                .join("\n"),
            if state.code_edit_mode { " · EDIT" } else { "" },
            state.editor,
            state.lsp
        ),
        DevSurface::App => {
            let mut content = format!(
                "APP WORKSPACE\n{}\n\n{}\n\nWORKFLOW\n{}\n\nKeys: j/k select · Enter activate · n address · t type · v visual · PgUp/PgDn page · H human · G Glass",
                state.browser, state.browser_detail, state.workflow
            );
            if state.browser_visual_live {
                content.push_str("\n\nLIVE VIEW · ANSI half-block rendering · v stops");
            }
            content
        }
        DevSurface::Terminal => format!(
            "MANAGED TERMINALS\n{}\n\nKeys: Enter start detected dev command · r restart · x stop",
            state.processes
        ),
        DevSurface::Tasks => format!(
            "TASK DAG\n\n{}\n\nKeys: Enter create task · p pause · u resume · x cancel",
            state.tasks
        ),
        DevSurface::Debug => format!(
            "DEBUG SESSIONS\n{}\n\nTESTS\n{}\n\nKeys: c continue · s step · b breakpoint",
            state.debugger, state.tests
        ),
        DevSurface::Git => format!(
            "{}\n\nKeys: Enter stage/unstage selected · d diff · c commit · D discard",
            state.git
        ),
        DevSurface::More => format!(
            "PROJECT / ONBOARDING\n{}\n\nPI READINESS\n{}\n\nKERNELS\n{}\n\nEXPERIMENTS\n{}\n\nREPLAY / OPERATIONS\n{}\n\nActions: :kernel · :experiment · :replay · :workspace · :trust",
            state.workspace_status,
            state.agent_readiness,
            state.kernels,
            state.experiments,
            state.replay,
        ),
    };
    frame.render_widget(
        Paragraph::new(content)
            .scroll((state.current_scroll(), 0))
            .block(
                Block::default()
                    .title(format!(" {} ", state.surface.label()))
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
    if state.surface == DevSurface::App
        && state.browser_visual_live
        && let Some(pane) = state.browser_pane.as_ref()
    {
        draw_ansi_pane(frame, area, pane);
    }
}

/// Paint an ANSI half-block pane into the lower half of a surface area.
fn draw_ansi_pane(frame: &mut Frame<'_>, area: Rect, pane: &glass_browser::tui::live_view::AnsiPane) {
    let inner = Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    // Anchor the pane to the bottom of the surface so semantic text stays on top.
    let pane_height = inner.height.min(pane.rows);
    let top = inner.bottom().saturating_sub(pane_height);
    for (row_index, row_cells) in pane.cells.chunks(pane.columns as usize).enumerate() {
        let y = top + row_index as u16;
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
                target.set_fg(ratatui::style::Color::Rgb(
                    cell.top.red,
                    cell.top.green,
                    cell.top.blue,
                ));
                target.set_bg(ratatui::style::Color::Rgb(
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
        "\n\nAUTHORITY\ntrust {} · project rev {}\nmutations require revision + confirmation",
        state.workspace.trust().label(),
        state.workspace.project().revision()
    );
    let content = match state.surface {
        DevSurface::Trust => format!("Selected: workspace trust{authority}"),
        DevSurface::Agent => format!(
            "SELECTED CONVERSATION\n{}\n\nActions\nnew · resume · model · thinking · abort{authority}",
            state.agent_readiness
        ),
        DevSurface::Code => format!(
            "SELECTED CODE\n{}\n\nLINKED APP\n{}\n\nActions\nsave · undo · redo · search · diagnostics · V open App{authority}",
            state.editor.lines().next().unwrap_or("No buffer selected"),
            state
                .browser_workspace
                .state()
                .selected()
                .map(|entity| format!("{} · {}", entity.name, entity.reference))
                .unwrap_or_else(|| "No current source/runtime link".into())
        ),
        DevSurface::App => format!(
            "SELECTED APP ENTITY\n{}\n\nActions\nactivate · type · inspect · workflow{authority}",
            state
                .browser_workspace
                .state()
                .selected()
                .map(|entity| format!("{} · {}", entity.name, entity.role))
                .unwrap_or_else(|| "No semantic entity selected".into())
        ),
        DevSurface::Terminal => format!(
            "SELECTED PROCESS\n{}{}",
            state
                .processes
                .lines()
                .next()
                .unwrap_or("No process selected"),
            authority
        ),
        DevSurface::Tasks => format!(
            "SELECTED TASK\n{}{}",
            state.tasks.lines().next().unwrap_or("No task selected"),
            authority
        ),
        DevSurface::Git => format!(
            "SELECTED CHANGE\n{}{}",
            state.git.lines().next().unwrap_or("No change selected"),
            authority
        ),
        DevSurface::Debug => format!(
            "SELECTED FRAME\n{}{}",
            state.debugger.lines().next().unwrap_or("No frame selected"),
            authority
        ),
        DevSurface::More => format!(
            "PROJECT SERVICES\n{} skills · {} custom tools{}",
            state.workspace.customization().skills().count(),
            state.workspace.customization().config().tools.len(),
            authority
        ),
    };
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
    let content = if state.composer_mode {
        format!("> {}", state.composer_input)
    } else if state.command_mode {
        let suggestions = state.palette_matches().join(" · ");
        let error = state
            .palette_error
            .as_deref()
            .map(|error| format!(" · {error}"))
            .unwrap_or_default();
        format!(":{}  [{}]{}", state.command_input, suggestions, error)
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
            let state = state(layout);
            let rendered = rendered(&state, width, height);
            assert!(rendered.contains("GLASS DEV"));
            assert!(rendered.contains("Agent"));
            assert!(rendered.contains("CONVERSATION"));
        }
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
            std::fs::read_to_string(state.workspace.root().join("Cargo.toml"))
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
        assert_eq!(state.workspace.trust(), crate::WorkspaceTrust::Untrusted);
        assert_eq!(state.surface, DevSurface::Agent);
        std::fs::remove_dir_all(root).unwrap();
    }
}
