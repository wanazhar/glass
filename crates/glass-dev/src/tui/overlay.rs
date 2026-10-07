//! Authoritative overlay precedence for TUI input, rendering, and redraws.

use super::state::DevTuiState;

/// The single overlay that owns the current TUI frame and input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum ActiveOverlay {
    QuitConfirmation = 0,
    EditorExitPrompt = 1,
    BrowserDialog = 2,
    Help = 3,
    CommandCenterMenu = 4,
    BrowserTargetPicker = 5,
    FilePicker = 6,
    SessionPicker = 7,
    AgentApproval = 8,
    MutationConfirmation = 9,
    FullscreenEditor = 10,
    PiSlashCommand = 11,
    Composer = 12,
    CommandPalette = 13,
    BrowserRecovery = 14,
}

impl ActiveOverlay {
    /// Whether this overlay covers the surface's full visual area.
    ///
    /// The composer owns input in its dock while leaving the App surface's
    /// browser pane visible behind it.
    pub(super) fn occludes_surface(self) -> bool {
        !matches!(self, Self::Composer)
    }
}

/// Select the highest-priority active overlay, independent of surface state.
pub(super) fn active_overlay(state: &DevTuiState) -> Option<ActiveOverlay> {
    if state.quit_confirmation {
        Some(ActiveOverlay::QuitConfirmation)
    } else if state.editor_exit_prompt.is_some() {
        Some(ActiveOverlay::EditorExitPrompt)
    } else if state.browser_dialog.is_some() {
        Some(ActiveOverlay::BrowserDialog)
    } else if state.help_open {
        Some(ActiveOverlay::Help)
    } else if state.menu_open {
        Some(ActiveOverlay::CommandCenterMenu)
    } else if state.browser_target_picker {
        Some(ActiveOverlay::BrowserTargetPicker)
    } else if state.file_picker_open {
        Some(ActiveOverlay::FilePicker)
    } else if state.session_picker_open {
        Some(ActiveOverlay::SessionPicker)
    } else if state.pending_agent_approval.is_some() {
        Some(ActiveOverlay::AgentApproval)
    } else if state.pending_confirmation.is_some() {
        Some(ActiveOverlay::MutationConfirmation)
    } else if state.code_edit_mode && !state.composer_mode {
        Some(ActiveOverlay::FullscreenEditor)
    } else if state.pi_command_mode {
        Some(ActiveOverlay::PiSlashCommand)
    } else if state.composer_mode {
        Some(ActiveOverlay::Composer)
    } else if state.command_mode {
        Some(ActiveOverlay::CommandPalette)
    } else if state.browser_recovery.is_some() {
        Some(ActiveOverlay::BrowserRecovery)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glass_browser::cli::args::TuiLayout;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn state() -> (DevTuiState, std::path::PathBuf) {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "glass-overlay-priority-{}-{sequence}",
            std::process::id(),
        ));
        std::fs::create_dir_all(&root).expect("create test workspace");
        (
            DevTuiState::open(&root, TuiLayout::Desktop).expect("open TUI state"),
            root,
        )
    }

    #[test]
    fn overlay_resolver_uses_documented_priority_for_conflicting_flags() {
        let (mut state, root) = state();
        state.code_edit_mode = true;
        state.pi_command_mode = true;
        state.file_picker_open = true;
        state.session_picker_open = true;
        state.command_mode = true;
        assert_eq!(active_overlay(&state), Some(ActiveOverlay::FilePicker));

        state.file_picker_open = false;
        assert_eq!(active_overlay(&state), Some(ActiveOverlay::SessionPicker));

        state.session_picker_open = false;
        state.help_open = true;
        state.menu_open = true;
        assert_eq!(active_overlay(&state), Some(ActiveOverlay::Help));

        state.quit_confirmation = true;
        assert_eq!(
            active_overlay(&state),
            Some(ActiveOverlay::QuitConfirmation)
        );
        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn composer_owns_input_over_the_editor_while_editor_exit_prompt_stays_topmost() {
        let (mut state, root) = state();
        state.code_edit_mode = true;
        state.composer_mode = true;
        assert_eq!(active_overlay(&state), Some(ActiveOverlay::Composer));

        state.editor_exit_prompt = Some(super::super::state::EditorExitPrompt::Unsaved);
        assert_eq!(
            active_overlay(&state),
            Some(ActiveOverlay::EditorExitPrompt)
        );
        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn browser_recovery_waits_behind_every_active_overlay() {
        let (mut state, root) = state();
        state.browser_recovery = Some(super::super::state::BrowserRecoveryOffer::from_error(
            "browser endpoint crashed",
            9222,
        ));
        assert_eq!(active_overlay(&state), Some(ActiveOverlay::BrowserRecovery));

        state.command_mode = true;
        assert_eq!(active_overlay(&state), Some(ActiveOverlay::CommandPalette));
        state.command_mode = false;
        state.composer_mode = true;
        assert_eq!(active_overlay(&state), Some(ActiveOverlay::Composer));
        state.composer_mode = false;
        state.pi_command_mode = true;
        assert_eq!(active_overlay(&state), Some(ActiveOverlay::PiSlashCommand));
        state.pi_command_mode = false;
        state.code_edit_mode = true;
        assert_eq!(
            active_overlay(&state),
            Some(ActiveOverlay::FullscreenEditor)
        );
        state.code_edit_mode = false;
        state.editor_exit_prompt = Some(super::super::state::EditorExitPrompt::Unsaved);
        assert_eq!(
            active_overlay(&state),
            Some(ActiveOverlay::EditorExitPrompt)
        );
        state.editor_exit_prompt = None;
        let (call, context) = state
            .tool_request("glass.git.stage", serde_json::json!({}), true)
            .expect("build pending mutation request");
        state.pending_confirmation = Some(super::super::state::PendingConfirmation {
            call,
            context,
            summary: "Stage file".into(),
        });
        assert_eq!(
            active_overlay(&state),
            Some(ActiveOverlay::MutationConfirmation)
        );
        state.pending_confirmation = None;
        state.pending_agent_approval = Some(super::super::state::PendingAgentApproval {
            agent_id: "agent-1".into(),
            frame_id: "frame-1".into(),
            tool_name: "glass.git.stage".into(),
            arguments: serde_json::json!({}),
        });
        assert_eq!(active_overlay(&state), Some(ActiveOverlay::AgentApproval));
        state.pending_agent_approval = None;
        state.session_picker_open = true;
        assert_eq!(active_overlay(&state), Some(ActiveOverlay::SessionPicker));
        state.session_picker_open = false;
        state.file_picker_open = true;
        assert_eq!(active_overlay(&state), Some(ActiveOverlay::FilePicker));
        state.file_picker_open = false;
        state.browser_target_picker = true;
        assert_eq!(
            active_overlay(&state),
            Some(ActiveOverlay::BrowserTargetPicker)
        );
        state.browser_target_picker = false;
        state.menu_open = true;
        assert_eq!(
            active_overlay(&state),
            Some(ActiveOverlay::CommandCenterMenu)
        );
        state.menu_open = false;
        state.browser_dialog = Some(super::super::state::BrowserDialogPrompt {
            pending: glass_browser::browser::NativePendingDialog {
                id: "dialog-1".into(),
                context_id: "context-1".into(),
                frame_id: "frame-1".into(),
                dialog: glass_browser::browser::session::PendingDialog {
                    dialog_type: "confirm".into(),
                    message: "Continue?".into(),
                    default_value: None,
                    url: "https://example.test".into(),
                },
            },
            prompt_input: String::new(),
            prompt_cursor: 0,
            error: None,
        });
        assert_eq!(active_overlay(&state), Some(ActiveOverlay::BrowserDialog));
        state.browser_dialog = None;
        state.help_open = true;
        assert_eq!(active_overlay(&state), Some(ActiveOverlay::Help));
        state.help_open = false;
        state.quit_confirmation = true;
        assert_eq!(
            active_overlay(&state),
            Some(ActiveOverlay::QuitConfirmation)
        );
        drop(state);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }
}
