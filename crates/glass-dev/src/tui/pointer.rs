//! Pointer hit-testing. Mouse and terminal-touch call the same reducers as keys.

use super::overlay::{self, ActiveOverlay};
use super::state::{DevSurface, DevTuiState};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::time::{Duration, Instant};

const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const LONG_PRESS: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HitRegion {
    Surface(DevSurface),
    Dock,
    File(usize),
    Git(usize),
    Process(usize),
    Debug(usize),
    Entity(usize),
    MoreRoute(usize),
    Menu(usize),
    Help,
    Other,
}

#[derive(Debug, Default)]
pub struct PointerState {
    down: Option<PointerDown>,
    last_click: Option<(HitRegion, Instant)>,
}

#[derive(Debug, Clone)]
struct PointerDown {
    column: u16,
    row: u16,
    at: Instant,
    button: MouseButton,
    hit: HitRegion,
    dragged: bool,
}

impl PointerState {
    pub fn reset(&mut self) {
        self.down = None;
        self.last_click = None;
    }

    pub fn handle(&mut self, state: &mut DevTuiState, mouse: MouseEvent, now: Instant) -> bool {
        match overlay::active_overlay(state) {
            Some(ActiveOverlay::Help) => {
                match mouse.kind {
                    MouseEventKind::ScrollUp => state.scroll_help(-3),
                    MouseEventKind::ScrollDown => state.scroll_help(3),
                    _ => {}
                }
                self.reset();
                return true;
            }
            Some(ActiveOverlay::CommandCenterMenu) => match mouse.kind {
                MouseEventKind::ScrollUp => {
                    state.move_menu_selection(-3);
                    return true;
                }
                MouseEventKind::ScrollDown => {
                    state.move_menu_selection(3);
                    return true;
                }
                _ => {}
            },
            Some(ActiveOverlay::Composer) => {
                if matches!(
                    mouse.kind,
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                ) {
                    return true;
                }
            }
            Some(_) => {
                self.reset();
                return true;
            }
            None => {}
        }

        match mouse.kind {
            MouseEventKind::ScrollUp => {
                state.scroll_surface(-3);
                true
            }
            MouseEventKind::ScrollDown => {
                state.scroll_surface(3);
                true
            }
            MouseEventKind::Down(button) => {
                if button == MouseButton::Right {
                    if overlay::active_overlay(state).is_none() {
                        state.open_menu();
                    }
                    self.reset();
                    return true;
                }
                let hit = hit_test(state, mouse.column, mouse.row);
                if let Some((previous, at)) = &self.last_click
                    && now.duration_since(*at) <= DOUBLE_CLICK
                    && *previous == hit
                    && button == MouseButton::Left
                {
                    self.last_click = None;
                    self.down = None;
                    return apply_primary(state, &hit);
                }
                self.down = Some(PointerDown {
                    column: mouse.column,
                    row: mouse.row,
                    at: now,
                    button,
                    hit: hit.clone(),
                    dragged: false,
                });
                apply_select(state, &hit);
                true
            }
            MouseEventKind::Drag(_) => {
                if let Some(down) = &mut self.down {
                    down.dragged = down.column != mouse.column || down.row != mouse.row;
                }
                true
            }
            MouseEventKind::Up(button) => {
                let Some(down) = self.down.take() else {
                    return false;
                };
                if button != down.button {
                    return true;
                }
                let held = now.duration_since(down.at);
                if !down.dragged && held >= LONG_PRESS && overlay::active_overlay(state).is_none() {
                    state.open_menu();
                    return true;
                }
                if !down.dragged && button == MouseButton::Left {
                    self.last_click = Some((down.hit.clone(), now));
                    return apply_click(state, &down.hit);
                }
                true
            }
            _ => false,
        }
    }

    pub fn poll(&mut self, state: &mut DevTuiState, now: Instant) {
        match overlay::active_overlay(state) {
            Some(ActiveOverlay::CommandCenterMenu)
                if self.down.as_ref().is_some_and(|down| {
                    down.button == MouseButton::Left && matches!(&down.hit, HitRegion::Menu(_))
                }) =>
            {
                // Menu clicks complete on mouse-up. Preserve only a press
                // that began on a visible menu row; overlay-transition resets
                // cancel presses that began elsewhere.
                return;
            }
            Some(_) => {
                self.reset();
                return;
            }
            None => {}
        }
        if let Some(down) = &self.down
            && down.button == MouseButton::Left
            && !down.dragged
            && now.duration_since(down.at) >= LONG_PRESS
        {
            self.down = None;
            state.open_menu();
        }
    }
}

