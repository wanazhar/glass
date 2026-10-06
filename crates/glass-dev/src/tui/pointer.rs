//! Pointer hit-testing. Mouse and terminal-touch call the same reducers as keys.

use super::overlay::{self, ActiveOverlay};
use super::state::{DevSurface, DevTuiState, ResponsiveClass};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
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
    let footer = footer_rows(state);
    match overlay::active_overlay(state) {
        Some(ActiveOverlay::Help) => return HitRegion::Help,
        Some(ActiveOverlay::CommandCenterMenu) => {
            return super::render::command_menu_hit_index(state, column, row)
                .map(HitRegion::Menu)
                .unwrap_or(HitRegion::Other);
        }
        Some(ActiveOverlay::Composer) => {
            return if row >= height.saturating_sub(footer) {
                HitRegion::Dock
            } else {
                HitRegion::Other
            };
        }
        Some(_) => return HitRegion::Other,
        None => {}
    }
    if row >= height.saturating_sub(footer) {
        return HitRegion::Dock;
    }
    if let Some(surface) = navigation_surface_at(
        state.responsive_class(width, height),
        header_rows(state),
        column,
        row,
        state.surface == DevSurface::Trust,
    ) {
        return HitRegion::Surface(surface);
    }
    if let Some(index) = super::render::more_route_at(state, column, row) {
        return HitRegion::MoreRoute(index);
    }
    match state.surface {
        DevSurface::Code if !state.files.is_empty() => {
            let index = list_index(state, row, state.files.len());
            HitRegion::File(index)
        }
        DevSurface::Git if !state.git_entries.is_empty() => {
            let index = list_index(state, row, state.git_entries.len());
            HitRegion::Git(index)
        }
        DevSurface::Terminal if !state.process_entries.is_empty() => {
            let index = list_index(state, row, state.process_entries.len());
            HitRegion::Process(index)
        }
        DevSurface::Debug if !state.debug_sessions.is_empty() => {
            let index = list_index(state, row, state.debug_sessions.len());
            HitRegion::Debug(index)
        }
        DevSurface::App => {
            let count = state.browser_workspace.state().entities.len().max(1);
            HitRegion::Entity(list_index(state, row, count))
        }
        _ => HitRegion::Other,
    }
}

fn header_rows(_state: &DevTuiState) -> u16 {
    2
}

fn footer_rows(state: &DevTuiState) -> u16 {
    super::render::footer_height(state)
}

fn list_index(state: &DevTuiState, row: u16, len: usize) -> usize {
    let start = header_rows(state).saturating_add(1);
    usize::from(row.saturating_sub(start)).min(len.saturating_sub(1))
}

fn navigation_surface_at(
    responsive: ResponsiveClass,
    header_height: u16,
    column: u16,
    row: u16,
    trust_surface: bool,
) -> Option<DevSurface> {
    let nav_width = match responsive {
        ResponsiveClass::Desktop => 24,
        ResponsiveClass::Compact => 22,
        ResponsiveClass::Phone => return None,
    };
    let first_item_row = header_height.saturating_add(1);
    if column >= nav_width || row < first_item_row {
        return None;
    }
    let index = usize::from(row - first_item_row);
    if trust_surface {
        if index == 0 {
            Some(DevSurface::Trust)
        } else {
            DevSurface::PRIMARY.get(index - 1).copied()
        }
    } else {
        DevSurface::PRIMARY.get(index).copied()
    }
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
