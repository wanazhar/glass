use super::command;
use crate::{ExperimentComparison, SharedDevelopmentWorkspace};
use glass_browser::browser::policy::PolicyPreset;
use glass_browser::browser_workspace::{
    BrowserConnectionPhase, BrowserWorkspaceAction, BrowserWorkspaceAdapterKind,
    BrowserWorkspaceController, BrowserWorkspaceEntity, BrowserWorkspaceIntent,
    BrowserWorkspaceLayout, BrowserWorkspaceTarget,
};
use glass_browser::cli::args::TuiLayout;
use glass_browser::terminal_graphics::AnsiCanvas;
use glass_browser::tui::live_view::AnsiPane;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_BROWSER_TOOL: AtomicU64 = AtomicU64::new(1);

pub struct PendingConfirmation {
    pub call: crate::development::ToolCall,
    pub context: crate::tools::DevelopmentToolContext,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct PendingAgentApproval {
    pub agent_id: String,
    pub frame_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatMessageState {
    Sending,
    Sent,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingChatMessage {
    pub text: String,
    pub state: ChatMessageState,
    pub job_id: Option<u64>,
    pub error: Option<String>,
}

/// In-TUI recovery choices offered when a browser start collides or fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserRecoveryOffer {
    pub reason: String,
    pub port: u16,
    /// A healthy Chrome CDP endpoint answered on the preferred port.
    pub compatible_endpoint: bool,
}

impl BrowserRecoveryOffer {
    pub fn from_error(error: &str, port: u16) -> Self {
        let lower = error.to_ascii_lowercase();
        Self {
            reason: error.to_string(),
            port,
            compatible_endpoint: ["attach", "devtools", "page target", "multiple page targets"]
                .iter()
                .any(|marker| lower.contains(marker)),
        }
    }

    pub fn actions(&self) -> &'static [(&'static str, &'static str)] {
        if self.compatible_endpoint {
            &[
                ("1", "attach after checking the running browser"),
                ("2", "launch an isolated browser on an automatic free port"),
                ("3", "retry the preferred port"),
                ("Esc", "dismiss"),
            ]
        } else {
            &[
                ("1", "launch an isolated browser on an automatic free port"),
                ("2", "retry the preferred port"),
                ("Esc", "dismiss"),
            ]
        }
    }

    pub fn guidance(&self) -> &'static str {
        let lower = self.reason.to_ascii_lowercase();
        if self.compatible_endpoint {
            "A DevTools endpoint answered. Attach only when its page is the intended browser; otherwise launch an isolated session."
        } else if lower.contains("occup") || lower.contains("address") || lower.contains("port") {
            "The preferred endpoint is busy or unavailable. Glass keeps the project and agent alive while you retry or move to an automatic free port."
        } else {
            "The browser did not become usable. Check Chrome/Chromium availability, then retry; the workspace remains intact."
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DevSurface {
    Trust,
    Agent,
    Code,
    App,
    Terminal,
    Tasks,
    Git,
    Debug,
    More,
}

impl DevSurface {
    pub const ALL: [Self; 8] = [
        Self::Agent,
        Self::Code,
        Self::App,
        Self::Terminal,
        Self::Tasks,
        Self::Git,
        Self::Debug,
        Self::More,
    ];

    pub const PRIMARY: [Self; 8] = [
        Self::Agent,
        Self::Code,
        Self::App,
        Self::Terminal,
        Self::Tasks,
        Self::Git,
        Self::Debug,
        Self::More,
    ];

    pub const PHONE: [Self; 5] = [Self::Agent, Self::Code, Self::App, Self::Tasks, Self::More];

    pub fn label(self) -> &'static str {
        match self {
            Self::Trust => "Trust",
            Self::Agent => "Agent",
            Self::Code => "Code",
            Self::App => "App",
            Self::Terminal => "Terminal",
            Self::Tasks => "Tasks",
            Self::Git => "Git",
            Self::Debug => "Debug",
            Self::More => "More",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponsiveClass {
    Desktop,
    Compact,
    Phone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorExitPrompt {
    Clean,
    Unsaved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductMode {
    Build,
    Agent,
    RunApp,
    Debug,
}

impl ProductMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Agent => "agent",
            Self::RunApp => "run",
            Self::Debug => "debug",
        }
    }
}

pub struct DevTuiState {
    pub workspace: SharedDevelopmentWorkspace,
    pub surface: DevSurface,
    pub layout: TuiLayout,
    /// Process-scoped unrestricted development mode from `glass --yolo`.
    pub yolo_mode: bool,
    pub quit_confirmation: bool,
    pub quit: bool,
    pub command_mode: bool,
    pub command_input: String,
    pub command_cursor: usize,
    pub command_history: Vec<String>,
    pub command_history_index: Option<usize>,
    pub palette_error: Option<String>,
    pub palette_scroll: u16,
    /// Index in the filtered, arrow-selectable palette action list.
    pub palette_selection: usize,
    pub menu_open: bool,
    pub menu_selection: usize,
    pub help_open: bool,
    pub help_scroll: u16,
    pub composer_mode: bool,
    pub composer_input: String,
    pub composer_cursor: usize,
    pub composer_steer: bool,
    pub pending_chat_messages: Vec<PendingChatMessage>,
    pub agent_send_job: Option<u64>,
    pub selected_agent: Option<crate::AgentId>,
    pub pending_confirmation: Option<PendingConfirmation>,
    /// URL retained while the TUI launches a detached browser before navigation.
    pub pending_browser_navigation: Option<String>,
    pub pending_agent_approval: Option<PendingAgentApproval>,
    pub editor_exit_prompt: Option<EditorExitPrompt>,
    pub queued_tool_request: Option<(
        crate::development::ToolCall,
        crate::tools::DevelopmentToolContext,
    )>,
    pub running_tool_job: Option<u64>,
    pub surface_scroll: std::collections::BTreeMap<DevSurface, usize>,
    pub terminal_width: u16,
    pub terminal_height: u16,
    pub status: String,
    pub snapshot_root: String,
    pub snapshot_trust_label: String,
    pub snapshot_trust_inspection: Vec<crate::customization::CustomizationInspectionItem>,
    pub snapshot_project_revision: u64,
    pub snapshot_generation: u64,
    pub snapshot_skills_count: usize,
    pub snapshot_tools_count: usize,
    /// Wall-clock cost of the last background refresh pass.
    pub refresh_latency_ms: u64,
    /// Highest agent event sequence folded into the live conversation.
    pub conversation_cursor: u64,
    /// Index of the editor buffer focused by the Code surface.
    pub editor_buffer_index: usize,
    pub focused_editor_path: String,
    /// Cached editor content used while the shared workspace is busy.
    pub focused_editor_content: String,
    pub focused_editor_dirty: bool,
    pub focused_editor_line: u32,
    pub focused_editor_column: u32,
    pub focused_editor_selection: Option<crate::development::TextSelection>,
    pub editor_comments: Vec<crate::development::EditorComment>,
    pub editor_proposals: Vec<crate::development::EditorProposal>,
    pub editor_checkpoints: Vec<crate::development::EditorCheckpoint>,
    pub agents: String,
    pub agent_readiness: String,
    pub harnesses: String,
    /// Set by an in-TUI login action; the event loop temporarily hands the
    /// terminal to Pi so `/login` can receive interactive input.
    pub agent_login_requested: bool,
    /// Set by a palette action; the event loop hands the terminal to the
    /// selected external harness and resumes Glass after it exits.
    pub harness_launch_requested: Option<String>,
    pub agent_conversation: String,
    pub tasks: String,
    pub editor: String,
    pub files: Vec<String>,
    pub selected_file: usize,
    pub code_edit_mode: bool,
    pub editor_soft_wrap: bool,
    pub editor_scroll_line: usize,
    pub editor_scroll_column: usize,
    pub lsp: String,
    pub processes: String,
    pub git: String,
    pub git_entries: Vec<crate::git::GitStatusEntry>,
    pub github: crate::github::GitHubStatus,
    pub github_review: String,
    pub selected_git_file: usize,
    pub git_diff_path: Option<String>,
    pub git_diff: String,
    pub git_diff_open: bool,
    pub git_diff_requested: bool,
    pub tests: String,
    pub kernels: String,
    pub debugger: String,
    pub replay: String,
    pub browser: String,
    pub browser_detail: String,
    pub browser_workspace: BrowserWorkspaceController,
    pub browser_recovery: Option<BrowserRecoveryOffer>,
    pub browser_target_picker: bool,
    pub browser_target_picker_requested: bool,
    pub browser_target_query: String,
    pub browser_target_selection: usize,
    pub browser_visual_live: bool,
    pub browser_observe_pending: bool,
    pub browser_ansi: AnsiCanvas,
    pub browser_pane: Option<AnsiPane>,
    pub workflow: String,
    pub workspace_status: String,
    pub experiment_comparison: Option<ExperimentComparison>,
    pub experiments: String,
    pub cockpit_url: String,
    pub private_cockpit: Option<crate::development::LocalCockpit>,
}

impl DevTuiState {
    /// Lock the shared workspace, mapping errors to strings for the palette.
    pub fn ws(&self) -> Result<std::sync::MutexGuard<'_, crate::DevelopmentWorkspace>, String> {
        self.workspace.try_lock().map_err(|error| error.to_string())
    }

    /// Lock the shared workspace for mutation, mapping errors to strings.
    pub fn ws_mut(&self) -> Result<std::sync::MutexGuard<'_, crate::DevelopmentWorkspace>, String> {
        self.workspace.try_lock().map_err(|error| error.to_string())
    }

    /// Lock the workspace for mutation inside UI callbacks; reports lock
    /// failures through the status line instead of `?`.
    fn locked<F, T>(&mut self, operation: F) -> Option<T>
    where
        F: FnOnce(&mut crate::DevelopmentWorkspace) -> T,
    {
        match self.workspace.try_lock() {
            Ok(mut workspace) => Some(operation(&mut workspace)),
            Err(error) => {
                self.status = format!("Workspace lock failed: {error}");
                None
            }
        }
    }

    pub fn product_mode(&self) -> ProductMode {
        match self.surface {
            DevSurface::Agent => ProductMode::Agent,
            DevSurface::App => ProductMode::RunApp,
            DevSurface::Debug => ProductMode::Debug,
            _ => ProductMode::Build,
        }
    }
    pub fn open(
        root: impl AsRef<Path>,
        layout: TuiLayout,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::open_internal(root, layout, true, false, PolicyPreset::Development)
    }

    /// Construct the interactive TUI without doing a full synchronous
    /// projection pass. The snapshot worker fills resident projections after
    /// the first frame, so a large repository can show the cockpit immediately.
    pub fn open_for_tui(
        root: impl AsRef<Path>,
        layout: TuiLayout,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::open_internal(root, layout, false, false, PolicyPreset::Development)
    }

    /// Construct the TUI with an explicit process-scoped development mode.
    pub fn open_for_tui_with_mode(
        root: impl AsRef<Path>,
        layout: TuiLayout,
        yolo_mode: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::open_for_tui_with_policy(root, layout, yolo_mode, PolicyPreset::Development)
    }

    /// Construct the TUI with explicit development and browser policies.
    pub fn open_for_tui_with_policy(
        root: impl AsRef<Path>,
        layout: TuiLayout,
        yolo_mode: bool,
        policy_preset: PolicyPreset,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::open_internal(root, layout, false, yolo_mode, policy_preset)
    }

    fn open_internal(
        root: impl AsRef<Path>,
        layout: TuiLayout,
        initial_refresh: bool,
        yolo_mode: bool,
        policy_preset: PolicyPreset,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let workspace = SharedDevelopmentWorkspace::open_with_policy(root, policy_preset)?;
        if yolo_mode {
            let mut workspace = workspace.lock()?;
            workspace.agents().set_default_unrestricted(true);
        }
        let locked = workspace.lock()?;
        let trust = locked.trust();
        let trust_inspection = locked.trust_inspection();
        let snapshot_root = locked.root().display().to_string();
        let snapshot_project_revision = locked.project().revision();
        let snapshot_generation = locked.generation();
        let snapshot_skills_count = locked.customization().skills().count();
        let snapshot_tools_count = locked.customization().config().tools.len();
        let trust_prompt = trust == crate::WorkspaceTrust::Untrusted
            && trust_inspection.iter().any(|item| item.trust_required);
        drop(locked);
        let agent_readiness = crate::pi_runtime::pi_readiness()
            .map(|readiness| format_pi_readiness(&readiness))
            .unwrap_or_else(|error| format!("Agent unavailable · {error}"));
        let initial_status = if trust_prompt {
            "Trust required · I inspect · O untrusted · 1 once · T project".to_string()
        } else if agent_readiness.starts_with("✓ Ready") {
            "Ready · describe a coding task".to_string()
        } else {
            "Pi setup required · press :actions or Enter to continue".to_string()
        };
        let mut state = Self {
            workspace,
            surface: if trust_prompt {
                DevSurface::Trust
            } else {
                DevSurface::Agent
            },
            layout,
            yolo_mode,
            quit: false,
            quit_confirmation: false,
            command_mode: false,
            command_input: String::new(),
            command_cursor: 0,
            command_history: Vec::new(),
            command_history_index: None,
            palette_error: None,
            palette_scroll: 0,
            palette_selection: 0,
            menu_open: false,
            menu_selection: 0,
            help_open: false,
            help_scroll: 0,
            composer_mode: false,
            composer_input: String::new(),
            composer_cursor: 0,
            composer_steer: false,
            pending_chat_messages: Vec::new(),
            agent_send_job: None,
            selected_agent: None,
            pending_confirmation: None,
            editor_exit_prompt: None,
            pending_agent_approval: None,
            pending_browser_navigation: None,
            queued_tool_request: None,
            running_tool_job: None,
            surface_scroll: std::collections::BTreeMap::new(),
            terminal_width: 80,
            terminal_height: 24,
            snapshot_root,
            snapshot_trust_label: trust.label().into(),
            snapshot_trust_inspection: trust_inspection,
            snapshot_project_revision,
            snapshot_generation,
            snapshot_skills_count,
            snapshot_tools_count,
            refresh_latency_ms: 0,
            conversation_cursor: 0,
            editor_buffer_index: 0,
            focused_editor_path: String::new(),
            focused_editor_content: String::new(),
            focused_editor_dirty: false,
            focused_editor_line: 0,
            focused_editor_column: 0,
            focused_editor_selection: None,
            editor_comments: Vec::new(),
            editor_proposals: Vec::new(),
            editor_checkpoints: Vec::new(),
            status: initial_status,
            agents: String::new(),
            agent_readiness,
            harnesses: crate::harness::summary(),
            agent_login_requested: false,
            harness_launch_requested: None,
            agent_conversation:
                "No conversation yet. Press Enter or start typing to compose a message.".into(),
            tasks: String::new(),
            editor: String::new(),
            files: Vec::new(),
            selected_file: 0,
            code_edit_mode: false,
            editor_soft_wrap: false,
            editor_scroll_line: 0,
            editor_scroll_column: 0,
            lsp: String::new(),
            processes: String::new(),
            git: String::new(),
            git_entries: Vec::new(),
            github: crate::github::GitHubStatus::default(),
            github_review: "No GitHub review yet".into(),
            selected_git_file: 0,
            git_diff_path: None,
            git_diff: String::new(),
            git_diff_open: false,
            git_diff_requested: false,
            tests: String::new(),
            kernels: String::new(),
            debugger: String::new(),
            replay: String::new(),
            browser: String::new(),
            browser_detail: "No browser observation yet".into(),
            browser_workspace: BrowserWorkspaceController::for_adapter(
                match layout {
                    TuiLayout::Mobile => BrowserWorkspaceLayout::Phone,
                    TuiLayout::Compact => BrowserWorkspaceLayout::Compact,
                    TuiLayout::Auto | TuiLayout::Desktop => BrowserWorkspaceLayout::Desktop,
                },
                BrowserWorkspaceAdapterKind::EmbeddedDevelopment,
            ),
            browser_recovery: None,
            browser_target_picker: false,
            browser_target_picker_requested: false,
            browser_target_query: String::new(),
            browser_target_selection: 0,
            browser_visual_live: false,
            browser_observe_pending: false,
            browser_ansi: AnsiCanvas::default(),
            browser_pane: None,
            workflow: "No workflow evidence yet".into(),
            workspace_status: String::new(),
            experiment_comparison: None,
            experiments: "No experiments. :experiment create ID BRANCH [PORT]".into(),
            cockpit_url: String::new(),
            private_cockpit: None,
        };
        if initial_refresh {
            state.refresh();
        }
        Ok(state)
    }

    pub fn responsive_class(&self, width: u16, height: u16) -> ResponsiveClass {
        match self.layout {
            TuiLayout::Desktop => ResponsiveClass::Desktop,
            TuiLayout::Compact => ResponsiveClass::Compact,
            TuiLayout::Mobile => ResponsiveClass::Phone,
            TuiLayout::Auto if width < 72 || height < 22 => ResponsiveClass::Phone,
            TuiLayout::Auto if width < 118 || height < 32 => ResponsiveClass::Compact,
            TuiLayout::Auto => ResponsiveClass::Desktop,
        }
    }

    pub fn request_quit(&mut self) {
        if self.quit {
            return;
        }
        self.quit_confirmation = true;
        self.status = "Quit confirmation · Enter exits · Esc stays".into();
    }

    pub fn confirm_quit(&mut self) {
        self.quit_confirmation = false;
        self.quit = true;
        self.status = "Closing Glass Dev".into();
    }

    pub fn cancel_quit(&mut self) {
        self.quit_confirmation = false;
        self.status = "Quit dismissed · workspace remains open".into();
    }

    pub fn quit_menu_index(&self) -> usize {
        self.surface_actions().len() + 1
    }

    /// Guided launchers for the current surface. `:actions` opens the command center.
    pub fn surface_actions(&self) -> &'static [command::SurfaceAction] {
        command::surface_actions(self.surface)
    }

    pub fn toggle_help(&mut self) {
        self.help_open = !self.help_open;
        self.help_scroll = 0;
        self.status = if self.help_open {
            "Help · j/k scroll · ? or Esc closes".into()
        } else {
            "Help closed".into()
        };
    }

    pub fn scroll_help(&mut self, delta: i32) {
        self.help_scroll = (self.help_scroll as i32 + delta).max(0) as u16;
    }

    pub fn open_menu(&mut self) {
        self.menu_open = true;
        self.menu_selection = 0;
        self.status = format!("Command center · {}", self.surface.label());
    }

    pub fn close_menu(&mut self) {
        self.menu_open = false;
    }

    pub fn start_private_cockpit(&mut self) -> Result<String, String> {
        if let Some(cockpit) = self.private_cockpit.as_ref() {
            return Ok(cockpit.local_url());
        }
        let cockpit = crate::development::LocalCockpit::start(self.workspace.clone())
            .map_err(|error| error.to_string())?;
        let url = cockpit.local_url();
        self.cockpit_url = url.clone();
        self.private_cockpit = Some(cockpit);
        self.status = format!("Private cockpit ready · {url}");
        Ok(url)
    }

    pub fn stop_private_cockpit(&mut self) {
        if self.private_cockpit.take().is_some() {
            self.cockpit_url.clear();
            self.status = "Private cockpit stopped".into();
        } else {
            self.status = "Private cockpit is not running".into();
        }
    }

    pub fn private_cockpit_status(&self) -> String {
        self.private_cockpit
            .as_ref()
            .map(|cockpit| format!("running · {}", cockpit.local_url()))
            .unwrap_or_else(|| "not running · use `cockpit start`".into())
    }

    /// Run the selected command-center launcher. Keyboard hints apply
    /// directly; strings starting with `:` open the palette prefilled with
    /// the command.
    pub fn run_menu_action(&mut self) {
        let search_index = self.surface_actions().len();
        let quit_index = self.quit_menu_index();
        if self.menu_selection == search_index {
            self.menu_open = false;
            self.open_palette();
            return;
        }
        if self.menu_selection == quit_index {
            self.menu_open = false;
            self.request_quit();
            return;
        }
        let Some(action) = self.surface_actions().get(self.menu_selection).copied() else {
            return;
        };
        self.menu_open = false;
        let name = action.label;
        let hint = action.command;
        if hint == "browser view" {
            self.surface = DevSurface::App;
            self.browser_visual_live = !self.browser_visual_live;
            self.status = if self.browser_visual_live {
                "Live view starting · command palette can stop it".into()
            } else {
                "Live view off · semantic inspection remains available".into()
            };
            return;
        }
        if hint == "process start dev" {
            self.request_detected_dev();
            return;
        }
        if hint == "git diff" && self.surface == DevSurface::Git {
            self.git_diff_requested = true;
            self.status = "Git diff queued · loading off-thread".into();
            return;
        }
        if self.surface == DevSurface::Trust && matches!(hint, "I" | "O" | "1" | "T") {
            self.handle_printable(hint.chars().next().expect("trust action hint is non-empty"));
            return;
        }
        let prefix = action.key;
        if prefix == ":" {
            // Strip documentation placeholders from the editable command so
            // users can type values immediately instead of backspacing
            // `NAME`, `QUERY`, or `RUN_ID` out of the input line.
            let prefill = hint
                .split_whitespace()
                .take_while(|token| {
                    !token
                        .chars()
                        .all(|character| character.is_ascii_uppercase() || character == '_')
                })
                .collect::<Vec<_>>()
                .join(" ");
            if prefill.is_empty() || prefill == hint {
                self.open_palette_with(hint);
            } else {
                self.open_palette_with(&format!("{prefill} "));
            }
        } else if hint == "i" {
            if self.surface == DevSurface::Agent {
                self.open_composer();
            } else {
                self.enter_code_edit();
            }
        } else if hint == "Enter" {
            if self.surface == DevSurface::Code {
                self.open_selected_file();
            }
        } else if hint == "Ctrl-S" && self.surface == DevSurface::Code {
            self.edit_code_key(
                crossterm::event::KeyCode::Char('s'),
                crossterm::event::KeyModifiers::CONTROL,
            );
        } else {
            self.status = format!("{name} is available from this surface");
        }
    }

    pub fn move_menu_selection(&mut self, delta: i32) {
        let count = self.surface_actions().len() as i32 + 2;
        self.menu_selection =
            (self.menu_selection as i32 + delta).rem_euclid(count.max(1)) as usize;
    }

    pub fn open_palette(&mut self) {
        self.command_mode = true;
        self.command_input.clear();
        self.command_cursor = 0;
        self.command_history_index = None;
        self.palette_error = None;
        self.palette_scroll = 0;
        self.palette_selection = 0;
        self.status = format!(
            "Command search · {} actions · ↑↓ select · Enter run",
            self.surface.label()
        );
    }

    pub fn close_palette(&mut self) {
        self.command_mode = false;
        self.command_input.clear();
        self.command_cursor = 0;
        self.command_history_index = None;
        self.palette_error = None;
        self.palette_scroll = 0;
        self.palette_selection = 0;
        self.status = "Command palette closed · press : for guided launchers".into();
    }

    pub fn palette_action_indices(&self) -> Vec<usize> {
        let query = self.command_input.trim();
        self.surface_actions()
            .iter()
            .enumerate()
            .filter_map(|(index, action)| {
                let haystack =
                    format!("{} {} {}", action.label, action.command, action.description);
                fuzzy_contains(&haystack, query).then_some(index)
            })
            .collect()
    }

    pub fn selected_palette_action(&self) -> Option<command::SurfaceAction> {
        let indices = self.palette_action_indices();
        indices
            .get(self.palette_selection)
            .and_then(|index| self.surface_actions().get(*index))
            .copied()
    }

    pub fn move_palette_selection(&mut self, delta: i32) {
        let count = self.palette_action_indices().len();
        if count == 0 {
            return;
        }
        self.palette_selection =
            (self.palette_selection as i32 + delta).rem_euclid(count as i32) as usize;
        self.palette_error = None;
        self.palette_scroll = 0;
        if let Some(action) = self.selected_palette_action() {
            self.status = format!("{} · Enter runs", action.label);
        }
    }

    pub fn submit_palette(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let typed = self.command_input.trim().to_string();
        let command = match (typed.as_str(), self.selected_palette_action()) {
            ("a" | "actions" | "help" | "?" | "q" | "quit", _) => {
                self.command_mode = false;
                self.command_input.clear();
                self.command_cursor = 0;
                self.command_history_index = None;
                self.palette_scroll = 0;
                self.palette_selection = 0;
                Some(typed)
            }
            (_, Some(action)) => self.prepare_palette_action(action),
            (_, None) if typed.is_empty() => {
                self.status = "No matching palette action · Esc closes".into();
                return;
            }
            (_, None) => {
                self.command_mode = false;
                self.command_input.clear();
                self.command_cursor = 0;
                self.command_history_index = None;
                self.palette_scroll = 0;
                self.palette_selection = 0;
                Some(typed)
            }
        };

        let Some(input) = command else {
            if !self.command_mode {
                self.submit_queued_tool(worker);
                worker.request_refresh();
            }
            return;
        };

        if self.command_history.last() != Some(&input) {
            self.command_history.push(input.clone());
            if self.command_history.len() > 32 {
                self.command_history.remove(0);
            }
        }
        match command::execute(self, &input) {
            Ok(message) => {
                self.palette_error = None;
                self.status = message;
            }
            Err(error) => {
                self.palette_error = Some(error.clone());
                self.status = format!("Error: {error}");
            }
        }
        self.submit_queued_tool(worker);
        worker.request_refresh();
    }

    fn prepare_palette_action(&mut self, action: command::SurfaceAction) -> Option<String> {
        self.palette_error = None;
        match action.command {
            "i" => {
                self.command_mode = false;
                if self.surface == DevSurface::Agent {
                    self.open_composer();
                } else {
                    self.enter_code_edit();
                }
                None
            }
            "Enter" => {
                self.command_mode = false;
                if self.surface == DevSurface::Code {
                    self.open_selected_file();
                }
                None
            }
            "browser navigate URL" => {
                self.surface = DevSurface::App;
                self.open_palette_with("browser navigate ");
                self.status =
                    "Navigate · type a URL or domain (https:// optional), then press Enter".into();
                None
            }
            "browser type TARGET TEXT" => {
                self.surface = DevSurface::App;
                self.open_palette_with("browser type ");
                self.status = "Type · enter the target and text, then press Enter".into();
                None
            }
            "browser view" => {
                self.command_mode = false;
                self.browser_visual_live = !self.browser_visual_live;
                self.status = if self.browser_visual_live {
                    "Live view starting · command palette can stop it".into()
                } else {
                    "Live view off · semantic inspection remains available".into()
                };
                None
            }
            "process start dev"
                if matches!(self.surface, DevSurface::Terminal | DevSurface::More) =>
            {
                self.command_mode = false;
                self.request_detected_dev();
                None
            }
            "git diff" if self.surface == DevSurface::Git => {
                self.command_mode = false;
                self.git_diff_requested = true;
                self.status = "Git diff queued · loading off-thread".into();
                None
            }
            "1" | "T" | "I" | "O" => {
                self.command_mode = false;
                self.handle_printable(action.command.chars().next().unwrap());
                None
            }
            "Ctrl-S" if self.surface == DevSurface::Code => {
                self.command_mode = false;
                self.edit_code_key(
                    crossterm::event::KeyCode::Char('s'),
                    crossterm::event::KeyModifiers::CONTROL,
                );
                None
            }
            command => {
                let prefill = command
                    .split_whitespace()
                    .take_while(|token| {
                        !token
                            .chars()
                            .all(|character| character.is_ascii_uppercase() || character == '_')
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                if prefill != command {
                    self.open_palette_with(&format!("{prefill} "));
                    self.status = format!(
                        "{} · enter only the required value(s), then press Enter",
                        action.label
                    );
                    None
                } else {
                    self.command_mode = false;
                    self.command_input.clear();
                    self.command_cursor = 0;
                    self.command_history_index = None;
                    self.palette_scroll = 0;
                    self.palette_selection = 0;
                    Some(command.into())
                }
            }
        }
    }

    pub fn insert_palette_char(&mut self, character: char) {
        self.command_input.insert(self.command_cursor, character);
        self.command_cursor += character.len_utf8();
        self.palette_error = None;
        self.command_history_index = None;
        self.palette_scroll = 0;
        self.palette_selection = 0;
    }

    pub fn insert_palette_text(&mut self, text: &str) {
        for character in text.replace(['\r', '\n'], " ").chars().take(8_192) {
            self.insert_palette_char(character);
        }
    }

    pub fn palette_backspace(&mut self) {
        if self.command_cursor == 0 {
            return;
        }
        let previous = self.command_input[..self.command_cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.command_input.drain(previous..self.command_cursor);
        self.command_cursor = previous;
        self.command_history_index = None;
        self.palette_scroll = 0;
        self.palette_selection = 0;
    }

    fn palette_history_indices(&self) -> Vec<usize> {
        let visible = command::palette_order(self.surface);
        self.command_history
            .iter()
            .enumerate()
            .filter_map(|(index, input)| {
                let root = input.split_whitespace().next()?;
                let root = match root {
                    "?" => "help",
                    "q" => "quit",
                    "gh" => "github",
                    "tasks" => "task",
                    "tests" => "test",
                    "experiments" => "experiment",
                    "knowledge" => "memory",
                    "surfaces" | "backend" => "surface",
                    "tools" => "tool",
                    _ => root,
                };
                visible.contains(&root).then_some(index)
            })
            .collect()
    }

    pub fn navigate_palette_history(&mut self, older: bool) {
        let history = self.palette_history_indices();
        if history.is_empty() {
            return;
        }
        let position = self
            .command_history_index
            .and_then(|index| history.iter().position(|candidate| *candidate == index));
        let next_position = match (position, older) {
            (None, true) => history.len() - 1,
            (None, false) => return,
            (Some(position), true) => position.saturating_sub(1),
            (Some(position), false) if position + 1 < history.len() => position + 1,
            (Some(_), false) => {
                self.command_history_index = None;
                self.command_input.clear();
                self.command_cursor = 0;
                self.palette_scroll = 0;
                self.palette_selection = 0;
                return;
            }
        };
        let index = history[next_position];
        self.command_history_index = Some(index);
        self.command_input.clone_from(&self.command_history[index]);
        self.command_cursor = self.command_input.len();
        self.palette_scroll = 0;
        self.palette_selection = 0;
    }

    pub fn complete_palette(&mut self) {
        let Some(completion) = self.palette_matches().first().copied() else {
            return;
        };
        let suffix = self
            .command_input
            .find(char::is_whitespace)
            .map(|index| self.command_input[index..].to_string())
            .unwrap_or_default();
        self.command_input = format!("{completion}{suffix}");
        self.command_cursor = self.command_input.len();
        self.command_history_index = None;
        self.palette_scroll = 0;
        self.palette_selection = 0;
        self.status = format!("Completed `{completion}` · Enter runs · Ctrl-P/N history");
    }

    pub fn open_palette_with(&mut self, prefix: &str) {
        self.open_palette();
        self.command_input = prefix.into();
        self.command_cursor = self.command_input.len();
    }

    pub fn move_palette_cursor(&mut self, right: bool) {
        if right {
            self.command_cursor = self.command_input[self.command_cursor..]
                .char_indices()
                .nth(1)
                .map(|(offset, _)| self.command_cursor + offset)
                .unwrap_or(self.command_input.len());
        } else if self.command_cursor > 0 {
            self.command_cursor = self.command_input[..self.command_cursor]
                .char_indices()
                .next_back()
                .map(|(index, _)| index)
                .unwrap_or(0);
        }
    }

    pub fn scroll_palette(&mut self, delta: i32) {
        self.move_palette_selection(delta.saturating_mul(5));
    }

    pub fn palette_matches(&self) -> Vec<&'static str> {
        let query = self
            .command_input
            .split_whitespace()
            .next()
            .unwrap_or_default();
        command::palette_order(self.surface)
            .into_iter()
            .filter(|candidate| fuzzy_contains(candidate, query))
            .take(6)
            .collect()
    }

    pub fn background_action_running(&self) -> bool {
        self.pending_confirmation.is_some()
            || self.pending_agent_approval.is_some()
            || self.running_tool_job.is_some()
            || self.queued_tool_request.is_some()
            || self.agent_send_job.is_some()
    }

    pub fn agent_browser_context(&self) -> serde_json::Value {
        let semantic_summary = self.browser_workspace_summary();
        let state = self.browser_workspace.state();
        let selected_entity = state.selected().map(|entity| {
            serde_json::json!({
                "reference": entity.reference,
                "role": entity.role,
                "name": entity.name,
                "actionable": entity.actionable,
                "revision": entity.revision,
            })
        });
        let selected_target = state
            .targets
            .iter()
            .find(|target| target.selected)
            .map(|target| {
                serde_json::json!({
                    "id": target.id,
                    "title": target.title,
                    "url": safe_browser_url(&target.url),
                })
            });
        serde_json::json!({
            "schemaVersion": "glass.agent-context.v1",
            "browser": {
                "attached": state.connection == glass_browser::browser_workspace::BrowserConnectionPhase::Connected,
                "connection": serde_json::to_value(state.connection).unwrap_or(serde_json::Value::Null),
                "targetId": selected_target.as_ref().and_then(|target| target.get("id")).cloned(),
                "origin": page_origin(&state.url),
                "url": safe_browser_url(&state.url),
                "title": state.title,
                "browserRevision": state.browser_revision,
                "semanticSummary": semantic_summary,
                "selectedEntity": selected_entity,
                "workflowState": state.workflow,
                "inputOwner": serde_json::to_value(state.input_owner).unwrap_or(serde_json::Value::Null),
                "freshness": if state.semantic_invalidated { "stale" } else { "current" },
                "target": selected_target,
                "bootstrap": if state.connection == glass_browser::browser_workspace::BrowserConnectionPhase::Connected {
                    "attached; observe before acting"
                } else {
                    "detached; call glass.browser.start, then navigate and observe the requested URL"
                },
            },
            "authority": {
                "source": "Glass browser workspace",
                "mutationLeaseRequired": selected_entity.as_ref().is_some_and(|entity| {
                    entity.get("actionable").and_then(serde_json::Value::as_bool).unwrap_or(false)
                }),
            },
            "project": {
                "root": self.snapshot_root,
            },
        })
    }
    /// Snapshot the focused editor buffer for the shared workspace chat.
    ///
    /// Chat must see the same unsaved text the human sees, not only the
    /// on-disk file. Keep the attachment bounded; the agent can request the
    /// remainder through its file tools.
    pub fn agent_editor_context(&self) -> serde_json::Value {
        let buffer = self.focused_buffer();
        let focused = buffer.as_ref().map(|buffer| {
            let (content, truncated) = bounded_editor_content(&buffer.content);
            serde_json::json!({
                "path": buffer.path,
                "cursor": {
                    "line": buffer.cursor_line,
                    "column": buffer.cursor_column,
                },
                "selection": buffer.selection,
                "dirty": buffer.dirty,
                "actor": buffer.actor,
                "content": content,
                "contentTruncated": truncated,
            })
        });
        let project_revision = self
            .workspace
            .try_lock()
            .ok()
            .map(|workspace| workspace.project().revision());
        let comments = self
            .editor_comments
            .iter()
            .filter(|comment| {
                self.focused_editor_path.is_empty() || comment.path == self.focused_editor_path
            })
            .cloned()
            .collect::<Vec<_>>();
        let proposals = self
            .editor_proposals
            .iter()
            .filter(|proposal| {
                self.focused_editor_path.is_empty() || proposal.path == self.focused_editor_path
            })
            .cloned()
            .collect::<Vec<_>>();
        serde_json::json!({
            "focusedPath": self.focused_editor_path,
            "cursor": {
                "line": self.focused_editor_line,
                "column": self.focused_editor_column,
            },
            "dirty": self.focused_editor_dirty,
            "projectRevision": project_revision,
            "focusedBuffer": focused,
            "comments": comments,
            "proposals": proposals,
        })
    }

    pub fn browser_chat_header(&self) -> String {
        let browser = self.browser_workspace.state();
        let page = if browser.url.is_empty() {
            "no page".to_string()
        } else {
            safe_browser_url(&browser.url).unwrap_or_else(|| "no page".into())
        };
        let title = browser.title.trim();
        let include_title = !title.is_empty() && !title.eq_ignore_ascii_case(&page);
        let revision = browser
            .browser_revision
            .map_or_else(|| "—".to_string(), |revision| revision.to_string());
        let bootstrap = if browser.connection
            == glass_browser::browser_workspace::BrowserConnectionPhase::Connected
        {
            "observe before acting"
        } else {
            "ask `open <url>` to attach"
        };
        let mut header = format!("APP · {}", browser.connection_label());
        if include_title {
            header.push_str(" · ");
            header.push_str(title);
        }
        header.push_str(&format!(" · {} · rev {} · {}", page, revision, bootstrap));
        if let Some(entity) = browser.selected() {
            header.push_str(" · selected ");
            header.push_str(&entity.name);
        }
        header
    }

    pub fn conversation_view(&self) -> String {
        let mut conversation = if self.agent_conversation.starts_with("No conversation yet.")
            && !self.pending_chat_messages.is_empty()
        {
            String::new()
        } else {
            self.agent_conversation.clone()
        };
        let mut observed = std::collections::BTreeMap::<String, usize>::new();
        for message in &self.pending_chat_messages {
            if message.state != ChatMessageState::Failed {
                let marker = format!("YOU\n{}", message.text);
                let seen = observed.entry(message.text.clone()).or_default();
                if *seen < conversation.matches(&marker).count() {
                    *seen += 1;
                    continue;
                }
            }
            if !conversation.is_empty() {
                conversation.push_str("\n\n");
            }
            conversation.push_str("YOU\n");
            conversation.push_str(&message.text);
            conversation.push('\n');
            match message.state {
                ChatMessageState::Sending => conversation.push_str("· sending…"),
                ChatMessageState::Sent => {
                    conversation.push_str("· sent · Glass Agent is thinking…")
                }
                ChatMessageState::Failed => {
                    conversation.push_str("× send failed");
                    if let Some(error) = message.error.as_deref() {
                        conversation.push_str(": ");
                        conversation.push_str(error);
                    }
                    conversation.push_str(" · press Enter to retry");
                }
            }
        }
        conversation
    }

    fn reconcile_pending_chat(&mut self) {
        let conversation = self.agent_conversation.clone();
        let mut confirmed = std::collections::BTreeMap::<String, usize>::new();
        self.pending_chat_messages.retain(|message| {
            if message.state == ChatMessageState::Failed {
                return true;
            }
            let marker = format!("YOU\n{}", message.text);
            let observed = conversation.matches(&marker).count();
            let seen = confirmed.entry(message.text.clone()).or_default();
            if *seen < observed {
                *seen += 1;
                false
            } else {
                true
            }
        });
    }
    /// Start the shortest in-TUI path to a usable agent conversation.
    pub fn start_agent_interaction(&mut self) {
        if self.snapshot_trust_label == "untrusted" {
            self.surface = DevSurface::Trust;
            self.status = "Trust this workspace before starting the Glass Agent · T or 1".into();
            return;
        }
        match crate::pi_runtime::pi_readiness() {
            Ok(readiness) if readiness.ready => self.open_composer(),
            Ok(readiness)
                if readiness.node.state != crate::pi_runtime::PiReadinessState::Ready
                    || readiness.sdk.state != crate::pi_runtime::PiReadinessState::Ready =>
            {
                self.request_agent_setup();
            }
            Ok(_) => {
                let _ = self.request_agent_login();
            }
            Err(error) => {
                self.status = format!("Pi readiness unavailable · use :agent setup: {error}");
            }
        }
    }

    /// Select a resident agent without forcing the user through the command
    /// palette. The selected agent becomes the target for follow-up and
    /// session-control actions.
    pub fn cycle_agent_selection(&mut self, delta: i32) {
        let agents = match self.workspace.try_lock() {
            Ok(mut workspace) => match workspace.agents().list() {
                Ok(agents) => agents,
                Err(error) => {
                    self.status = format!("Agent list unavailable: {error}");
                    return;
                }
            },
            Err(error) => {
                self.status = format!("Agent list unavailable: {error}");
                return;
            }
        };
        if agents.is_empty() {
            self.selected_agent = None;
            self.status = "No resident agents · Enter starts the Glass Agent".into();
            return;
        }
        let current = self
            .selected_agent
            .as_ref()
            .and_then(|selected| agents.iter().position(|agent| &agent.id == selected))
            .unwrap_or(0);
        let next = if delta.is_negative() {
            (current + agents.len() - (delta.unsigned_abs() as usize % agents.len())) % agents.len()
        } else {
            (current + (delta as usize % agents.len())) % agents.len()
        };
        let selected = agents[next].id.clone();
        self.selected_agent = Some(selected.clone());
        self.status = format!(
            "Selected {} · Enter chats · ]/[ switches agent",
            selected.as_str()
        );
    }

    pub fn open_composer(&mut self) {
        self.composer_mode = true;
        self.composer_cursor = self.composer_input.len();
        self.status = "Agent composer · Enter sends · draft stays open · Esc cancels".into();
    }
    /// Move from the native editor into the shared agent conversation with
    /// the focused buffer attached as unsaved, bounded context.
    pub fn prepare_editor_agent_prompt(&mut self) {
        let Some(buffer) = self.focused_buffer() else {
            self.status = "No open buffer · open a file before asking Pi".into();
            return;
        };
        let line = buffer
            .content
            .lines()
            .nth(buffer.cursor_line.saturating_sub(1) as usize)
            .unwrap_or_default()
            .trim();
        let line_preview = line.chars().take(240).collect::<String>();
        self.composer_input = format!(
            "Help me with {}:{}:{}.\nInspect the attached editor buffer and explain the safest minimal change. Current line: `{line_preview}`. Do not edit files until I approve a concrete proposal.",
            buffer.path, buffer.cursor_line, buffer.cursor_column
        );
        self.composer_cursor = self.composer_input.len();
        self.composer_steer = false;
        self.code_edit_mode = false;
        self.editor_exit_prompt = None;
        self.surface = DevSurface::Agent;
        self.open_composer();
        self.status = format!(
            "Editor context attached · {}:{} · review the prompt, then press Enter",
            buffer.path, buffer.cursor_line
        );
    }

    /// Prepare an editable review prompt using the current workspace evidence.
    pub fn prepare_review_prompt(&mut self) {
        self.composer_input = "Review the current workspace changes. Inspect the Git diff, changed files, diagnostics, and latest test results. Report concrete correctness, security, regression, and missing-test risks. Do not edit files until I approve a fix.".into();
        self.composer_cursor = self.composer_input.len();
        self.composer_steer = false;
        self.open_composer();
        self.status =
            "Review prompt ready · edit it, then press Enter to ask the Glass Agent".into();
    }

    pub fn toggle_composer_steer(&mut self) {
        self.composer_steer = !self.composer_steer;
        self.status = if self.composer_steer {
            "Steer mode · Enter interrupts the running agent".into()
        } else {
            "Follow-up mode · Enter sends during or after the current turn".into()
        };
    }

    /// Queue the managed Pi SDK install behind the same one-use confirmation
    /// sheet used by every other mutating TUI action.
    pub fn request_agent_setup(&mut self) {
        self.request_agent_setup_mode(false);
    }

    /// Queue a forced reinstall of the pinned managed Pi SDK.
    pub fn request_agent_update(&mut self) {
        self.request_agent_setup_mode(true);
    }

    fn request_agent_setup_mode(&mut self, update: bool) {
        if self.background_action_running() {
            self.status = "Another background action is still running".into();
            return;
        }
        let (call, context) = match self.tool_request(
            "glass.agent.setup",
            serde_json::json!({"login": false, "update": update}),
            true,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.status = if update {
                    format!("Pi update unavailable: {error}")
                } else {
                    format!("Pi setup unavailable: {error}")
                };
                return;
            }
        };
        let summary = if update {
            "Refresh the pinned managed Pi SDK"
        } else {
            "Install or repair the pinned managed Pi SDK"
        };
        let queued = match self.queue_or_confirm(call, context, summary.into()) {
            Ok(queued) => queued,
            Err(error) => {
                self.status = format!("Could not queue Pi setup: {error}");
                return;
            }
        };
        self.surface = DevSurface::Agent;
        if !queued {
            self.status = if update {
                "Pi update ready · Enter approves once · Esc cancels".into()
            } else {
                "Pi setup ready · Enter approves once · Esc cancels".into()
            };
        }
    }

    /// Request an interactive Pi login; the outer TUI loop performs the
    /// terminal handoff because the login program must own stdin.
    pub fn request_agent_login(&mut self) -> Result<(), String> {
        if self.background_action_running() {
            let error = "Finish the current background action before signing in".to_string();
            self.status = error.clone();
            return Err(error);
        }
        self.agent_login_requested = true;
        self.surface = DevSurface::Agent;
        self.status = "Pi login will open in this terminal · exit Pi to return".into();
        Ok(())
    }

    pub fn close_composer(&mut self) {
        self.composer_mode = false;
        self.composer_steer = false;
        self.status = "Composer closed".into();
    }

    pub fn move_composer_cursor(&mut self, right: bool) {
        if right {
            self.composer_cursor = self.composer_input[self.composer_cursor..]
                .char_indices()
                .nth(1)
                .map(|(offset, _)| self.composer_cursor + offset)
                .unwrap_or(self.composer_input.len());
        } else if self.composer_cursor > 0 {
            self.composer_cursor = self.composer_input[..self.composer_cursor]
                .char_indices()
                .next_back()
                .map(|(index, _)| index)
                .unwrap_or(0);
        }
    }

    pub fn delete_composer_word(&mut self) {
        let before = self.composer_input[..self.composer_cursor].trim_end();
        let start = before.len();
        let target = before[..start]
            .trim_end_matches(|c: char| c.is_whitespace())
            .char_indices()
            .rev()
            .find_map(|(index, character)| character.is_whitespace().then_some(index))
            .map(|index| before[..index].trim_end().len())
            .unwrap_or(0);
        self.composer_input.drain(target..self.composer_cursor);
        self.composer_cursor = target;
    }

    pub fn abort_selected_agent(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(agent) = self.selected_agent.clone() else {
            self.status = "No active agent to abort".into();
            return;
        };
        let (call, context) = match self.tool_request(
            "glass.agent.abort",
            serde_json::json!({"agentId": agent.as_str()}),
            true,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.status = format!("Abort unavailable: {error}");
                return;
            }
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = format!("Stopping {} in background…", agent.as_str());
            }
            Err(error) => self.status = format!("Abort unavailable: {error}"),
        }
    }

