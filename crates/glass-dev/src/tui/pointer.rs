//! Pointer hit-testing. Mouse and terminal-touch call the same reducers as keys.

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
    pub fn handle(&mut self, state: &mut DevTuiState, mouse: MouseEvent, now: Instant) -> bool {
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
                let hit = hit_test(state, mouse.column, mouse.row);
                if button == MouseButton::Right {
                    state.open_menu();
                    self.down = None;
                    return true;
                }
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
                if !down.dragged && held >= LONG_PRESS {
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
    if row >= height.saturating_sub(footer) {
        return HitRegion::Dock;
    }
    if state.help_open {
        return HitRegion::Help;
    }
    if state.menu_open {
        let index =
            usize::from(row.saturating_sub(3)).min(state.surface_actions().len().saturating_sub(1));
        return HitRegion::Menu(index);
    }
    if let Some(surface) = navigation_surface_at(
        state.responsive_class(width, height),
        header_rows(state),
        column,
        row,
    ) {
        return HitRegion::Surface(surface);
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

fn footer_rows(_state: &DevTuiState) -> u16 {
    3
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
    DevSurface::PRIMARY
        .into_iter()
        .nth(usize::from(row - first_item_row))
}

fn apply_select(state: &mut DevTuiState, hit: &HitRegion) {
    match hit {
        HitRegion::Surface(surface) => {
            state.surface = *surface;
            state.status = format!("{} selected", surface.label());
        }
        HitRegion::File(index) => {
            state.selected_file = *index;
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
        HitRegion::Other if state.composer_mode => {
            state.close_composer();
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
        HitRegion::Surface(surface) => {
            state.surface = *surface;
            true
        }
        _ => apply_click(state, hit),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glass_browser::cli::args::TuiLayout;

    #[test]
    fn footer_click_is_the_chat_dock() {
        let root = std::env::temp_dir().join(format!("glass-hit-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut state = DevTuiState::open_for_tui(&root, TuiLayout::Desktop).unwrap();
        state.terminal_width = 140;
        state.terminal_height = 40;
        state.snapshot_trust_label = "trusted".into();
        state.agent_readiness = "✓ Ready · Node ✓ · SDK 0.84.3 · auth ✓".into();
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
}
