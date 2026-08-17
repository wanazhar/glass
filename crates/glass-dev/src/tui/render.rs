use super::state::{DevSurface, DevTuiState, ResponsiveClass};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

pub fn render(frame: &mut Frame<'_>, state: &DevTuiState) {
    let area = frame.area();
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

fn render_help(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect) {
    let content = format!(
        "KEYBOARD COCKPIT · {}\n\nNAVIGATION\n  1–7      switch surfaces (phone uses 1–5)\n  j/k ↑/↓  move and scroll\n  PgUp/Dn  page content · Home/End bounds\n  a        current-surface actions\n  ?        this help · Esc closes overlays\n\nAGENT\n  i        compose · Ctrl-D steer/follow-up · Ctrl-X abort\n  Ctrl-C   quit from every mode\n\nCODE\n  Enter    open selected file · i edit\n  [/]]     switch open buffers · Ctrl-S save\n\nAPP\n  n        navigate · Enter activate selected entity\n  t        type into selected · v live view\n  Alt-←/→  browser back/forward · Ctrl-R reload\n\nGIT\n  d        inline diff · PgUp/Dn scroll diff\n\nEXPERT ROUTE\n  :        command palette · type help for all governed routes\n\nEvery mutation shows its authority, revision, and confirmation state.",
        state.surface.label()
    );
    frame.render_widget(
        Paragraph::new(content)
            .scroll((state.help_scroll, 0))
            .block(
                Block::default()
                    .title(" Help · j/k scroll ")
                    .borders(Borders::ALL),
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
    let footer_lines = if state.composer_mode {
        vec![
            Line::from(format!("> {}", state.composer_input)),
            Line::from(state.status.clone()),
        ]
    } else if state.command_mode {
        vec![
            Line::from(format!(":{}", state.command_input)),
            Line::from(format!(
                "{} · {}",
                state.status,
                state.palette_matches().join(" · ")
            )),
        ]
    } else if state.surface == DevSurface::Trust {
        vec![
            Line::from(state.status.clone()),
            Line::from("I inspect · O open untrusted · 1 trust once · T trust project"),
        ]
    } else {
        vec![
            Line::from(state.status.clone()),
            Line::from(format!(
                "1 Agent  2 Code  3 App  4 Tasks  5 More · {}",
                state.surface.label()
            )),
        ]
    };
    frame.render_widget(
        Paragraph::new(footer_lines)
            .block(Block::default().borders(Borders::TOP))
            .wrap(Wrap { trim: true }),
        rows[2],
    );
}

fn render_header(frame: &mut Frame<'_>, state: &DevTuiState, area: Rect, mode: &str) {
    let brand = Span::styled(
        " GLASS DEV ",
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    let lines = if area.width < 72 {
        vec![
            Line::from(vec![
                brand.clone(),
                Span::raw(format!(" · {}", state.surface.label())),
            ]),
            Line::from(format!(
                " {} · {}",
                compact_path(&state.snapshot_root, area.width.saturating_sub(18)),
                state.product_mode().label()
            )),
        ]
    } else if area.width < 104 {
        vec![
            Line::from(vec![
                brand.clone(),
                Span::raw(format!(
                    " · {} · {} · {}",
                    state.surface.label(),
                    state.product_mode().label(),
                    mode
                )),
            ]),
            Line::from(format!(
                " {}",
                compact_path(&state.snapshot_root, area.width.saturating_sub(4))
            )),
        ]
    } else {
        vec![Line::from(vec![
            brand,
            Span::raw(format!(
                " {} · {} · {} · {}",
                compact_path(&state.snapshot_root, area.width.saturating_sub(38)),
                state.surface.label(),
                state.product_mode().label(),
                mode
            )),
        ])]
    };
    frame.render_widget(Paragraph::new(lines), area);
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
    if state.menu_open {
        let actions = state
            .surface_actions()
            .iter()
            .enumerate()
            .map(|(index, (name, hint, prefix))| {
                let hint_label = if *prefix == ":" {
                    format!(":{hint}")
                } else {
                    (*hint).to_string()
                };
                format!(
                    "{} {} · {}",
                    if index == state.menu_selection {
                        "◆"
                    } else {
                        "○"
                    },
                    name,
                    hint_label
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        frame.render_widget(
            Paragraph::new(format!(
                "ACTIONS · {}\n\n{}\n\nj/k select · Enter runs · Esc closes",
                state.surface.label(),
                actions
            ))
            .block(Block::default().title(" Actions ").borders(Borders::ALL))
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
    let content = if state.git_diff_open && state.surface == DevSurface::Git {
        format!(
            "GIT DIFF\n\n{}\n\nEsc closes · PgUp/PgDn scroll",
            state.git_diff
        )
    } else {
        match state.surface {
            DevSurface::Trust => {
                let inspection = super::projection::trust_items(&state.snapshot_trust_inspection);
                format!(
                    "WORKSPACE TRUST\n\nThis repository contains executable Glass settings.\nCurrent state: {}\n\n[I] Inspect configuration\n[O] Open untrusted\n[1] Trust once\n[T] Trust this project\n\nCONFIGURATION BY AUTHORITY / RISK\n{}",
                    state.snapshot_trust_label.as_str(),
                    inspection
                )
            }
            DevSurface::Agent => format!(
                "CONVERSATION\n{}\n\n{}\n\nComposer: i compose · Enter sends · Ctrl-X abort · Ctrl-D toggles steer/follow-up · Esc closes",
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
                "MANAGED TERMINALS\n{}\n\nUse a for terminal actions · : for governed process commands",
                state.processes
            ),
            DevSurface::Tasks => format!(
                "TASK DAG\n\n{}\n\nUse a for task actions · : for dependencies, evidence, and verification",
                state.tasks
            ),
            DevSurface::Debug => format!(
                "DEBUG SESSIONS\n{}\n\nTESTS\n{}\n\nUse a for debugger/test actions · : for the full route",
                state.debugger, state.tests
            ),
            DevSurface::Git => format!(
                "{}\n\nUse a for Git actions · d opens inline diff · : for staging, commits, and branches",
                state.git
            ),
            DevSurface::More => format!(
                "{}\n\nPI READINESS\n{}\n\nKERNELS\n{}\n\nEXPERIMENTS\n{}\n\nREPLAY / OPERATIONS\n{}\n\nKeys: a actions · : expert commands",
                state.workspace_status,
                state.agent_readiness,
                state.kernels,
                state.experiments,
                state.replay,
            ),
        }
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
fn draw_ansi_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    pane: &glass_browser::tui::live_view::AnsiPane,
) {
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
        state.snapshot_trust_label.as_str(),
        state.snapshot_project_revision
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
                "SELECTED APP ENTITY\n{}\n\nWORKSPACE\nrev {} · {} · owner {}{}",
                browser
                    .selected()
                    .map(|entity| format!(
                        "◆ {} · {} · {}",
                        entity.name, entity.role, entity.reference
                    ))
                    .unwrap_or_else(|| "No semantic entity selected".into()),
                browser
                    .browser_revision
                    .map_or_else(|| "—".into(), |revision| revision.to_string()),
                browser.presentation_label(),
                browser.input_owner_label(),
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
        if state.refresh_latency_ms >= 200 {
            format!("{} · refresh {}ms", state.status, state.refresh_latency_ms)
        } else {
            state.status.clone()
        }
    };
    frame.render_widget(
        Paragraph::new(content).style(Style::default().fg(Color::Yellow)),
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
        state.move_menu_selection(2);
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
        assert_eq!(state.command_input, "process start dev ");
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
        assert!(output.contains("ACTIONS · Agent"));
        assert!(output.contains("Compose message"));
        state.move_menu_selection(1);
        state.run_menu_action();
        assert!(state.command_mode);
        assert!(state.command_input.starts_with("agent setup"));
        state.close_palette();

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
}