    pub fn insert_composer_text(&mut self, text: &str) {
        for character in text.chars().take(16_384) {
            self.composer_input.insert(self.composer_cursor, character);
            self.composer_cursor += character.len_utf8();
        }
    }

    pub fn composer_backspace(&mut self) {
        if self.composer_cursor == 0 {
            return;
        }
        let previous = self.composer_input[..self.composer_cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.composer_input.drain(previous..self.composer_cursor);
        self.composer_cursor = previous;
    }

    pub fn submit_composer(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        if self.background_action_running() {
            self.status = if self.agent_send_job.is_some() {
                "Sending the previous message · keep typing, then press Enter again".into()
            } else {
                "Background operation running · message kept in composer".into()
            };
            return;
        }
        if self.snapshot_trust_label == "untrusted" {
            self.composer_mode = false;
            self.surface = DevSurface::Trust;
            self.status = "Trust this workspace before starting the Glass Agent · T or 1".into();
            return;
        }
        if !self.agent_readiness.starts_with("✓ Ready") {
            self.status = "Pi is not ready · use :agent setup or :agent setup login".into();
            return;
        }
        if self.composer_input.trim().is_empty() {
            self.status = "Message is empty · type a prompt, then press Enter".into();
            return;
        }
        let display_text = self.composer_input.clone();
        let text = std::mem::take(&mut self.composer_input);
        let steer = self.composer_steer;
        self.composer_cursor = 0;
        self.composer_steer = false;
        self.composer_mode = true;
        let mut context = self.agent_browser_context();
        context["editor"] = self.agent_editor_context();
        if context["browser"]["selectedEntity"].is_object() {
            self.browser_workspace.state_mut().input_owner =
                glass_browser::browser_workspace::BrowserInputOwner::Agent;
        }
        let mut arguments = serde_json::json!({
            "text": text,
            "mode": if steer { "steer" } else { "follow-up" },
            "context": context,
        });
        if let Some(agent) = self.selected_agent.as_ref() {
            arguments["agentId"] = serde_json::Value::String(agent.as_str().to_string());
        }
        let (call, context) = match self.tool_request("glass.agent.send", arguments, true) {
            Ok(request) => request,
            Err(error) => {
                self.composer_input = display_text;
                self.composer_cursor = self.composer_input.len();
                self.status = format!("Message unavailable · edit and retry: {error}");
                return;
            }
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.agent_send_job = Some(id);
                self.pending_chat_messages.push(PendingChatMessage {
                    text: display_text,
                    state: ChatMessageState::Sending,
                    job_id: Some(id),
                    error: None,
                });
                self.status = if steer {
                    "Sent · steering Glass Agent…".into()
                } else {
                    "Sent · Glass Agent is thinking…".into()
                };
                worker.request_conversation();
            }
            Err(error) => {
                self.composer_input = display_text;
                self.composer_cursor = self.composer_input.len();
                self.status = format!("Message unavailable · edit and retry: {error}");
            }
        }
    }