pub fn hit_test(state: &DevTuiState, column: u16, row: u16) -> HitRegion {
    let width = state.terminal_width.max(1);
    let height = state.terminal_height.max(1);
    let screen = Rect::new(0, 0, width, height);
    if column >= width || row >= height {
        return HitRegion::Other;
    }
    let geometry = super::render::screen_geometry(state, screen);
    match overlay::active_overlay(state) {
        Some(ActiveOverlay::Help) => return HitRegion::Help,
        Some(ActiveOverlay::CommandCenterMenu) => {
            return super::render::command_menu_hit_index(state, column, row)
                .map(HitRegion::Menu)
                .unwrap_or(HitRegion::Other);
        }
        Some(ActiveOverlay::Composer) => {
            return if contains(geometry.footer, column, row) {
                HitRegion::Dock
            } else {
                HitRegion::Other
            };
        }
        Some(_) => return HitRegion::Other,
        None => {}
    }
    if contains(geometry.footer, column, row) {
        return HitRegion::Dock;
    }
    if let Some(surface) = super::render::navigation_surface_at(state, screen, column, row) {
        return HitRegion::Surface(surface);
    }
    if let Some(index) = super::render::more_route_at(state, column, row) {
        return HitRegion::MoreRoute(index);
    }
    match state.surface {
        DevSurface::Code => super::render::file_hit_at(state, geometry.surface, column, row)
            .map(HitRegion::File)
            .unwrap_or(HitRegion::Other),
        DevSurface::Git => super::render::git_file_hit_at(state, geometry.surface, column, row)
            .map(HitRegion::Git)
            .unwrap_or(HitRegion::Other),
        DevSurface::Terminal => super::render::process_hit_at(state, geometry.surface, column, row)
            .map(HitRegion::Process)
            .unwrap_or(HitRegion::Other),
        DevSurface::Debug => {
            super::render::debug_session_hit_at(state, geometry.surface, column, row)
                .map(HitRegion::Debug)
                .unwrap_or(HitRegion::Other)
        }
        DevSurface::App => super::render::browser_entity_at(state, geometry.surface, column, row)
            .map(HitRegion::Entity)
            .unwrap_or(HitRegion::Other),
        _ => HitRegion::Other,
    }
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x && column < area.right() && row >= area.y && row < area.bottom()
}

fn apply_select(state: &mut DevTuiState, hit: &HitRegion) {
    match hit {
        HitRegion::Surface(surface) => {
            state.show_surface(*surface);
        }
        HitRegion::File(index) => {
            state.selected_file = *index;
            if !state.code_edit_mode {
                state.open_selected_file();
            }
        }
        HitRegion::Git(index) => {
            state.selected_git_file = *index;
        }
        HitRegion::Process(index) => {
            state.selected_process = *index;
        }
        HitRegion::Debug(index) => {
            state.selected_debug_session = *index;
            state.debug_pane = super::state::DebugPane::Sessions;
        }
        HitRegion::Entity(index) => {
            let current = state.browser_workspace.state().selected_entity.unwrap_or(0) as i32;
            let delta = *index as i32 - current;
            if delta != 0 {
                let _ = state.browser_workspace.reduce(
                    glass_browser::browser_workspace::BrowserWorkspaceIntent::MoveSelection {
                        delta,
                    },
                );
                state.browser = state.browser_workspace_summary();
            }
        }
        HitRegion::MoreRoute(index) => state.select_more(*index),
        _ => {}
    }
}

fn apply_click(state: &mut DevTuiState, hit: &HitRegion) -> bool {
    match hit {
        HitRegion::Dock => {
            if state.composer_mode {
                true
            } else {
                state.focus_composer_dock();
                true
            }
        }
        HitRegion::Help => {
            state.toggle_help();
            true
        }
        HitRegion::Menu(index) => {
            state.menu_selection = *index;
            state.run_menu_action();
            true
        }
        _ => true,
    }
}

fn apply_primary(state: &mut DevTuiState, hit: &HitRegion) -> bool {
    match hit {
        HitRegion::Dock => {
            state.focus_composer_dock();
            true
        }
        HitRegion::File(_) => {
            state.open_selected_file_for_edit();
            true
        }
        HitRegion::Git(_) => {
            state.git_diff_requested = true;
            true
        }
        HitRegion::Process(_) => {
            state.process_logs_requested = true;
            true
        }
        HitRegion::Debug(_) => {
            state.debug_threads_requested = true;
            true
        }
        HitRegion::Entity(_) => {
            state.queue_browser_intent(
                glass_browser::browser_workspace::BrowserWorkspaceIntent::ActivateSelected,
            );
            true
        }
        HitRegion::MoreRoute(index) => {
            state.select_more(*index);
            state.activate_more_selection();
            true
        }
        HitRegion::Surface(surface) => {
            state.show_surface(*surface);
            true
        }
        _ => apply_click(state, hit),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glass_browser::cli::args::TuiLayout;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn overlay_state() -> (DevTuiState, std::path::PathBuf) {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "glass-pointer-overlay-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create test workspace");
        let state = DevTuiState::open(&root, TuiLayout::Desktop).expect("open TUI state");
        (state, root)
    }

    fn screen(state: &DevTuiState) -> Rect {
        Rect::new(
            0,
            0,
            state.terminal_width.max(1),
            state.terminal_height.max(1),
        )
    }

    fn first_content_point(area: Rect) -> (u16, u16) {
        let content = super::super::render::panel_content_area(area);
        assert!(
            content.width > 0 && content.height > 0,
            "panel has no content"
        );
        (content.x, content.y)
    }

    fn set_viewport(state: &mut DevTuiState, layout: TuiLayout, width: u16, height: u16) {
        state.layout = layout;
        state.terminal_width = width;
        state.terminal_height = height;
    }

    #[test]
    fn code_file_hits_follow_rendered_list_bounds_and_scroll_on_each_layout() {
        for (layout, width, height) in [
            (TuiLayout::Desktop, 140, 40),
            (TuiLayout::Compact, 96, 32),
            (TuiLayout::Mobile, 48, 26),
        ] {
            let (mut state, root) = overlay_state();
            set_viewport(&mut state, layout, width, height);
            state.surface = DevSurface::Code;
            state.files = (0..12)
                .map(|index| format!("src/file-{index}.rs"))
                .collect();
            state.selected_file = 7;

            let geometry = super::super::render::screen_geometry(&state, screen(&state));
            let files = super::super::render::code_file_list_area(&state, geometry.surface);
            let content = super::super::render::panel_content_area(files);
            assert!(
                content.height > 0,
                "file list should have visible rows for {layout:?}"
            );
            let first_visible = 7usize
                .saturating_add(1)
                .saturating_sub(usize::from(content.height));
            let selected_row = content.y + u16::try_from(7 - first_visible).unwrap();
            assert_eq!(
                hit_test(&state, content.x, selected_row),
                HitRegion::File(7),
                "scrolled selected file should match the visible list row for {layout:?}: screen={:?}, surface={:?}, files={files:?}, content={content:?}, direct={:?}",
                screen(&state),
                geometry.surface,
                super::super::render::file_hit_at(
                    &state,
                    geometry.surface,
                    content.x,
                    selected_row
                )
            );
            assert_eq!(hit_test(&state, files.x, files.y), HitRegion::Other);
            let adjacent_panel_point = if files.right() < geometry.surface.right() {
                (files.right(), content.y)
            } else {
                (content.x, files.bottom())
            };
            assert_eq!(
                hit_test(&state, adjacent_panel_point.0, adjacent_panel_point.1),
                HitRegion::Other,
                "the adjacent editor panel is outside the file list for {layout:?}"
            );

            state.files.truncate(1);
            state.selected_file = 0;
            let content = super::super::render::panel_content_area(files);
            if content.height > 1 {
                assert_eq!(hit_test(&state, content.x, content.y + 1), HitRegion::Other);
            }
            state.files.clear();
            assert_eq!(hit_test(&state, content.x, content.y), HitRegion::Other);
            drop(state);
            std::fs::remove_dir_all(root).expect("remove test workspace");
        }
    }

    #[test]
    fn git_file_hits_reject_summary_diff_and_unused_rows() {
        let (mut state, root) = overlay_state();
        set_viewport(&mut state, TuiLayout::Desktop, 160, 32);
        state.surface = DevSurface::Git;
        state.snapshot_ready = true;
        state.git_entries = (0..30)
            .map(|index| crate::git::GitStatusEntry {
                path: format!("src/file-{index}.rs"),
                original_path: None,
                index_status: ' ',
                worktree_status: 'M',
                untracked: false,
            })
            .collect();
        state.selected_git_file = 24;
        state.git_diff_open = true;

        let geometry = super::super::render::screen_geometry(&state, screen(&state));
        let files = super::super::render::git_file_list_area(&state, geometry.surface);
        let (column, first_row) = first_content_point(files);
        let content = super::super::render::panel_content_area(files);
        let first_visible = 24usize
            .saturating_add(1)
            .saturating_sub(usize::from(content.height));
        assert_eq!(
            hit_test(&state, column, first_row),
            HitRegion::Git(first_visible)
        );
        assert_eq!(
            hit_test(&state, column, content.bottom() - 1),
            HitRegion::Git(24),
            "selected Git row should use the same rendered scroll offset"
        );
        assert_eq!(
            hit_test(&state, geometry.surface.x, geometry.surface.y),
            HitRegion::Other
        );
        assert_eq!(
            hit_test(&state, geometry.surface.right() - 1, first_row),
            HitRegion::Other,
            "diff panel must not select a file"
        );
        state.git_entries.truncate(1);
        state.selected_git_file = 0;
        if content.height > 1 {
            assert_eq!(hit_test(&state, column, first_row + 1), HitRegion::Other);
        }
        state.git_entries.clear();
        assert_eq!(hit_test(&state, column, first_row), HitRegion::Other);

        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn terminal_process_hits_reject_summary_logs_and_stale_empty_state() {
        let (mut state, root) = overlay_state();
        set_viewport(&mut state, TuiLayout::Desktop, 160, 32);
        state.surface = DevSurface::Terminal;
        state.snapshot_ready = true;
        state.process_entries = (0..30)
            .map(|index| super::super::state::ProcessRow {
                name: format!("dev-{index}"),
                command: "cargo run".into(),
                pid: Some(42),
                health: crate::development::ProcessHealth::Healthy,
                url: None,
            })
            .collect();
        state.selected_process = 24;

        let geometry = super::super::render::screen_geometry(&state, screen(&state));
        let processes = super::super::render::terminal_process_list_area(geometry.surface);
        let (column, first_row) = first_content_point(processes);
        let content = super::super::render::panel_content_area(processes);
        let first_visible = 24usize
            .saturating_add(1)
            .saturating_sub(usize::from(content.height));
        assert_eq!(
            hit_test(&state, column, first_row),
            HitRegion::Process(first_visible)
        );
        assert_eq!(
            hit_test(&state, column, content.bottom() - 1),
            HitRegion::Process(24),
            "selected process row should use the same rendered scroll offset"
        );
        assert_eq!(
            hit_test(&state, geometry.surface.x, geometry.surface.y),
            HitRegion::Other
        );
        assert_eq!(
            hit_test(&state, column, processes.bottom()),
            HitRegion::Other,
            "logs panel must not select a process"
        );

        state.process_entries.clear();
        assert_eq!(hit_test(&state, column, first_row), HitRegion::Other);
        state.process_entries.push(super::super::state::ProcessRow {
            name: "stale".into(),
            command: "sleep".into(),
            pid: Some(42),
            health: crate::development::ProcessHealth::Healthy,
            url: None,
        });
        state.snapshot_ready = false;
        assert_eq!(hit_test(&state, column, first_row), HitRegion::Other);
        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn debug_hits_are_limited_to_visible_unwrapped_session_rows() {
        let (mut state, root) = overlay_state();
        set_viewport(&mut state, TuiLayout::Mobile, 80, 32);
        state.surface = DevSurface::Debug;
        state.debug_sessions = vec![super::super::state::DebugSessionRow {
            name: "api".into(),
            state: crate::debugger::DebugSessionState::Stopped,
            pid: 7,
            breakpoints: 0,
            watches: 0,
        }];

        let geometry = super::super::render::screen_geometry(&state, screen(&state));
        let sessions = super::super::render::debug_session_area(&state, geometry.surface);
        let (column, row) = first_content_point(sessions);
        assert_eq!(hit_test(&state, column, row), HitRegion::Debug(0));
        let adjacent_panel_point = if sessions.right() < geometry.surface.right() {
            (sessions.right(), row)
        } else {
            (column, sessions.bottom())
        };
        assert_eq!(
            hit_test(&state, adjacent_panel_point.0, adjacent_panel_point.1),
            HitRegion::Other,
            "other debugger panels are not session rows"
        );

        state.debug_sessions[0].name = "long".repeat(40);
        assert_eq!(hit_test(&state, column, row), HitRegion::Other);
        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn app_entity_hits_require_a_visible_unambiguous_inspector_row() {
        let (mut state, root) = overlay_state();
        set_viewport(&mut state, TuiLayout::Desktop, 240, 45);
        state.surface = DevSurface::App;
        let browser = state.browser_workspace.state_mut();
        browser.title = "Example".into();
        browser.url = "https://example.test".into();
        browser.entities = vec![glass_browser::browser_workspace::BrowserWorkspaceEntity {
            reference: "submit".into(),
            role: "button".into(),
            name: "Sign in".into(),
            actionable: true,
            revision: 1,
        }];

        let geometry = super::super::render::screen_geometry(&state, screen(&state));
        let inspector = super::super::render::browser_inspector_area(&state, geometry.surface)
            .expect("desktop inspector panel");
        let content = super::super::render::panel_content_area(inspector);
        let entity_point = (content.y..content.bottom())
            .find_map(|row| {
                super::super::render::browser_entity_at(&state, geometry.surface, content.x, row)
                    .map(|index| (content.x, row, index))
            })
            .expect("visible entity row");
        assert_eq!(entity_point.2, 0);
        assert_eq!(
            hit_test(&state, entity_point.0, entity_point.1),
            HitRegion::Entity(0)
        );
        assert_eq!(hit_test(&state, inspector.x, inspector.y), HitRegion::Other);

        state.browser_workspace.state_mut().entities.clear();
        assert_eq!(
            hit_test(&state, entity_point.0, entity_point.1),
            HitRegion::Other
        );

        let browser = state.browser_workspace.state_mut();
        browser.title = "A".repeat(200);
        browser.entities = vec![glass_browser::browser_workspace::BrowserWorkspaceEntity {
            reference: "submit".into(),
            role: "button".into(),
            name: "Sign in".into(),
            actionable: true,
            revision: 1,
        }];
        assert_eq!(
            hit_test(&state, entity_point.0, entity_point.1),
            HitRegion::Other
        );

        state.layout = TuiLayout::Mobile;
        state.terminal_width = 48;
        state.terminal_height = 20;
        let phone = super::super::render::screen_geometry(&state, screen(&state));
        assert!(super::super::render::browser_inspector_area(&state, phone.surface).is_none());
        assert_eq!(hit_test(&state, 10, phone.surface.y), HitRegion::Other);

        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn pointer_hits_outside_terminal_bounds_are_rejected() {
        let (mut state, root) = overlay_state();
        set_viewport(&mut state, TuiLayout::Desktop, 100, 30);
        state.surface = DevSurface::More;
        assert_eq!(hit_test(&state, state.terminal_width, 10), HitRegion::Other);
        assert_eq!(
            hit_test(&state, 10, state.terminal_height),
            HitRegion::Other
        );
        assert_eq!(hit_test(&state, u16::MAX, u16::MAX), HitRegion::Other);
        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn composer_footer_hit_region_tracks_dynamic_rendered_height() {
        let (mut state, root) = overlay_state();
        set_viewport(&mut state, TuiLayout::Desktop, 140, 40);
        state.surface = DevSurface::Code;
        let screen = screen(&state);
        let short_footer = super::super::render::screen_geometry(&state, screen).footer;

        state.composer_mode = true;
        state.composer_input = "one\ntwo\nthree\nfour\nfive\nsix".into();
        let geometry = super::super::render::screen_geometry(&state, screen);
        assert!(geometry.footer.height > short_footer.height);
        assert_eq!(hit_test(&state, 0, geometry.footer.y), HitRegion::Dock);
        assert_eq!(
            hit_test(
                &state,
                state.terminal_width - 1,
                geometry.footer.bottom() - 1
            ),
            HitRegion::Dock
        );
        assert_eq!(
            hit_test(&state, 10, geometry.footer.y - 1),
            HitRegion::Other
        );

        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn overlays_block_covered_mouse_paths_while_help_and_menu_keep_their_routes() {
        let (mut state, root) = overlay_state();
        state.terminal_width = 100;
        state.terminal_height = 40;
        state.surface = DevSurface::Git;
        state.surface_scroll.insert(DevSurface::Git, 5);
        state.file_picker_open = true;
        state.command_mode = true;
        state.pi_command_mode = true;
        state.code_edit_mode = true;
        assert_eq!(hit_test(&state, 50, 10), HitRegion::Other);

        let mut pointer = PointerState::default();
        pointer.handle(
            &mut state,
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 50,
                row: 10,
                modifiers: crossterm::event::KeyModifiers::empty(),
            },
            Instant::now(),
        );
        assert_eq!(state.surface_scroll.get(&DevSurface::Git), Some(&5));

        let press = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 50,
            row: 10,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        pointer.handle(&mut state, press, Instant::now());
        pointer.handle(
            &mut state,
            MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: 51,
                row: 11,
                ..press
            },
            Instant::now(),
        );
        pointer.handle(
            &mut state,
            MouseEvent {
                kind: MouseEventKind::Up(MouseButton::Left),
                ..press
            },
            Instant::now(),
        );
        assert_eq!(state.surface_scroll.get(&DevSurface::Git), Some(&5));

        state.file_picker_open = false;
        state.command_mode = false;
        state.pi_command_mode = false;
        state.code_edit_mode = false;
        state.help_open = true;
        pointer.handle(
            &mut state,
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 50,
                row: 10,
                modifiers: crossterm::event::KeyModifiers::empty(),
            },
            Instant::now(),
        );
        assert_eq!(state.help_scroll, 3);
        assert_eq!(state.surface_scroll.get(&DevSurface::Git), Some(&5));

        state.help_open = false;
        state.open_menu();
        assert!(matches!(hit_test(&state, 50, 3), HitRegion::Menu(0)));
        let previous_selection = state.menu_selection;
        pointer.handle(
            &mut state,
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 50,
                row: 10,
                modifiers: crossterm::event::KeyModifiers::empty(),
            },
            Instant::now(),
        );
        assert_ne!(state.menu_selection, previous_selection);
        let search_index = state.surface_actions().len();
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 50,
            row: 3 + search_index as u16,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        pointer.handle(&mut state, click, Instant::now());
        pointer.handle(
            &mut state,
            MouseEvent {
                kind: MouseEventKind::Up(MouseButton::Left),
                ..click
            },
            Instant::now(),
        );
        assert!(!state.menu_open);
        assert!(state.command_mode);
        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn command_menu_details_panel_cannot_select_or_run_an_action() {
        let (mut state, root) = overlay_state();
        state.layout = TuiLayout::Mobile;
        state.terminal_width = 48;
        state.terminal_height = 18;
        state.surface = DevSurface::Agent;
        state.open_menu();

        let geometry = super::super::render::command_menu_geometry_for_screen(&state);
        assert!(geometry.details_area.height > 1);
        let first_list_row = geometry.list_inner.y;
        assert_eq!(
            hit_test(&state, geometry.list_inner.x, first_list_row),
            HitRegion::Menu(0)
        );

        // The top border is part of the details panel, and the following row
        // is its body. Neither is an action row in the menu list.
        let details_points = [
            (geometry.details_area.x + 1, geometry.details_area.y),
            (geometry.details_area.x + 1, geometry.details_area.y + 1),
        ];
        for (column, row) in details_points {
            assert_eq!(hit_test(&state, column, row), HitRegion::Other);
        }

        let original_selection = state.menu_selection;
        let mut pointer = PointerState::default();
        for (column, row) in details_points {
            let down = MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                modifiers: crossterm::event::KeyModifiers::empty(),
            };
            let at = Instant::now();
            pointer.handle(&mut state, down, at);
            pointer.handle(
                &mut state,
                MouseEvent {
                    kind: MouseEventKind::Up(MouseButton::Left),
                    ..down
                },
                at + Duration::from_millis(1),
            );
        }
        assert!(state.menu_open);
        assert_eq!(state.menu_selection, original_selection);
        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn clicking_live_browser_action_queues_the_shared_menu_request() {
        let (mut state, root) = overlay_state();
        state.surface = DevSurface::App;
        state.terminal_width = 120;
        state.terminal_height = 40;
        state
            .ws_mut()
            .expect("workspace lock")
            .apply_local_trust_decision(crate::LocalTrustDecision::TrustProject)
            .expect("trust project");
        state.open_menu();
        let action_index = state
            .surface_actions()
            .iter()
            .position(|action| action.label == "Live browser view")
            .expect("live browser menu action");
        let geometry = super::super::render::command_menu_geometry_for_screen(&state);
        let column = geometry.list_inner.x + 1;
        let row = geometry.list_inner.y + action_index as u16;
        assert_eq!(hit_test(&state, column, row), HitRegion::Menu(action_index));

        let mut pointer = PointerState::default();
        let down = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let pressed_at = Instant::now();
        pointer.handle(&mut state, down, pressed_at);
        pointer.handle(
            &mut state,
            MouseEvent {
                kind: MouseEventKind::Up(MouseButton::Left),
                ..down
            },
            pressed_at + Duration::from_millis(1),
        );

        assert!(state.browser_visual_live);
        assert_eq!(
            state
                .take_browser_visual_request()
                .expect("pointer browser visual request")
                .live,
            true
        );
        assert_eq!(
            state.status,
            "Live view starting · screenshot worker will update the pane"
        );
        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn command_menu_hit_testing_rejects_coordinates_outside_surface_pane() {
        let (mut state, root) = overlay_state();
        state.terminal_width = 120;
        state.terminal_height = 18;
        state.surface = DevSurface::Agent;
        state.open_menu();

        let geometry = super::super::render::command_menu_geometry_for_screen(&state);
        assert!(geometry.list_area.x > 0);
        let list_row = geometry.list_inner.y;
        let outside_pane_points = [
            (0, list_row),
            (geometry.list_area.x + geometry.list_area.width, list_row),
        ];
        assert!(outside_pane_points[1].0 < state.terminal_width);
        for (column, row) in outside_pane_points {
            assert_eq!(hit_test(&state, column, row), HitRegion::Other);
        }

        let original_selection = state.menu_selection;
        let mut pointer = PointerState::default();
        for (column, row) in outside_pane_points {
            let down = MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                modifiers: crossterm::event::KeyModifiers::empty(),
            };
            let at = Instant::now();
            pointer.handle(&mut state, down, at);
            pointer.handle(
                &mut state,
                MouseEvent {
                    kind: MouseEventKind::Up(MouseButton::Left),
                    ..down
                },
                at + Duration::from_millis(1),
            );
        }
        assert!(state.menu_open);
        assert_eq!(state.menu_selection, original_selection);
        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn command_menu_hit_testing_tracks_the_rendered_scroll_offset() {
        let (mut state, root) = overlay_state();
        state.layout = TuiLayout::Mobile;
        state.terminal_width = 48;
        state.terminal_height = 16;
        state.surface = DevSurface::Agent;
        state.open_menu();
        let item_count = state.surface_actions().len() + 2;
        state.menu_selection = item_count - 1;

        let geometry = super::super::render::command_menu_geometry_for_screen(&state);
        assert!(geometry.list_inner.height > 0);
        assert!(geometry.scroll_offset > 0);
        assert_eq!(geometry.item_count, item_count);
        assert_eq!(
            hit_test(&state, geometry.list_inner.x, geometry.list_inner.y),
            HitRegion::Menu(geometry.scroll_offset)
        );
        let selected_row = geometry.list_inner.y
            + u16::try_from(state.menu_selection - geometry.scroll_offset).unwrap();
        assert_eq!(
            hit_test(&state, geometry.list_inner.x, selected_row),
            HitRegion::Menu(state.menu_selection)
        );

        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn command_menu_click_runs_after_the_event_loop_poll() {
        let (mut state, root) = overlay_state();
        state.terminal_width = 120;
        state.terminal_height = 40;
        state.surface = DevSurface::Agent;
        state.open_menu();
        let search_index = state.surface_actions().len();
        state.menu_selection = search_index;

        let geometry = super::super::render::command_menu_geometry_for_screen(&state);
        let selected_row =
            geometry.list_inner.y + u16::try_from(search_index - geometry.scroll_offset).unwrap();
        let down = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: geometry.list_inner.x,
            row: selected_row,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        let started = Instant::now();
        let mut pointer = PointerState::default();
        let mut previous = overlay::active_overlay(&state);

        pointer.handle(&mut state, down, started);
        super::super::reset_pointer_on_overlay_transition(&state, &mut pointer, &mut previous);
        pointer.poll(&mut state, started + Duration::from_millis(20));
        super::super::reset_pointer_on_overlay_transition(&state, &mut pointer, &mut previous);
        assert!(state.menu_open);
        assert!(!state.command_mode);

        pointer.handle(
            &mut state,
            MouseEvent {
                kind: MouseEventKind::Up(MouseButton::Left),
                ..down
            },
            started + Duration::from_millis(40),
        );
        assert!(!state.menu_open);
        assert!(state.command_mode);
        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn modal_transition_cancels_a_pending_long_press() {
        let (mut state, root) = overlay_state();
        state.terminal_width = 100;
        state.terminal_height = 40;
        let started = Instant::now();
        let mut pointer = PointerState::default();
        pointer.handle(
            &mut state,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 50,
                row: 20,
                modifiers: crossterm::event::KeyModifiers::empty(),
            },
            started,
        );
        state.file_picker_open = true;
        pointer.poll(&mut state, started + LONG_PRESS);
        assert!(!state.menu_open);
        assert!(matches!(hit_test(&state, 50, 20), HitRegion::Other));
        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn trust_sidebar_row_matches_the_highlighted_prompt() {
        let root = std::env::temp_dir().join(format!("glass-trust-hit-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("glass.toml"),
            "[tools.probe]\ndescription='probe'\ncommand='echo unsafe'\n",
        )
        .unwrap();
        let mut state = DevTuiState::open(&root, TuiLayout::Desktop).unwrap();
        state.terminal_width = 140;
        state.terminal_height = 40;
        assert_eq!(state.surface, DevSurface::Trust);

        assert_eq!(
            hit_test(&state, 2, 3),
            HitRegion::Surface(DevSurface::Trust)
        );
        assert_eq!(
            hit_test(&state, 2, 4),
            HitRegion::Surface(DevSurface::Agent)
        );
        let mut pointer = PointerState::default();
        pointer.handle(
            &mut state,
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 2,
                row: 4,
                modifiers: crossterm::event::KeyModifiers::empty(),
            },
            Instant::now(),
        );
        assert_eq!(state.surface, DevSurface::Trust);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn footer_click_is_the_chat_dock() {
        let root = std::env::temp_dir().join(format!("glass-hit-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut state = DevTuiState::open_for_tui(&root, TuiLayout::Desktop).unwrap();
        state.terminal_width = 140;
        state.terminal_height = 40;
        state
            .ws_mut()
            .unwrap()
            .apply_local_trust_decision(crate::LocalTrustDecision::TrustOnce)
            .unwrap();
        state.agent_readiness = format!(
            "✓ Ready · Node ✓ · SDK {} · auth ✓",
            crate::pi_runtime::PINNED_PI_SDK_VERSION
        );
        assert_eq!(hit_test(&state, 10, 39), HitRegion::Dock);
        assert!(matches!(
            hit_test(&state, 2, 3),
            HitRegion::Surface(DevSurface::Agent)
        ));
        let mut pointer = PointerState::default();
        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 10,
            row: 39,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        pointer.handle(&mut state, mouse, Instant::now());
        pointer.handle(
            &mut state,
            MouseEvent {
                kind: MouseEventKind::Up(MouseButton::Left),
                ..mouse
            },
            Instant::now(),
        );
        assert!(state.composer_mode);
        assert_eq!(state.surface, DevSurface::Agent);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn more_route_click_selects_and_double_click_runs_the_launcher() {
        let root = std::env::temp_dir().join(format!("glass-more-hit-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut state = DevTuiState::open_for_tui(&root, TuiLayout::Desktop).unwrap();
        state.terminal_width = 140;
        state.terminal_height = 40;
        state.surface = DevSurface::More;

        let target = (0..state.terminal_height).find_map(|row| {
            (0..state.terminal_width).find_map(|column| {
                (hit_test(&state, column, row) == HitRegion::MoreRoute(2)).then_some((column, row))
            })
        });
        let (column, row) = target.expect("kernel route should have a mouse target");
        let mut pointer = PointerState::default();
        let at = Instant::now();
        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: crossterm::event::KeyModifiers::empty(),
        };
        pointer.handle(&mut state, mouse, at);
        pointer.handle(
            &mut state,
            MouseEvent {
                kind: MouseEventKind::Up(MouseButton::Left),
                ..mouse
            },
            at + Duration::from_millis(10),
        );
        assert_eq!(state.selected_more, 2);
        assert!(state.status.contains("Route 3/5"));

        pointer.handle(&mut state, mouse, at + Duration::from_millis(100));
        assert!(state.command_mode);
        assert_eq!(state.command_input, "kernel start ");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn every_more_route_is_visible_to_pointer_on_phone_layout() {
        let root =
            std::env::temp_dir().join(format!("glass-more-phone-hit-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut state = DevTuiState::open_for_tui(&root, TuiLayout::Mobile).unwrap();
        state.terminal_width = 48;
        state.terminal_height = 18;
        state.surface = DevSurface::More;

        let mut routes = Vec::new();
        for row in 0..state.terminal_height {
            for column in 0..state.terminal_width {
                if let HitRegion::MoreRoute(index) = hit_test(&state, column, row)
                    && !routes.contains(&index)
                {
                    routes.push(index);
                }
            }
        }
        assert_eq!(
            routes,
            (0..DevTuiState::MORE_ROUTES.len()).collect::<Vec<_>>()
        );

        std::fs::remove_dir_all(root).unwrap();
    }
}