    pub fn approve_confirmation_async(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(pending) = self.pending_confirmation.take() else {
            return;
        };
        match worker.submit_tool(pending.call, pending.context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = format!(
                    "Running {} · Esc keeps the workspace responsive",
                    pending.summary
                );
            }
            Err(error) => self.status = format!("Could not queue mutation: {error}"),
        }
    }

    pub fn resolve_agent_approval(
        &mut self,
        approved: bool,
        worker: &mut super::snapshot::SnapshotWorker,
    ) {
        let Some(pending) = self.pending_agent_approval.take() else {
            return;
        };
        let arguments = serde_json::json!({
            "agentId": pending.agent_id,
            "frameId": pending.frame_id,
            "approved": approved,
        });
        let (call, context) = match self.tool_request("glass.agent.approve", arguments, true) {
            Ok(request) => request,
            Err(error) => {
                self.pending_agent_approval = Some(pending);
                self.status = format!("Agent approval unavailable: {error}");
                return;
            }
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = if approved {
                    format!("Approved {} · Glass Agent continues…", pending.tool_name)
                } else {
                    format!("Denied {} · Glass Agent continues…", pending.tool_name)
                };
            }
            Err(error) => {
                self.pending_agent_approval = Some(pending);
                self.status = format!("Could not queue agent approval: {error}");
            }
        }
    }

    pub fn apply_visual_job_result(&mut self, result: super::snapshot::VisualJobResult) {
        self.apply_visual_job_result_with_fit(
            result,
            glass_browser::terminal_graphics::FrameFit::Contain,
        );
    }

    pub fn apply_visual_job_result_with_fit(
        &mut self,
        result: super::snapshot::VisualJobResult,
        fit: glass_browser::terminal_graphics::FrameFit,
    ) {
        let png = match result.result {
            Ok(value) => {
                let Some(encoded) = value.get("base64").and_then(serde_json::Value::as_str) else {
                    self.browser_visual_live = false;
                    self.browser_workspace.state_mut().presentation_reason =
                        Some("screenshot payload did not contain base64 PNG data".into());
                    self.status = "Live view unavailable · screenshot payload was empty".into();
                    return;
                };
                use base64::Engine as _;
                match base64::engine::general_purpose::STANDARD.decode(encoded) {
                    Ok(png) => png,
                    Err(error) => {
                        self.browser_visual_live = false;
                        self.browser_workspace.state_mut().presentation_reason =
                            Some(format!("screenshot payload was not valid base64: {error}"));
                        self.status = "Live view unavailable · invalid screenshot payload".into();
                        return;
                    }
                }
            }
            Err(error) => {
                self.browser_visual_live = false;
                self.browser_workspace.state_mut().presentation_reason =
                    Some(format!("browser screenshot failed: {error}"));
                self.status = format!("Live view unavailable · {error}");
                return;
            }
        };
        match AnsiPane::from_png(
            &mut self.browser_ansi,
            &png,
            result.columns.clamp(8, 80),
            result.rows.clamp(4, 40),
            fit,
        ) {
            Ok(pane) => {
                self.browser_pane = Some(pane);
                let browser = self.browser_workspace.state_mut();
                browser.presentation =
                    glass_browser::browser_workspace::BrowserPresentationPath::Ansi;
                browser.frame_revision = browser.browser_revision;
                browser.presentation_reason = Some(
                    "bounded ANSI half-block frame · semantic controls remain authoritative".into(),
                );
                self.status = "Live view updated · ANSI half-block".into();
            }
            Err(error) => {
                self.browser_visual_live = false;
                self.browser_workspace.state_mut().presentation_reason = Some(error.to_string());
                self.status = format!("Live view unavailable: {error}");
            }
        }
    }

    pub fn queue_tool_request(
        &mut self,
        call: crate::development::ToolCall,
        context: crate::tools::DevelopmentToolContext,
    ) -> Result<(), String> {
        if self.background_action_running() {
            return Err("another background tool is already running".into());
        }
        self.queued_tool_request = Some((call, context));
        Ok(())
    }
    pub(super) fn queue_or_confirm(
        &mut self,
        call: crate::development::ToolCall,
        context: crate::tools::DevelopmentToolContext,
        summary: String,
    ) -> Result<bool, String> {
        if self.yolo_mode {
            self.queue_tool_request(call, context)?;
            self.status = format!("YOLO · {summary} queued");
            Ok(true)
        } else {
            self.pending_confirmation = Some(PendingConfirmation {
                call,
                context,
                summary,
            });
            Ok(false)
        }
    }

    pub fn submit_queued_tool(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some((call, context)) = self.queued_tool_request.take() else {
            return;
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status.push_str(" · running in background");
            }
            Err(error) => self.status = format!("Could not queue tool: {error}"),
        }
    }

    pub fn apply_tool_job_result(&mut self, result: super::snapshot::ToolJobResult) {
        if result.tool == "glass.agent.send" && self.agent_send_job == Some(result.id) {
            self.agent_send_job = None;
            match result.result {
                Ok(value) => {
                    let agent = value
                        .get("agentId")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|value| crate::AgentId::parse(value).ok());
                    let restarted = value
                        .get("restarted")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    if let Some(agent_id) = agent {
                        self.selected_agent = Some(agent_id.clone());
                        self.status = if restarted {
                            format!("Restarted {} · Glass Agent is thinking…", agent_id.as_str())
                        } else {
                            format!("Sent to {} · Glass Agent is thinking…", agent_id.as_str())
                        };
                    } else {
                        self.status = if restarted {
                            "Restarted Glass Agent · thinking…".into()
                        } else {
                            "Sent · Glass Agent is thinking…".into()
                        };
                    }
                    if let Some(message) = self
                        .pending_chat_messages
                        .iter_mut()
                        .find(|message| message.job_id == Some(result.id))
                    {
                        message.state = ChatMessageState::Sent;
                        message.job_id = None;
                    }
                }
                Err(error) => {
                    let failed_text = self
                        .pending_chat_messages
                        .iter_mut()
                        .find(|message| message.job_id == Some(result.id))
                        .map(|message| {
                            message.state = ChatMessageState::Failed;
                            message.job_id = None;
                            message.error = Some(error.clone());
                            message.text.clone()
                        });
                    if self.composer_input.is_empty()
                        && let Some(text) = failed_text
                    {
                        self.composer_input = text;
                        self.composer_cursor = self.composer_input.len();
                    }
                    self.composer_mode = true;
                    self.status = format!("Could not send message · edit and retry: {error}");
                }
            }
            return;
        }

        if self.running_tool_job != Some(result.id) {
            return;
        }
        self.running_tool_job = None;
        match result.result {
            Ok(value) => {
                if result.tool.starts_with("glass.browser") {
                    self.apply_browser_result(&result.tool, &value);
                    self.browser_detail = super::projection::browser_result(&result.tool, &value);
                    if result.tool == "glass.browser.act" || result.tool == "glass.browser.navigate"
                    {
                        self.browser_observe_pending = true;
                    }
                } else if result.tool == "glass.git.diff" {
                    let empty = value.as_str().is_none_or(|diff| diff.trim().is_empty());
                    self.git_diff = if empty {
                        self.git_diff_path
                            .as_deref()
                            .map(|path| format!("{path}\n\nNo tracked diff for this file."))
                            .unwrap_or_else(|| "Working tree clean · no diff to show".into())
                    } else {
                        value.as_str().unwrap_or_default().to_string()
                    };
                    self.git_diff_open = true;
                    self.status = self
                        .git_diff_path
                        .as_deref()
                        .map(|path| format!("Git diff · {path} · PgUp/PgDn scroll · Esc closes"))
                        .unwrap_or_else(|| "Git diff · PgUp/PgDn scroll · Esc closes".into());
                } else if result.tool == "glass.agent.setup" {
                    match self.refresh_agent_readiness() {
                        Ok(true) => {
                            self.open_composer();
                            self.status =
                                "Pi runtime ready · type a prompt, then press Enter".into();
                        }
                        Ok(false) => {
                            self.status =
                                "Pi runtime installed · use :agent setup login, then Enter to chat"
                                    .into();
                        }
                        Err(error) => self.status = format!("Pi readiness check failed: {error}"),
                    }
                } else if result.tool == "glass.agent.send" {
                    if let Some(agent_id) = value
                        .get("agentId")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|value| crate::AgentId::parse(value).ok())
                    {
                        self.selected_agent = Some(agent_id.clone());
                        self.status =
                            format!("Sent to {} · Glass Agent is thinking…", agent_id.as_str());
                    } else {
                        self.status = "Sent · Glass Agent is thinking…".into();
                    }
                } else if result.tool == "glass.agent.delegate" {
                    self.surface = DevSurface::Agent;
                    let harness = value
                        .get("harness")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("external harness");
                    let outcome = if value.get("success").and_then(serde_json::Value::as_bool)
                        == Some(true)
                    {
                        "completed"
                    } else if value.get("timedOut").and_then(serde_json::Value::as_bool)
                        == Some(true)
                    {
                        "timed out"
                    } else {
                        "failed"
                    };
                    let detail = super::projection::external_agent_output(&value);
                    self.status = format!("Temporary {harness} {outcome} · {detail}");
                } else if result.tool == "glass.github.review" {
                    match serde_json::from_value::<crate::github::GitHubReview>(value) {
                        Ok(review) => {
                            self.github = crate::github::GitHubStatus {
                                repository: review.repository.clone(),
                                availability: review.availability,
                            };
                            self.github_review = review.display();
                            self.surface = DevSurface::Git;
                            self.status = "GitHub review refreshed".into();
                        }
                        Err(error) => {
                            self.status = format!("GitHub review response invalid: {error}");
                        }
                    }
                } else if result.tool == "glass.github.ship" {
                    self.surface = DevSurface::Git;
                    self.status = value
                        .get("url")
                        .and_then(serde_json::Value::as_str)
                        .map(|url| format!("Pull request created · {url}"))
                        .unwrap_or_else(|| "Pull request created · review GitHub status".into());
                } else if result.tool.starts_with("glass.lsp") {
                    self.editor = if result.tool == "glass.lsp.diagnostics" {
                        super::projection::lsp(Some(&value))
                    } else {
                        super::projection::first_meaningful(&value)
                    };
                }
                if !matches!(
                    result.tool.as_str(),
                    "glass.agent.send"
                        | "glass.agent.setup"
                        | "glass.agent.delegate"
                        | "glass.git.diff"
                        | "glass.github.review"
                        | "glass.github.ship"
                ) {
                    self.status = format!("Completed {} · workspace refreshed", result.tool);
                }
            }
            Err(error) => {
                if result.tool.starts_with("glass.browser") {
                    self.note_browser_failure(&result.tool, &error);
                }
                if result.tool == "glass.git.diff" {
                    self.git_diff = format!("Git diff unavailable: {error}");
                    self.git_diff_open = true;
                }
                self.status = format!("{} failed: {error}", result.tool);
            }
        }
    }

    pub fn deny_confirmation(&mut self) {
        self.pending_browser_navigation = None;
        if let Some(pending) = self.pending_confirmation.take() {
            self.status = format!("Denied · {}", pending.summary);
        }
    }

    /// Queue browser startup or a fresh observation as part of a requested navigation.
    pub fn prepare_browser_navigation(&mut self, url: &str) -> Result<String, String> {
        if self.background_action_running() {
            return Err("another browser action is already awaiting or running".into());
        }
        if self.browser_workspace.state().connection == BrowserConnectionPhase::Connected {
            let (call, context) =
                self.tool_request("glass.browser.observe", serde_json::json!({}), false)?;
            self.pending_browser_navigation = Some(url.to_string());
            self.queued_tool_request = Some((call, context));
            self.surface = DevSurface::App;
            return Ok("Browser connected · refreshing page before navigation".into());
        }
        let (call, context) =
            self.tool_request("glass.browser.start", serde_json::json!({}), true)?;
        self.pending_browser_navigation = Some(url.to_string());
        let queued = self.queue_or_confirm(
            call,
            context,
            format!("Start browser before navigating to {url}"),
        )?;
        self.surface = DevSurface::App;
        Ok(if queued {
            "Browser detached · launching; navigation continues automatically".into()
        } else {
            "Browser detached · Enter approves launch; navigation continues automatically".into()
        })
    }

    pub fn handle_printable(&mut self, character: char) {
        if character == ':' {
            self.open_palette();
            return;
        }
        if character == 'a' && self.surface != DevSurface::Agent {
            self.open_menu();
            return;
        }
        if self.surface == DevSurface::Trust {
            let decision = match character {
                'i' | 'I' => {
                    self.status = "Inspecting exact repository configuration".into();
                    return;
                }
                'o' | 'O' => Some(crate::LocalTrustDecision::OpenUntrusted),
                '1' => Some(crate::LocalTrustDecision::TrustOnce),
                'T' => Some(crate::LocalTrustDecision::TrustProject),
                _ => None,
            };
            if let Some(decision) = decision {
                match self
                    .workspace
                    .try_lock()
                    .and_then(|mut workspace| workspace.apply_local_trust_decision(decision))
                {
                    Ok(trust) => {
                        self.surface = DevSurface::Agent;
                        self.snapshot_trust_label = trust.label().into();
                        self.status = format!("Workspace opened with {} authority", trust.label());
                    }
                    Err(error) => self.status = format!("Trust decision failed: {error}"),
                }
                return;
            }
        }
        if self.surface == DevSurface::Agent && self.snapshot_trust_label == "untrusted" {
            let decision = match character {
                't' => Some(crate::LocalTrustDecision::TrustOnce),
                'T' => Some(crate::LocalTrustDecision::TrustProject),
                _ => None,
            };
            if let Some(decision) = decision {
                match self
                    .workspace
                    .try_lock()
                    .and_then(|mut workspace| workspace.apply_local_trust_decision(decision))
                {
                    Ok(trust) => {
                        self.snapshot_trust_label = trust.label().into();
                        self.status = format!("Workspace opened with {} authority", trust.label());
                    }
                    Err(error) => self.status = format!("Trust decision failed: {error}"),
                }
                return;
            }
        }

        // Agent text wins over navigation. The event loop reserves only
        // explicit modal controls and the `:` command prefix.
        if self.surface == DevSurface::Agent
            && self.snapshot_trust_label != "untrusted"
            && !character.is_ascii_digit()
        {
            if self.agent_readiness.starts_with("✓ Ready") {
                self.open_composer();
            } else {
                self.start_agent_interaction();
            }
            if self.composer_mode {
                self.insert_composer_text(&character.to_string());
            }
            return;
        }

        if self.surface == DevSurface::Code && character == 'i' {
            self.enter_code_edit();
            return;
        }
        if self.responsive_class(self.terminal_width, self.terminal_height)
            == ResponsiveClass::Phone
        {
            let surface = match character {
                '1' => Some(DevSurface::Agent),
                '2' => Some(DevSurface::Code),
                '3' => Some(DevSurface::App),
                '4' => Some(DevSurface::Tasks),
                '5' => Some(DevSurface::More),
                _ => None,
            };
            if let Some(surface) = surface {
                self.surface = surface;
                self.status = format!("{} selected", surface.label());
                return;
            }
        }
        let surface = match character {
            '1' => Some(DevSurface::Agent),
            '2' | 'c' => Some(DevSurface::Code),
            '3' | 'v' => Some(DevSurface::App),
            '4' => Some(DevSurface::Terminal),
            '5' | 'w' => Some(DevSurface::Tasks),
            '6' | 'g' => Some(DevSurface::Git),
            '7' | 'd' => Some(DevSurface::Debug),
            '8' | 'm' => Some(DevSurface::More),
            _ => None,
        };
        if let Some(surface) = surface {
            self.surface = surface;
            self.status = format!("{} selected", surface.label());
        } else if self.surface == DevSurface::Agent && self.snapshot_trust_label != "untrusted" {
            self.open_composer();
            self.insert_composer_text(&character.to_string());
        }
    }
    /// Queue the project-detected development command without making users
    /// copy it from project inspection into the command palette.
    pub fn request_detected_dev(&mut self) {
        if self.background_action_running() {
            self.status = "Another background action is still running".into();
            return;
        }
        let command_result = {
            match self.ws() {
                Ok(workspace) => workspace
                    .project()
                    .detection()
                    .dev_command
                    .clone()
                    .ok_or_else(|| "no detected dev command".to_string()),
                Err(error) => Err(error),
            }
        };
        let command = match command_result {
            Ok(command) => command,
            Err(error) if error == "no detected dev command" => {
                self.status =
                    "No development command detected · use :process start NAME COMMAND".into();
                return;
            }
            Err(error) => {
                self.status = if error.to_string().contains("workspace busy") {
                    "Development suite unavailable: workspace refresh is still running · use :s again"
                        .into()
                } else {
                    format!("Development command unavailable: {error}")
                };
                return;
            }
        };
        let (call, context) = match self.tool_request(
            "glass.process.start",
            serde_json::json!({"name": "dev", "command": command}),
            true,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.status = format!("Development suite unavailable: {error}");
                return;
            }
        };
        let queued = match self.queue_or_confirm(
            call,
            context,
            "Start the detected development suite".into(),
        ) {
            Ok(queued) => queued,
            Err(error) => {
                self.status = format!("Development suite unavailable: {error}");
                return;
            }
        };
        self.surface = DevSurface::Terminal;
        if !queued {
            self.status = "Development suite ready · Enter approves once · Esc cancels".into();
        }
    }

    pub fn next_surface(&mut self) {
        let surfaces: &[DevSurface] = if self
            .responsive_class(self.terminal_width, self.terminal_height)
            == ResponsiveClass::Phone
        {
            &DevSurface::PHONE
        } else {
            &DevSurface::PRIMARY
        };
        let index = surfaces
            .iter()
            .position(|surface| *surface == self.surface)
            .unwrap_or(0);
        self.surface = surfaces[(index + 1) % surfaces.len()];
        self.status = format!("{} selected", self.surface.label());
    }

    pub fn scroll_surface(&mut self, delta: i32) {
        let scroll = self.surface_scroll.entry(self.surface).or_default();
        *scroll = (*scroll as i64 + i64::from(delta)).max(0) as usize;
    }

    pub fn scroll_home(&mut self) {
        self.surface_scroll.insert(self.surface, 0);
    }

    pub fn scroll_end(&mut self) {
        let end = self.surface_content_height().saturating_sub(1);
        self.surface_scroll.insert(self.surface, end);
    }

    /// Bounded logical height of the current surface's content, used for End.
    fn surface_content_height(&self) -> usize {
        if self.surface == DevSurface::Agent {
            return self.conversation_view().lines().count().max(1);
        }
        let content = match self.surface {
            DevSurface::Code => &self.editor,
            DevSurface::App => &self.browser,
            DevSurface::Terminal => &self.processes,
            DevSurface::Tasks => &self.tasks,
            DevSurface::Git => &self.git,
            DevSurface::Debug => &self.debugger,
            DevSurface::More | DevSurface::Trust => &self.workspace_status,
            DevSurface::Agent => unreachable!("agent content handled above"),
        };
        content.lines().count().max(1)
    }

    pub fn current_scroll(&self) -> u16 {
        self.surface_scroll
            .get(&self.surface)
            .copied()
            .unwrap_or(0)
            .min(u16::MAX as usize) as u16
    }

    pub fn move_file_selection(&mut self, delta: i32) {
        if self.files.is_empty() {
            self.selected_file = 0;
            return;
        }
        self.selected_file = (self.selected_file as i32 + delta)
            .clamp(0, self.files.len().saturating_sub(1) as i32)
            as usize;
    }
    pub fn move_git_selection(&mut self, delta: i32) {
        if self.git_entries.is_empty() {
            self.selected_git_file = 0;
            self.status = "No changed files selected".into();
            return;
        }
        self.selected_git_file = (self.selected_git_file as i32 + delta)
            .clamp(0, self.git_entries.len().saturating_sub(1) as i32)
            as usize;
        if let Some(entry) = self.git_entries.get(self.selected_git_file) {
            self.status = format!("{} · Enter diff", entry.path);
        }
    }

    pub fn selected_git_entry(&self) -> Option<&crate::git::GitStatusEntry> {
        self.git_entries.get(self.selected_git_file)
    }

    pub fn open_selected_file(&mut self) {
        let Some(path) = self.files.get(self.selected_file).cloned() else {
            self.status = "No project file selected".into();
            return;
        };
        let already_open = self
            .workspace
            .try_lock()
            .map(|workspace| workspace.project().buffer(&path).is_some())
            .unwrap_or(false);
        let result = if already_open {
            Ok(())
        } else {
            self.workspace.try_lock().and_then(|mut workspace| {
                workspace
                    .project_mut()
                    .open_buffer(&path, crate::development::Actor::local())
                    .map(|_| ())
            })
        };
        match result {
            Ok(()) => {
                self.editor_buffer_index = self
                    .workspace
                    .try_lock()
                    .ok()
                    .and_then(|workspace| {
                        workspace
                            .project()
                            .buffers()
                            .position(|buffer| buffer.path == path)
                    })
                    .unwrap_or(0);
                self.refresh_editor_projection();
                self.status = if already_open {
                    format!("Selected {path} · Enter opens the full-screen editor")
                } else {
                    format!("Opened {path} · Enter opens the full-screen editor")
                };
            }
            Err(error) => self.status = format!("Open failed: {error}"),
        }
    }

    pub fn open_selected_file_for_edit(&mut self) {
        let selected_path = self.files.get(self.selected_file).cloned();
        self.open_selected_file();
        if selected_path.as_deref() == Some(self.focused_editor_path.as_str()) {
            self.surface = DevSurface::Code;
            self.enter_code_edit();
        }
    }

    /// Load the current Git diff without blocking key handling.
    pub fn queue_git_diff(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        if self.background_action_running() {
            self.status = "Git diff waits for the current background operation".into();
            return;
        }
        let selected = self.selected_git_entry().map(|entry| {
            (
                entry.path.clone(),
                entry.untracked,
                entry.index_status,
                entry.worktree_status,
            )
        });
        if let Some((path, true, _, _)) = selected.as_ref() {
            self.git_diff_path = Some(path.clone());
            self.git_diff = format!(
                "{path}\n\nUntracked file · Git has no tracked diff yet.\nUse Enter on Code to inspect the file."
            );
            self.git_diff_open = true;
            self.surface_scroll.insert(DevSurface::Git, 0);
            self.status = format!("{path} is untracked · no Git diff to show");
            return;
        }
        let staged = selected
            .as_ref()
            .is_some_and(|(_, _, index, worktree)| *index != ' ' && *worktree == ' ');
        let mut arguments = serde_json::json!({"staged": staged});
        if let Some((path, _, _, _)) = selected.as_ref() {
            arguments["path"] = serde_json::Value::String(path.clone());
        }
        let (call, context) = match self.tool_request("glass.git.diff", arguments, false) {
            Ok(request) => request,
            Err(error) => {
                self.status = format!("Git diff unavailable: {error}");
                return;
            }
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.git_diff_path = selected.map(|(path, _, _, _)| path);
                self.git_diff_open = true;
                self.surface_scroll.insert(DevSurface::Git, 0);
                self.git_diff = "Loading Git diff…".into();
                self.status = self
                    .git_diff_path
                    .as_deref()
                    .map(|path| format!("Loading diff for {path} in background…"))
                    .unwrap_or_else(|| "Loading full worktree diff in background…".into());
            }
            Err(error) => self.status = format!("Git diff unavailable: {error}"),
        }
    }

    /// Synchronous helper retained for callers outside the terminal event loop.
    pub fn open_git_diff(&mut self) {
        let diff = self.workspace.try_lock().and_then(|workspace| {
            workspace
                .git()
                .ok_or_else(|| {
                    crate::development::DevelopmentError::NotFound("not a Git repository".into())
                })?
                .diff(false, None)
                .map_err(|error| crate::development::DevelopmentError::Process(error.to_string()))
        });
        self.git_diff_path = None;
        match diff {
            Ok(diff) if diff.trim().is_empty() => {
                self.git_diff = "Working tree clean · no diff to show".into();
                self.git_diff_open = true;
                self.surface_scroll.insert(DevSurface::Git, 0);
                self.status = "Git diff · no changes".into();
            }
            Ok(diff) => {
                self.git_diff = diff;
                self.git_diff_open = true;
                self.surface_scroll.insert(DevSurface::Git, 0);
                self.status = "Git diff · PgUp/PgDn scroll · Esc closes".into();
            }
            Err(error) => self.status = format!("Git diff unavailable: {error}"),
        }
    }

    pub fn close_git_diff(&mut self) {
        self.git_diff_open = false;
        self.git_diff_requested = false;
        self.git_diff_path = None;
        self.surface_scroll.insert(DevSurface::Git, 0);
        self.status = "Git status · select a file with ↑/↓".into();
    }

    pub fn focused_buffer(&self) -> Option<crate::development::EditorBuffer> {
        self.workspace.try_lock().ok().and_then(|workspace| {
            workspace
                .project()
                .buffers()
                .nth(self.editor_buffer_index)
                .cloned()
        })
    }

    pub fn refresh_editor_projection(&mut self) {
        let (buffers, comments, proposals, checkpoints) = self
            .workspace
            .lock()
            .map(|workspace| {
                (
                    workspace.project().buffers().cloned().collect::<Vec<_>>(),
                    workspace.project().editor_comments(None),
                    workspace.project().editor_proposals(),
                    workspace.project().editor_checkpoints(),
                )
            })
            .unwrap_or_default();
        self.editor_comments = comments;
        self.editor_proposals = proposals;
        self.editor_checkpoints = checkpoints;
        if let Some(buffer) = buffers.get(self.editor_buffer_index) {
            self.focused_editor_path = buffer.path.clone();
            self.focused_editor_content = buffer.content.clone();
            self.focused_editor_dirty = buffer.dirty;
            self.focused_editor_line = buffer.cursor_line;
            self.focused_editor_column = buffer.cursor_column;
            self.focused_editor_selection = buffer.selection.clone();
            self.ensure_editor_cursor_visible();
        } else {
            self.focused_editor_path.clear();
            self.focused_editor_content.clear();
            self.focused_editor_dirty = false;
            self.focused_editor_line = 0;
            self.focused_editor_column = 0;
            self.focused_editor_selection = None;
            self.editor_scroll_line = 0;
            self.editor_scroll_column = 0;
        }
        self.editor = format_editor_buffers(&buffers);
    }

    pub fn editor_collaboration_summary(&self) -> String {
        let open_comments = self
            .editor_comments
            .iter()
            .filter(|comment| comment.state == crate::development::EditorCommentState::Open)
            .count();
        let pending = self
            .editor_proposals
            .iter()
            .filter(|proposal| proposal.state == crate::development::EditorProposalState::Pending)
            .count();
        format!(
            "comments {} open · proposals {} pending · checkpoints {}",
            open_comments,
            pending,
            self.editor_checkpoints.len()
        )
    }

    pub fn cycle_editor_buffer(&mut self, delta: i32) {
        let count = self
            .workspace
            .lock()
            .map(|workspace| workspace.project().buffers().count())
            .unwrap_or(0);
        if count == 0 {
            self.status = "No open buffers · open a file from the list".into();
            return;
        }
        let index = self.editor_buffer_index as i32 + delta;
        self.editor_buffer_index = index.rem_euclid(count as i32) as usize;
        let name = self
            .workspace
            .lock()
            .ok()
            .and_then(|workspace| {
                workspace
                    .project()
                    .buffers()
                    .nth(self.editor_buffer_index)
                    .map(|buffer| buffer.path.clone())
            })
            .unwrap_or_default();
        self.refresh_editor_projection();
        self.status = format!("Buffer {}/{} · {name}", self.editor_buffer_index + 1, count);
    }

    pub fn enter_code_edit(&mut self) {
        let has_buffer = self.focused_buffer().is_some();
        if !has_buffer {
            self.open_selected_file();
        }
        if self.focused_buffer().is_some() {
            self.surface = DevSurface::Code;
            self.code_edit_mode = true;
            self.editor_exit_prompt = None;
            self.ensure_editor_cursor_visible();
            self.status =
                "EDITING · arrows move · Shift+arrows select · Ctrl-S save · Alt-A ask Pi · Esc exit"
                    .into();
        }
    }

    pub fn close_code_edit(&mut self) {
        self.code_edit_mode = false;
        self.editor_exit_prompt = None;
        self.surface = DevSurface::Code;
        self.status = "Code navigation · select a file and press Enter to edit".into();
    }

    pub fn request_editor_exit(&mut self) {
        if !self.code_edit_mode {
            return;
        }
        self.refresh_editor_projection();
        let prompt = if self.focused_editor_dirty {
            EditorExitPrompt::Unsaved
        } else {
            EditorExitPrompt::Clean
        };
        self.editor_exit_prompt = Some(prompt);
        self.status = match prompt {
            EditorExitPrompt::Clean => "Exit editor? Enter leaves · Esc stays".into(),
            EditorExitPrompt::Unsaved => {
                "Unsaved changes · S save · D discard · Q discard and quit · Esc stays".into()
            }
        };
    }

    pub fn cancel_editor_exit(&mut self) {
        self.editor_exit_prompt = None;
        self.status = "Still editing · Esc opens exit choices".into();
    }

    pub fn handle_editor_exit_key(&mut self, code: crossterm::event::KeyCode) {
        let Some(prompt) = self.editor_exit_prompt else {
            return;
        };
        match prompt {
            EditorExitPrompt::Clean => match code {
                crossterm::event::KeyCode::Enter
                | crossterm::event::KeyCode::Char('q' | 'Q' | 'y' | 'Y') => {
                    self.close_code_edit();
                }
                crossterm::event::KeyCode::Esc | crossterm::event::KeyCode::Char('n' | 'N') => {
                    self.cancel_editor_exit()
                }
                _ => {}
            },
            EditorExitPrompt::Unsaved => match code {
                crossterm::event::KeyCode::Char('s' | 'S') => match self.save_editor_buffer() {
                    Ok(()) => {
                        let path = self.focused_editor_path.clone();
                        self.refresh_editor_projection();
                        self.close_code_edit();
                        self.status = format!("Saved {path} · editor closed");
                    }
                    Err(error) => {
                        self.status =
                            format!("Save failed: {error} · S retry · D discard · Q quit");
                    }
                },
                crossterm::event::KeyCode::Char('d' | 'D') => match self.discard_editor_buffer() {
                    Ok(()) => {
                        let path = self.focused_editor_path.clone();
                        self.refresh_editor_projection();
                        self.close_code_edit();
                        self.status = format!("Discarded changes in {path} · editor closed");
                    }
                    Err(error) => {
                        self.status =
                            format!("Discard failed: {error} · S save · D retry · Q quit");
                    }
                },
                crossterm::event::KeyCode::Char('q' | 'Q') => match self.discard_editor_buffer() {
                    Ok(()) => {
                        self.code_edit_mode = false;
                        self.editor_exit_prompt = None;
                        self.surface = DevSurface::Code;
                        self.quit = true;
                        self.status = "Changes discarded · closing Glass Dev".into();
                    }
                    Err(error) => {
                        self.status =
                            format!("Discard failed: {error} · S save · D retry · Q quit");
                    }
                },
                crossterm::event::KeyCode::Esc => self.cancel_editor_exit(),
                _ => {}
            },
        }
    }

    fn save_editor_buffer(&mut self) -> Result<(), String> {
        let path = self.focused_editor_path.clone();
        self.workspace
            .try_lock()
            .map_err(|error| error.to_string())?
            .project_mut()
            .save_buffer(&path)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn discard_editor_buffer(&mut self) -> Result<(), String> {
        let path = self.focused_editor_path.clone();
        self.workspace
            .try_lock()
            .map_err(|error| error.to_string())?
            .project_mut()
            .open_buffer(&path, crate::development::Actor::local())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn ensure_editor_cursor_visible(&mut self) {
        if self.focused_editor_path.is_empty() {
            return;
        }
        let viewport_height = usize::from(self.terminal_height.saturating_sub(8).max(1));
        if self.editor_soft_wrap {
            let wrapped = super::file_view::render_editable_source_wrapped(
                &self.focused_editor_path,
                &self.focused_editor_content,
                self.focused_editor_line,
                self.focused_editor_column,
                self.focused_editor_selection.as_ref(),
                self.terminal_width.saturating_sub(4).max(1),
            );
            let Some(cursor) = wrapped.cursor else {
                return;
            };
            self.editor_scroll_column = 0;
            if cursor.row < self.editor_scroll_line {
                self.editor_scroll_line = cursor.row;
            } else if cursor.row >= self.editor_scroll_line + viewport_height {
                self.editor_scroll_line = cursor.row + 1 - viewport_height;
            }
            return;
        }
        let lines = self.focused_editor_content.split('\n').collect::<Vec<_>>();
        let cursor_line = self.focused_editor_line.saturating_sub(1) as usize;
        if cursor_line < self.editor_scroll_line {
            self.editor_scroll_line = cursor_line;
        } else if cursor_line >= self.editor_scroll_line + viewport_height {
            self.editor_scroll_line = cursor_line + 1 - viewport_height;
        }
        let gutter_width = lines.len().max(1).to_string().len().max(3);
        let viewport_width = usize::from(
            self.terminal_width
                .saturating_sub((gutter_width + 8).min(u16::MAX as usize) as u16)
                .max(1),
        );
        let cursor_column = self.focused_editor_column.saturating_sub(1) as usize;
        if cursor_column < self.editor_scroll_column {
            self.editor_scroll_column = cursor_column;
        } else if cursor_column >= self.editor_scroll_column + viewport_width {
            self.editor_scroll_column = cursor_column + 1 - viewport_width;
        }
    }

    pub fn toggle_editor_soft_wrap(&mut self) {
        self.editor_soft_wrap = !self.editor_soft_wrap;
        self.editor_scroll_line = 0;
        self.editor_scroll_column = 0;
        self.ensure_editor_cursor_visible();
        self.status = if self.editor_soft_wrap {
            "Soft wrap ON · source lines reflow to the editor width".into()
        } else {
            "Soft wrap OFF · horizontal scrolling follows source columns".into()
        };
    }

    pub fn edit_code_key(
        &mut self,
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) {
        if code == crossterm::event::KeyCode::Char('w')
            && modifiers.contains(crossterm::event::KeyModifiers::ALT)
        {
            self.toggle_editor_soft_wrap();
            return;
        }
        let Some(buffer) = self.focused_buffer() else {
            self.status = "No open buffer".into();
            return;
        };
        let path = buffer.path.clone();
        let result = match (code, modifiers) {
            (crossterm::event::KeyCode::Char('a'), value)
                if value.contains(crossterm::event::KeyModifiers::ALT) =>
            {
                self.prepare_editor_agent_prompt();
                Ok(())
            }
            (crossterm::event::KeyCode::Char('s'), value)
                if value.contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.locked(|workspace| workspace.project_mut().save_buffer(&path).map(|_| ()))
                    .unwrap_or_else(|| {
                        Err(crate::development::DevelopmentError::Conflict(
                            "workspace lock failed".into(),
                        ))
                    })
            }
            (crossterm::event::KeyCode::Char('z'), value)
                if value.contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.locked(|workspace| workspace.project_mut().undo_buffer(&path).map(|_| ()))
                    .unwrap_or_else(|| {
                        Err(crate::development::DevelopmentError::Conflict(
                            "workspace lock failed".into(),
                        ))
                    })
            }
            (crossterm::event::KeyCode::Char('y'), value)
                if value.contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.locked(|workspace| workspace.project_mut().redo_buffer(&path).map(|_| ()))
                    .unwrap_or_else(|| {
                        Err(crate::development::DevelopmentError::Conflict(
                            "workspace lock failed".into(),
                        ))
                    })
            }
            (crossterm::event::KeyCode::Left, value) => self.move_editor_cursor(
                &path,
                0,
                -1,
                value.contains(crossterm::event::KeyModifiers::SHIFT),
            ),
            (crossterm::event::KeyCode::Right, value) => self.move_editor_cursor(
                &path,
                0,
                1,
                value.contains(crossterm::event::KeyModifiers::SHIFT),
            ),
            (crossterm::event::KeyCode::Up, value) => self.move_editor_cursor(
                &path,
                -1,
                0,
                value.contains(crossterm::event::KeyModifiers::SHIFT),
            ),
            (crossterm::event::KeyCode::Down, value) => self.move_editor_cursor(
                &path,
                1,
                0,
                value.contains(crossterm::event::KeyModifiers::SHIFT),
            ),
            (crossterm::event::KeyCode::Enter, _) => self.insert_editor_text(&path, "\n"),
            (crossterm::event::KeyCode::Backspace, _) => self.backspace_editor(&path),
            (crossterm::event::KeyCode::Char(character), _) => {
                self.insert_editor_text(&path, &character.to_string())
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.status = format!("Editor action failed: {error}");
        }
        self.refresh_editor_projection();
    }

    fn move_editor_cursor(
        &mut self,
        path: &str,
        line_delta: i32,
        column_delta: i32,
        extend_selection: bool,
    ) -> crate::development::DevelopmentResult<()> {
        let buffer = self
            .workspace
            .try_lock()?
            .project()
            .buffer(path)
            .cloned()
            .ok_or_else(|| {
                crate::development::DevelopmentError::NotFound(format!("buffer {path}"))
            })?;
        let lines = buffer.content.split('\n').collect::<Vec<_>>();
        let line =
            (buffer.cursor_line as i32 + line_delta).clamp(1, lines.len().max(1) as i32) as u32;
        let max_column = lines
            .get(line.saturating_sub(1) as usize)
            .map(|line| line.chars().count() + 1)
            .unwrap_or(1) as u32;
        let column = if line_delta != 0 {
            buffer.cursor_column.min(max_column)
        } else {
            (buffer.cursor_column as i32 + column_delta).clamp(1, max_column as i32) as u32
        };
        let anchor = if extend_selection {
            buffer
                .selection
                .map(|selection| selection.anchor)
                .unwrap_or(crate::development::TextPosition {
                    line: buffer.cursor_line,
                    column: buffer.cursor_column,
                })
        } else {
            crate::development::TextPosition { line, column }
        };
        let mut workspace = self.workspace.try_lock()?;
        workspace
            .project_mut()
            .set_buffer_cursor(path, line, column)?;
        workspace.project_mut().set_buffer_selection(
            path,
            extend_selection.then_some(crate::development::TextSelection {
                anchor,
                active: crate::development::TextPosition { line, column },
            }),
            crate::development::Actor::local(),
        )?;
        Ok(())
    }

    fn insert_editor_text(
        &mut self,
        path: &str,
        text: &str,
    ) -> crate::development::DevelopmentResult<()> {
        let mut workspace = self.workspace.try_lock()?;
        let project = workspace.project_mut();
        let buffer = project.buffer(path).cloned().ok_or_else(|| {
            crate::development::DevelopmentError::NotFound(format!("buffer {path}"))
        })?;
        if buffer
            .selection
            .as_ref()
            .is_some_and(|selection| !selection.is_empty())
        {
            project.replace_buffer_selection(
                path,
                text.to_string(),
                crate::development::Actor::local(),
            )?;
            return Ok(());
        }
        let offset = editor_offset(&buffer.content, buffer.cursor_line, buffer.cursor_column);
        let mut content = buffer.content;
        content.insert_str(offset, text);
        let cursor =
            crate::development::editor::text_position_at_offset(&content, offset + text.len())
                .ok_or_else(|| {
                    crate::development::DevelopmentError::InvalidInput(
                        "inserted text ended at an invalid UTF-8 boundary".into(),
                    )
                })?;
        let actor = crate::development::Actor::local();
        project.edit_buffer(path, content, actor.clone())?;
        project.set_buffer_cursor(path, cursor.line, cursor.column)?;
        project.set_buffer_selection(path, None, actor)?;
        Ok(())
    }

    fn backspace_editor(&mut self, path: &str) -> crate::development::DevelopmentResult<()> {
        let mut workspace = self.workspace.try_lock()?;
        let project = workspace.project_mut();
        let buffer = project.buffer(path).cloned().ok_or_else(|| {
            crate::development::DevelopmentError::NotFound(format!("buffer {path}"))
        })?;
        if buffer
            .selection
            .as_ref()
            .is_some_and(|selection| !selection.is_empty())
        {
            project.replace_buffer_selection(
                path,
                String::new(),
                crate::development::Actor::local(),
            )?;
            return Ok(());
        }
        let offset = editor_offset(&buffer.content, buffer.cursor_line, buffer.cursor_column);
        if offset == 0 {
            return Ok(());
        }
        let previous = buffer.content[..offset]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
        let mut content = buffer.content;
        content.drain(previous..offset);
        let cursor = crate::development::editor::text_position_at_offset(&content, previous)
            .ok_or_else(|| {
                crate::development::DevelopmentError::InvalidInput(
                    "backspace ended at an invalid UTF-8 boundary".into(),
                )
            })?;
        let actor = crate::development::Actor::local();
        project.edit_buffer(path, content, actor.clone())?;
        project.set_buffer_cursor(path, cursor.line, cursor.column)?;
        project.set_buffer_selection(path, None, actor)?;
        Ok(())
    }

    pub fn previous_surface(&mut self) {
        let surfaces: &[DevSurface] = if self
            .responsive_class(self.terminal_width, self.terminal_height)
            == ResponsiveClass::Phone
        {
            &DevSurface::PHONE
        } else {
            &DevSurface::PRIMARY
        };
        let index = surfaces
            .iter()
            .position(|surface| *surface == self.surface)
            .unwrap_or(0);
        self.surface = surfaces[(index + surfaces.len() - 1) % surfaces.len()];
        self.status = format!("{} selected", self.surface.label());
    }

    /// Apply one background snapshot without touching UI-only fields.
    pub fn apply_snapshot(&mut self, snapshot: &super::snapshot::DisplaySnapshot) {
        self.refresh_latency_ms = snapshot.duration.as_millis() as u64;
        let selected_agent_status = self.selected_agent.as_ref().and_then(|selected| {
            snapshot
                .agent_states
                .iter()
                .find(|(id, _)| id == selected)
                .map(|(_, status)| *status)
        });
        if let super::snapshot::BrowserHealth::Crashed {
            last_process_id,
            last_revision,
        } = &snapshot.browser_health
            && self.browser_recovery.is_none()
        {
            let pid_label = last_process_id
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "unknown".into());
            let revision_label = last_revision
                .map(|revision| revision.to_string())
                .unwrap_or_else(|| "none".into());
            self.surface = DevSurface::App;
            self.browser_recovery = Some(BrowserRecoveryOffer::from_error(
                &format!(
                    "browser endpoint crashed unexpectedly (pid {pid_label}, last revision {revision_label})"
                ),
                9222,
            ));
            self.browser_workspace
                .disconnected("browser endpoint crashed".to_string(), true);
            self.status = "Browser endpoint crashed · recovery choices below".into();
        }
        self.harnesses = snapshot.harnesses.clone();
        self.agents = snapshot.agents.clone();
        self.agent_conversation = snapshot.agent_conversation.clone();
        self.reconcile_pending_chat();
        if self.agent_send_job.is_none()
            && self.pending_chat_messages.is_empty()
            && self.status.contains("Glass Agent is thinking")
        {
            match selected_agent_status {
                Some(crate::AgentStatus::Idle) => {
                    self.status = "Glass Agent ready · response received".into();
                }
                Some(crate::AgentStatus::Failed) => {
                    self.status = "Glass Agent failed · press Enter to retry".into();
                }
                Some(crate::AgentStatus::Cancelled) => {
                    self.status = "Glass Agent stopped · press Enter to retry".into();
                }
                Some(crate::AgentStatus::Waiting) => {
                    self.status = "Glass Agent waiting for approval".into();
                }
                _ => {}
            }
        }
        self.tasks = snapshot.tasks.clone();
        if !snapshot.editor.is_empty() {
            self.editor = snapshot.editor.clone();
        }
        self.lsp = snapshot.lsp.clone();
        self.processes = snapshot.processes.clone();
        self.git = snapshot.git.clone();
        self.git_entries = snapshot.git_entries.clone();
        self.github = snapshot.github.clone();
        self.github_review = snapshot.github_review.clone();
        if self.git_entries.is_empty() {
            self.selected_git_file = 0;
        } else {
            self.selected_git_file = self
                .selected_git_file
                .min(self.git_entries.len().saturating_sub(1));
        }
        self.tests = snapshot.tests.clone();
        self.kernels = snapshot.kernels.clone();
        self.debugger = snapshot.debugger.clone();
        self.replay = snapshot.replay.clone();
        self.workflow = snapshot.workflow.clone();
        self.workspace_status = snapshot.workspace_status.clone();
        self.browser = snapshot.browser.clone();
        self.snapshot_root = snapshot.root.clone();
        self.snapshot_trust_label = snapshot.trust_label.clone();
        self.snapshot_trust_inspection = snapshot.trust_inspection.clone();
        self.snapshot_project_revision = snapshot.project_revision;
        self.snapshot_generation = snapshot.generation;
        self.snapshot_skills_count = snapshot.skills_count;
        self.snapshot_tools_count = snapshot.tools_count;
        self.experiments = snapshot.experiments.clone();
        if !snapshot.files.is_empty() {
            self.files = snapshot.files.clone();
            if !self.files.is_empty() {
                self.selected_file = self.selected_file.min(self.files.len() - 1);
            }
        }
    }

    pub fn set_terminal_size(&mut self, width: u16, height: u16) {
        self.terminal_width = width;
        self.terminal_height = height;
        self.ensure_editor_cursor_visible();
    }

    pub fn refresh(&mut self) {
        self.harnesses = crate::harness::summary();
        let Ok(mut workspace) = self.workspace.try_lock() else {
            self.status = "Workspace lock failed".into();
            return;
        };
        self.files = workspace
            .project()
            .list_files()
            .map(|entries| {
                entries
                    .into_iter()
                    .filter(|entry| matches!(entry.kind, crate::development::FileKind::File))
                    .map(|entry| entry.path)
                    .take(512)
                    .collect()
            })
            .unwrap_or_default();
        if !self.files.is_empty() {
            self.selected_file = self.selected_file.min(self.files.len() - 1);
        }
        let agent_snapshots = workspace.agents().list();
        self.pending_agent_approval = pending_agent_approval(agent_snapshots.as_ref().ok());
        self.agents = match agent_snapshots {
            Ok(agents) if agents.is_empty() => "No agents. :agent spawn ROLE TASK".into(),
            Ok(agents) => agents
                .iter()
                .map(|agent| {
                    format!(
                        "{}  {}  {} · {}\n  target {} · model {} · thinking {} · events {} · dropped {}{}\n  evidence {}",
                        agent.id.as_str(),
                        agent.status.label(),
                        agent.role,
                        agent.task,
                        agent.worktree.display(),
                        agent.model.as_deref().unwrap_or("default"),
                        agent.thinking.as_deref().unwrap_or("default"),
                        agent.event_count,
                        agent.dropped_event_count,
                        agent
                            .last_error
                            .as_deref()
                            .map(|error| format!(" · {error}"))
                            .unwrap_or_default(),
                        agent
                            .evidence
                            .iter()
                            .rev()
                            .take(3)
                            .map(|evidence| evidence.to_string())
                            .collect::<Vec<_>>()
                            .join(" · ")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
            Err(error) => format!("Agent registry failed: {error}"),
        };
        self.agent_conversation = match workspace.agents().history(0) {
            Ok(events) if events.is_empty() => {
                "No conversation yet. Press Enter or start typing to compose a message.".into()
            }
            Ok(events) => super::projection::conversation(
                &events
                    .iter()
                    .rev()
                    .take(40)
                    .rev()
                    .cloned()
                    .collect::<Vec<_>>(),
            ),
            Err(error) => format!("Conversation unavailable · {error}"),
        };
        self.tasks = match workspace.tasks() {
            Ok(tasks) if tasks.is_empty() => {
                "No tasks. :task create TITLE PROMPT creates an autonomous task".into()
            }
            Ok(tasks) => tasks
                .iter()
                .map(|task| {
                    let glyph = match task.state {
                        crate::TaskState::Succeeded
                            if task.verification == crate::VerificationRequirement::Settled => "◇",
                        crate::TaskState::Succeeded => "✓",
                        crate::TaskState::Failed | crate::TaskState::Cancelled => "×",
                        crate::TaskState::Blocked => "!",
                        crate::TaskState::Running | crate::TaskState::Verifying => "●",
                        _ => "○",
                    };
                    format!(
                        "{} {}  {}  {}\n  goal {}\n  agent {} · attempt {} · model {} · thinking {}\n  depends {}\n  verification {}{}\n  evidence {}",
                        glyph,
                        task.id.as_str(),
                        task.state.label(),
                        task.title,
                        task.goal,
                        task.assigned_agent
                            .as_ref()
                            .map(|agent| agent.as_str())
                            .unwrap_or("unassigned"),
                        task.attempt,
                        task.model.as_deref().unwrap_or("default"),
                        task.thinking.as_deref().unwrap_or("default"),
                        task.dependencies
                            .iter()
                            .map(|dependency| dependency.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                        serde_json::to_string(&task.verification).unwrap_or_default(),
                        task.last_error
                            .as_deref()
                            .map(|error| format!(" · {error}"))
                            .unwrap_or_default(),
                        task.evidence
                            .iter()
                            .rev()
                            .take(3)
                            .map(|evidence| {
                                format!(
                                    "{}={}",
                                    evidence.kind,
                                    evidence.passed.map_or("?".into(), |passed| passed.to_string())
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(" · ")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
            Err(error) => format!("Task scheduler failed: {error}"),
        };
        self.processes = workspace
            .project_mut()
            .processes()
            .list_checked()
            .map(|items| {
                if items.is_empty() {
                    "No managed terminals. Start the detected development command from More.".into()
                } else {
                    items
                        .into_iter()
                        .map(|item| {
                            format!(
                                "{} {} · health {} · pid {} · {}\n  {}",
                                if matches!(item.health, crate::development::ProcessHealth::Healthy)
                                {
                                    "●"
                                } else {
                                    "○"
                                },
                                item.name,
                                item.health.label(),
                                item.pid.map_or_else(|| "—".into(), |pid| pid.to_string()),
                                if item.pty { "PTY" } else { "pipes" },
                                item.detected_urls
                                    .first()
                                    .map_or(item.command.as_str(), String::as_str),
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n\n")
                }
            })
            .unwrap_or_else(|error| format!("Process state failed: {error}"));
        let buffers = workspace.project().buffers().cloned().collect::<Vec<_>>();
        self.editor = if buffers.is_empty() {
            "No file open. Select a file below and press Enter to open the full-screen editor."
                .into()
        } else {
            buffers
                .iter()
                .map(|buffer| {
                    // Viewport follows the cursor; long files stay editable.
                    let lines: Vec<&str> = buffer.content.lines().collect();
                    let cursor = buffer.cursor_line as usize;
                    let viewport_rows = 16;
                    let start = cursor
                        .saturating_sub(viewport_rows / 2)
                        .min(lines.len().saturating_sub(viewport_rows.min(lines.len())));
                    let end = (start + viewport_rows).min(lines.len());
                    let gutter_width = lines.len().to_string().len().max(3);
                    let viewport = lines[start..end]
                        .iter()
                        .enumerate()
                        .map(|(index, line)| {
                            let number = start + index + 1;
                            let marker = if number == cursor { "▶" } else { " " };
                            format!(
                                "{marker}{:>gutter_width$} │ {line}",
                                number,
                                gutter_width = gutter_width
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    format!(
                        "{}{} · cursor {}:{} · actor {} · {} lines\n{}",
                        if buffer.dirty { "● " } else { "○ " },
                        buffer.path,
                        buffer.cursor_line,
                        buffer.cursor_column,
                        buffer.actor.id,
                        lines.len(),
                        viewport
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        self.editor_comments = workspace.project().editor_comments(None);
        self.editor_proposals = workspace.project().editor_proposals();
        self.editor_checkpoints = workspace.project().editor_checkpoints();
        if let Some(buffer) = buffers.get(self.editor_buffer_index) {
            self.focused_editor_path = buffer.path.clone();
            self.focused_editor_dirty = buffer.dirty;
            self.focused_editor_line = buffer.cursor_line;
            self.focused_editor_column = buffer.cursor_column;
            self.focused_editor_selection = buffer.selection.clone();
        }
        self.lsp = {
            let language = workspace.language();
            let servers = language.names().collect::<Vec<_>>();
            let event_count = language.events(0).len();
            if servers.is_empty() {
                "No language server active · diagnostics unavailable".into()
            } else {
                format!("● {} · {} recent events", servers.join(" · "), event_count)
            }
        };
        let git_status = workspace.git().map(|git| git.status());
        self.git = match git_status {
            Some(Ok(status)) => {
                self.git_entries = status.entries.clone();
                let header = format!(
                    "branch {} · ↑{} ↓{} · upstream {}",
                    status.branch.as_deref().unwrap_or("detached"),
                    status.ahead,
                    status.behind,
                    status.upstream.as_deref().unwrap_or("none")
                );
                let entries = status
                    .entries
                    .iter()
                    .map(|entry| {
                        format!(
                            "{}{} {}{}",
                            if entry.untracked { "?" } else { "●" },
                            if status.conflicts.contains(&entry.path) {
                                "!"
                            } else {
                                " "
                            },
                            entry.index_status,
                            entry.path
                        )
                    })
                    .collect::<Vec<_>>();
                if entries.is_empty() {
                    format!("{header}\n✓ working tree clean")
                } else {
                    format!("{header}\n{}", entries.join("\n"))
                }
            }
            Some(Err(error)) => {
                self.git_entries.clear();
                format!("Git state failed: {error}")
            }
            None => {
                self.git_entries.clear();
                "Not a Git repository".into()
            }
        };
        if self.git_entries.is_empty() {
            self.selected_git_file = 0;
        } else {
            self.selected_git_file = self
                .selected_git_file
                .min(self.git_entries.len().saturating_sub(1));
        }
        let _ = workspace.tests_mut().poll();
        let test_runs = workspace
            .tests()
            .results()
            .rev()
            .take(32)
            .collect::<Vec<_>>();
        self.tests = if test_runs.is_empty() {
            "No test runs".into()
        } else {
            test_runs
                .iter()
                .map(|run| {
                    format!(
                        "{} {} · {} · {} ms · {} cases",
                        if run.exit_code == Some(0) {
                            "✓"
                        } else {
                            "×"
                        },
                        run.suite_id,
                        run.state.label(),
                        run.duration_ms.unwrap_or_default(),
                        run.cases.len()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let kernels = workspace.kernels().snapshots().collect::<Vec<_>>();
        self.kernels = if kernels.is_empty() {
            "No persistent kernels".into()
        } else {
            kernels
                .iter()
                .map(|kernel| {
                    format!(
                        "{} {} · {} · {} executions · rev {}",
                        if matches!(kernel.state, crate::kernels::KernelState::Ready) {
                            "●"
                        } else {
                            "○"
                        },
                        kernel.name,
                        kernel.kind.label(),
                        kernel.executions,
                        kernel.workspace_revision
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let debugger_names = workspace
            .debugger_names()
            .map(str::to_string)
            .collect::<Vec<_>>();
        self.debugger = if debugger_names.is_empty() {
            "No debugger sessions. :debug start NAME COMMAND [ARGS...]".into()
        } else {
            let snapshots: Result<Vec<_>, _> = debugger_names
                .iter()
                .map(|name| {
                    workspace
                        .debugger_mut(name)
                        .and_then(|debugger| debugger.snapshot())
                        .map(|snapshot| (name.to_string(), snapshot))
                })
                .collect();
            match snapshots {
                Ok(snapshots) => snapshots
                    .iter()
                    .map(|(name, snapshot)| {
                        format!(
                            "● {} · {} · pid {} · {} breakpoints · {} watches · {} threads/processes",
                            name,
                            match snapshot.state {
                                crate::debugger::DebugSessionState::Starting => "starting",
                                crate::debugger::DebugSessionState::Initialized => "initialized",
                                crate::debugger::DebugSessionState::Running => "running",
                                crate::debugger::DebugSessionState::Stopped => "stopped",
                                crate::debugger::DebugSessionState::Terminated => "terminated",
                                crate::debugger::DebugSessionState::Failed => "failed",
                            },
                            snapshot.adapter_process_id,
                            snapshot.breakpoints.values().map(Vec::len).sum::<usize>(),
                            snapshot.watches.len(),
                            snapshot.debuggee_processes.len()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                Err(error) => format!("Debugger state failed: {error}"),
            }
        };
        self.replay = workspace
            .intelligence()
            .replay(0, 128)
            .map(|events| {
                if events.is_empty() {
                    "No observable replay events".into()
                } else {
                    events
                        .iter()
                        .rev()
                        .take(24)
                        .rev()
                        .map(|event| {
                            format!(
                                "{} {} · {} · {} · rev {}",
                                event.sequence,
                                event.actor,
                                event.subsystem,
                                event.kind,
                                event.workspace_revision
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            })
            .unwrap_or_else(|error| format!("Replay failed: {error}"));
        self.browser = match workspace.browser().state() {
            Ok(state) => {
                let connected = state
                    .get("connected")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                let revision = state
                    .get("browserRevision")
                    .and_then(serde_json::Value::as_u64);
                if connected {
                    if self.browser_workspace.state().connection
                        != BrowserConnectionPhase::Connected
                    {
                        self.browser_workspace.connected(true, None, revision);
                    } else if let Some(revision) = revision {
                        self.browser_workspace.state_mut().browser_revision = Some(revision);
                    }
                } else {
                    self.browser_workspace.state_mut().connection =
                        BrowserConnectionPhase::Detached;
                }
                self.browser_workspace_summary()
            }
            Err(error) => format!("Browser state failed: {error}"),
        };
        self.workflow = workspace
            .browser()
            .list_workflows()
            .map(|state| super::projection::workflow(Some(&state)))
            .unwrap_or_else(|error| format!("Workflow state failed: {error}"));
        let root = workspace.root().display().to_string();
        let generation = workspace.generation();
        let project_revision = workspace.project().revision();
        let agent_count = workspace
            .agents()
            .list()
            .map(|items| items.len())
            .unwrap_or(0);
        let task_count = workspace.tasks().map(|items| items.len()).unwrap_or(0);
        let kernel_count = workspace.kernels().snapshots().count();
        let debugger_count = workspace.debugger_names().count();
        let detection = workspace.project().detection().clone();
        let trust = workspace.trust();
        let trust_ready = trust.permits_project_execution();
        let dev_hint = detection
            .dev_command
            .as_deref()
            .map(|command| format!("● run `{command}`"))
            .unwrap_or_else(|| "○ configure a dev command".into());
        let app_hint = detection
            .browser_url
            .as_deref()
            .map(|url| format!("● open App {url}"))
            .unwrap_or_else(|| "○ start App after a URL is detected".into());
        let project_line = if detection.languages.is_empty() {
            "○ project type unknown".to_string()
        } else {
            format!("✓ {} project", detection.languages.join("/"))
        };
        let workspace_line = format!("✓ workspace {}", trust.label());
        let agent_hint = if self.agent_readiness.starts_with("✓ Ready") {
            ":agent ready · type a message or press Enter to chat"
        } else {
            "○ Pi onboarding · use :agent setup or :agent setup login"
        };
        self.workspace_status = format!(
            "WELCOME · {}\n{}\n\n{}\n{}\n\nNEXT ACTIONS\n◆ :actions guided launchers · per-surface flows\n◆ type a message or press Enter to talk to Glass Agent\n◆ :process start dev · start the detected dev suite\n◆ Enter · open the selected file\n{}\n{}\n\nSTATE\nroot {}\ngeneration {} · project revision {} · trust {}\nresident: {} agents · {} tasks · {} kernels · {} debuggers",
            detection.git_branch.as_deref().unwrap_or("no branch"),
            project_line,
            agent_hint,
            workspace_line,
            dev_hint,
            app_hint,
            root,
            generation,
            project_revision,
            trust_ready,
            agent_count,
            task_count,
            kernel_count,
            debugger_count,
        );
        if workspace.trust().permits_project_execution()
            && let Ok(experiments) = workspace.experiments()
        {
            let snapshots = experiments.snapshots();
            self.experiments = if snapshots.is_empty() {
                "No experiments".into()
            } else {
                snapshots
                    .iter()
                    .map(|experiment| {
                        format!(
                            "{} {} · {} · port {} · agent {}",
                            if experiment.evidence.tests_failed == 0
                                && experiment.evidence.tests_passed > 0
                            {
                                "✓"
                            } else {
                                "○"
                            },
                            experiment.id,
                            experiment.state.label(),
                            experiment
                                .port
                                .map_or_else(|| "—".into(), |port| port.to_string()),
                            experiment
                                .agent_id
                                .as_ref()
                                .map(|id| id.as_str())
                                .unwrap_or("none")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            self.experiment_comparison = Some(experiments.compare());
        }
        drop(workspace);
        self.reconcile_pending_chat();
    }

    pub fn apply_browser_result(&mut self, tool: &str, result: &serde_json::Value) {
        match tool {
            "glass.browser.observe" => {
                let revision = result
                    .pointer("/accessibility/revision")
                    .and_then(serde_json::Value::as_u64);
                let title = result
                    .pointer("/page/title")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Untitled");
                let url = result
                    .pointer("/page/url")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                let loading = result
                    .pointer("/page/loading")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                if let Some(revision) = revision {
                    self.browser_workspace
                        .update_page(title, url, loading, Some(revision));
                    let entities = result
                        .pointer("/accessibility/interactive")
                        .and_then(serde_json::Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|entity| {
                            Some(BrowserWorkspaceEntity {
                                reference: entity.get("reference")?.as_str()?.into(),
                                role: entity
                                    .get("role")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("unknown")
                                    .into(),
                                name: entity
                                    .get("name")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("unnamed")
                                    .into(),
                                actionable: entity
                                    .get("actionable")
                                    .and_then(serde_json::Value::as_bool)
                                    .unwrap_or(true),
                                revision,
                            })
                        })
                        .collect();
                    self.browser_workspace.replace_entities(revision, entities);
                }
            }
            "glass.browser.targets" => {
                let values = result
                    .get("targets")
                    .and_then(serde_json::Value::as_array)
                    .or_else(|| result.as_array());
                let targets = values
                    .into_iter()
                    .flatten()
                    .filter_map(|target| {
                        Some(BrowserWorkspaceTarget {
                            id: target.get("id")?.as_str()?.into(),
                            title: target
                                .get("title")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("Untitled")
                                .into(),
                            url: target
                                .get("url")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default()
                                .into(),
                            selected: target
                                .get("active")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(false),
                        })
                    })
                    .collect();
                self.browser_workspace.replace_targets(targets);
                if self.browser_target_picker_requested {
                    self.browser_target_picker_requested = false;
                    self.browser_target_picker = true;
                    self.browser_target_selection = 0;
                    self.status = if self.browser_workspace.state().targets.is_empty() {
                        "No page targets found · Esc closes".into()
                    } else {
                        "Target picker · type to filter · j/k select · Enter choose · Esc close"
                            .into()
                    };
                }
            }
            "glass.browser.target.select" => {
                if let Some(target_id) = result
                    .pointer("/target/id")
                    .and_then(serde_json::Value::as_str)
                {
                    let mut targets = self.browser_workspace.state().targets.clone();
                    for target in &mut targets {
                        target.selected = target.id == target_id;
                    }
                    self.browser_workspace.replace_targets(targets);
                }
                if let Some(observation) = result.get("observation") {
                    self.apply_browser_result("glass.browser.observe", observation);
                }
            }
            "glass.browser.start" => {
                let connected = result
                    .get("connected")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true);
                let revision = result
                    .get("browserRevision")
                    .and_then(serde_json::Value::as_u64);
                if connected {
                    self.browser_workspace.connected(true, None, revision);
                    self.browser_recovery = None;
                }
            }
            "glass.browser.stop" => {
                self.browser_workspace.state_mut().connection = BrowserConnectionPhase::Detached;
            }
            _ => {}
        }
        self.browser = self.browser_workspace_summary();
    }

    pub fn browser_workspace_summary(&self) -> String {
        let browser = self.browser_workspace.state();
        let entities = if browser.entities.is_empty() {
            "No current semantic entities".into()
        } else {
            browser
                .entities
                .iter()
                .enumerate()
                .skip(browser.semantic_scroll)
                .take(24)
                .map(|(index, entity)| {
                    format!(
                        "{} {} · {} · {}",
                        if Some(index) == browser.selected_entity {
                            "◆"
                        } else {
                            "○"
                        },
                        entity.name,
                        entity.role,
                        entity.reference
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let targets = if browser.targets.is_empty() {
            "No page targets loaded · use `browser targets`".into()
        } else {
            browser
                .targets
                .iter()
                .take(8)
                .map(|target| {
                    format!(
                        "{} {} · {}",
                        if target.selected { "◆" } else { "○" },
                        target.title,
                        safe_browser_url(&target.url).unwrap_or_else(|| "no URL".into())
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        format!(
            "{} · rev {} · {} · owner {}\n{}\n{}\n\nTARGETS\n{}\n\nUNDERSTANDING\n{}\n\nWORKFLOW\n{}",
            browser.connection_label(),
            browser
                .browser_revision
                .map_or_else(|| "—".into(), |revision| revision.to_string()),
            browser.presentation_label(),
            browser.input_owner_label(),
            browser.title,
            safe_browser_url(&browser.url).unwrap_or_else(|| "no page".into()),
            targets,
            entities,
            browser.workflow
        )
    }

    pub fn queue_browser_targets(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        if self.background_action_running() {
            self.status = "Target picker waits for the current background operation".into();
            return;
        }
        if let Err(error) = self.request_browser_target_picker("") {
            self.status = format!("Could not load browser targets: {error}");
            return;
        }
        let Some((call, context)) = self.queued_tool_request.take() else {
            return;
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = "Loading browser targets…".into();
            }
            Err(error) => {
                self.browser_target_picker_requested = false;
                self.status = format!("Could not load browser targets: {error}");
            }
        }
    }

    pub fn request_browser_target_picker(
        &mut self,
        query: impl Into<String>,
    ) -> Result<(), String> {
        if self.background_action_running() {
            return Err("Target picker waits for the current background operation".into());
        }
        self.browser_target_query = query.into();
        self.browser_target_selection = 0;
        self.browser_target_picker = false;
        self.browser_target_picker_requested = true;
        let (call, context) =
            match self.tool_request("glass.browser.targets", serde_json::json!({}), false) {
                Ok(request) => request,
                Err(error) => {
                    self.browser_target_picker_requested = false;
                    return Err(error);
                }
            };
        self.queued_tool_request = Some((call, context));
        self.status = "Loading browser targets…".into();
        Ok(())
    }

    pub fn close_browser_target_picker(&mut self) {
        self.browser_target_picker = false;
        self.browser_target_picker_requested = false;
        self.status = "Target picker closed".into();
    }

    pub fn browser_target_matches(&self) -> Vec<usize> {
        let query = self.browser_target_query.to_ascii_lowercase();
        let terms = query.split_whitespace().collect::<Vec<_>>();
        self.browser_workspace
            .state()
            .targets
            .iter()
            .enumerate()
            .filter(|(_, target)| {
                terms.iter().all(|term| {
                    target.id.to_ascii_lowercase().contains(term)
                        || target.title.to_ascii_lowercase().contains(term)
                        || target.url.to_ascii_lowercase().contains(term)
                })
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub fn insert_browser_target_query(&mut self, character: char) {
        if self.browser_target_query.len() < 512 {
            self.browser_target_query.push(character);
            self.browser_target_selection = 0;
        }
    }

    pub fn browser_target_backspace(&mut self) {
        self.browser_target_query.pop();
        self.browser_target_selection = 0;
    }

    pub fn clear_browser_target_query(&mut self) {
        self.browser_target_query.clear();
        self.browser_target_selection = 0;
    }

    pub fn move_browser_target_selection(&mut self, delta: i32) {
        let count = self.browser_target_matches().len();
        if count == 0 {
            self.browser_target_selection = 0;
            return;
        }
        self.browser_target_selection =
            (self.browser_target_selection as i32 + delta).rem_euclid(count as i32) as usize;
    }

    pub fn select_browser_target(&mut self) {
        if self.background_action_running() {
            self.status = "Finish the current browser operation before selecting a target".into();
            return;
        }
        let matches = self.browser_target_matches();
        let Some(index) = matches.get(self.browser_target_selection).copied() else {
            self.status = "No matching browser target".into();
            return;
        };
        let Some(target) = self.browser_workspace.state().targets.get(index).cloned() else {
            self.status = "Selected browser target disappeared".into();
            return;
        };
        let (call, context) = match self.tool_request(
            "glass.browser.target.select",
            serde_json::json!({"targetId": target.id}),
            true,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.status = format!("Target selection unavailable: {error}");
                return;
            }
        };
        self.browser_target_picker = false;
        let queued = match self.queue_or_confirm(
            call,
            context,
            format!("Select browser target · {}", target.title),
        ) {
            Ok(queued) => queued,
            Err(error) => {
                self.status = format!("Target selection unavailable: {error}");
                return;
            }
        };
        if !queued {
            self.status = "Target selection ready · Enter approves once · Esc cancels".into();
        }
    }

    pub fn browser_target_picker_view(&self) -> String {
        let matches = self.browser_target_matches();
        let mut lines = vec![
            format!(
                "FILTER  {}",
                if self.browser_target_query.is_empty() {
                    "all pages"
                } else {
                    self.browser_target_query.as_str()
                }
            ),
            String::new(),
        ];
        if matches.is_empty() {
            lines.push("No matching page targets.".into());
        } else {
            for (position, index) in matches.iter().take(16).enumerate() {
                let Some(target) = self.browser_workspace.state().targets.get(*index) else {
                    continue;
                };
                let marker = if position == self.browser_target_selection {
                    "›"
                } else {
                    " "
                };
                let url = safe_browser_url(&target.url).unwrap_or_else(|| "no URL".into());
                lines.push(format!(
                    "{marker} {} · {} · {}",
                    target.title, url, target.id
                ));
            }
        }
        lines.push(String::new());
        lines.push("Type filters · Backspace edits · Ctrl-U clears".into());
        lines.push("j/k or ↑/↓ selects · Enter chooses · Esc closes".into());
        lines.join("\n")
    }

    pub fn queue_browser_observe(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        if !self.browser_observe_pending || self.background_action_running() {
            return;
        }
        self.browser_observe_pending = false;
        let (call, context) =
            match self.tool_request("glass.browser.observe", serde_json::json!({}), false) {
                Ok(request) => request,
                Err(error) => {
                    self.status = format!("Could not refresh browser evidence: {error}");
                    return;
                }
            };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = "Refreshing semantic browser evidence…".into();
            }
            Err(error) => self.status = format!("Could not refresh browser evidence: {error}"),
        }
    }

    /// Refresh the newly started page before issuing a revision-bound navigation.
    pub fn continue_pending_browser_navigation(
        &mut self,
        worker: &mut super::snapshot::SnapshotWorker,
    ) {
        let Some(url) = self.pending_browser_navigation.as_deref() else {
            return;
        };
        let (call, context) =
            match self.tool_request("glass.browser.observe", serde_json::json!({}), false) {
                Ok(request) => request,
                Err(error) => {
                    let url = self.pending_browser_navigation.take().unwrap_or_default();
                    self.open_palette_with(&format!("browser navigate {url}"));
                    self.status = format!("Browser ready · navigation retry required: {error}");
                    return;
                }
            };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = format!("Browser ready · preparing navigation to {url}…");
            }
            Err(error) => {
                let url = self.pending_browser_navigation.take().unwrap_or_default();
                self.open_palette_with(&format!("browser navigate {url}"));
                self.status = format!("Browser ready · navigation retry required: {error}");
            }
        }
    }

    /// Submit the retained navigation after its fresh page revision arrives.
    pub fn submit_pending_browser_navigation(
        &mut self,
        worker: &mut super::snapshot::SnapshotWorker,
    ) {
        let Some(url) = self.pending_browser_navigation.take() else {
            return;
        };
        let Some(revision) = self.browser_workspace.state().browser_revision else {
            self.open_palette_with(&format!("browser navigate {url}"));
            self.status = "Browser launch completed without a usable revision".into();
            return;
        };
        let (call, context) = match self.tool_request(
            "glass.browser.navigate",
            serde_json::json!({"url": url, "browserRevision": revision}),
            true,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.open_palette_with(&format!("browser navigate {url}"));
                self.status = format!("Browser ready · navigation retry required: {error}");
                return;
            }
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = format!("Browser ready · navigating to {url}…");
            }
            Err(error) => {
                self.open_palette_with(&format!("browser navigate {url}"));
                self.status = format!("Browser ready · navigation retry required: {error}");
            }
        }
    }

    pub fn queue_browser_intent(&mut self, intent: BrowserWorkspaceIntent) {
        if self.background_action_running() {
            self.status = "Browser action waits for the current background operation".into();
            return;
        }
        let action = match self.browser_workspace.reduce(intent) {
            Ok(Some(action)) => action,
            Ok(None) => return,
            Err(error) => {
                self.status = format!("App action unavailable: {error}");
                return;
            }
        };
        let (tool, arguments) = match action {
            BrowserWorkspaceAction::Navigate {
                url,
                expected_revision,
            } => (
                "glass.browser.navigate",
                serde_json::json!({"url": url, "browserRevision": expected_revision}),
            ),
            BrowserWorkspaceAction::Back { expected_revision }
            | BrowserWorkspaceAction::Forward { expected_revision }
            | BrowserWorkspaceAction::Reload { expected_revision }
            | BrowserWorkspaceAction::StopLoading { expected_revision } => {
                let action = match action {
                    BrowserWorkspaceAction::Back { .. } => "back",
                    BrowserWorkspaceAction::Forward { .. } => "forward",
                    BrowserWorkspaceAction::Reload { .. } => "reload",
                    BrowserWorkspaceAction::StopLoading { .. } => "stopLoading",
                    _ => unreachable!(),
                };
                (
                    "glass.browser.act",
                    serde_json::json!({"action": action, "browserRevision": expected_revision}),
                )
            }
            BrowserWorkspaceAction::Click {
                target,
                expected_revision,
            } => (
                "glass.browser.act",
                serde_json::json!({"action":"click", "target": target, "browserRevision": expected_revision}),
            ),
            BrowserWorkspaceAction::Type {
                target,
                text,
                expected_revision,
            } => (
                "glass.browser.act",
                serde_json::json!({"action":"type", "target": target, "text": text, "browserRevision": expected_revision}),
            ),
            BrowserWorkspaceAction::Scroll {
                dx,
                dy,
                expected_revision,
            } => (
                "glass.browser.act",
                serde_json::json!({"action":"scroll", "dx": dx, "dy": dy, "browserRevision": expected_revision}),
            ),
            _ => return,
        };
        let (call, context) = match self.tool_request(tool, arguments, true) {
            Ok(request) => request,
            Err(error) => {
                self.status = format!("App action unavailable: {error}");
                return;
            }
        };
        let queued = match self.queue_or_confirm(
            call,
            context,
            format!("{tool} · browser revision guarded"),
        ) {
            Ok(queued) => queued,
            Err(error) => {
                self.status = format!("App action unavailable: {error}");
                return;
            }
        };
        if !queued {
            self.status = "Browser mutation ready · Enter approves once · Esc cancels".into();
        }
    }

    pub(super) fn tool_request(
        &self,
        name: &str,
        arguments: serde_json::Value,
        mutating: bool,
    ) -> Result<
        (
            crate::development::ToolCall,
            crate::tools::DevelopmentToolContext,
        ),
        String,
    > {
        // Snapshot refresh owns the workspace lock while it performs filesystem
        // and process inspection. Cached revisions keep chat and actions
        // responsive; the executor still rejects stale guards before mutation.
        let expected_generation = self.snapshot_generation;
        let expected_project_revision = self.snapshot_project_revision;
        Ok((
            crate::development::ToolCall {
                id: format!(
                    "tui-browser-{}",
                    NEXT_BROWSER_TOOL.fetch_add(1, Ordering::Relaxed)
                ),
                name: name.into(),
                arguments,
            },
            crate::tools::DevelopmentToolContext {
                authorization: crate::development::ToolAuthorization {
                    actor: crate::development::Actor::local(),
                    allow_mutation: mutating,
                    confirmed: mutating,
                },
                initiator: None,
                expected_generation,
                expected_project_revision,
            },
        ))
    }

    /// Offer in-TUI recovery whenever a browser tool fails with a launch or
    /// connection error, so a port collision never strands the session.
    pub fn note_browser_failure(&mut self, tool: &str, error: &str) {
        let lower = error.to_ascii_lowercase();
        if tool.contains("browser")
            && (lower.contains("occupied")
                || lower.contains("attach")
                || lower.contains("connect")
                || lower.contains("launch")
                || lower.contains("timeout")
                || lower.contains("devtools"))
        {
            self.browser_target_picker = false;
            self.browser_target_picker_requested = false;
            self.surface = DevSurface::App;
            self.browser_recovery = Some(BrowserRecoveryOffer::from_error(
                error,
                self.browser_recovery
                    .as_ref()
                    .map_or(9222, |offer| offer.port),
            ));
            self.browser_workspace.disconnected(error.to_string(), true);
        }
    }

    /// Queue a recovery choice without blocking the terminal on browser launch.
    pub fn accept_browser_recovery(
        &mut self,
        choice: usize,
        worker: &mut super::snapshot::SnapshotWorker,
    ) {
        let Some(offer) = self.browser_recovery.take() else {
            return;
        };
        let (port, attach) = match (offer.compatible_endpoint, choice) {
            (true, 0) => (offer.port, true),
            (true, 1) | (false, 0) => (free_local_port().unwrap_or(0), false),
            (true, 2) | (false, 1) => (offer.port, false),
            _ => {
                self.status = "Recovery dismissed · project and agent remain available".into();
                return;
            }
        };
        if port == 0 {
            self.status =
                "No free local port was available · retry or use an existing browser".into();
            return;
        }
        let (call, context) = match self.tool_request(
            "glass.browser.start",
            browser_recovery_arguments(port, attach),
            true,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.status = format!("Could not queue recovery: {error}");
                return;
            }
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = if attach {
                    format!("Attaching to browser on port {port} in background…")
                } else {
                    format!("Recovering browser on port {port} in background…")
                };
            }
            Err(error) => self.status = format!("Could not queue recovery: {error}"),
        }
    }

    pub fn refresh_agent_readiness(&mut self) -> Result<bool, String> {
        let readiness = crate::pi_runtime::pi_readiness().map_err(|error| error.to_string())?;
        let ready = readiness.ready;
        self.agent_readiness = format_pi_readiness(&readiness);
        Ok(ready)
    }
}

fn format_editor_buffers(buffers: &[crate::development::EditorBuffer]) -> String {
    if buffers.is_empty() {
        return "No file open. Select a file below and press Enter to open the full-screen editor."
            .into();
    }
    buffers
        .iter()
        .map(|buffer| {
            let lines: Vec<&str> = buffer.content.lines().collect();
            let cursor = buffer.cursor_line as usize;
            let viewport_rows = 16;
            let start = cursor
                .saturating_sub(viewport_rows / 2)
                .min(lines.len().saturating_sub(viewport_rows.min(lines.len())));
            let end = (start + viewport_rows).min(lines.len());
            let gutter_width = lines.len().to_string().len().max(3);
            let viewport = lines[start..end]
                .iter()
                .enumerate()
                .map(|(index, line)| {
                    let number = start + index + 1;
                    let marker = if number == cursor { "▶" } else { " " };
                    format!(
                        "{marker}{:>gutter_width$} │ {line}",
                        number,
                        gutter_width = gutter_width
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "{}{} · cursor {}:{} · actor {} · {} lines\n{}",
                if buffer.dirty { "● " } else { "○ " },
                buffer.path,
                buffer.cursor_line,
                buffer.cursor_column,
                buffer.actor.id,
                lines.len(),
                viewport
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn format_pi_readiness(readiness: &crate::PiReadiness) -> String {
    let provider = readiness.provider.as_deref().unwrap_or("not selected");
    let session = readiness.session.as_deref().unwrap_or("new session");
    let remediation = readiness
        .remediation
        .first()
        .map(|item| format!(" · next: {item}"))
        .unwrap_or_default();
    let state_line = format!(
        "{} · Node {} · SDK {} · auth {}",
        if readiness.ready {
            "✓ Ready"
        } else {
            "○ Needs setup"
        },
        component_label(&readiness.node),
        readiness.sdk.version.as_deref().unwrap_or("missing"),
        component_label(&readiness.authentication),
    );
    if readiness.ready {
        format!("{state_line}\nprovider {provider} · {session}")
    } else {
        format!(
            "{state_line}\nprovider {provider} · {session}{remediation}\n\nUse `:agent setup` to repair the pinned runtime · `:agent update` to refresh it · `:agent setup login` to open Pi `/login`"
        )
    }
}

fn component_label(component: &crate::pi_runtime::PiReadinessComponent) -> String {
    match component.state {
        crate::pi_runtime::PiReadinessState::Ready => "✓".into(),
        crate::pi_runtime::PiReadinessState::Missing => "× missing".into(),
        crate::pi_runtime::PiReadinessState::Incompatible => "! incompatible".into(),
        crate::pi_runtime::PiReadinessState::Expired => "! expired".into(),
        crate::pi_runtime::PiReadinessState::Unknown => "? unknown".into(),
    }
}

fn safe_browser_url(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    let sanitized = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .chars()
        .take(2_048)
        .collect::<String>();
    (!sanitized.is_empty()).then_some(sanitized)
}

fn page_origin(url: &str) -> Option<String> {
    let url = safe_browser_url(url)?;
    let scheme_end = url.find("://")?;
    let authority_start = scheme_end + 3;
    let authority_end = url[authority_start..]
        .find('/')
        .map(|offset| authority_start + offset)
        .unwrap_or(url.len());
    Some(url[..authority_end].to_string())
}
fn bounded_editor_content(content: &str) -> (String, bool) {
    const MAX_CONTEXT_BYTES: usize = 64 * 1024;
    if content.len() <= MAX_CONTEXT_BYTES {
        return (content.to_string(), false);
    }
    let mut end = MAX_CONTEXT_BYTES;
    while end > 0 && !content.is_char_boundary(end) {
        end -= 1;
    }
    (content[..end].to_string(), true)
}

fn pending_agent_approval(
    agents: Option<&Vec<crate::AgentSnapshot>>,
) -> Option<PendingAgentApproval> {
    let agents = agents?;
    for agent in agents {
        let mut pending = None;
        for event in &agent.evidence {
            match event.get("type").and_then(serde_json::Value::as_str) {
                Some("glass_tool_approval_request") => {
                    let Some(frame_id) = event.get("frameId").and_then(serde_json::Value::as_str)
                    else {
                        continue;
                    };
                    pending = Some(PendingAgentApproval {
                        agent_id: agent.id.as_str().to_string(),
                        frame_id: frame_id.to_string(),
                        tool_name: event
                            .get("toolName")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("unknown tool")
                            .to_string(),
                        arguments: event.get("arguments").cloned().unwrap_or_default(),
                    });
                }
                Some("glass_tool_approval_resolved") => {
                    let frame_id = event.get("frameId").and_then(serde_json::Value::as_str);
                    if pending
                        .as_ref()
                        .is_some_and(|approval| Some(approval.frame_id.as_str()) == frame_id)
                    {
                        pending = None;
                    }
                }
                _ => {}
            }
        }
        if pending.is_some() {
            return pending;
        }
    }
    None
}

fn fuzzy_contains(candidate: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let mut query = query
        .chars()
        .map(|character| character.to_ascii_lowercase());
    let mut expected = query.next();
    for character in candidate
        .chars()
        .map(|character| character.to_ascii_lowercase())
    {
        if Some(character) == expected {
            expected = query.next();
            if expected.is_none() {
                return true;
            }
        }
    }
    false
}

fn browser_recovery_arguments(port: u16, attach: bool) -> serde_json::Value {
    serde_json::json!({
        "port": port,
        "attach": attach,
        "incognito": !attach,
    })
}

/// Bind an ephemeral localhost port to discover a free one.
fn free_local_port() -> Option<u16> {
    std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .ok()
        .and_then(|listener| listener.local_addr().ok())
        .map(|address| address.port())
}

fn editor_offset(content: &str, line: u32, column: u32) -> usize {
    let mut offset = 0;
    for (index, value) in content.split_inclusive('\n').enumerate() {
        if index + 1 == line as usize {
            let column_bytes = value
                .trim_end_matches('\n')
                .char_indices()
                .nth(column.saturating_sub(1) as usize)
                .map(|(offset, _)| offset)
                .unwrap_or_else(|| value.trim_end_matches('\n').len());
            return offset + column_bytes;
        }
        offset += value.len();
    }
    content.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_confirmation_requires_explicit_follow_through() {
        let root = std::env::temp_dir().join(format!("glass-tui-quit-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");

        state.request_quit();
        assert!(state.quit_confirmation);
        assert!(!state.quit);
        assert_eq!(state.status, "Quit confirmation · Enter exits · Esc stays");

        state.cancel_quit();
        assert!(!state.quit_confirmation);
        assert!(!state.quit);

        state.request_quit();
        state.confirm_quit();
        assert!(!state.quit_confirmation);
        assert!(state.quit);
        assert_eq!(state.status, "Closing Glass Dev");

        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }
    #[test]
    fn trust_action_menu_dispatches_keyboard_hints() {
        let root = std::env::temp_dir().join(format!("glass-trust-menu-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.surface = DevSurface::Trust;

        state.open_menu();
        state.menu_selection = 0;
        state.run_menu_action();
        assert!(!state.menu_open);
        assert!(
            state
                .status
                .contains("Inspecting exact repository configuration")
        );

        state.open_menu();
        state.menu_selection = 1;
        state.run_menu_action();
        assert_eq!(state.surface, DevSurface::Agent);
        assert!(
            state
                .status
                .contains("Workspace opened with untrusted authority")
        );

        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn browser_context_url_redaction_preserves_path_only() {
        assert_eq!(
            safe_browser_url("https://example.test/orders/7?token=secret#receipt"),
            Some("https://example.test/orders/7".into())
        );
        assert_eq!(
            page_origin("https://example.test/orders/7?token=secret"),
            Some("https://example.test".into())
        );
        assert_eq!(safe_browser_url(""), None);
    }

    #[test]
    fn browser_recovery_offer_explains_port_collision_choices() {
        let offer = BrowserRecoveryOffer::from_error("address already in use on port 9222", 9222);
        assert!(!offer.compatible_endpoint);
        assert_eq!(offer.actions().len(), 3);
        assert!(offer.guidance().contains("preferred endpoint"));

        let attach =
            BrowserRecoveryOffer::from_error("DevTools page target is available for attach", 9222);
        assert!(attach.compatible_endpoint);
        assert_eq!(attach.actions().len(), 4);
        assert!(attach.guidance().contains("Attach"));
    }

    #[test]
    fn browser_recovery_attach_disables_incognito() {
        assert_eq!(
            browser_recovery_arguments(9222, true),
            serde_json::json!({
                "port": 9222,
                "attach": true,
                "incognito": false,
            })
        );
        assert_eq!(
            browser_recovery_arguments(42123, false),
            serde_json::json!({
                "port": 42123,
                "attach": false,
                "incognito": true,
            })
        );
    }

    #[test]
    fn browser_target_picker_filters_and_redacts_urls() {
        let root = std::env::temp_dir().join(format!("glass-target-picker-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.browser_workspace.replace_targets(vec![
            BrowserWorkspaceTarget {
                id: "page-docs".into(),
                title: "Project docs".into(),
                url: "https://example.test/docs?token=secret".into(),
                selected: false,
            },
            BrowserWorkspaceTarget {
                id: "page-app".into(),
                title: "Application".into(),
                url: "http://localhost:3000".into(),
                selected: false,
            },
        ]);
        state
            .request_browser_target_picker("docs")
            .expect("queue target picker");
        state.queued_tool_request = None;
        let matches = state.browser_target_matches();
        assert_eq!(matches, vec![0]);
        let view = state.browser_target_picker_view();
        assert!(view.contains("Project docs"));
        assert!(view.contains("https://example.test/docs"));
        assert!(!view.contains("token=secret"));
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }
    #[test]
    fn agent_typing_opens_composer_before_surface_aliases() {
        let root = std::env::temp_dir().join(format!("glass-agent-compose-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.surface = DevSurface::Agent;
        state.snapshot_trust_label = "trusted".into();
        state.agent_readiness = "✓ Ready · Node ✓ · SDK 0.84.3 · auth ✓".into();

        state.handle_printable('c');

        assert_eq!(state.surface, DevSurface::Agent);
        assert!(state.composer_mode);
        assert_eq!(state.composer_input, "c");
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn leading_agent_message_characters_are_not_commands() {
        for character in ['a', 'q', 's', 'u', 'l', 'i', 'n'] {
            let root = std::env::temp_dir().join(format!(
                "glass-agent-leading-{character}-{}",
                std::process::id()
            ));
            std::fs::create_dir_all(&root).expect("create temporary workspace");
            let mut state = DevTuiState::open_for_tui(&root, TuiLayout::Desktop)
                .expect("open temporary workspace");
            state.surface = DevSurface::Agent;
            state.snapshot_trust_label = "trusted".into();
            state.agent_readiness = "✓ Ready · Node ✓ · SDK 0.84.3 · auth ✓".into();

            state.handle_printable(character);

            assert!(
                state.composer_mode,
                "character {character:?} did not open composer"
            );
            assert_eq!(state.composer_input, character.to_string());
            std::fs::remove_dir_all(root).expect("remove temporary workspace");
        }
    }
    #[test]
    fn action_shortcut_opens_menu_without_stealing_ready_agent_text() {
        let root =
            std::env::temp_dir().join(format!("glass-action-shortcut-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");

        state.surface = DevSurface::Terminal;
        state.handle_printable('a');
        assert!(state.menu_open);
        state.close_menu();

        state.surface = DevSurface::Agent;
        state.snapshot_trust_label = "trusted".into();
        state.agent_readiness = "✓ Ready · Node ✓ · SDK 0.84.3 · auth ✓".into();
        state.handle_printable('a');
        assert!(state.composer_mode);
        assert_eq!(state.composer_input, "a");

        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn colon_opens_palette_from_every_surface() {
        for surface in DevSurface::ALL {
            let root = std::env::temp_dir().join(format!(
                "glass-palette-surface-{}-{}",
                surface.label(),
                std::process::id()
            ));
            std::fs::create_dir_all(&root).expect("create temporary workspace");
            let mut state = DevTuiState::open_for_tui(&root, TuiLayout::Desktop)
                .expect("open temporary workspace");
            state.surface = surface;

            state.handle_printable(':');

            assert!(
                state.command_mode,
                "colon did not open palette on {surface:?}"
            );
            assert!(!state.composer_mode);
            std::fs::remove_dir_all(root).expect("remove temporary workspace");
        }
    }
    #[test]
    fn external_delegate_result_is_visible_in_tui_status() {
        let root =
            std::env::temp_dir().join(format!("glass-delegate-status-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.running_tool_job = Some(7);
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 7,
            tool: "glass.agent.delegate".into(),
            result: Ok(serde_json::json!({
                "harness": "codex",
                "success": true,
                "timedOut": false,
                "output": "REAL_RESULT\nsecond line\n",
                "stderr": "",
            })),
        });
        assert!(state.status.contains("Temporary codex completed"));
        assert!(state.status.contains("REAL_RESULT"));
        assert_eq!(state.surface, DevSurface::Agent);
        assert!(state.running_tool_job.is_none());
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn agent_editor_context_attaches_unsaved_buffer_and_inline_prompt() {
        let root =
            std::env::temp_dir().join(format!("glass-editor-agent-context-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open editor buffer");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .edit_buffer(
                "src/main.rs",
                "fn main() {\n    println!(\"unsaved\");\n}\n".into(),
                crate::development::Actor::local(),
            )
            .expect("edit buffer");
        state.refresh_editor_projection();

        let context = state.agent_editor_context();
        assert_eq!(context["focusedPath"], "src/main.rs");
        assert_eq!(
            context["focusedBuffer"]["content"],
            "fn main() {\n    println!(\"unsaved\");\n}\n"
        );
        assert_eq!(context["focusedBuffer"]["dirty"], true);

        state.enter_code_edit();
        state.edit_code_key(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::ALT,
        );
        assert_eq!(state.surface, DevSurface::Agent);
        assert!(state.composer_mode);
        assert!(!state.code_edit_mode);
        assert!(state.composer_input.contains("src/main.rs:1:1"));
        assert!(state.composer_input.contains("Do not edit files"));

        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn editor_soft_wrap_toggle_scrolls_visual_rows_and_resets_horizontal_scroll() {
        let root =
            std::env::temp_dir().join(format!("glass-editor-soft-wrap-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        let content = format!("{}\n", "word ".repeat(80));
        std::fs::write(root.join("src/main.rs"), content).expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Mobile).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open editor buffer");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .set_buffer_cursor("src/main.rs", 1, 180)
            .expect("set editor cursor");
        state.refresh_editor_projection();
        state.set_terminal_size(32, 16);
        state.enter_code_edit();
        state.edit_code_key(
            crossterm::event::KeyCode::Char('w'),
            crossterm::event::KeyModifiers::ALT,
        );

        assert!(state.editor_soft_wrap);
        assert!(state.editor_scroll_line > 0);
        assert_eq!(state.editor_scroll_column, 0);
        assert!(state.status.contains("Soft wrap ON"));

        state.edit_code_key(
            crossterm::event::KeyCode::Char('w'),
            crossterm::event::KeyModifiers::ALT,
        );
        assert!(!state.editor_soft_wrap);
        assert!(state.editor_scroll_column > 0);
        assert!(state.status.contains("Soft wrap OFF"));
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn editor_cursor_scroll_matches_fullscreen_source_height() {
        let root =
            std::env::temp_dir().join(format!("glass-editor-viewport-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        let content = (1..=30)
            .map(|line| format!("line {line}\n"))
            .collect::<String>();
        std::fs::write(root.join("src/main.rs"), content).expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open editor buffer");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .set_buffer_cursor("src/main.rs", 23, 1)
            .expect("set editor cursor");
        state.refresh_editor_projection();
        state.editor_scroll_line = 0;
        state.set_terminal_size(100, 30);

        assert_eq!(state.focused_editor_line, 23);
        assert_eq!(
            state.editor_scroll_line, 1,
            "line 23 must scroll into the 22-row source viewport"
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn editor_shift_selection_replaces_text_and_clears_anchor() {
        let root =
            std::env::temp_dir().join(format!("glass-editor-selection-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(root.join("src/main.rs"), "alpha\nbeta\n").expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open editor buffer");
        state.refresh_editor_projection();
        state.enter_code_edit();
        for _ in 0..5 {
            state.edit_code_key(
                crossterm::event::KeyCode::Right,
                crossterm::event::KeyModifiers::SHIFT,
            );
        }
        assert!(
            state
                .focused_editor_selection
                .as_ref()
                .is_some_and(|selection| !selection.is_empty())
        );
        state.edit_code_key(
            crossterm::event::KeyCode::Char('X'),
            crossterm::event::KeyModifiers::empty(),
        );
        let buffer = state.focused_buffer().expect("focused editor buffer");
        assert_eq!(buffer.content, "X\nbeta\n");
        assert_eq!((buffer.cursor_line, buffer.cursor_column), (1, 2));
        assert!(buffer.selection.is_none());
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn editor_exit_prompts_save_discard_and_discard_quit() {
        let root = std::env::temp_dir().join(format!("glass-editor-exit-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        let original = "fn main() {}\n";
        std::fs::write(root.join("src/main.rs"), original).expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open editor buffer");
        state.refresh_editor_projection();
        state.enter_code_edit();
        state.edit_code_key(
            crossterm::event::KeyCode::Char('#'),
            crossterm::event::KeyModifiers::NONE,
        );
        state.request_editor_exit();
        assert_eq!(state.editor_exit_prompt, Some(EditorExitPrompt::Unsaved));
        state.handle_editor_exit_key(crossterm::event::KeyCode::Char('d'));
        assert!(!state.code_edit_mode);
        assert_eq!(
            std::fs::read_to_string(root.join("src/main.rs")).expect("read source"),
            original
        );

        state.enter_code_edit();
        state.edit_code_key(
            crossterm::event::KeyCode::Char('#'),
            crossterm::event::KeyModifiers::NONE,
        );
        state.request_editor_exit();
        state.handle_editor_exit_key(crossterm::event::KeyCode::Char('s'));
        assert!(!state.code_edit_mode);
        assert!(
            std::fs::read_to_string(root.join("src/main.rs"))
                .expect("read saved source")
                .starts_with('#')
        );
        let saved = std::fs::read_to_string(root.join("src/main.rs")).expect("read saved source");

        state.enter_code_edit();
        state.edit_code_key(
            crossterm::event::KeyCode::Char('!'),
            crossterm::event::KeyModifiers::NONE,
        );
        state.request_editor_exit();
        state.handle_editor_exit_key(crossterm::event::KeyCode::Char('q'));
        assert!(state.quit);
        assert_eq!(
            std::fs::read_to_string(root.join("src/main.rs")).expect("read discarded source"),
            saved
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn conversation_view_does_not_echo_pending_message_after_snapshot() {
        let root = std::env::temp_dir().join(format!("glass-chat-merge-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.agent_conversation = "YOU\ninspect the failing test\n\nGLASS AGENT\nworking".into();
        state.pending_chat_messages.push(PendingChatMessage {
            text: "inspect the failing test".into(),
            state: ChatMessageState::Sent,
            job_id: None,
            error: None,
        });

        let view = state.conversation_view();
        assert_eq!(view.matches("YOU\ninspect the failing test").count(), 1);
        assert!(!view.contains("Glass Agent is thinking"));

        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn palette_filters_actions_and_prefills_selected_arguments() {
        let root =
            std::env::temp_dir().join(format!("glass-palette-actions-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.surface = DevSurface::Agent;
        state.open_palette();
        state.insert_palette_text("agent setup");
        assert_eq!(
            state.selected_palette_action().map(|action| action.label),
            Some("Setup Pi runtime")
        );
        state.move_palette_selection(1);
        assert_eq!(
            state.selected_palette_action().map(|action| action.label),
            Some("Authenticate")
        );

        state.open_palette_with("agent rewind ENTRY_ID");
        let action = state.selected_palette_action().expect("rewind action");
        assert_eq!(action.label, "Rewind Pi session");
        assert!(state.prepare_palette_action(action).is_none());
        assert!(state.command_mode);
        assert_eq!(state.command_input, "agent rewind ");
        assert!(state.status.contains("required value"));

        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn arrow_surface_cycle_matches_layout_order() {
        let root =
            std::env::temp_dir().join(format!("glass-surface-arrows-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut desktop =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open desktop workspace");
        desktop.surface = DevSurface::Agent;
        desktop.next_surface();
        assert_eq!(desktop.surface, DevSurface::Code);
        assert_eq!(desktop.status, "Code selected");
        desktop.previous_surface();
        assert_eq!(desktop.surface, DevSurface::Agent);
        desktop.surface = DevSurface::Debug;
        desktop.next_surface();
        assert_eq!(desktop.surface, DevSurface::More);
        desktop.next_surface();
        assert_eq!(desktop.surface, DevSurface::Agent);

        let mut phone =
            DevTuiState::open_for_tui(&root, TuiLayout::Mobile).expect("open phone workspace");
        phone.surface = DevSurface::Agent;
        phone.previous_surface();
        assert_eq!(phone.surface, DevSurface::More);
        assert_eq!(phone.status, "More selected");
        phone.next_surface();
        assert_eq!(phone.surface, DevSurface::Agent);

        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }
}
