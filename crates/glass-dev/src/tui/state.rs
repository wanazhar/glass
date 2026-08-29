use super::command;
use super::editor::{
    self as native, EditorEngine, EditorMode, GhostText, Motion, Operator, TextObject,
    apply_motion, compile_prove_it, complete_mention, evidence_card, expand_mentions,
    inferred_app_path, join_app_url, line_hunks, local_fim, multi_delete, multi_insert,
    next_edit_after_accept, pair_apply_caret, pair_apply_step, parse_inlay_hints, same_word_ranges,
    split_ghost_word, textobject_from_key, textobject_selection,
};
use super::parse::IncrementalSyntax;
use crate::development::TextSelection;
use crate::{ExperimentComparison, SharedDevelopmentWorkspace};
use glass_browser::browser::WorkflowRecorder;
use glass_browser::browser::policy::PolicyPreset;
use glass_browser::browser::session::VerificationPredicate;
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
pub struct TuiWorkflowRecording {
    pub name: String,
    pub recorder: WorkflowRecorder,
    step: usize,
}

impl TuiWorkflowRecording {
    fn next_id(&mut self) -> String {
        self.step += 1;
        format!("step-{}", self.step)
    }
}

enum PendingFim {
    Thread {
        path: String,
        offset: usize,
        rx: std::sync::mpsc::Receiver<Option<String>>,
    },
    Pi {
        path: String,
        offset: usize,
        agent_id: crate::AgentId,
        since: u64,
    },
}

impl PendingFim {
    fn path(&self) -> &str {
        match self {
            Self::Thread { path, .. } | Self::Pi { path, .. } => path,
        }
    }

    fn offset(&self) -> usize {
        match self {
            Self::Thread { offset, .. } | Self::Pi { offset, .. } => *offset,
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspacePlan {
    pub id: String,
    pub goal: String,
    pub body: String,
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPickerItem {
    pub path: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRow {
    pub name: String,
    pub command: String,
    pub pid: Option<u32>,
    pub health: crate::development::ProcessHealth,
    pub url: Option<String>,
}

impl ProcessRow {
    pub fn from_snapshot(item: &crate::development::ProcessSnapshot) -> Self {
        Self {
            name: item.name.clone(),
            command: item.command.clone(),
            pid: item.pid,
            health: item.health.clone(),
            url: item.detected_urls.first().cloned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugSessionRow {
    pub name: String,
    pub state: crate::debugger::DebugSessionState,
    pub pid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugThreadRow {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugFrameRow {
    pub id: i64,
    pub name: String,
    pub path: Option<String>,
    pub line: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DebugPane {
    #[default]
    Sessions,
    Threads,
    Frames,
}

impl DebugPane {
    pub fn label(self) -> &'static str {
        match self {
            Self::Sessions => "sessions",
            Self::Threads => "threads",
            Self::Frames => "frames",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Sessions => Self::Threads,
            Self::Threads => Self::Frames,
            Self::Frames => Self::Sessions,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Sessions => Self::Frames,
            Self::Threads => Self::Sessions,
            Self::Frames => Self::Threads,
        }
    }
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
    pub composer_run_mode: crate::AgentTurnMode,
    pub pending_plan: Option<WorkspacePlan>,
    pub composer_input: String,
    pub composer_cursor: usize,
    pub composer_steer: bool,
    pub last_app_comment: Option<String>,
    pub session_todos: crate::SessionTodoList,
    pub composer_history: Vec<String>,
    pub composer_history_index: Option<usize>,
    pub composer_history_draft: String,
    pub file_picker_open: bool,
    pub file_picker_query: String,
    pub file_picker_cursor: usize,
    pub file_picker_selection: usize,
    pub transcript_selection: usize,
    pub transcript_expanded: bool,
    pub session_picker_open: bool,
    pub session_picker_items: Vec<SessionPickerItem>,
    pub session_picker_selection: usize,
    pub conversation_items: Vec<super::projection::ConversationEntry>,
    pub agent_model: String,
    pub agent_thinking: String,
    pub agent_session_name: String,
    pub agent_token_summary: String,
    pub git_branch: String,
    pub git_dirty: bool,
    pub pending_chat_messages: Vec<PendingChatMessage>,
    pub agent_send_job: Option<u64>,
    pub selected_agent: Option<crate::AgentId>,
    pub pending_confirmation: Option<PendingConfirmation>,
    /// URL retained while the TUI launches a detached browser before navigation.
    pub pending_browser_navigation: Option<String>,
    pub pending_page_entity: Option<String>,
    pub process_urls: Vec<String>,
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
    pub editor_diagnostics: Vec<crate::development::LanguageDiagnostic>,
    pub editor_inlays: Vec<(u32, String)>,
    pub last_crew_wake: Option<String>,
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
    pub editor_engine: EditorEngine,
    pub syntax: IncrementalSyntax,
    pub last_verify: Option<String>,
    pub last_proof_ok: Option<bool>,
    pub pending_verify: Option<serde_json::Value>,
    pub factory_split: bool,
    pub lsp: String,
    pub processes: String,
    pub process_entries: Vec<ProcessRow>,
    pub selected_process: usize,
    pub process_logs: String,
    pub process_logs_requested: bool,
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
    pub debug_sessions: Vec<DebugSessionRow>,
    pub selected_debug_session: usize,
    pub debug_threads: Vec<DebugThreadRow>,
    pub selected_debug_thread: usize,
    pub debug_frames: Vec<DebugFrameRow>,
    pub selected_debug_frame: usize,
    pub debug_pane: DebugPane,
    pub debug_threads_requested: bool,
    pub debug_stack_requested: bool,
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
    pub workflow_recording: Option<TuiWorkflowRecording>,
    pending_fim: Option<PendingFim>,
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
            "Ready · Enter chat · Ctrl-P files · : commands · ? help".to_string()
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
            composer_run_mode: crate::AgentTurnMode::Agent,
            pending_plan: None,
            composer_input: String::new(),
            composer_cursor: 0,
            composer_steer: false,
            last_app_comment: None,
            session_todos: crate::SessionTodoList::default(),
            composer_history: Vec::new(),
            composer_history_index: None,
            composer_history_draft: String::new(),
            file_picker_open: false,
            file_picker_query: String::new(),
            file_picker_cursor: 0,
            file_picker_selection: 0,
            transcript_selection: 0,
            transcript_expanded: false,
            session_picker_open: false,
            session_picker_items: Vec::new(),
            session_picker_selection: 0,
            conversation_items: Vec::new(),
            agent_model: String::new(),
            agent_thinking: String::new(),
            agent_session_name: String::new(),
            agent_token_summary: String::new(),
            git_branch: String::new(),
            git_dirty: false,
            pending_chat_messages: Vec::new(),
            agent_send_job: None,
            selected_agent: None,
            pending_confirmation: None,
            editor_exit_prompt: None,
            pending_agent_approval: None,
            pending_browser_navigation: None,
            pending_page_entity: None,
            process_urls: Vec::new(),
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
            editor_diagnostics: Vec::new(),
            editor_inlays: Vec::new(),
            last_crew_wake: None,
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
            editor_engine: EditorEngine {
                mode: EditorMode::Insert,
                ..EditorEngine::default()
            },
            syntax: IncrementalSyntax::new(),
            last_verify: None,
            last_proof_ok: None,
            pending_verify: None,
            factory_split: true,
            lsp: String::new(),
            processes: String::new(),
            process_entries: Vec::new(),
            selected_process: 0,
            process_logs: String::new(),
            process_logs_requested: false,
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
            debug_sessions: Vec::new(),
            selected_debug_session: 0,
            debug_threads: Vec::new(),
            selected_debug_thread: 0,
            debug_frames: Vec::new(),
            selected_debug_frame: 0,
            debug_pane: DebugPane::Sessions,
            debug_threads_requested: false,
            debug_stack_requested: false,
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
            workflow_recording: None,
            pending_fim: None,
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

    pub fn start_workflow_recording(&mut self, name: &str) -> Result<String, String> {
        if let Some(recording) = &self.workflow_recording {
            return Err(format!(
                "already recording {} · :workflow record stop first",
                recording.name
            ));
        }
        let name = workflow_slug(name);
        self.workflow_recording = Some(TuiWorkflowRecording {
            name: name.clone(),
            recorder: WorkflowRecorder::new(name.clone(), "1.0.0"),
            step: 0,
        });
        self.surface = DevSurface::App;
        self.refresh_workflow_recording_status();
        self.status = format!("Recording {name} · Enter activates and records locators");
        Ok(self.status.clone())
    }

    pub fn workflow_recording_status(&self) -> String {
        match &self.workflow_recording {
            Some(recording) => format!(
                "recording {} · {} step(s)",
                recording.name,
                recording.recorder.draft().steps.len()
            ),
            None => "not recording · :workflow record start NAME".into(),
        }
    }

    pub fn capture_workflow_click(&mut self) -> Result<bool, String> {
        if self.workflow_recording.is_none() {
            return Ok(false);
        }
        let selected = self
            .browser_workspace
            .state()
            .selected()
            .cloned()
            .filter(|entity| entity.actionable)
            .ok_or("no actionable semantic entity is selected")?;
        let Some(recording) = self.workflow_recording.as_mut() else {
            return Ok(false);
        };
        let id = recording.next_id();
        recording
            .recorder
            .record_click(id, selected.role, selected.name, None)
            .map_err(|error| error.to_string())?;
        self.refresh_workflow_recording_status();
        Ok(true)
    }

    pub fn record_workflow_type(&mut self, input_name: &str) -> Result<String, String> {
        let selected = self
            .browser_workspace
            .state()
            .selected()
            .cloned()
            .ok_or("no semantic entity is selected")?;
        let recording = self
            .workflow_recording
            .as_mut()
            .ok_or("start a recording first")?;
        let id = recording.next_id();
        let input = workflow_slug(input_name);
        recording
            .recorder
            .record_type_input(id, selected.role, selected.name, input.clone())
            .map_err(|error| error.to_string())?;
        let steps = recording.recorder.draft().steps.len();
        self.refresh_workflow_recording_status();
        self.status = format!("REC {steps} · typed ${{inputs.{input}}}");
        Ok(self.status.clone())
    }

    pub fn record_workflow_verify(&mut self) -> Result<String, String> {
        let value = self
            .pending_verify
            .clone()
            .ok_or("no pending prove-it predicate to attach")?;
        let expect: VerificationPredicate = serde_json::from_value(value)
            .map_err(|error| format!("pending verify is not a workflow predicate: {error}"))?;
        let recording = self
            .workflow_recording
            .as_mut()
            .ok_or("start a recording first")?;
        recording
            .recorder
            .attach_expect_to_last(expect)
            .map_err(|error| error.to_string())?;
        let steps = recording.recorder.draft().steps.len();
        self.refresh_workflow_recording_status();
        self.status = format!("REC {steps} · attached prove-it to the last step");
        Ok(self.status.clone())
    }

    pub fn stop_workflow_recording(&mut self) -> Result<String, String> {
        let recording = self
            .workflow_recording
            .take()
            .ok_or("no click-path recording is active")?;
        let draft = recording.recorder.draft();
        if draft.steps.is_empty() {
            self.workflow = "No workflow evidence yet".into();
            return Err("recording had no steps · activate entities before stop".into());
        }
        let dir = Path::new(&self.snapshot_root)
            .join(".glass")
            .join("workflows");
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        let path = dir.join(format!("{}.draft.json", recording.name));
        let encoded = serde_json::to_vec_pretty(draft).map_err(|error| error.to_string())?;
        std::fs::write(&path, &encoded).map_err(|error| error.to_string())?;
        self.workflow = format!(
            "Draft {} · {} step(s) · {}",
            recording.name,
            draft.steps.len(),
            path.display()
        );
        self.status = format!(
            "Recorded {} step(s) · {}",
            draft.steps.len(),
            path.display()
        );
        Ok(self.status.clone())
    }

    fn refresh_workflow_recording_status(&mut self) {
        if let Some(recording) = &self.workflow_recording {
            self.workflow = format!(
                "REC {} · {} step(s) · :workflow record stop",
                recording.name,
                recording.recorder.draft().steps.len()
            );
        }
    }

    fn capture_workflow_type_from_selection(&mut self) {
        if self.workflow_recording.is_none() {
            return;
        }
        let input = self
            .browser_workspace
            .state()
            .selected()
            .map(|entity| slug_input_name(&entity.name))
            .unwrap_or_else(|| "value".into());
        let _ = self.record_workflow_type(&input);
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
        self.close_file_picker();
        self.command_mode = true;
        self.command_input.clear();
        self.command_cursor = 0;
        self.command_history_index = None;
        self.palette_error = None;
        self.palette_scroll = 0;
        self.palette_selection = 0;
        self.status = format!(
            "Command search · {} actions · ↑↓ select · Enter run · Ctrl-P files",
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
        let typed_root = typed.split_whitespace().next().unwrap_or("");
        let command = match (typed_root, self.selected_palette_action()) {
            ("a" | "actions" | "help" | "?" | "q" | "quit" | "open" | "search" | "doctor", _) => {
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
            "debug threads SESSION" | "debug continue SESSION THREAD_ID" => {
                if let Some(session) = self.selected_debug_session().map(|row| row.name.clone()) {
                    let action = action
                        .command
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("threads");
                    if action == "continue" {
                        if let Some(thread) = self.selected_debug_thread() {
                            self.open_palette_with(&format!(
                                "debug continue {session} {}",
                                thread.id
                            ));
                        } else {
                            self.open_palette_with(&format!("debug continue {session} "));
                        }
                    } else {
                        self.open_palette_with(&format!("debug threads {session}"));
                    }
                    None
                } else {
                    self.open_palette_with("debug threads ");
                    None
                }
            }
            "process logs NAME" | "process stop NAME" | "process restart NAME" => {
                if let Some(name) = self
                    .selected_process_entry()
                    .map(|entry| entry.name.clone())
                {
                    let action = action.command.split_whitespace().nth(1).unwrap_or("logs");
                    self.open_palette_with(&format!("process {action} {name}"));
                    None
                } else {
                    self.open_palette_with(&format!(
                        "{} ",
                        action
                            .command
                            .split_whitespace()
                            .take(2)
                            .collect::<Vec<_>>()
                            .join(" ")
                    ));
                    None
                }
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

    pub fn conversation_entries_view(&self) -> Vec<super::projection::ConversationEntry> {
        let mut entries = self.conversation_items.clone();
        if entries.is_empty() && !self.agent_conversation.starts_with("No conversation yet.") {
            entries = parse_conversation_view(&self.agent_conversation);
        }
        let mut observed = std::collections::BTreeMap::<String, usize>::new();
        for message in &self.pending_chat_messages {
            if message.state != ChatMessageState::Failed {
                let seen = observed.entry(message.text.clone()).or_default();
                let existing = entries
                    .iter()
                    .filter(|entry| {
                        entry.kind == super::projection::ConversationKind::User
                            && entry.text == message.text
                    })
                    .count();
                if *seen < existing {
                    *seen += 1;
                    continue;
                }
            }
            let suffix = match message.state {
                ChatMessageState::Sending => "· sending…",
                ChatMessageState::Sent => "· sent · Glass Agent is thinking…",
                ChatMessageState::Failed => "× send failed · press Enter to retry",
            };
            entries.push(super::projection::ConversationEntry {
                kind: super::projection::ConversationKind::User,
                text: format!("{}\n{suffix}", message.text),
                streaming: false,
                entry_id: None,
                tool_name: None,
            });
        }
        entries
    }

    pub fn composer_context_chips(&self) -> String {
        let mut chips = Vec::new();
        if !self.focused_editor_path.is_empty() {
            chips.push(format!("@{}", self.focused_editor_path));
        } else if let Some(path) = self.files.get(self.selected_file) {
            chips.push(format!("@{path}"));
        }
        if self.focused_editor_selection.is_some() {
            chips.push("selection".into());
        }
        if let Some(entity) = self.browser_workspace.state().selected() {
            chips.push(format!("app {}", entity.name));
        }
        if self.pending_verify.is_some() {
            chips.push("prove-it".into());
        }
        if self.composer_steer {
            chips.push("steer".into());
        }
        chips.join(" · ")
    }

    pub fn agent_chrome_line(&self) -> String {
        let mut parts = Vec::new();
        if !self.agent_model.is_empty() {
            parts.push(self.agent_model.clone());
        }
        if !self.agent_thinking.is_empty() {
            parts.push(format!("think {}", self.agent_thinking));
        }
        if !self.agent_session_name.is_empty() {
            parts.push(self.agent_session_name.clone());
        }
        if !self.agent_token_summary.is_empty() {
            parts.push(self.agent_token_summary.clone());
        }
        let queued = self
            .pending_chat_messages
            .iter()
            .filter(|message| matches!(message.state, ChatMessageState::Sending))
            .count();
        if queued > 0 {
            parts.push(format!("queued {queued}"));
        }
        parts.join(" · ")
    }

    pub fn move_transcript_selection(&mut self, delta: i32) {
        let len = self.conversation_entries_view().len();
        if len == 0 {
            self.scroll_surface(delta);
            return;
        }
        let next = self.transcript_selection as i32 + delta;
        self.transcript_selection = next.clamp(0, len as i32 - 1) as usize;
        self.transcript_expanded = false;
        self.status = format!(
            "Bubble {}/{} · f fork · r rewind · e edit last · o expand",
            self.transcript_selection + 1,
            len
        );
    }

    pub fn toggle_transcript_expand(&mut self) {
        self.transcript_expanded = !self.transcript_expanded;
        self.status = if self.transcript_expanded {
            "Expanded tool card · o collapse".into()
        } else {
            "Collapsed · o expand".into()
        };
    }

    pub fn fork_selected_transcript(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        self.branch_selected_transcript("glass.agent.fork", worker);
    }

    pub fn rewind_selected_transcript(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        self.branch_selected_transcript("glass.agent.rewind", worker);
    }

    fn branch_selected_transcript(
        &mut self,
        tool: &str,
        worker: &mut super::snapshot::SnapshotWorker,
    ) {
        let Some(entry) = self
            .conversation_entries_view()
            .get(self.transcript_selection)
            .cloned()
        else {
            self.status = "No transcript bubble to branch".into();
            return;
        };
        let Some(entry_id) = entry.entry_id.clone() else {
            self.status = "Selected bubble has no Pi entry · send a turn first".into();
            return;
        };
        let mut arguments = serde_json::json!({"entryId": entry_id});
        if let Some(agent) = self.selected_agent.as_ref() {
            arguments["agentId"] = serde_json::Value::String(agent.as_str().to_string());
        }
        match self.tool_request(tool, arguments, true) {
            Ok((call, context)) => match worker.submit_tool(call, context) {
                Ok(_) => {
                    self.status = format!("{tool} · branching at {entry_id}");
                    worker.request_conversation();
                }
                Err(error) => self.status = format!("Could not {tool}: {error}"),
            },
            Err(error) => self.status = format!("Could not {tool}: {error}"),
        }
    }

    pub fn edit_last_user_message(&mut self) {
        let entries = self.conversation_entries_view();
        let Some(entry) = entries
            .iter()
            .rev()
            .find(|entry| entry.kind == super::projection::ConversationKind::User)
        else {
            self.status = "No user message to edit".into();
            return;
        };
        let text = entry
            .text
            .lines()
            .next()
            .unwrap_or(entry.text.as_str())
            .to_string();
        self.composer_input = text;
        self.composer_cursor = self.composer_input.len();
        self.composer_steer = false;
        self.open_composer();
        if let Some(entry_id) = entry.entry_id.clone() {
            self.status = format!("Editing last user message · rewind {entry_id} on send");
        } else {
            self.status = "Editing last user message · Enter resends".into();
        }
    }

    pub fn open_session_picker(&mut self, value: &serde_json::Value) {
        self.session_picker_items = parse_session_picker_items(value);
        self.session_picker_selection = 0;
        self.session_picker_open = !self.session_picker_items.is_empty();
        self.status = if self.session_picker_open {
            format!(
                "{} Pi session(s) · Enter switch · Esc close",
                self.session_picker_items.len()
            )
        } else {
            "No persisted Pi sessions".into()
        };
    }

    pub fn close_session_picker(&mut self) {
        self.session_picker_open = false;
    }

    pub fn move_session_picker_selection(&mut self, delta: i32) {
        if self.session_picker_items.is_empty() {
            return;
        }
        let next = self.session_picker_selection as i32 + delta;
        self.session_picker_selection =
            next.clamp(0, self.session_picker_items.len() as i32 - 1) as usize;
    }

    pub fn submit_session_picker(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(item) = self.session_picker_items.get(self.session_picker_selection) else {
            self.status = "No session selected".into();
            return;
        };
        let mut arguments = serde_json::json!({"path": item.path});
        if let Some(agent) = self.selected_agent.as_ref() {
            arguments["agentId"] = serde_json::Value::String(agent.as_str().to_string());
        }
        match self.tool_request("glass.agent.switch-session", arguments, true) {
            Ok((call, context)) => match worker.submit_tool(call, context) {
                Ok(_) => {
                    self.agent_session_name = item.label.clone();
                    self.session_picker_open = false;
                    self.status = format!("Switching to {}", item.label);
                    worker.request_conversation();
                }
                Err(error) => self.status = format!("Could not switch session: {error}"),
            },
            Err(error) => self.status = format!("Could not switch session: {error}"),
        }
    }

    fn composer_send_blocked(&self) -> bool {
        self.pending_confirmation.is_some()
            || self.pending_agent_approval.is_some()
            || self.queued_tool_request.is_some()
            || (self.running_tool_job.is_some() && self.agent_send_job.is_none())
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
        self.close_file_picker();
        self.composer_mode = true;
        self.composer_cursor = self.composer_input.len();
        self.status = format!(
            "{} dock on {} · Enter sends · Tab @mention · Shift-Enter newline · Ctrl-Shift-A mode · Esc back",
            self.composer_run_mode.label(),
            self.surface.label()
        );
    }

    /// Focus the shared chat dock without changing the active surface.
    pub fn focus_composer_dock(&mut self) {
        if self.snapshot_trust_label == "untrusted" {
            self.surface = DevSurface::Trust;
            self.status = "Trust this workspace before chatting · T or 1".into();
            return;
        }
        if !self.agent_readiness.starts_with("✓ Ready") {
            self.start_agent_interaction();
            return;
        }
        self.open_composer();
    }

    pub fn cycle_composer_run_mode(&mut self) {
        self.composer_run_mode = self.composer_run_mode.next();
        let _ = self.ws_mut().map(|mut workspace| {
            workspace.set_agent_turn_mode(self.composer_run_mode);
        });
        self.status = format!(
            "{} mode · Ask is read-only · Plan writes a reviewable plan · Agent proposes",
            self.composer_run_mode.label()
        );
        if self.composer_mode {
            self.open_composer();
        }
    }

    pub fn set_composer_run_mode(&mut self, mode: crate::AgentTurnMode) {
        self.composer_run_mode = mode;
        let _ = self.ws_mut().map(|mut workspace| {
            workspace.set_agent_turn_mode(mode);
        });
        self.status = format!("{} mode", mode.label());
    }

    pub fn toggle_selected_git_stage(&mut self) {
        let Some(entry) = self.selected_git_entry().cloned() else {
            self.status = "Select a changed file · Space stages or unstages".into();
            return;
        };
        let staged = entry.index_status != ' ' && !entry.untracked;
        let tool = if staged {
            "glass.git.unstage"
        } else {
            "glass.git.stage"
        };
        let arguments = serde_json::json!({"paths": [entry.path]});
        match self.tool_request(tool, arguments, true) {
            Ok((call, context)) => {
                let summary = format!(
                    "{} {}",
                    if staged { "Unstage" } else { "Stage" },
                    self.selected_git_entry()
                        .map(|entry| entry.path.as_str())
                        .unwrap_or("path")
                );
                let _ = self.queue_or_confirm(call, context, summary);
            }
            Err(error) => self.status = format!("Git stage unavailable: {error}"),
        }
    }

    pub fn jump_to_app_keep_dock(&mut self) {
        self.surface = DevSurface::App;
        self.status = if self.composer_mode {
            "App · dock stays open · watch the agent or type a follow-up".into()
        } else {
            "App selected · Ctrl-L talks about this page".into()
        };
    }

    pub fn watch_agent_on_app(&mut self, tool: &str, value: &serde_json::Value) {
        self.browser_workspace.state_mut().input_owner =
            glass_browser::browser_workspace::BrowserInputOwner::Agent;
        if !self.browser_visual_live {
            self.browser_visual_live = true;
        }
        let target = value
            .get("target")
            .or_else(|| value.get("name"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("page");
        let action = tool.rsplit('.').next().unwrap_or("act");
        self.status = format!("Agent {action} {target} · watching · Ctrl-L to steer");
    }

    pub fn comment_selected_app_entity(&mut self) {
        let Some(entity) = self.browser_workspace.state().selected().cloned() else {
            self.status = "No App entity selected · ↑/↓ then C comments".into();
            return;
        };
        self.last_app_comment = Some(format!("{} ({})", entity.name, entity.role));
        self.composer_input = format!(
            "About the selected App control [{}] {}: ",
            entity.role, entity.name
        );
        self.composer_cursor = self.composer_input.len();
        self.focus_composer_dock();
        self.status = format!(
            "App comment on {} · finish the note, then Enter",
            entity.name
        );
    }

    pub fn capture_plan_from_goal(&mut self, goal: &str) {
        self.pending_plan = Some(WorkspacePlan {
            id: format!(
                "plan-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_millis())
                    .unwrap_or(0)
            ),
            goal: goal.chars().take(240).collect(),
            body: String::new(),
            accepted: false,
        });
    }

    pub fn accept_pending_plan(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(mut plan) = self.pending_plan.clone() else {
            self.status = "No plan to accept · switch to Plan and send a goal".into();
            return;
        };
        if plan.body.is_empty()
            && let Some(entry) = self
                .conversation_entries_view()
                .into_iter()
                .rev()
                .find(|entry| entry.kind == super::projection::ConversationKind::Assistant)
        {
            plan.body = entry.text;
        }
        plan.accepted = true;
        self.persist_plan(&plan);
        if let Ok(list) = self.ws_mut().and_then(|mut workspace| {
            workspace
                .seed_todos_from_plan(&plan.goal, &plan.body)
                .map_err(|error| error.to_string())
        }) {
            self.session_todos = list;
        }
        self.pending_plan = Some(plan.clone());
        self.set_composer_run_mode(crate::AgentTurnMode::Agent);
        self.composer_input = format!(
            "Implement this accepted plan. Stay in proposals unless I say otherwise.\n\nGoal: {}\n\n{}",
            plan.goal, plan.body
        );
        self.composer_cursor = self.composer_input.len();
        self.open_composer();
        self.submit_composer(worker);
        self.status = format!("Plan {} accepted · Agent implementing", plan.id);
    }

    pub fn reject_pending_plan(&mut self) {
        if let Some(plan) = self.pending_plan.as_mut() {
            plan.accepted = false;
            self.status = format!("Plan {} rejected · stay in Plan and revise", plan.id);
        } else {
            self.status = "No plan to reject".into();
        }
        self.set_composer_run_mode(crate::AgentTurnMode::Plan);
    }

    fn persist_plan(&self, plan: &WorkspacePlan) {
        let Ok(workspace) = self.ws() else {
            return;
        };
        let dir = workspace.root().join(".glass/plans");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            dir.join("latest.json"),
            serde_json::to_vec_pretty(plan).unwrap_or_default(),
        );
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
        self.open_composer();
        self.status = format!(
            "Editor context attached · stay on Code · {}:{} · Enter sends",
            buffer.path, buffer.cursor_line
        );
    }

    pub fn review_object_text(&self) -> String {
        let proposals = self
            .editor_proposals
            .iter()
            .map(|item| {
                (
                    item.id.clone(),
                    item.path.clone(),
                    format!("{:?}", item.state),
                )
            })
            .collect::<Vec<_>>();
        let checkpoint = self
            .editor_checkpoints
            .last()
            .map(|item| item.name.as_str());
        let diff = if self.git_diff.starts_with("REVIEW") {
            None
        } else {
            Some(self.git_diff.as_str()).filter(|diff| !diff.trim().is_empty())
        };
        let wake = self.last_crew_wake.as_deref();
        let packed = wake.is_some_and(|wake| wake.starts_with("WAKE"));
        super::editor::review_object(
            &self.github.summary(),
            &self.github_review,
            &proposals,
            super::editor::ReviewEvidence {
                last_verify: if packed {
                    None
                } else {
                    self.last_verify.as_deref()
                },
                git_diff: if packed { None } else { diff },
                tasks: if packed {
                    None
                } else {
                    Some(self.tasks.as_str()).filter(|tasks| !tasks.trim().is_empty())
                },
                checkpoint: if packed { None } else { checkpoint },
                wake,
            },
        )
    }

    pub fn refresh_review_object(&mut self) {
        self.refresh_crew_wake_from_live();
        let text = self.review_object_text();
        self.editor = text.clone();
        self.git_diff = text;
        self.git_diff_path = Some("REVIEW".into());
        self.git_diff_open = true;
        self.status = "REVIEW · :review accept applies the pack · ship TITLE · ask".into();
    }

    pub fn accept_review_pack(&mut self) -> Result<String, String> {
        self.auto_checkpoint("before-review-accept");
        match self.locked(|workspace| {
            workspace
                .project_mut()
                .accept_pending_editor_proposals(crate::development::Actor::local())
        }) {
            Some(Ok(buffers)) => {
                self.refresh_editor_hunks();
                self.refresh_editor_projection();
                self.refresh_review_object();
                Ok(format!(
                    "Accepted {} proposal(s) from the wake pack",
                    buffers.len()
                ))
            }
            Some(Err(crate::development::DevelopmentError::NotFound(_))) => {
                Err("No pending proposals in the wake pack".into())
            }
            Some(Err(error)) => Err(error.to_string()),
            None => Err("Accept failed · workspace busy".into()),
        }
    }

    pub fn accept_review_proposal(&mut self, id: Option<&str>) -> Result<String, String> {
        let id = self.resolve_review_proposal_id(id)?;
        self.auto_checkpoint("before-review-accept");
        match self.locked(|workspace| {
            workspace
                .project_mut()
                .accept_editor_proposal(&id, crate::development::Actor::local())
                .map(|_| ())
        }) {
            Some(Ok(())) => {
                self.refresh_editor_hunks();
                self.refresh_editor_projection();
                self.refresh_review_object();
                Ok(format!("Accepted proposal {id}"))
            }
            Some(Err(error)) => Err(error.to_string()),
            None => Err("Accept failed · workspace busy".into()),
        }
    }

    pub fn reject_review_proposal(&mut self, id: Option<&str>) -> Result<String, String> {
        let id = self.resolve_review_proposal_id(id)?;
        match self.locked(|workspace| {
            workspace
                .project_mut()
                .reject_editor_proposal(&id, crate::development::Actor::local())
                .map(|_| ())
        }) {
            Some(Ok(())) => {
                self.refresh_editor_hunks();
                self.refresh_review_object();
                Ok(format!("Rejected proposal {id}"))
            }
            Some(Err(error)) => Err(error.to_string()),
            None => Err("Reject failed · workspace busy".into()),
        }
    }

    fn resolve_review_proposal_id(&self, id: Option<&str>) -> Result<String, String> {
        if let Some(id) = id {
            return Ok(id.to_string());
        }
        self.editor_proposals
            .iter()
            .find(|item| item.state == crate::development::EditorProposalState::Pending)
            .map(|item| item.id.clone())
            .ok_or_else(|| "No pending editor proposal".into())
    }

    fn refresh_crew_wake_from_live(&mut self) {
        let accept = self
            .editor_proposals
            .iter()
            .find(|proposal| proposal.state == crate::development::EditorProposalState::Pending)
            .map(|proposal| proposal.id.clone());
        let browser = self.browser_workspace.state();
        let page = if browser.url.is_empty() && browser.selected().is_none() {
            None
        } else {
            Some(self.page_evidence())
        };
        let live = crate::CrewWakeLiveEvidence {
            verify: self.last_verify.clone(),
            page,
            accept,
        };
        if let Some(Ok(Some(wake))) = self.locked(|workspace| workspace.refresh_crew_wake(live)) {
            self.last_crew_wake = Some(wake.render());
        }
    }

    fn page_evidence(&self) -> String {
        let browser = self.browser_workspace.state();
        let url = safe_browser_url(&browser.url).unwrap_or_else(|| "—".into());
        let entity = browser
            .selected()
            .map(|entity| entity.reference.as_str())
            .unwrap_or("—");
        format!(
            "url {url}\n  entity {entity}\n  revision {}",
            browser.browser_revision.unwrap_or(0)
        )
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

    pub fn insert_composer_newline(&mut self) {
        self.insert_composer_text("\n");
        self.status = "Shift-Enter newline · Enter send · ↑ history".into();
    }

    pub fn navigate_composer_history(&mut self, previous: bool) {
        if previous && !self.composer_on_first_line() {
            self.move_composer_line(-1);
            return;
        }
        if !previous && !self.composer_on_last_line() {
            self.move_composer_line(1);
            return;
        }
        if self.composer_history.is_empty() {
            return;
        }
        if previous {
            let index = match self.composer_history_index {
                None => {
                    self.composer_history_draft = self.composer_input.clone();
                    self.composer_history.len().saturating_sub(1)
                }
                Some(0) => 0,
                Some(index) => index - 1,
            };
            self.composer_history_index = Some(index);
            self.composer_input = self.composer_history[index].clone();
            self.composer_cursor = self.composer_input.len();
            return;
        }
        match self.composer_history_index {
            None => {}
            Some(index) if index + 1 < self.composer_history.len() => {
                self.composer_history_index = Some(index + 1);
                self.composer_input = self.composer_history[index + 1].clone();
                self.composer_cursor = self.composer_input.len();
            }
            Some(_) => {
                self.composer_history_index = None;
                self.composer_input = std::mem::take(&mut self.composer_history_draft);
                self.composer_cursor = self.composer_input.len();
            }
        }
    }

    fn composer_on_first_line(&self) -> bool {
        !self.composer_input[..self.composer_cursor.min(self.composer_input.len())].contains('\n')
    }

    fn composer_on_last_line(&self) -> bool {
        !self.composer_input[self.composer_cursor.min(self.composer_input.len())..].contains('\n')
    }

    fn move_composer_line(&mut self, delta: i32) {
        let cursor = self.composer_cursor.min(self.composer_input.len());
        if delta < 0 {
            if let Some(offset) = self.composer_input[..cursor].rfind('\n') {
                self.composer_cursor = offset;
            }
            return;
        }
        if let Some(relative) = self.composer_input[cursor..].find('\n') {
            self.composer_cursor = (cursor + relative + 1).min(self.composer_input.len());
        }
    }

    pub fn remember_composer_history(&mut self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        if self.composer_history.last().map(String::as_str) != Some(trimmed) {
            self.composer_history.push(text.to_string());
            if self.composer_history.len() > 64 {
                self.composer_history.remove(0);
            }
        }
        self.composer_history_index = None;
        self.composer_history_draft.clear();
    }

    pub fn open_file_picker(&mut self) {
        self.command_mode = false;
        self.file_picker_open = true;
        self.file_picker_query.clear();
        self.file_picker_cursor = 0;
        self.file_picker_selection = 0;
        self.status = if self.files.is_empty() {
            "Open file · no files yet · wait for refresh or Esc close".into()
        } else {
            format!(
                "Open file · {} paths · type to filter · Enter open · Esc close",
                self.files.len()
            )
        };
    }

    pub fn close_file_picker(&mut self) {
        if !self.file_picker_open {
            return;
        }
        self.file_picker_open = false;
        self.file_picker_query.clear();
        self.file_picker_cursor = 0;
        self.file_picker_selection = 0;
        self.status = "File picker closed · Ctrl-P to reopen".into();
    }

    pub fn file_picker_matches(&self) -> Vec<usize> {
        self.files
            .iter()
            .enumerate()
            .filter_map(|(index, path)| {
                fuzzy_contains(path, self.file_picker_query.trim()).then_some(index)
            })
            .collect()
    }

    pub fn insert_file_picker_char(&mut self, character: char) {
        if character.is_control() {
            return;
        }
        self.file_picker_query
            .insert(self.file_picker_cursor, character);
        self.file_picker_cursor += character.len_utf8();
        self.file_picker_selection = 0;
    }

    pub fn file_picker_backspace(&mut self) {
        if self.file_picker_cursor == 0 {
            return;
        }
        let previous = self.file_picker_query[..self.file_picker_cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.file_picker_query
            .drain(previous..self.file_picker_cursor);
        self.file_picker_cursor = previous;
        self.file_picker_selection = 0;
    }

    pub fn move_file_picker_selection(&mut self, delta: i32) {
        let count = self.file_picker_matches().len();
        if count == 0 {
            self.file_picker_selection = 0;
            return;
        }
        self.file_picker_selection =
            (self.file_picker_selection as i32 + delta).rem_euclid(count as i32) as usize;
    }

    pub fn submit_file_picker(&mut self) {
        let matches = self.file_picker_matches();
        let Some(&index) = matches.get(self.file_picker_selection) else {
            self.status = "No matching file".into();
            return;
        };
        self.selected_file = index;
        self.close_file_picker();
        self.open_selected_file_for_edit();
    }

    pub fn open_path(&mut self, path: &str) -> Result<String, String> {
        if path.trim().is_empty() {
            return Err("open requires PATH".into());
        }
        if let Some(index) = self.files.iter().position(|item| {
            item == path || item.ends_with(path) || item.rsplit('/').next() == Some(path)
        }) {
            self.selected_file = index;
        } else {
            self.files.insert(0, path.to_string());
            self.selected_file = 0;
        }
        self.open_selected_file_for_edit();
        Ok(format!("Opened {path}"))
    }

    pub fn insert_composer_text(&mut self, text: &str) {
        for character in text.chars().take(16_384) {
            self.composer_input.insert(self.composer_cursor, character);
            self.composer_cursor += character.len_utf8();
        }
    }

    pub fn complete_composer_mention(&mut self) {
        match complete_mention(&self.composer_input, self.composer_cursor, &self.files) {
            Some((text, cursor)) => {
                self.composer_input = text;
                self.composer_cursor = cursor;
                self.status = "Mention completed · Tab cycles @file".into();
            }
            None => {
                self.status = "Mentions: @file @page @entity @workflow @workspace".into();
            }
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
        if self.composer_input.trim_start().starts_with('/') {
            self.submit_composer_slash(worker);
            return;
        }
        if self.composer_send_blocked() {
            self.status = "Background operation running · message kept in composer".into();
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
        let file = if !self.focused_editor_path.is_empty() {
            Some(self.focused_editor_path.as_str())
        } else {
            self.files.get(self.selected_file).map(String::as_str)
        };
        let display_text = expand_mentions(&self.composer_input, file);
        let text = expand_mentions(&std::mem::take(&mut self.composer_input), file);
        self.remember_composer_history(&text);
        if self.composer_run_mode == crate::AgentTurnMode::Plan {
            self.capture_plan_from_goal(&text);
        }
        let prove = compile_prove_it(&text);
        let steer = self.composer_steer;
        let queued_follow_up = self.agent_send_job.is_some();
        self.composer_cursor = 0;
        self.composer_steer = false;
        self.composer_mode = true;
        let _ = self.ws_mut().map(|mut workspace| {
            workspace.set_agent_turn_mode(self.composer_run_mode);
        });
        let mut context = self.agent_browser_context();
        context["editor"] = self.agent_editor_context();
        context["surface"] = serde_json::Value::String(self.surface.label().to_ascii_lowercase());
        context["runMode"] =
            serde_json::Value::String(self.composer_run_mode.label().to_ascii_lowercase());
        context["playbook"] =
            serde_json::Value::String(super::playbooks::playbook_name(self.surface).into());
        context["playbookText"] =
            serde_json::Value::String(super::playbooks::playbook(self.surface).into());
        context["todos"] =
            serde_json::to_value(&self.session_todos).unwrap_or(serde_json::json!({}));
        if let Some(path) = self.selected_git_entry().map(|entry| entry.path.clone()) {
            context["git"] = serde_json::json!({
                "branch": self.git_branch,
                "selectedPath": path,
            });
        }
        if let Some(process) = self.selected_process_entry() {
            context["process"] = serde_json::json!({
                "name": process.name,
                "health": process.health.label(),
                "pid": process.pid,
                "url": process.url,
            });
        }
        if let Some(session) = self.selected_debug_session() {
            context["debug"] = serde_json::json!({
                "session": session.name,
                "state": session.state.label(),
                "threadId": self.selected_debug_thread().map(|thread| thread.id),
                "frameId": self.selected_debug_frame().map(|frame| frame.id),
                "path": self.selected_debug_frame().and_then(|frame| frame.path.clone()),
                "line": self.selected_debug_frame().and_then(|frame| frame.line),
            });
        }
        if let Some(plan) = &self.pending_plan {
            context["plan"] = serde_json::json!({
                "id": plan.id,
                "goal": plan.goal,
                "accepted": plan.accepted,
            });
        }
        if let Some(comment) = &self.last_app_comment {
            context["appComment"] = serde_json::Value::String(comment.clone());
        }
        if context["browser"]["selectedEntity"].is_object() {
            self.browser_workspace.state_mut().input_owner =
                glass_browser::browser_workspace::BrowserInputOwner::Agent;
        }
        let playbook = super::playbooks::playbook(self.surface);
        let prefixed = match self.composer_run_mode.instruction() {
            "" => format!("{playbook}\n\n{text}"),
            instruction => format!("{instruction}\n\n{playbook}\n\n{text}"),
        };
        let mut arguments = serde_json::json!({
            "text": prefixed,
            "mode": if steer { "steer" } else { "follow-up" },
            "context": context,
        });
        if let Some(prove) = prove {
            arguments["verify"] = prove.verify.clone();
            arguments["intent"] = serde_json::Value::String(prove.intent);
            self.pending_verify = Some(prove.verify);
            self.last_proof_ok = None;
            self.last_verify = Some(evidence_card(
                arguments["context"]["browser"]["url"]
                    .as_str()
                    .unwrap_or("about:blank"),
                arguments["context"]["browser"]["revision"]
                    .as_u64()
                    .unwrap_or(0),
                arguments["context"]["browser"]["selectedEntity"].as_str(),
                "composer prove-it queued",
                false,
            ));
            self.auto_checkpoint("before-prove-it");
        }
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
                } else if self.composer_run_mode == crate::AgentTurnMode::Plan {
                    "Plan turn · inspect only · :plan accept when ready".into()
                } else if queued_follow_up {
                    "Queued follow-up · Glass Agent will continue".into()
                } else {
                    format!("Sent · {} is thinking…", self.composer_run_mode.label())
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

    fn submit_composer_slash(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let raw = self.composer_input.trim().to_string();
        let mut parts = raw.trim_start_matches('/').split_whitespace();
        let Some(command) = parts.next() else {
            self.status = "Empty slash command".into();
            return;
        };
        let rest = parts.collect::<Vec<_>>();
        let mut arguments = serde_json::json!({});
        if let Some(agent) = self.selected_agent.as_ref() {
            arguments["agentId"] = serde_json::Value::String(agent.as_str().to_string());
        }
        let (tool, mutating) = match command {
            "compact" => {
                if !rest.is_empty() {
                    arguments["instructions"] = serde_json::Value::String(rest.join(" "));
                }
                ("glass.agent.compact", true)
            }
            "model" => {
                let provider = rest.first().copied().unwrap_or("");
                let model = rest.get(1).copied().unwrap_or("");
                if provider.is_empty() || model.is_empty() {
                    self.status = "/model requires PROVIDER MODEL".into();
                    return;
                }
                arguments["provider"] = serde_json::Value::String(provider.into());
                arguments["modelId"] = serde_json::Value::String(model.into());
                self.agent_model = format!("{provider}/{model}");
                ("glass.agent.model", true)
            }
            "think" | "thinking" => {
                let level = rest.first().copied().unwrap_or("");
                if level.is_empty() {
                    self.status = "/think requires LEVEL".into();
                    return;
                }
                arguments["level"] = serde_json::Value::String(level.into());
                self.agent_thinking = level.into();
                ("glass.agent.thinking", true)
            }
            "new" => ("glass.agent.new-session", true),
            "clone" => ("glass.agent.clone-session", true),
            "name" => {
                let name = rest.join(" ");
                if name.is_empty() {
                    self.status = "/name requires TITLE".into();
                    return;
                }
                arguments["name"] = serde_json::Value::String(name.clone());
                self.agent_session_name = name;
                ("glass.agent.name", true)
            }
            "stats" => ("glass.agent.stats", false),
            "sessions" => ("glass.agent.sessions", false),
            "tree" => ("glass.agent.tree", false),
            "todo" => {
                self.composer_input = self.session_todos.render();
                self.composer_cursor = 0;
                self.status = "Session todos · edit or Esc".into();
                return;
            }
            "ask" | "plan" | "agent" => {
                let mode = match command {
                    "ask" => crate::AgentTurnMode::Ask,
                    "plan" => crate::AgentTurnMode::Plan,
                    _ => crate::AgentTurnMode::Agent,
                };
                self.set_composer_run_mode(mode);
                self.remember_composer_history(&raw);
                self.composer_input = rest.join(" ");
                self.composer_cursor = self.composer_input.len();
                if self.composer_input.trim().is_empty() {
                    return;
                }
                self.submit_composer(worker);
                return;
            }
            _ => {
                self.status = format!("Unknown slash command /{command}");
                return;
            }
        };
        match self.tool_request(tool, arguments, mutating) {
            Ok((call, context)) => match worker.submit_tool(call, context) {
                Ok(id) => {
                    self.running_tool_job = Some(id);
                    self.remember_composer_history(&raw);
                    self.composer_input.clear();
                    self.composer_cursor = 0;
                    self.status = format!("{tool} · queued from composer");
                }
                Err(error) => self.status = format!("{tool} failed: {error}"),
            },
            Err(error) => self.status = format!("{tool} unavailable: {error}"),
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

    pub fn submit_pending_verify(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(predicate) = self.pending_verify.take() else {
            return;
        };
        let arguments = serde_json::json!({ "predicate": predicate, "timeoutSeconds": 8 });
        let (call, context) = match self.tool_request("glass.browser.verify", arguments, false) {
            Ok(request) => request,
            Err(error) => {
                self.last_proof_ok = Some(false);
                self.status = format!("Prove-it unavailable: {error}");
                return;
            }
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = "Prove-it running against the live page".into();
            }
            Err(error) => {
                self.last_proof_ok = Some(false);
                self.status = format!("Prove-it failed to start: {error}");
            }
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
                if result.tool == "glass.agent.sessions" {
                    self.open_session_picker(&value);
                    return;
                }
                if result.tool == "glass.agent.stats" {
                    self.agent_token_summary = stats_summary(&value);
                    self.status = format!("Session stats · {}", self.agent_token_summary);
                    return;
                }
                if result.tool == "glass.browser.verify" {
                    let ok = value.get("status").and_then(serde_json::Value::as_str)
                        == Some("satisfied");
                    self.last_proof_ok = Some(ok);
                    self.last_verify = Some(evidence_card(
                        value
                            .pointer("/predicate/urlEquals")
                            .and_then(serde_json::Value::as_str)
                            .or_else(|| value.get("state").and_then(serde_json::Value::as_str))
                            .unwrap_or("live page"),
                        value
                            .get("elapsedMs")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0),
                        None,
                        value
                            .get("state")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("verify"),
                        ok,
                    ));
                    self.status = if ok {
                        "Prove-it passed · gutter ✓".into()
                    } else {
                        "Prove-it failed · inspect last verify".into()
                    };
                } else if result.tool.starts_with("glass.browser") {
                    self.apply_browser_result(&result.tool, &value);
                    self.browser_detail = super::projection::browser_result(&result.tool, &value);
                    if result.tool == "glass.browser.act" || result.tool == "glass.browser.navigate"
                    {
                        self.browser_observe_pending = true;
                        self.watch_agent_on_app(&result.tool, &value);
                    }
                } else if result.tool == "glass.process.logs" {
                    self.process_logs = value
                        .get("output")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .chars()
                        .rev()
                        .take(8 * 1024)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect();
                    self.surface = DevSurface::Terminal;
                    self.status = format!(
                        "Logs · {}",
                        value
                            .get("name")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("process")
                    );
                } else if result.tool == "glass.debug.threads" {
                    self.debug_threads = parse_debug_threads(&value);
                    self.selected_debug_thread = self
                        .selected_debug_thread
                        .min(self.debug_threads.len().saturating_sub(1));
                    self.debug_frames.clear();
                    if !self.debug_threads.is_empty() {
                        self.debug_pane = DebugPane::Threads;
                        self.debug_stack_requested = true;
                    }
                    self.status = format!(
                        "{} thread(s) · Enter stack · Space continue",
                        self.debug_threads.len()
                    );
                } else if result.tool == "glass.debug.stack" {
                    self.debug_frames = parse_debug_frames(&value);
                    self.selected_debug_frame = 0;
                    if !self.debug_frames.is_empty() {
                        self.debug_pane = DebugPane::Frames;
                    }
                    self.status = format!(
                        "{} frame(s) · Enter jumps to source",
                        self.debug_frames.len()
                    );
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
                } else if result.tool == "glass.task.wake" {
                    self.last_crew_wake = serde_json::from_value::<crate::CrewWake>(value.clone())
                        .ok()
                        .map(|wake| wake.render());
                    self.surface = DevSurface::Tasks;
                    self.status = "Crew wake loaded · :review".into();
                } else if result.tool == "glass.task.crew" {
                    let wake = value.get("wake").cloned().unwrap_or_else(|| value.clone());
                    self.last_crew_wake = serde_json::from_value::<crate::CrewWake>(wake)
                        .ok()
                        .map(|wake| wake.render())
                        .or_else(|| {
                            value
                                .get("goal")
                                .and_then(serde_json::Value::as_str)
                                .map(|goal| format!("WAKE\n  goal {goal}"))
                        });
                    self.surface = DevSurface::Tasks;
                    self.status = format!(
                        "Crew wake {} · :review",
                        value
                            .pointer("/wake/id")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("queued")
                    );
                } else if result.tool.starts_with("glass.lsp") {
                    if result.tool == "glass.lsp.inlay_hints" {
                        self.editor_inlays =
                            parse_inlay_hints(value.get("result").unwrap_or(&value));
                    }
                    if result.tool == "glass.lsp.diagnostics" {
                        self.editor_diagnostics =
                            parse_editor_diagnostics(value.get("result").unwrap_or(&value));
                    }
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
                        | "glass.browser.verify"
                        | "glass.task.crew"
                        | "glass.task.wake"
                        | "glass.process.logs"
                        | "glass.debug.threads"
                        | "glass.debug.stack"
                ) {
                    self.status = format!("Completed {} · workspace refreshed", result.tool);
                }
            }
            Err(error) => {
                if result.tool == "glass.browser.verify" {
                    self.last_proof_ok = Some(false);
                    self.last_verify = Some(evidence_card("live page", 0, None, &error, false));
                }
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
        self.pending_page_entity = None;
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

    pub fn move_process_selection(&mut self, delta: i32) {
        if self.process_entries.is_empty() {
            self.selected_process = 0;
            self.status = "No managed process selected".into();
            return;
        }
        self.selected_process = (self.selected_process as i32 + delta)
            .clamp(0, self.process_entries.len().saturating_sub(1) as i32)
            as usize;
        if let Some(entry) = self.selected_process_entry() {
            self.status = format!(
                "{} · {} · Enter logs · Space restart",
                entry.name,
                entry.health.label()
            );
        }
    }

    pub fn selected_process_entry(&self) -> Option<&ProcessRow> {
        self.process_entries.get(self.selected_process)
    }

    pub fn queue_selected_process_logs(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(name) = self
            .selected_process_entry()
            .map(|entry| entry.name.clone())
        else {
            self.status = "Select a process · j/k then Enter logs".into();
            return;
        };
        if self.background_action_running() {
            self.status = "Process logs wait for the current background operation".into();
            return;
        }
        let (call, context) = match self.tool_request(
            "glass.process.logs",
            serde_json::json!({"name": name}),
            false,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.status = format!("Process logs unavailable: {error}");
                return;
            }
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = format!("Loading logs for {name}…");
            }
            Err(error) => self.status = format!("Process logs unavailable: {error}"),
        }
    }

    pub fn restart_selected_process(&mut self) {
        let Some(name) = self
            .selected_process_entry()
            .map(|entry| entry.name.clone())
        else {
            self.status = "Select a process · j/k then Space restarts".into();
            return;
        };
        match self.tool_request(
            "glass.process.restart",
            serde_json::json!({"name": name}),
            true,
        ) {
            Ok((call, context)) => {
                let _ = self.queue_or_confirm(call, context, format!("Restart {name}"));
            }
            Err(error) => self.status = format!("Process restart unavailable: {error}"),
        }
    }

    pub fn cycle_debug_pane(&mut self, delta: i32) {
        self.debug_pane = if delta < 0 {
            self.debug_pane.previous()
        } else {
            self.debug_pane.next()
        };
        self.status = format!(
            "Debug {} · j/k select · Enter · Space continue",
            self.debug_pane.label()
        );
    }

    pub fn move_debug_selection(&mut self, delta: i32) {
        match self.debug_pane {
            DebugPane::Sessions => {
                if self.debug_sessions.is_empty() {
                    self.selected_debug_session = 0;
                    self.status = "No debugger session · :debug start NAME COMMAND".into();
                    return;
                }
                self.selected_debug_session = (self.selected_debug_session as i32 + delta)
                    .clamp(0, self.debug_sessions.len().saturating_sub(1) as i32)
                    as usize;
                if let Some(session) = self.selected_debug_session() {
                    self.status = format!(
                        "{} · {} · Enter threads",
                        session.name,
                        session.state.label()
                    );
                }
            }
            DebugPane::Threads => {
                if self.debug_threads.is_empty() {
                    self.status = "No threads · Enter on a session to refresh".into();
                    return;
                }
                self.selected_debug_thread = (self.selected_debug_thread as i32 + delta)
                    .clamp(0, self.debug_threads.len().saturating_sub(1) as i32)
                    as usize;
                if let Some(thread) = self.selected_debug_thread() {
                    self.status = format!("{} · Enter stack · Space continue", thread.name);
                }
            }
            DebugPane::Frames => {
                if self.debug_frames.is_empty() {
                    self.status = "No frames · Enter on a thread to load the stack".into();
                    return;
                }
                self.selected_debug_frame = (self.selected_debug_frame as i32 + delta)
                    .clamp(0, self.debug_frames.len().saturating_sub(1) as i32)
                    as usize;
                if let Some(frame) = self.selected_debug_frame() {
                    self.status = format!(
                        "{} · Enter jumps to {}",
                        frame.name,
                        frame.path.as_deref().unwrap_or("source")
                    );
                }
            }
        }
    }

    pub fn selected_debug_session(&self) -> Option<&DebugSessionRow> {
        self.debug_sessions.get(self.selected_debug_session)
    }

    pub fn selected_debug_thread(&self) -> Option<&DebugThreadRow> {
        self.debug_threads.get(self.selected_debug_thread)
    }

    pub fn selected_debug_frame(&self) -> Option<&DebugFrameRow> {
        self.debug_frames.get(self.selected_debug_frame)
    }

    pub fn activate_debug_selection(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        match self.debug_pane {
            DebugPane::Sessions => self.queue_debug_threads(worker),
            DebugPane::Threads => self.queue_debug_stack(worker),
            DebugPane::Frames => self.jump_selected_debug_frame(),
        }
    }

    pub fn queue_debug_threads(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(session) = self.selected_debug_session().map(|row| row.name.clone()) else {
            self.status = "Start a debugger with :debug start NAME COMMAND".into();
            return;
        };
        if self.background_action_running() {
            self.status = "Debug threads wait for the current background operation".into();
            return;
        }
        let (call, context) = match self.tool_request(
            "glass.debug.threads",
            serde_json::json!({"session": session}),
            false,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.status = format!("Debug threads unavailable: {error}");
                return;
            }
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = format!("Loading threads for {session}…");
            }
            Err(error) => self.status = format!("Debug threads unavailable: {error}"),
        }
    }

    pub fn queue_debug_stack(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(session) = self.selected_debug_session().map(|row| row.name.clone()) else {
            self.status = "Select a debugger session first".into();
            return;
        };
        let Some(thread_id) = self.selected_debug_thread().map(|thread| thread.id) else {
            self.status = "Select a thread · Enter on a session first".into();
            return;
        };
        if self.background_action_running() {
            self.status = "Debug stack waits for the current background operation".into();
            return;
        }
        let (call, context) = match self.tool_request(
            "glass.debug.stack",
            serde_json::json!({"session": session, "threadId": thread_id}),
            false,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.status = format!("Debug stack unavailable: {error}");
                return;
            }
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = format!("Loading stack for thread {thread_id}…");
            }
            Err(error) => self.status = format!("Debug stack unavailable: {error}"),
        }
    }

    pub fn continue_selected_debug(&mut self) {
        let Some(session) = self.selected_debug_session().map(|row| row.name.clone()) else {
            self.status = "Select a debugger session · Space continues".into();
            return;
        };
        let Some(thread_id) = self.selected_debug_thread().map(|thread| thread.id) else {
            self.status = "Refresh threads before continue".into();
            return;
        };
        match self.tool_request(
            "glass.debug.continue",
            serde_json::json!({"session": session, "threadId": thread_id}),
            true,
        ) {
            Ok((call, context)) => {
                let _ = self.queue_or_confirm(
                    call,
                    context,
                    format!("Continue {session} thread {thread_id}"),
                );
            }
            Err(error) => self.status = format!("Debug continue unavailable: {error}"),
        }
    }

    pub fn jump_selected_debug_frame(&mut self) {
        let Some(frame) = self.selected_debug_frame().cloned() else {
            self.status = "Select a stack frame · Enter jumps to source".into();
            return;
        };
        let Some(path) = frame.path.clone() else {
            self.status = format!("{} has no source path", frame.name);
            return;
        };
        match self.open_path(&path) {
            Ok(_) => {
                if let Some(line) = frame.line {
                    let _ = self.set_editor_cursor(
                        &self.focused_editor_path.clone(),
                        crate::development::TextPosition {
                            line: u32::try_from(line).unwrap_or(u32::MAX).max(1),
                            column: 1,
                        },
                        false,
                    );
                    self.refresh_editor_projection();
                }
                self.status = format!(
                    "Debug {} · {}{}",
                    frame.name,
                    path,
                    frame
                        .line
                        .map(|line| format!(":{line}"))
                        .unwrap_or_default()
                );
            }
            Err(error) => self.status = format!("Could not open {path}: {error}"),
        }
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
        let (buffers, comments, proposals, checkpoints, diagnostics) = self
            .workspace
            .lock()
            .map(|workspace| {
                (
                    workspace.project().buffers().cloned().collect::<Vec<_>>(),
                    workspace.project().editor_comments(None),
                    workspace.project().editor_proposals(),
                    workspace.project().editor_checkpoints(),
                    workspace.project().diagnostics().clone(),
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
            self.editor_diagnostics = diagnostics.get(&buffer.path).cloned().unwrap_or_default();
            self.ensure_editor_cursor_visible();
            let path = self.focused_editor_path.clone();
            let content = self.focused_editor_content.clone();
            self.syntax.sync(&path, &content);
            if let Some(proposal) = self.editor_proposals.iter().find(|item| {
                item.path == path && item.state == crate::development::EditorProposalState::Pending
            }) {
                let line = native::line_hunks(&proposal.original, &proposal.proposed)
                    .first()
                    .map(|hunk| hunk.start_line)
                    .unwrap_or(1);
                self.editor_engine.agent_caret = Some(native::Jump {
                    path: path.clone(),
                    line,
                    column: 1,
                });
            } else {
                self.editor_engine.agent_caret = None;
            }
            self.maybe_begin_pair_apply();
        } else {
            self.focused_editor_path.clear();
            self.focused_editor_content.clear();
            self.focused_editor_dirty = false;
            self.focused_editor_line = 0;
            self.focused_editor_column = 0;
            self.focused_editor_selection = None;
            self.editor_scroll_line = 0;
            self.editor_scroll_column = 0;
            self.editor_inlays.clear();
            self.editor_diagnostics.clear();
        }
        self.editor = format_editor_buffers(&buffers);
    }

    fn refresh_editor_inlays(&mut self) {
        let path = self.focused_editor_path.clone();
        if path.is_empty() {
            self.editor_inlays.clear();
            return;
        }
        let response = self.workspace.try_lock().ok().and_then(|mut workspace| {
            let name = workspace.language().names().next()?.to_string();
            workspace.language().inlay_hints(&name, "local", &path).ok()
        });
        self.editor_inlays = response
            .map(|response| parse_inlay_hints(&response.result))
            .unwrap_or_default();
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
            self.editor_engine.enter_insert();
            self.refresh_editor_hunks();
            self.refresh_editor_inlays();
            self.status = "INSERT · Esc normal · gm/gn extra carets · dif · gd · Ctrl-S".into();
        }
    }

    pub fn handle_editor_escape(&mut self) {
        if self.editor_engine.overlay.take().is_some()
            || !self.editor_engine.symbols.is_empty()
            || self.editor_engine.ghost.take().is_some()
        {
            self.editor_engine.symbols.clear();
            self.editor_engine.clear_pending();
            self.status = format!("{} · overlay closed", self.editor_engine.mode.label());
            return;
        }
        match self.editor_engine.mode {
            EditorMode::Agent => {
                self.editor_engine.stop_pair_apply();
                self.status = "NORMAL · agent yielded · hjkl · i insert · Esc exit".into();
            }
            EditorMode::Insert | EditorMode::Select => {
                self.editor_engine.enter_normal();
                self.status = self.editor_normal_status();
            }
            EditorMode::Normal => {
                if self.editor_engine.clear_extra_cursors() {
                    self.status = "NORMAL · extra carets cleared".into();
                    self.refresh_editor_projection();
                    return;
                }
                self.request_editor_exit();
            }
        }
    }

    fn editor_normal_status(&self) -> String {
        let extras = self.editor_engine.extra_caret_count();
        if extras > 0 {
            format!(
                "NORMAL · {} carets · gm match · gn next · d/c/i apply to all · Esc clear",
                extras + 1
            )
        } else {
            "NORMAL · hjkl · d/c/y · iw/if/ia · gm/gn extra carets · i insert · Esc exit".into()
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
            let marks = self
                .editor_gutter_marks()
                .into_iter()
                .map(|(line, mark)| (line, mark.glyph()))
                .collect::<Vec<_>>();
            let notes = self.editor_source_notes();
            let decorations = super::file_view::EditorDecorations {
                marks: &marks,
                inlays: &notes,
                extra_selections: &self.editor_engine.extra_selections,
            };
            let wrapped = super::file_view::render_editable_source_wrapped(
                &self.focused_editor_path,
                &self.focused_editor_content,
                self.focused_editor_line,
                self.focused_editor_column,
                self.focused_editor_selection.as_ref(),
                self.terminal_width.saturating_sub(4).max(1),
                &decorations,
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
        if self.editor_engine.mode == EditorMode::Agent {
            self.edit_agent_key(code, modifiers);
            return;
        }
        if self.editor_engine.mode == EditorMode::Insert
            || modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
            || modifiers.contains(crossterm::event::KeyModifiers::ALT)
        {
            self.edit_insert_or_chords(code, modifiers);
            return;
        }
        self.edit_normal_key(code, modifiers);
    }

    fn edit_agent_key(
        &mut self,
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) {
        match code {
            crossterm::event::KeyCode::Enter => {
                let id = self
                    .editor_engine
                    .pair_apply
                    .as_ref()
                    .map(|apply| apply.proposal_id.clone());
                self.editor_engine.stop_pair_apply();
                if let Some(id) = id {
                    match self.accept_review_proposal(Some(&id)) {
                        Ok(message) => self.status = message,
                        Err(error) => self.status = error,
                    }
                }
            }
            crossterm::event::KeyCode::Char('n') => {
                let id = self
                    .editor_engine
                    .pair_apply
                    .as_ref()
                    .map(|apply| apply.proposal_id.clone());
                self.editor_engine.stop_pair_apply();
                if let Some(id) = id {
                    match self.reject_review_proposal(Some(&id)) {
                        Ok(message) => self.status = message,
                        Err(error) => self.status = error,
                    }
                }
            }
            crossterm::event::KeyCode::Esc => self.handle_editor_escape(),
            _ => {
                self.editor_engine.stop_pair_apply();
                self.editor_engine.enter_insert();
                self.edit_insert_or_chords(code, modifiers);
            }
        }
        self.refresh_editor_projection();
    }

    fn edit_insert_or_chords(
        &mut self,
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) {
        let Some(buffer) = self.focused_buffer() else {
            self.status = "No open buffer".into();
            return;
        };
        let path = buffer.path.clone();
        if code == crossterm::event::KeyCode::Tab
            && let Some(ghost) = self.editor_engine.ghost.take()
        {
            self.accept_ghost_text(&path, &ghost.text);
            return;
        }
        if matches!(
            code,
            crossterm::event::KeyCode::Right | crossterm::event::KeyCode::Char('f')
        ) && modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
            && !modifiers.contains(crossterm::event::KeyModifiers::SHIFT)
            && self.editor_engine.ghost.is_some()
        {
            self.accept_ghost_word(&path);
            return;
        }
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
            (crossterm::event::KeyCode::Char('o'), value)
                if value.contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.open_symbol_picker();
                Ok(())
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
            (crossterm::event::KeyCode::Char(character), _)
                if self.editor_engine.mode == EditorMode::Insert =>
            {
                let result = self.insert_editor_text(&path, &character.to_string());
                self.request_ghost_from_line();
                result
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.status = format!("Editor action failed: {error}");
        }
        self.refresh_editor_projection();
    }

    fn edit_normal_key(
        &mut self,
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) {
        let Some(buffer) = self.focused_buffer() else {
            self.status = "No open buffer".into();
            return;
        };
        let path = buffer.path.clone();
        let content = buffer.content.clone();
        let cursor = crate::development::TextPosition {
            line: buffer.cursor_line,
            column: buffer.cursor_column,
        };
        if let crossterm::event::KeyCode::Char(digit) = code
            && digit.is_ascii_digit()
            && !(digit == '0' && self.editor_engine.pending_count == 0)
        {
            self.editor_engine
                .push_digit(digit.to_digit(10).unwrap_or(0));
            return;
        }
        let count = self.editor_engine.count();
        if self.editor_engine.pending_find.is_some() {
            if let crossterm::event::KeyCode::Char(needle) = code {
                let jumping_mark = self.editor_engine.pending_find == Some('\'');
                self.editor_engine.pending_find = None;
                if jumping_mark {
                    if let Some(mark) = self.editor_engine.marks.get(&needle).cloned() {
                        self.editor_engine
                            .record_jump(&path, cursor.line, cursor.column);
                        if mark.path == path {
                            let _ = self.set_editor_cursor(
                                &path,
                                crate::development::TextPosition {
                                    line: mark.line,
                                    column: mark.column,
                                },
                                false,
                            );
                        } else {
                            self.status = format!("Mark '{needle}' is {}", mark.path);
                        }
                    } else {
                        self.status = format!("Mark '{needle}' is empty");
                    }
                } else {
                    let motion = Motion::Find {
                        needle,
                        till: self.editor_engine.pending_find_till,
                        reverse: self.editor_engine.pending_find_reverse,
                    };
                    if let Some(operator) = self.editor_engine.pending_operator.take() {
                        let _ = self.apply_operator(&path, &content, cursor, operator, motion);
                    } else {
                        let next = apply_motion(&content, cursor, motion);
                        let _ = self.set_editor_cursor(&path, next, false);
                    }
                }
                self.refresh_editor_projection();
            } else {
                self.editor_engine.clear_pending();
            }
            return;
        }
        if self.editor_engine.pending_mark {
            if let crossterm::event::KeyCode::Char(name) = code {
                self.editor_engine.marks.insert(
                    name,
                    native::Jump {
                        path: path.clone(),
                        line: cursor.line,
                        column: cursor.column,
                    },
                );
                self.status = format!("Mark '{name}' set");
            }
            self.editor_engine.pending_mark = false;
            return;
        }
        if let crossterm::event::KeyCode::Char(character) = code
            && let Some(around) = self.editor_engine.pending_around
        {
            if let Some(object) = textobject_from_key(character, around) {
                self.apply_textobject(&path, &content, cursor, object);
            } else {
                self.editor_engine.clear_pending();
                self.status = format!("Unknown textobject '{character}'");
            }
            return;
        }
        if self.editor_engine.pending_operator.is_some() {
            match code {
                crossterm::event::KeyCode::Char('i') => {
                    self.editor_engine.pending_around = Some(false);
                    self.status =
                        "inner textobject · w word · f fn · a arg · c comment · s string".into();
                    return;
                }
                crossterm::event::KeyCode::Char('a') => {
                    self.editor_engine.pending_around = Some(true);
                    self.status =
                        "around textobject · w word · f fn · a arg · c comment · s string".into();
                    return;
                }
                crossterm::event::KeyCode::Char('f') => {
                    self.apply_textobject(
                        &path,
                        &content,
                        cursor,
                        TextObject::Function { around: true },
                    );
                    return;
                }
                _ => {}
            }
        }
        let motion = match code {
            crossterm::event::KeyCode::Char('h') | crossterm::event::KeyCode::Left => {
                Some(Motion::Left)
            }
            crossterm::event::KeyCode::Char('l') | crossterm::event::KeyCode::Right => {
                Some(Motion::Right)
            }
            crossterm::event::KeyCode::Char('j') | crossterm::event::KeyCode::Down => {
                Some(Motion::Down)
            }
            crossterm::event::KeyCode::Char('k') | crossterm::event::KeyCode::Up => {
                Some(Motion::Up)
            }
            crossterm::event::KeyCode::Char('w') => Some(Motion::WordForward),
            crossterm::event::KeyCode::Char('b') => Some(Motion::WordBackward),
            crossterm::event::KeyCode::Char('e') => Some(Motion::WordEnd),
            crossterm::event::KeyCode::Char('0') => Some(Motion::LineStart),
            crossterm::event::KeyCode::Char('$') => Some(Motion::LineEnd),
            crossterm::event::KeyCode::Char('%') => Some(Motion::MatchPair),
            crossterm::event::KeyCode::Char('G') => Some(Motion::FileEnd),
            _ => None,
        };
        if code == crossterm::event::KeyCode::Char('g') {
            if self.editor_engine.pending_g {
                self.editor_engine.pending_g = false;
                self.apply_editor_motion(&path, &content, cursor, Motion::FileStart, false);
            } else {
                self.editor_engine.pending_g = true;
            }
            return;
        }
        if let Some(motion) = motion {
            if let Some(operator) = self.editor_engine.pending_operator.take() {
                let _ = self.apply_operator(&path, &content, cursor, operator, motion);
            } else {
                let mut position = cursor;
                for _ in 0..count {
                    position = apply_motion(&content, position, motion);
                }
                self.editor_engine.clear_pending();
                let extend = self.editor_engine.mode == EditorMode::Select
                    || modifiers.contains(crossterm::event::KeyModifiers::SHIFT);
                let _ = self.set_editor_cursor(&path, position, extend);
            }
            self.refresh_editor_projection();
            return;
        }
        match code {
            crossterm::event::KeyCode::Char('i') => self.editor_engine.enter_insert(),
            crossterm::event::KeyCode::Char('a') => {
                let next = apply_motion(&content, cursor, Motion::Right);
                let _ = self.set_editor_cursor(&path, next, false);
                self.editor_engine.enter_insert();
            }
            crossterm::event::KeyCode::Char('v') => self.editor_engine.enter_select(),
            crossterm::event::KeyCode::Char('W') => {
                self.apply_textobject(&path, &content, cursor, TextObject::Word { around: true });
            }
            crossterm::event::KeyCode::Char('d') if self.editor_engine.pending_g => {
                self.editor_engine.pending_g = false;
                self.editor_go_to_definition();
            }
            crossterm::event::KeyCode::Char('r') if self.editor_engine.pending_g => {
                self.editor_engine.pending_g = false;
                self.editor_references();
            }
            crossterm::event::KeyCode::Char('p') if self.editor_engine.pending_g => {
                self.editor_engine.pending_g = false;
                self.jump_page_from_source();
            }
            crossterm::event::KeyCode::Char('c') if self.editor_engine.pending_g => {
                self.editor_engine.pending_g = false;
                self.show_comment_thread();
            }
            crossterm::event::KeyCode::Char('m') if self.editor_engine.pending_g => {
                self.editor_engine.pending_g = false;
                self.match_same_objects(&path, &content, cursor, true);
            }
            crossterm::event::KeyCode::Char('n') if self.editor_engine.pending_g => {
                self.editor_engine.pending_g = false;
                self.match_same_objects(&path, &content, cursor, false);
            }
            crossterm::event::KeyCode::Char('d') => {
                self.editor_engine.pending_operator = Some(Operator::Delete);
            }
            crossterm::event::KeyCode::Char('c') => {
                self.editor_engine.pending_operator = Some(Operator::Change);
            }
            crossterm::event::KeyCode::Char('y')
                if !modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.editor_engine.pending_operator = Some(Operator::Yank);
            }
            crossterm::event::KeyCode::Char('x') => {
                let _ =
                    self.apply_operator(&path, &content, cursor, Operator::Delete, Motion::Right);
            }
            crossterm::event::KeyCode::Char('u') => {
                let _ =
                    self.locked(|workspace| workspace.project_mut().undo_buffer(&path).map(|_| ()));
            }
            crossterm::event::KeyCode::Char('p') => {
                if !self.editor_engine.yank.is_empty() {
                    let _ = self.insert_editor_text(&path, &self.editor_engine.yank.clone());
                }
            }
            crossterm::event::KeyCode::Char('m') => {
                self.editor_engine.pending_mark = true;
                self.status = "Mark · next character names the mark".into();
            }
            crossterm::event::KeyCode::Char('\'') => {
                self.status = "Jump to mark · next character".into();
                self.editor_engine.pending_find = Some('\'');
            }
            crossterm::event::KeyCode::Char('K') => self.editor_hover(),
            crossterm::event::KeyCode::Char(']') => {
                if let Some(hunk) = self.editor_engine.step_hunk(1).cloned() {
                    let _ = self.set_editor_cursor(
                        &path,
                        crate::development::TextPosition {
                            line: hunk.start_line,
                            column: 1,
                        },
                        false,
                    );
                    self.status = format!(
                        "Hunk {}/{}",
                        self.editor_engine.hunk_index + 1,
                        self.editor_engine.hunks.len()
                    );
                }
            }
            crossterm::event::KeyCode::Char('[') => {
                if let Some(hunk) = self.editor_engine.step_hunk(-1).cloned() {
                    let _ = self.set_editor_cursor(
                        &path,
                        crate::development::TextPosition {
                            line: hunk.start_line,
                            column: 1,
                        },
                        false,
                    );
                }
            }
            crossterm::event::KeyCode::Enter => self.accept_current_hunk(),
            crossterm::event::KeyCode::Char('n') => self.reject_current_hunk(),
            crossterm::event::KeyCode::Char('o') => self.open_symbol_picker(),
            crossterm::event::KeyCode::Char('f') => {
                self.editor_engine.pending_find = Some('f');
                self.editor_engine.pending_find_till = false;
                self.editor_engine.pending_find_reverse = false;
                self.status = "find · next character".into();
            }
            crossterm::event::KeyCode::Char('t') => {
                self.editor_engine.pending_find = Some('t');
                self.editor_engine.pending_find_till = true;
                self.editor_engine.pending_find_reverse = false;
                self.status = "till · next character".into();
            }
            crossterm::event::KeyCode::Char('F') => {
                self.editor_engine.pending_find = Some('F');
                self.editor_engine.pending_find_till = false;
                self.editor_engine.pending_find_reverse = true;
                self.status = "find back · next character".into();
            }
            crossterm::event::KeyCode::Char('T') => {
                self.editor_engine.pending_find = Some('T');
                self.editor_engine.pending_find_till = true;
                self.editor_engine.pending_find_reverse = true;
                self.status = "till back · next character".into();
            }
            _ => {}
        }
        self.refresh_editor_projection();
    }

    fn set_editor_cursor(
        &mut self,
        path: &str,
        position: crate::development::TextPosition,
        extend: bool,
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
        let position = native::clamp_position(&buffer.content, position);
        let anchor = if extend {
            buffer
                .selection
                .map(|selection| selection.anchor)
                .unwrap_or(crate::development::TextPosition {
                    line: buffer.cursor_line,
                    column: buffer.cursor_column,
                })
        } else {
            position
        };
        let mut workspace = self.workspace.try_lock()?;
        workspace
            .project_mut()
            .set_buffer_cursor(path, position.line, position.column)?;
        workspace.project_mut().set_buffer_selection(
            path,
            extend.then_some(crate::development::TextSelection {
                anchor,
                active: position,
            }),
            crate::development::Actor::local(),
        )?;
        Ok(())
    }

    fn apply_editor_motion(
        &mut self,
        path: &str,
        content: &str,
        cursor: crate::development::TextPosition,
        motion: Motion,
        extend: bool,
    ) {
        let next = apply_motion(content, cursor, motion);
        let _ = self.set_editor_cursor(path, next, extend);
    }

    fn apply_textobject(
        &mut self,
        path: &str,
        content: &str,
        cursor: crate::development::TextPosition,
        object: TextObject,
    ) {
        let selection = self
            .syntax
            .textobject(path, content, cursor, object)
            .or_else(|| textobject_selection(content, cursor, object));
        let Some(selection) = selection else {
            self.editor_engine.clear_pending();
            self.status = "No textobject under cursor".into();
            return;
        };
        self.editor_engine.last_textobject = Some(object);
        if let Some(operator) = self.editor_engine.pending_operator.take() {
            let mut ranges = vec![selection];
            for extra in self.editor_engine.extra_selections.clone() {
                let origin = extra.active;
                if let Some(range) = self
                    .syntax
                    .textobject(path, content, origin, object)
                    .or_else(|| textobject_selection(content, origin, object))
                {
                    ranges.push(range);
                }
            }
            if let Err(error) = self.apply_operator_ranges(path, content, operator, ranges) {
                self.status = format!("Operator failed: {error}");
            }
        } else {
            self.editor_engine.enter_select();
            let _ = self.set_editor_cursor(path, selection.anchor, false);
            let _ = self.set_editor_cursor(path, selection.active, true);
            self.status = format!("{} · textobject selected", self.editor_engine.mode.label());
        }
        self.refresh_editor_projection();
    }

    fn match_same_objects(
        &mut self,
        path: &str,
        content: &str,
        cursor: crate::development::TextPosition,
        all: bool,
    ) {
        let object = self
            .editor_engine
            .last_textobject
            .unwrap_or(TextObject::Word { around: false });
        let mut ranges = if matches!(object, TextObject::Word { .. }) {
            same_word_ranges(
                content,
                cursor,
                matches!(object, TextObject::Word { around: true }),
            )
        } else {
            let mut found = self.syntax.same_textobjects(path, content, cursor, object);
            if found.is_empty() {
                found = textobject_selection(content, cursor, object)
                    .into_iter()
                    .collect();
            }
            found
        };
        if ranges.is_empty() {
            self.status = "No matching objects under the caret".into();
            return;
        }
        ranges.sort_by_key(|selection| (selection.anchor.line, selection.anchor.column));
        let primary_index = ranges
            .iter()
            .position(|selection| selection_covers(content, selection, cursor))
            .unwrap_or(0);
        if all {
            let primary = ranges.remove(primary_index.min(ranges.len().saturating_sub(1)));
            self.editor_engine.extra_selections = ranges;
            let _ = self.set_editor_cursor(path, primary.anchor, false);
            let _ = self.set_editor_cursor(path, primary.active, true);
            self.editor_engine.last_textobject = Some(object);
            self.status = self.editor_normal_status();
        } else {
            let extras = &self.editor_engine.extra_selections;
            let next = ranges
                .iter()
                .cycle()
                .skip(primary_index + 1)
                .take(ranges.len().saturating_sub(1))
                .find(|candidate| {
                    !extras.iter().any(|existing| {
                        existing.anchor == candidate.anchor && existing.active == candidate.active
                    })
                })
                .cloned();
            if let Some(next) = next {
                self.editor_engine.extra_selections.push(next);
                self.editor_engine.last_textobject = Some(object);
                self.status = self.editor_normal_status();
            } else {
                self.status = "No further matching objects".into();
            }
        }
        self.refresh_editor_projection();
    }

    fn apply_operator(
        &mut self,
        path: &str,
        content: &str,
        cursor: crate::development::TextPosition,
        operator: Operator,
        motion: Motion,
    ) -> crate::development::DevelopmentResult<()> {
        let mut ranges = vec![TextSelection {
            anchor: cursor,
            active: apply_motion(content, cursor, motion),
        }];
        for extra in &self.editor_engine.extra_selections {
            let origin = extra.active;
            ranges.push(TextSelection {
                anchor: origin,
                active: apply_motion(content, origin, motion),
            });
        }
        self.apply_operator_ranges(path, content, operator, ranges)
    }

    fn apply_operator_ranges(
        &mut self,
        path: &str,
        content: &str,
        operator: Operator,
        ranges: Vec<TextSelection>,
    ) -> crate::development::DevelopmentResult<()> {
        let mut offsets = ranges
            .iter()
            .filter_map(|selection| {
                crate::development::editor::selection_offsets(content, selection)
            })
            .collect::<Vec<_>>();
        offsets.sort_by_key(|(start, _)| *start);
        offsets.dedup();
        if offsets.is_empty() {
            self.editor_engine.clear_pending();
            return Ok(());
        }
        self.auto_checkpoint("before-operator");
        match operator {
            Operator::Yank => {
                self.editor_engine.yank = offsets
                    .iter()
                    .map(|(start, stop)| content[*start..*stop].to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                self.status = format!("Yanked {} bytes", self.editor_engine.yank.len());
            }
            Operator::Delete | Operator::Change => {
                let (next, carets) = multi_delete(content, &offsets);
                let mut extras = Vec::new();
                let mut primary = crate::development::TextPosition { line: 1, column: 1 };
                for (index, offset) in carets.into_iter().enumerate() {
                    let position =
                        crate::development::editor::text_position_at_offset(&next, offset)
                            .unwrap_or(primary);
                    if index == 0 {
                        primary = position;
                    } else {
                        extras.push(TextSelection::collapsed(position));
                    }
                }
                let actor = crate::development::Actor::local();
                let mut workspace = self.workspace.try_lock()?;
                workspace
                    .project_mut()
                    .edit_buffer(path, next, actor.clone())?;
                workspace
                    .project_mut()
                    .set_buffer_cursor(path, primary.line, primary.column)?;
                workspace
                    .project_mut()
                    .set_buffer_selection(path, None, actor)?;
                drop(workspace);
                self.editor_engine.extra_selections = extras;
                if operator == Operator::Change {
                    self.editor_engine.enter_insert();
                }
            }
        }
        self.editor_engine.clear_pending();
        Ok(())
    }

    fn maybe_begin_pair_apply(&mut self) {
        if self.editor_engine.pair_apply.is_some() {
            return;
        }
        let path = self.focused_editor_path.clone();
        if path.is_empty() {
            return;
        }
        let Some(proposal) = self
            .editor_proposals
            .iter()
            .find(|item| {
                item.path == path
                    && item.state == crate::development::EditorProposalState::Pending
                    && item.original == self.focused_editor_content
                    && item.original != item.proposed
            })
            .cloned()
        else {
            return;
        };
        self.editor_engine.begin_pair_apply(
            proposal.id,
            proposal.actor.name.clone(),
            proposal.path,
            proposal.original,
            proposal.proposed,
        );
        self.status = format!(
            "AGENT · {} pair-apply · Enter accept · n reject · type to yield",
            proposal.actor.name
        );
    }

    pub fn tick_pair_apply(&mut self) -> bool {
        let Some(apply) = self.editor_engine.pair_apply.clone() else {
            return false;
        };
        if self.focused_editor_content == apply.proposed {
            return false;
        }
        let (content, revealed, _done) =
            pair_apply_step(&apply.original, &apply.proposed, apply.revealed, 24);
        let path = apply.path.clone();
        let actor = apply.actor.clone();
        let _ = self.locked(|workspace| {
            workspace
                .project_mut()
                .edit_buffer(
                    &path,
                    content.clone(),
                    crate::development::Actor::embedded(),
                )
                .map(|_| ())
        });
        if let Some(live) = self.editor_engine.pair_apply.as_mut() {
            live.revealed = revealed;
        }
        self.focused_editor_content = content.clone();
        self.focused_editor_dirty = true;
        let caret_offset = pair_apply_caret(&apply.original, &apply.proposed, revealed);
        if let Some(position) =
            crate::development::editor::text_position_at_offset(&content, caret_offset)
        {
            self.focused_editor_line = position.line;
            self.focused_editor_column = position.column;
            self.editor_engine.agent_caret = Some(native::Jump {
                path,
                line: position.line,
                column: position.column,
            });
            let _ = self.set_editor_cursor(&self.focused_editor_path.clone(), position, false);
        }
        self.editor_engine.mode = EditorMode::Agent;
        self.status = format!("AGENT · {actor} pair-apply · Enter accept · n reject");
        self.ensure_editor_cursor_visible();
        true
    }

    pub fn editor_gutter_marks(&self) -> Vec<(u32, native::GutterMark)> {
        let mut marks = Vec::new();
        let path = &self.focused_editor_path;
        for diagnostic in &self.editor_diagnostics {
            if diagnostic_matches_path(diagnostic, path) {
                marks.push((
                    diagnostic.start.line.saturating_add(1),
                    native::GutterMark::Lsp,
                ));
            }
        }
        let page_lines = self.page_source_lines(path);
        match self.last_proof_ok {
            Some(true) => {
                for line in &page_lines {
                    marks.push((*line, native::GutterMark::Proof));
                }
            }
            Some(false) => {
                for line in &page_lines {
                    marks.push((*line, native::GutterMark::Lsp));
                }
            }
            None => {}
        }
        for hunk in &self.editor_engine.hunks {
            marks.push((hunk.start_line, native::GutterMark::Git));
        }
        if let Some(caret) = &self.editor_engine.agent_caret
            && caret.path == *path
        {
            marks.push((caret.line, native::GutterMark::Agent));
        }
        for line in page_lines {
            marks.push((line, native::GutterMark::Page));
        }
        for comment in &self.editor_comments {
            if comment.state == crate::development::EditorCommentState::Open
                && (path.is_empty() || comment.path == *path)
            {
                marks.push((comment.start_line, native::GutterMark::Comment));
            }
        }
        marks
    }

    pub fn editor_source_notes(&self) -> Vec<(u32, String)> {
        let mut by_line = std::collections::BTreeMap::<u32, Vec<String>>::new();
        for (line, hint) in &self.editor_inlays {
            by_line.entry(*line).or_default().push(hint.clone());
        }
        let path = &self.focused_editor_path;
        for diagnostic in &self.editor_diagnostics {
            if diagnostic_matches_path(diagnostic, path) {
                by_line
                    .entry(diagnostic.start.line.saturating_add(1))
                    .or_default()
                    .push(format!("! {}", truncate_note(&diagnostic.message)));
            }
        }
        for comment in &self.editor_comments {
            if comment.state == crate::development::EditorCommentState::Open
                && (path.is_empty() || comment.path == *path)
            {
                by_line
                    .entry(comment.start_line)
                    .or_default()
                    .push(format!("# {}", truncate_note(&comment.text)));
            }
        }
        by_line
            .into_iter()
            .map(|(line, parts)| (line, parts.join("  ")))
            .collect()
    }

    fn page_source_lines(&self, path: &str) -> Vec<u32> {
        self.workspace
            .try_lock()
            .ok()
            .map(|workspace| {
                workspace
                    .project()
                    .graph()
                    .entities_for_source(path, None)
                    .into_iter()
                    .map(|link| link.source.start_line)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn show_comment_thread(&mut self) {
        let path = self.focused_editor_path.clone();
        let line = self.focused_editor_line.max(1);
        let thread = self
            .editor_comments
            .iter()
            .filter(|comment| {
                comment.state == crate::development::EditorCommentState::Open
                    && (path.is_empty() || comment.path == path)
                    && line >= comment.start_line
                    && line <= comment.end_line
            })
            .map(|comment| {
                format!(
                    "{} · {}:{}\n  {}",
                    comment.actor.name, comment.path, comment.start_line, comment.text
                )
            })
            .collect::<Vec<_>>();
        if thread.is_empty() {
            self.status = "No open comment on this line · :editor comment-selection".into();
            return;
        }
        self.editor_engine.overlay = Some(thread.join("\n\n"));
        self.status = "Comment thread · Esc closes".into();
    }

    fn refresh_editor_hunks(&mut self) {
        let path = self.focused_editor_path.clone();
        if path.is_empty() {
            return;
        }
        let proposal = self.editor_proposals.iter().find(|item| item.path == path);
        if let Some(proposal) = proposal {
            self.editor_engine
                .set_hunks(line_hunks(&proposal.original, &proposal.proposed));
        }
    }

    fn auto_checkpoint(&mut self, name: &str) {
        let _ = self.locked(|workspace| {
            workspace
                .project_mut()
                .create_editor_checkpoint(name.to_string(), crate::development::Actor::local())
                .map(|_| ())
        });
    }

    fn editor_hover(&mut self) {
        let path = self.focused_editor_path.clone();
        let line = self.focused_editor_line.saturating_sub(1);
        let character = self.focused_editor_column.saturating_sub(1);
        match self.locked(|workspace| {
            let server = workspace
                .language()
                .names()
                .next()
                .ok_or_else(|| {
                    crate::development::DevelopmentError::NotFound("language server".into())
                })?
                .to_string();
            workspace
                .language()
                .hover(&server, "local", &path, line, character)
        }) {
            Some(Ok(response)) => {
                self.editor_engine.overlay = Some(response.result.to_string());
                self.status = "Hover · Esc closes".into();
            }
            Some(Err(error)) => self.status = format!("Hover unavailable: {error}"),
            None => self.status = "Hover unavailable · workspace busy".into(),
        }
    }

    fn editor_references(&mut self) {
        let path = self.focused_editor_path.clone();
        let line = self.focused_editor_line.saturating_sub(1);
        let character = self.focused_editor_column.saturating_sub(1);
        match self.locked(|workspace| {
            let server = workspace
                .language()
                .names()
                .next()
                .ok_or_else(|| {
                    crate::development::DevelopmentError::NotFound("language server".into())
                })?
                .to_string();
            workspace
                .language()
                .references(&server, "local", &path, line, character)
        }) {
            Some(Ok(response)) => {
                self.editor_engine.overlay = Some(format_lsp_locations(&response.result));
                self.status = "References · Esc closes".into();
            }
            Some(Err(error)) => self.status = format!("References unavailable: {error}"),
            None => self.status = "References unavailable · workspace busy".into(),
        }
    }

    fn editor_go_to_definition(&mut self) {
        let path = self.focused_editor_path.clone();
        let line = self.focused_editor_line.saturating_sub(1);
        let character = self.focused_editor_column.saturating_sub(1);
        self.editor_engine
            .record_jump(&path, self.focused_editor_line, self.focused_editor_column);
        match self.locked(|workspace| {
            let server = workspace
                .language()
                .names()
                .next()
                .ok_or_else(|| {
                    crate::development::DevelopmentError::NotFound("language server".into())
                })?
                .to_string();
            workspace
                .language()
                .definition(&server, "local", &path, line, character)
        }) {
            Some(Ok(response)) => {
                self.editor_engine.overlay = Some(format!("definition {}", response.result));
                if let Some(target) = parse_lsp_location(&response.result) {
                    let _ = self.open_path(&target.path);
                    let _ = self.set_editor_cursor(
                        &self.focused_editor_path.clone(),
                        crate::development::TextPosition {
                            line: target.line.max(1),
                            column: target.column.max(1),
                        },
                        false,
                    );
                }
                self.status = "Definition · Ctrl-O back".into();
            }
            Some(Err(error)) => self.status = format!("Definition unavailable: {error}"),
            None => self.status = "Definition unavailable · workspace busy".into(),
        }
    }

    fn open_symbol_picker(&mut self) {
        let path = self.focused_editor_path.clone();
        match self.locked(|workspace| {
            let server = workspace
                .language()
                .names()
                .next()
                .ok_or_else(|| {
                    crate::development::DevelopmentError::NotFound("language server".into())
                })?
                .to_string();
            workspace
                .language()
                .document_symbols(&server, "local", &path)
        }) {
            Some(Ok(response)) => {
                self.editor_engine.symbols = parse_lsp_symbols(&response.result);
                self.editor_engine.symbol_selection = 0;
                self.status = format!(
                    "Symbols · {} · Enter jumps",
                    self.editor_engine.symbols.len()
                );
            }
            _ => {
                self.editor_engine.symbols = self
                    .focused_editor_content
                    .lines()
                    .enumerate()
                    .filter(|(_, line)| {
                        let trimmed = line.trim_start();
                        trimmed.starts_with("fn ")
                            || trimmed.starts_with("struct ")
                            || trimmed.starts_with("impl ")
                            || trimmed.starts_with("pub ")
                    })
                    .map(|(index, line)| (line.trim().to_string(), index as u32 + 1))
                    .collect();
                self.status = format!("Symbols (lexical) · {}", self.editor_engine.symbols.len());
            }
        }
    }

    fn accept_current_hunk(&mut self) {
        let path = self.focused_editor_path.clone();
        let Some(proposal) = self
            .editor_proposals
            .iter()
            .find(|item| item.path == path)
            .cloned()
        else {
            self.status = "No proposal hunk on this buffer".into();
            return;
        };
        self.auto_checkpoint("before-hunk-accept");
        match self.locked(|workspace| {
            workspace
                .project_mut()
                .accept_editor_proposal(&proposal.id, crate::development::Actor::local())
                .map(|_| ())
        }) {
            Some(Ok(())) => {
                self.status = format!("Accepted proposal {}", proposal.id);
                self.refresh_editor_hunks();
            }
            Some(Err(error)) => self.status = format!("Accept failed: {error}"),
            None => self.status = "Accept failed · workspace busy".into(),
        }
    }

    fn reject_current_hunk(&mut self) {
        let path = self.focused_editor_path.clone();
        let Some(proposal) = self
            .editor_proposals
            .iter()
            .find(|item| item.path == path)
            .cloned()
        else {
            return;
        };
        let _ = self.locked(|workspace| {
            workspace
                .project_mut()
                .reject_editor_proposal(&proposal.id, crate::development::Actor::local())
                .map(|_| ())
        });
        self.status = format!("Rejected proposal {}", proposal.id);
        self.refresh_editor_hunks();
    }

    pub fn jump_page_from_source(&mut self) {
        let path = self.focused_editor_path.clone();
        if path.is_empty() {
            self.status = "Open a buffer, then gp jumps to App".into();
            return;
        }
        let line = self.focused_editor_line.max(1);
        let entity = self
            .locked(|workspace| {
                let _ = workspace.project_mut().discover_runtime_links();
                workspace
                    .project()
                    .graph()
                    .entities_for_source(&path, Some(line))
                    .first()
                    .map(|link| link.entity_id.clone())
            })
            .flatten();
        if let Some(entity) = entity.clone() {
            self.pending_page_entity = Some(entity.clone());
            if self.select_page_entity(&entity) {
                self.surface = DevSurface::App;
                self.code_edit_mode = false;
                self.status = format!("Page bound · entity {entity}");
            }
        }
        let route = inferred_app_path(&path);
        let Some(base) = self.resolved_app_url() else {
            if entity.is_none() {
                self.status = "No App URL · start the detected suite, then gp".into();
            }
            return;
        };
        let url = join_app_url(&base, route.as_deref());
        let current = self.browser_workspace.state().url.clone();
        let current_origin = current.trim_end_matches('/');
        if entity.is_some()
            && !current.is_empty()
            && (current == url || current.starts_with(&url) || url.starts_with(current_origin))
        {
            self.surface = DevSurface::App;
            self.code_edit_mode = false;
            return;
        }
        self.auto_checkpoint("before-app-jump");
        match self.prepare_browser_navigation(&url) {
            Ok(message) => {
                self.status = if entity.is_some() {
                    format!("{message} · will bind page entity")
                } else {
                    message
                };
            }
            Err(error) => self.status = error,
        }
    }

    pub fn jump_source_from_page(&mut self) {
        let Some(entity) = self.browser_workspace.state().selected().cloned() else {
            self.status = "Select an App entity, then g jumps to source".into();
            return;
        };
        let reference = entity.reference.clone();
        let name = entity.name.clone();
        let link = self
            .locked(|workspace| {
                let _ = workspace.project_mut().discover_runtime_links();
                let graph = workspace.project().graph();
                graph
                    .best_link(&reference)
                    .cloned()
                    .or_else(|| graph.best_link(&name).cloned())
                    .or_else(|| {
                        graph
                            .links
                            .values()
                            .flatten()
                            .find(|link| {
                                reference.ends_with(&link.entity_id)
                                    || name.contains(&link.entity_id)
                            })
                            .cloned()
                    })
            })
            .flatten();
        let Some(link) = link else {
            self.status = format!("No source link for {reference} · :project graph discover");
            return;
        };
        self.open_source_at(&link.source.path, link.source.start_line);
    }

    pub fn attach_detected_app(&mut self) -> Result<String, String> {
        let url = self
            .resolved_app_url()
            .ok_or_else(|| "No App URL · start the detected suite".to_string())?;
        self.auto_checkpoint("before-app-attach");
        self.prepare_browser_navigation(&url)
    }

    pub fn resolved_app_url(&self) -> Option<String> {
        self.process_urls
            .iter()
            .find(|url| !url.is_empty())
            .cloned()
            .or_else(|| {
                self.workspace.try_lock().ok().and_then(|workspace| {
                    let detection = workspace.project().detection();
                    detection.browser_url.clone().or_else(|| {
                        detection
                            .local_development_urls
                            .iter()
                            .find(|url| !url.is_empty())
                            .cloned()
                    })
                })
            })
            .map(|url| join_app_url(&url, None))
    }

    fn select_page_entity(&mut self, entity_id: &str) -> bool {
        let index = self
            .browser_workspace
            .state()
            .entities
            .iter()
            .position(|entity| {
                entity.reference == entity_id
                    || entity.reference.ends_with(entity_id)
                    || entity.name == entity_id
                    || (!entity_id.is_empty() && entity.name.contains(entity_id))
            });
        let Some(index) = index else {
            return false;
        };
        self.browser_workspace.state_mut().selected_entity = Some(index);
        self.browser = self.browser_workspace_summary();
        true
    }

    fn open_source_at(&mut self, path: &str, line: u32) {
        match self.open_path(path) {
            Ok(_) => {
                let focused = self.focused_editor_path.clone();
                let _ = self.set_editor_cursor(
                    &focused,
                    crate::development::TextPosition {
                        line: line.max(1),
                        column: 1,
                    },
                    false,
                );
                self.refresh_editor_projection();
                self.ensure_editor_cursor_visible();
                self.status = format!("Source · {path}:{line}");
            }
            Err(error) => self.status = format!("Source jump failed: {error}"),
        }
    }

    fn accept_ghost_text(&mut self, path: &str, text: &str) {
        let _ = self.insert_editor_text(path, text);
        self.refresh_editor_projection();
        self.advance_next_edit();
    }

    fn accept_ghost_word(&mut self, path: &str) {
        let Some(ghost) = self.editor_engine.ghost.take() else {
            return;
        };
        match split_ghost_word(&ghost.text) {
            Some((word, rest)) if !rest.is_empty() => {
                let _ = self.insert_editor_text(path, word);
                self.refresh_editor_projection();
                self.editor_engine.ghost = Some(GhostText {
                    text: rest.to_string(),
                });
                self.status = "Ghost word accepted · Tab rest · Ctrl-Right another word".into();
            }
            Some((word, _)) => self.accept_ghost_text(path, word),
            None => self.accept_ghost_text(path, &ghost.text),
        }
    }

    fn advance_next_edit(&mut self) {
        let content = self.focused_editor_content.clone();
        let caret = crate::development::editor::text_position_offset(
            &content,
            crate::development::TextPosition {
                line: self.focused_editor_line.max(1),
                column: self.focused_editor_column.max(1),
            },
        )
        .unwrap_or(content.len());
        let Some(offset) = next_edit_after_accept(&content, caret) else {
            self.request_ghost_from_line();
            self.status = if self.editor_engine.ghost.is_some() {
                "Ghost accepted · next edit here".into()
            } else {
                "Ghost accepted".into()
            };
            return;
        };
        let Some(position) = crate::development::editor::text_position_at_offset(&content, offset)
        else {
            self.request_ghost_from_line();
            self.status = "Ghost accepted".into();
            return;
        };
        let path = self.focused_editor_path.clone();
        self.editor_engine
            .record_jump(&path, position.line, position.column);
        let _ = self.set_editor_cursor(&path, position, false);
        self.refresh_editor_projection();
        self.request_ghost_from_line();
        self.status = format!(
            "Ghost accepted · next edit {}:{}",
            position.line, position.column
        );
    }

    pub fn request_ghost_from_line(&mut self) {
        let path = self.focused_editor_path.clone();
        let Some(offset) = crate::development::editor::text_position_offset(
            &self.focused_editor_content,
            crate::development::TextPosition {
                line: self.focused_editor_line.max(1),
                column: self.focused_editor_column.max(1),
            },
        ) else {
            return;
        };
        if let Some(text) = local_fim(&self.focused_editor_content, offset) {
            self.editor_engine.ghost = Some(GhostText { text });
            return;
        }
        if self.take_ready_fim(&path, offset) {
            return;
        }
        self.spawn_hosted_fim(&path, offset);
        let line = self.focused_editor_line.saturating_sub(1);
        let character = self.focused_editor_column.saturating_sub(1);
        if let Some(text) = self.lsp_ghost_insert(&path, line, character) {
            self.editor_engine.ghost = Some(GhostText { text });
            return;
        }
        let line = self
            .focused_editor_content
            .lines()
            .nth(self.focused_editor_line.saturating_sub(1) as usize)
            .unwrap_or("")
            .trim()
            .to_string();
        if line.is_empty() {
            return;
        }
        let ghost = if line.ends_with('{') {
            "\n    \n}"
        } else if line.contains("todo") || line.contains("TODO") {
            " // implemented"
        } else {
            return;
        };
        self.editor_engine.ghost = Some(GhostText { text: ghost.into() });
    }

    pub fn tick_fim(&mut self) -> bool {
        let path = self.focused_editor_path.clone();
        let Some(offset) = crate::development::editor::text_position_offset(
            &self.focused_editor_content,
            crate::development::TextPosition {
                line: self.focused_editor_line.max(1),
                column: self.focused_editor_column.max(1),
            },
        ) else {
            return false;
        };
        self.take_ready_fim(&path, offset)
    }

    fn take_ready_fim(&mut self, path: &str, offset: usize) -> bool {
        let stale = self
            .pending_fim
            .as_ref()
            .is_some_and(|pending| pending.path() != path || pending.offset() != offset);
        if stale {
            self.pending_fim = None;
            return false;
        }
        let pi = match &self.pending_fim {
            Some(PendingFim::Pi {
                agent_id, since, ..
            }) => Some((agent_id.clone(), *since)),
            _ => None,
        };
        if let Some((agent_id, since)) = pi {
            let text = self
                .locked(|workspace| {
                    workspace
                        .agents()
                        .history(since)
                        .ok()?
                        .into_iter()
                        .rev()
                        .find_map(|event| {
                            if event.agent_id != agent_id {
                                return None;
                            }
                            if event
                                .payload
                                .get("operation")
                                .and_then(serde_json::Value::as_str)
                                != Some("complete")
                            {
                                return None;
                            }
                            crate::fim::parse_fim_text(&event.payload)
                        })
                })
                .flatten();
            return match text {
                Some(text) => {
                    self.pending_fim = None;
                    self.editor_engine.ghost = Some(GhostText { text });
                    true
                }
                None => false,
            };
        }
        let Some(PendingFim::Thread { rx, .. }) = self.pending_fim.as_mut() else {
            return false;
        };
        match rx.try_recv() {
            Ok(Some(text)) if !text.is_empty() => {
                self.pending_fim = None;
                self.editor_engine.ghost = Some(GhostText { text });
                true
            }
            Ok(_) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.pending_fim = None;
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => false,
        }
    }

    fn spawn_hosted_fim(&mut self, path: &str, offset: usize) {
        if self
            .pending_fim
            .as_ref()
            .is_some_and(|pending| pending.path() == path && pending.offset() == offset)
        {
            return;
        }
        let Some(provider) = self.workspace.try_lock().ok().and_then(|workspace| {
            crate::fim::FimProvider::from_editor(&workspace.customization().config().editor)
        }) else {
            return;
        };
        let prefix = self.focused_editor_content[..offset].to_string();
        let suffix = self.focused_editor_content[offset..].to_string();
        match provider.backend {
            crate::fim::FimBackend::Stub => {
                let (tx, rx) = std::sync::mpsc::channel();
                let _ = std::thread::Builder::new()
                    .name("glass-fim".into())
                    .spawn(move || {
                        let _ = tx.send(provider.complete(&prefix, &suffix).ok());
                    });
                self.pending_fim = Some(PendingFim::Thread {
                    path: path.to_string(),
                    offset,
                    rx,
                });
            }
            crate::fim::FimBackend::Pi => {
                let selected = self.selected_agent.clone();
                let queued = self
                    .locked(|workspace| {
                        let since = workspace
                            .agents()
                            .history(0)
                            .ok()?
                            .last()
                            .map(|event| event.sequence)
                            .unwrap_or(0);
                        let snapshots = workspace.agents().list().ok()?;
                        let id = selected
                            .as_ref()
                            .filter(|id| {
                                snapshots.iter().any(|item| {
                                    &item.id == *id && item.status == crate::AgentStatus::Idle
                                })
                            })
                            .cloned()
                            .or_else(|| {
                                snapshots.into_iter().find_map(|item| {
                                    (item.status == crate::AgentStatus::Idle).then_some(item.id)
                                })
                            })?;
                        workspace
                            .agents()
                            .request(
                                &id,
                                crate::pi_runtime::PiSessionRequest::Complete { prefix, suffix },
                            )
                            .ok()?;
                        Some((id, since))
                    })
                    .flatten();
                if let Some((agent_id, since)) = queued {
                    self.pending_fim = Some(PendingFim::Pi {
                        path: path.to_string(),
                        offset,
                        agent_id,
                        since,
                    });
                }
            }
        }
    }

    fn lsp_ghost_insert(&mut self, path: &str, line: u32, character: u32) -> Option<String> {
        let response = self.locked(|workspace| {
            let server = workspace
                .language()
                .names()
                .next()
                .ok_or_else(|| {
                    crate::development::DevelopmentError::NotFound("language server".into())
                })?
                .to_string();
            workspace
                .language()
                .completion(&server, "local", path, line, character)
        })?;
        parse_completion_insert(&response.ok()?.result)
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
        let extras = self.editor_engine.extra_selections.clone();
        let mut workspace = self.workspace.try_lock()?;
        let project = workspace.project_mut();
        let buffer = project.buffer(path).cloned().ok_or_else(|| {
            crate::development::DevelopmentError::NotFound(format!("buffer {path}"))
        })?;
        if buffer
            .selection
            .as_ref()
            .is_some_and(|selection| !selection.is_empty())
            && extras.is_empty()
        {
            project.replace_buffer_selection(
                path,
                text.to_string(),
                crate::development::Actor::local(),
            )?;
            return Ok(());
        }
        let primary = editor_offset(&buffer.content, buffer.cursor_line, buffer.cursor_column);
        if extras.is_empty() {
            let mut content = buffer.content;
            content.insert_str(primary, text);
            let cursor =
                crate::development::editor::text_position_at_offset(&content, primary + text.len())
                    .ok_or_else(|| {
                        crate::development::DevelopmentError::InvalidInput(
                            "inserted text ended at an invalid UTF-8 boundary".into(),
                        )
                    })?;
            let actor = crate::development::Actor::local();
            project.edit_buffer(path, content, actor.clone())?;
            project.set_buffer_cursor(path, cursor.line, cursor.column)?;
            project.set_buffer_selection(path, None, actor)?;
            return Ok(());
        }
        let mut sites = vec![primary];
        for extra in &extras {
            if let Some(offset) =
                crate::development::editor::text_position_offset(&buffer.content, extra.active)
            {
                sites.push(offset);
            }
        }
        let (content, carets) = multi_insert(&buffer.content, &sites, text);
        let mut next_extras = Vec::new();
        let mut primary_pos = crate::development::TextPosition { line: 1, column: 1 };
        for (index, offset) in carets.into_iter().enumerate() {
            let position = crate::development::editor::text_position_at_offset(&content, offset)
                .ok_or_else(|| {
                    crate::development::DevelopmentError::InvalidInput(
                        "inserted text ended at an invalid UTF-8 boundary".into(),
                    )
                })?;
            if index == 0 {
                primary_pos = position;
            } else {
                next_extras.push(TextSelection::collapsed(position));
            }
        }
        let actor = crate::development::Actor::local();
        project.edit_buffer(path, content, actor.clone())?;
        project.set_buffer_cursor(path, primary_pos.line, primary_pos.column)?;
        project.set_buffer_selection(path, None, actor)?;
        drop(workspace);
        self.editor_engine.extra_selections = next_extras;
        Ok(())
    }

    fn backspace_editor(&mut self, path: &str) -> crate::development::DevelopmentResult<()> {
        let extras = self.editor_engine.extra_selections.clone();
        let mut workspace = self.workspace.try_lock()?;
        let project = workspace.project_mut();
        let buffer = project.buffer(path).cloned().ok_or_else(|| {
            crate::development::DevelopmentError::NotFound(format!("buffer {path}"))
        })?;
        if buffer
            .selection
            .as_ref()
            .is_some_and(|selection| !selection.is_empty())
            && extras.is_empty()
        {
            project.replace_buffer_selection(
                path,
                String::new(),
                crate::development::Actor::local(),
            )?;
            return Ok(());
        }
        let mut sites = vec![editor_offset(
            &buffer.content,
            buffer.cursor_line,
            buffer.cursor_column,
        )];
        for extra in &extras {
            if let Some(offset) =
                crate::development::editor::text_position_offset(&buffer.content, extra.active)
            {
                sites.push(offset);
            }
        }
        let mut ranges = Vec::new();
        for offset in sites {
            if offset == 0 {
                continue;
            }
            let previous = buffer.content[..offset]
                .char_indices()
                .next_back()
                .map(|(index, _)| index)
                .unwrap_or(0);
            ranges.push((previous, offset));
        }
        if ranges.is_empty() {
            return Ok(());
        }
        if extras.is_empty() && ranges.len() == 1 {
            let (previous, offset) = ranges[0];
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
            return Ok(());
        }
        let (content, carets) = multi_delete(&buffer.content, &ranges);
        let mut next_extras = Vec::new();
        let mut primary_pos = crate::development::TextPosition { line: 1, column: 1 };
        for (index, offset) in carets.into_iter().enumerate() {
            let position = crate::development::editor::text_position_at_offset(&content, offset)
                .ok_or_else(|| {
                    crate::development::DevelopmentError::InvalidInput(
                        "backspace ended at an invalid UTF-8 boundary".into(),
                    )
                })?;
            if index == 0 {
                primary_pos = position;
            } else {
                next_extras.push(TextSelection::collapsed(position));
            }
        }
        let actor = crate::development::Actor::local();
        project.edit_buffer(path, content, actor.clone())?;
        project.set_buffer_cursor(path, primary_pos.line, primary_pos.column)?;
        project.set_buffer_selection(path, None, actor)?;
        drop(workspace);
        self.editor_engine.extra_selections = next_extras;
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
        if let Ok(list) = self.ws().map(|workspace| workspace.todos()) {
            self.session_todos = list;
        }
        self.agents = snapshot.agents.clone();
        self.agent_conversation = snapshot.agent_conversation.clone();
        self.conversation_items = snapshot.conversation_items.clone();
        if let Some(chrome) = self
            .selected_agent
            .as_ref()
            .and_then(|selected| {
                snapshot
                    .agent_chrome
                    .iter()
                    .find(|item| item.id == *selected)
            })
            .or_else(|| snapshot.agent_chrome.first())
        {
            if self.selected_agent.is_none() {
                self.selected_agent = Some(chrome.id.clone());
            }
            self.agent_model = chrome.model.clone().unwrap_or_default();
            self.agent_thinking = chrome.thinking.clone().unwrap_or_default();
            if let Some(path) = chrome.session_file.as_deref() {
                self.agent_session_name = std::path::Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(path)
                    .to_string();
            }
        }
        let entry_count = self.conversation_entries_view().len();
        if entry_count == 0 {
            self.transcript_selection = 0;
        } else {
            self.transcript_selection = self.transcript_selection.min(entry_count - 1);
        }
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
        self.process_entries = snapshot.process_entries.clone();
        if self.process_entries.is_empty() {
            self.selected_process = 0;
            self.process_urls.clear();
        } else {
            self.selected_process = self
                .selected_process
                .min(self.process_entries.len().saturating_sub(1));
            self.process_urls = self
                .process_entries
                .iter()
                .filter_map(|entry| entry.url.clone())
                .collect();
            self.process_urls.sort();
            self.process_urls.dedup();
        }
        self.git = snapshot.git.clone();
        self.git_entries = snapshot.git_entries.clone();
        self.git_branch = snapshot.git_branch.clone();
        self.git_dirty = snapshot.git_dirty;
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
        let previous_debug = self
            .selected_debug_session()
            .map(|session| session.name.clone());
        self.debug_sessions = snapshot.debug_sessions.clone();
        if self.debug_sessions.is_empty() {
            self.selected_debug_session = 0;
            self.debug_threads.clear();
            self.debug_frames.clear();
        } else {
            self.selected_debug_session = self
                .selected_debug_session
                .min(self.debug_sessions.len().saturating_sub(1));
            let current = self
                .selected_debug_session()
                .map(|session| session.name.clone());
            if previous_debug != current {
                self.debug_threads.clear();
                self.debug_frames.clear();
                self.selected_debug_thread = 0;
                self.selected_debug_frame = 0;
            }
        }
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
        if let Some(wake) = workspace.latest_crew_wake() {
            self.last_crew_wake = Some(wake.render());
        }
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
        match workspace.project_mut().processes().list_checked() {
            Ok(items) => {
                self.process_urls = items
                    .iter()
                    .flat_map(|item| item.detected_urls.iter().cloned())
                    .collect();
                self.process_urls.sort();
                self.process_urls.dedup();
                self.process_entries = items.iter().map(ProcessRow::from_snapshot).collect();
                self.selected_process = if self.process_entries.is_empty() {
                    0
                } else {
                    self.selected_process
                        .min(self.process_entries.len().saturating_sub(1))
                };
                self.processes = if items.is_empty() {
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
                };
            }
            Err(error) => {
                self.process_urls.clear();
                self.process_entries.clear();
                self.selected_process = 0;
                self.processes = format!("Process state failed: {error}");
            }
        }
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
                self.git_branch = status.branch.clone().unwrap_or_else(|| "detached".into());
                self.git_dirty = !status.conflicts.is_empty()
                    || status.ahead > 0
                    || status.behind > 0
                    || status.entries.iter().any(|entry| {
                        entry.untracked || entry.index_status != ' ' || entry.worktree_status != ' '
                    });
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
                self.git_branch.clear();
                self.git_dirty = false;
                format!("Git state failed: {error}")
            }
            None => {
                self.git_entries.clear();
                self.git_branch.clear();
                self.git_dirty = false;
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
            self.debug_sessions.clear();
            self.selected_debug_session = 0;
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
                Ok(snapshots) => {
                    self.debug_sessions = snapshots
                        .iter()
                        .map(|(name, snapshot)| DebugSessionRow {
                            name: name.clone(),
                            state: snapshot.state,
                            pid: snapshot.adapter_process_id,
                        })
                        .collect();
                    self.selected_debug_session = self
                        .selected_debug_session
                        .min(self.debug_sessions.len().saturating_sub(1));
                    snapshots
                    .iter()
                    .map(|(name, snapshot)| {
                        format!(
                            "● {} · {} · pid {} · {} breakpoints · {} watches · {} threads/processes",
                            name,
                            snapshot.state.label(),
                            snapshot.adapter_process_id,
                            snapshot.breakpoints.values().map(Vec::len).sum::<usize>(),
                            snapshot.watches.len(),
                            snapshot.debuggee_processes.len()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
                }
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
                    if let Some(entity) = self.pending_page_entity.clone()
                        && self.select_page_entity(&entity)
                    {
                        self.pending_page_entity = None;
                        self.status = format!("Page bound · {entity}");
                    }
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
            } => {
                let _ = self.capture_workflow_click();
                (
                    "glass.browser.act",
                    serde_json::json!({"action":"click", "target": target, "browserRevision": expected_revision}),
                )
            }
            BrowserWorkspaceAction::Type {
                target,
                text,
                expected_revision,
            } => {
                self.capture_workflow_type_from_selection();
                (
                    "glass.browser.act",
                    serde_json::json!({"action":"type", "target": target, "text": text, "browserRevision": expected_revision}),
                )
            }
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
                    unrestricted: self.yolo_mode,
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

fn diagnostic_matches_path(
    diagnostic: &crate::development::LanguageDiagnostic,
    path: &str,
) -> bool {
    path.is_empty()
        || diagnostic.path == path
        || diagnostic.path.ends_with(path)
        || path.ends_with(&diagnostic.path)
}

fn truncate_note(text: &str) -> String {
    let text = text.trim().replace('\n', " ");
    if text.chars().count() <= 80 {
        text
    } else {
        format!("{}…", text.chars().take(79).collect::<String>())
    }
}

fn parse_editor_diagnostics(
    value: &serde_json::Value,
) -> Vec<crate::development::LanguageDiagnostic> {
    if let Ok(list) =
        serde_json::from_value::<Vec<crate::development::LanguageDiagnostic>>(value.clone())
    {
        return list;
    }
    value
        .get("diagnostics")
        .and_then(|item| {
            serde_json::from_value::<Vec<crate::development::LanguageDiagnostic>>(item.clone()).ok()
        })
        .unwrap_or_default()
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

fn parse_conversation_view(conversation: &str) -> Vec<super::projection::ConversationEntry> {
    let mut entries = Vec::new();
    for block in conversation.split("\n\n") {
        let mut lines = block.lines();
        let Some(first) = lines.next() else {
            continue;
        };
        let body = lines.collect::<Vec<_>>().join("\n");
        let (kind, text) = match first.trim() {
            "YOU" => (super::projection::ConversationKind::User, body),
            "GLASS AGENT" => (super::projection::ConversationKind::Assistant, body),
            "ALERT" => (super::projection::ConversationKind::Alert, body),
            "ERROR" => (super::projection::ConversationKind::Error, body),
            "SYSTEM" => (super::projection::ConversationKind::System, body),
            _ => continue,
        };
        entries.push(super::projection::ConversationEntry {
            kind,
            text,
            streaming: false,
            entry_id: None,
            tool_name: None,
        });
    }
    entries
}

fn parse_debug_threads(value: &serde_json::Value) -> Vec<DebugThreadRow> {
    value
        .get("threads")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|thread| {
            Some(DebugThreadRow {
                id: thread.get("id").and_then(serde_json::Value::as_i64)?,
                name: thread
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("thread")
                    .to_string(),
            })
        })
        .collect()
}

fn parse_debug_frames(value: &serde_json::Value) -> Vec<DebugFrameRow> {
    value
        .get("stackFrames")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|frame| {
            Some(DebugFrameRow {
                id: frame.get("id").and_then(serde_json::Value::as_i64)?,
                name: frame
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("frame")
                    .to_string(),
                path: frame
                    .pointer("/source/path")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                line: frame.get("line").and_then(serde_json::Value::as_u64),
            })
        })
        .collect()
}

fn parse_session_picker_items(value: &serde_json::Value) -> Vec<SessionPickerItem> {
    let items = value
        .as_array()
        .or_else(|| value.get("sessions").and_then(serde_json::Value::as_array))
        .cloned()
        .unwrap_or_default();
    items
        .into_iter()
        .filter_map(|item| {
            if let Some(path) = item.as_str() {
                return Some(SessionPickerItem {
                    label: std::path::Path::new(path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(path)
                        .to_string(),
                    path: path.to_string(),
                });
            }
            let path = item
                .get("path")
                .or_else(|| item.get("file"))
                .or_else(|| item.get("sessionFile"))
                .and_then(serde_json::Value::as_str)?;
            let label = item
                .get("name")
                .or_else(|| item.get("sessionName"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    std::path::Path::new(path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(path)
                        .to_string()
                });
            Some(SessionPickerItem {
                path: path.to_string(),
                label,
            })
        })
        .collect()
}

fn stats_summary(value: &serde_json::Value) -> String {
    let input = value
        .pointer("/inputTokens")
        .or_else(|| value.pointer("/tokens/input"))
        .or_else(|| value.get("input"))
        .and_then(serde_json::Value::as_u64);
    let output = value
        .pointer("/outputTokens")
        .or_else(|| value.pointer("/tokens/output"))
        .or_else(|| value.get("output"))
        .and_then(serde_json::Value::as_u64);
    match (input, output) {
        (Some(input), Some(output)) => format!("{input} in · {output} out"),
        (Some(input), None) => format!("{input} tokens"),
        _ => super::projection::first_meaningful(value)
            .lines()
            .next()
            .unwrap_or("stats")
            .to_string(),
    }
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

struct LspLocation {
    path: String,
    line: u32,
    column: u32,
}

fn parse_completion_insert(value: &serde_json::Value) -> Option<String> {
    let items = value
        .get("items")
        .and_then(serde_json::Value::as_array)
        .or_else(|| value.as_array())?;
    let item = items.first()?;
    item.pointer("/textEdit/newText")
        .or_else(|| item.pointer("/textEdit/insert/newText"))
        .or_else(|| item.get("insertText"))
        .or_else(|| item.get("label"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .filter(|text| !text.is_empty())
}

fn format_lsp_locations(value: &serde_json::Value) -> String {
    let items = value
        .as_array()
        .map(|items| items.as_slice())
        .unwrap_or_else(|| std::slice::from_ref(value));
    let mut lines = vec!["REFERENCES".to_string()];
    for item in items.iter().take(32) {
        if let Some(location) = parse_lsp_location(item) {
            lines.push(format!(
                "  {}:{}:{}",
                location.path, location.line, location.column
            ));
        }
    }
    if lines.len() == 1 {
        lines.push("  none".into());
    }
    lines.join("\n")
}

fn parse_lsp_location(value: &serde_json::Value) -> Option<LspLocation> {
    let target = value
        .as_array()
        .and_then(|items| items.first())
        .unwrap_or(value);
    let uri = target
        .get("uri")
        .or_else(|| target.get("targetUri"))
        .and_then(serde_json::Value::as_str)?;
    let range = target
        .get("range")
        .or_else(|| target.get("targetRange"))
        .or_else(|| target.get("targetSelectionRange"))?;
    let line = range
        .pointer("/start/line")
        .and_then(serde_json::Value::as_u64)? as u32
        + 1;
    let column = range
        .pointer("/start/character")
        .and_then(serde_json::Value::as_u64)? as u32
        + 1;
    let path = uri
        .rsplit("://")
        .next()
        .unwrap_or(uri)
        .rsplit('/')
        .take(3)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("/");
    Some(LspLocation {
        path: path.trim_start_matches('/').to_string(),
        line,
        column,
    })
}

fn parse_lsp_symbols(value: &serde_json::Value) -> Vec<(String, u32)> {
    let mut symbols = Vec::new();
    let items = value
        .get("result")
        .or(Some(value))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    for item in items {
        let name = item
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("symbol")
            .to_string();
        let line = item
            .pointer("/range/start/line")
            .or_else(|| item.pointer("/location/range/start/line"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32
            + 1;
        symbols.push((name, line));
    }
    symbols
}

fn workflow_slug(name: &str) -> String {
    let mut slug = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if matches!(character, '-' | '_' | ' ' | '.')
            && !slug.is_empty()
            && !slug.ends_with('-')
        {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "click-path".into()
    } else {
        slug
    }
}

fn slug_input_name(name: &str) -> String {
    workflow_slug(name).replace('-', "_")
}

fn selection_covers(
    content: &str,
    selection: &crate::development::TextSelection,
    cursor: crate::development::TextPosition,
) -> bool {
    let Some(offset) = crate::development::editor::text_position_offset(content, cursor) else {
        return false;
    };
    let Some((start, end)) = crate::development::editor::selection_offsets(content, selection)
    else {
        return false;
    };
    offset >= start && offset <= end
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
    fn debug_thread_and_frame_packets_parse_source_locations() {
        let threads = parse_debug_threads(&serde_json::json!({
            "threads": [{"id": 1, "name": "main"}, {"id": 2, "name": "worker"}]
        }));
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[1].name, "worker");
        let frames = parse_debug_frames(&serde_json::json!({
            "stackFrames": [{
                "id": 12,
                "name": "checkout",
                "line": 40,
                "source": {"path": "src/main.rs"}
            }]
        }));
        assert_eq!(frames[0].path.as_deref(), Some("src/main.rs"));
        assert_eq!(frames[0].line, Some(40));
    }

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
    fn typed_open_in_palette_runs_instead_of_fuzzy_action() {
        let root = std::env::temp_dir().join(format!("glass-typed-open-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.files = vec!["src/lib.rs".into(), "Cargo.toml".into()];
        state.open_palette();
        state.command_input = "open".into();
        state.command_cursor = 4;
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);
        state.submit_palette(&mut worker);
        drop(worker);
        assert!(
            state.file_picker_open,
            "typed :open must open the file picker, not a fuzzy action"
        );
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
    fn file_picker_filters_and_opens_a_matching_path() {
        let root = std::env::temp_dir().join(format!("glass-file-picker-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create temporary workspace");
        std::fs::write(root.join("src/lib.rs"), "fn lib() {}\n").unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.files = vec![
            "src/lib.rs".into(),
            "src/main.rs".into(),
            "README.md".into(),
        ];
        state.open_file_picker();
        assert!(state.file_picker_open);
        for character in "main".chars() {
            state.insert_file_picker_char(character);
        }
        let matches = state.file_picker_matches();
        assert_eq!(matches.len(), 1);
        assert_eq!(state.files[matches[0]], "src/main.rs");
        state.submit_file_picker();
        assert!(!state.file_picker_open);
        assert_eq!(state.focused_editor_path, "src/main.rs");
        assert!(state.code_edit_mode);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn composer_shift_enter_and_history_are_local_edits() {
        let root =
            std::env::temp_dir().join(format!("glass-composer-history-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.open_composer();
        state.insert_composer_text("first");
        state.remember_composer_history("first");
        state.composer_input.clear();
        state.composer_cursor = 0;
        state.insert_composer_text("second");
        state.navigate_composer_history(true);
        assert_eq!(state.composer_input, "first");
        state.navigate_composer_history(false);
        assert_eq!(state.composer_input, "second");
        state.insert_composer_newline();
        assert_eq!(state.composer_input, "second\n");
        state.navigate_composer_history(true);
        assert!(
            state.composer_cursor < state.composer_input.len(),
            "Up on a wrapped draft should move to the previous line before history"
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
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
    fn composer_tab_completes_file_mentions_and_submit_pins_the_path() {
        let root =
            std::env::temp_dir().join(format!("glass-composer-mention-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.files = vec!["src/main.rs".into(), "src/lib.rs".into()];
        state.focused_editor_path = "src/main.rs".into();
        state.open_composer();
        state.insert_composer_text("inspect @fi");
        state.complete_composer_mention();
        assert_eq!(state.composer_input, "inspect @file");
        state.complete_composer_mention();
        assert_eq!(state.composer_input, "inspect @file:src/main.rs");
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn editor_gutter_marks_open_comments_on_the_focused_buffer() {
        let root = std::env::temp_dir().join(format!(
            "glass-editor-comment-gutter-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.focused_editor_path = "src/main.rs".into();
        state
            .editor_comments
            .push(crate::development::EditorComment {
                id: "comment-1".into(),
                path: "src/main.rs".into(),
                start_line: 4,
                end_line: 4,
                text: "simplify this".into(),
                actor: crate::development::Actor::local(),
                state: crate::development::EditorCommentState::Open,
                created_revision: 1,
                updated_revision: 1,
            });
        let marks = state.editor_gutter_marks();
        assert!(marks.contains(&(4, native::GutterMark::Comment)));
        let notes = state.editor_source_notes();
        assert!(
            notes
                .iter()
                .any(|(line, note)| *line == 4 && note.contains("simplify this"))
        );
        state.focused_editor_line = 4;
        state.show_comment_thread();
        assert!(
            state
                .editor_engine
                .overlay
                .as_deref()
                .is_some_and(|overlay| overlay.contains("simplify this"))
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn gutter_marks_diagnostics_and_proof_on_source_lines() {
        let root = std::env::temp_dir().join(format!("glass-gutter-proof-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(root.join("src/main.rs"), "fn main() {}\nfn extra() {}\n")
            .expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.focused_editor_path = "src/main.rs".into();
        state.focused_editor_line = 2;
        state
            .editor_diagnostics
            .push(crate::development::LanguageDiagnostic {
                path: "src/main.rs".into(),
                start: crate::development::DiagnosticPosition {
                    line: 0,
                    character: 3,
                },
                end: crate::development::DiagnosticPosition {
                    line: 0,
                    character: 7,
                },
                severity: Some(1),
                code: None,
                source: Some("rustc".into()),
                message: "unused variable".into(),
            });
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .link_runtime_source(
                "action.main",
                "src/main.rs",
                1,
                1,
                crate::development::LinkProvenance::ExplicitMarker,
                "handler",
                1.0,
                crate::development::Actor::local(),
            )
            .expect("link");
        state.last_proof_ok = Some(true);
        let marks = state.editor_gutter_marks();
        assert!(
            marks.contains(&(1, native::GutterMark::Lsp)),
            "diagnostic is on LSP line 0 → editor line 1: {marks:?}"
        );
        assert!(
            marks.contains(&(1, native::GutterMark::Proof)),
            "proof sits on the handler, not the cursor: {marks:?}"
        );
        assert!(!marks.contains(&(2, native::GutterMark::Proof)));
        let notes = state.editor_source_notes();
        assert!(
            notes
                .iter()
                .any(|(line, note)| *line == 1 && note.contains("unused variable"))
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn page_gutter_marks_graph_linked_handlers() {
        let root = std::env::temp_dir().join(format!("glass-page-gutter-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(root.join("src/button.tsx"), "export function Pay() {}\n")
            .expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .link_runtime_source(
                "action.checkout.submit",
                "src/button.tsx",
                1,
                1,
                crate::development::LinkProvenance::ExplicitMarker,
                "test link",
                1.0,
                crate::development::Actor::local(),
            )
            .expect("link source");
        state.focused_editor_path = "src/button.tsx".into();
        let marks = state.editor_gutter_marks();
        assert!(marks.contains(&(1, native::GutterMark::Page)));
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn gp_navigates_to_the_inferred_app_route() {
        let root = std::env::temp_dir().join(format!("glass-gp-app-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.focused_editor_path = "app/settings/page.tsx".into();
        state.focused_editor_line = 1;
        state.process_urls = vec!["http://localhost:3000/".into()];
        state.jump_page_from_source();
        assert_eq!(
            state.pending_browser_navigation.as_deref(),
            Some("http://localhost:3000/settings")
        );
        assert_eq!(state.surface, DevSurface::App);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn g_on_app_opens_the_linked_source() {
        let root = std::env::temp_dir().join(format!("glass-app-source-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(
            root.join("src/button.tsx"),
            "<button data-glass-entity=\"action.checkout.submit\">Pay</button>\n",
        )
        .expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.files = vec!["src/button.tsx".into()];
        state.browser_workspace.replace_entities(
            1,
            vec![BrowserWorkspaceEntity {
                reference: "action.checkout.submit".into(),
                role: "button".into(),
                name: "Pay".into(),
                actionable: true,
                revision: 1,
            }],
        );
        state.browser_workspace.state_mut().selected_entity = Some(0);
        state.jump_source_from_page();
        assert_eq!(state.focused_editor_path, "src/button.tsx");
        assert_eq!(state.focused_editor_line, 1);
        assert!(state.code_edit_mode);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn hosted_fim_stub_fills_an_empty_block() {
        let root = std::env::temp_dir().join(format!("glass-fim-stub-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(
            root.join("glass.toml"),
            "[editor.fim]\nendpoint = \"stub://test\"\nmodel = \"stub\"\n",
        )
        .expect("write fim config");
        std::fs::write(root.join("src/main.rs"), "fn main() {\n    \n}\n").expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open");
        state.refresh_editor_projection();
        state
            .ws_mut()
            .expect("lock")
            .project_mut()
            .set_buffer_cursor("src/main.rs", 2, 5)
            .expect("cursor");
        state.refresh_editor_projection();
        state.request_ghost_from_line();
        let mut attempts = 0;
        while state.editor_engine.ghost.is_none() && attempts < 50 {
            let _ = state.tick_fim();
            std::thread::sleep(std::time::Duration::from_millis(5));
            attempts += 1;
        }
        assert_eq!(
            state
                .editor_engine
                .ghost
                .as_ref()
                .map(|ghost| ghost.text.as_str()),
            Some("todo!()"),
            "configured stub FIM should fill the empty block"
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn ghost_tab_jumps_to_the_next_incomplete_ident() {
        let root = std::env::temp_dir().join(format!("glass-ghost-next-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        let source = "fn hello_world() {}\nfn hello_\nfn greet_user() {}\nfn greet_\n";
        std::fs::write(root.join("src/main.rs"), source).expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open buffer");
        state.refresh_editor_projection();
        state.enter_code_edit();
        let _ = state.set_editor_cursor(
            "src/main.rs",
            crate::development::TextPosition {
                line: 2,
                column: 10,
            },
            false,
        );
        state.refresh_editor_projection();
        state.editor_engine.ghost = Some(GhostText {
            text: "world".into(),
        });
        state.edit_code_key(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        );
        assert!(
            state.focused_editor_content.contains("fn hello_world\n"),
            "tab should apply the ghost: {}",
            state.focused_editor_content
        );
        assert_eq!(state.focused_editor_line, 4);
        assert_eq!(
            state
                .editor_engine
                .ghost
                .as_ref()
                .map(|ghost| ghost.text.as_str()),
            Some("user")
        );
        assert!(state.status.contains("next edit"));
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn crew_tool_result_stores_the_wake_artifact() {
        let root = std::env::temp_dir().join(format!("glass-crew-wake-tui-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.running_tool_job = Some(3);
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 3,
            tool: "glass.task.crew".into(),
            result: Ok(serde_json::json!({
                "goal": "add settings toggle",
                "wake": {
                    "id": "add-settings-toggle",
                    "goal": "add settings toggle",
                    "worktree": "/tmp/worktree",
                    "checkpoint": "before-crew:add settings toggle",
                    "createdAtMs": 1,
                    "tasks": [{"id":"task-0001","role":"architect","title":"architect: add settings toggle","state":"queued"}]
                }
            })),
        });
        let wake = state.last_crew_wake.expect("wake");
        assert!(wake.contains("WAKE add-settings-toggle"));
        assert!(wake.contains("architect task-0001 queued"));
        assert_eq!(state.surface, DevSurface::Tasks);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn review_accept_applies_the_wake_proposal_pack() {
        let root = std::env::temp_dir().join(format!("glass-review-pack-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open buffer");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .propose_editor_change(
                "src/main.rs",
                "fn main() {}\n".into(),
                "fn main() { 1 }\n".into(),
                "add one".into(),
                crate::development::Actor::local(),
            )
            .expect("propose");
        state.refresh_editor_projection();
        let accepted = state.accept_review_pack().expect("accept pack");
        assert!(accepted.contains("1 proposal"));
        assert_eq!(
            state.focused_editor_content, "fn main() { 1 }\n",
            "pack accept must write the proposed buffer"
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn gm_adds_structural_carets_and_inserts_at_each() {
        let root = std::env::temp_dir().join(format!("glass-multi-cursor-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(root.join("src/main.rs"), "fn foo() { foo(); foo }\n").expect("write");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open");
        state.refresh_editor_projection();
        state.enter_code_edit();
        state.handle_editor_escape();
        state
            .ws_mut()
            .expect("lock")
            .project_mut()
            .set_buffer_cursor("src/main.rs", 1, 4)
            .expect("cursor");
        state.refresh_editor_projection();
        state.edit_code_key(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::empty(),
        );
        state.edit_code_key(
            crossterm::event::KeyCode::Char('m'),
            crossterm::event::KeyModifiers::empty(),
        );
        assert_eq!(
            state.editor_engine.extra_selections.len(),
            2,
            "gm should add a caret on every other foo"
        );
        state.edit_code_key(
            crossterm::event::KeyCode::Char('i'),
            crossterm::event::KeyModifiers::empty(),
        );
        state.edit_code_key(
            crossterm::event::KeyCode::Char('X'),
            crossterm::event::KeyModifiers::empty(),
        );
        let content = state.focused_editor_content.clone();
        assert_eq!(
            content.matches('X').count(),
            3,
            "insert should hit every caret: {content}"
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn app_enter_records_role_name_locators_into_a_workflow_draft() {
        let root =
            std::env::temp_dir().join(format!("glass-workflow-record-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.browser_workspace.replace_entities(
            7,
            vec![BrowserWorkspaceEntity {
                reference: "r7:save".into(),
                role: "button".into(),
                name: "Save settings".into(),
                actionable: true,
                revision: 7,
            }],
        );
        state
            .start_workflow_recording("Save Settings")
            .expect("start recording");
        state.queue_browser_intent(BrowserWorkspaceIntent::ActivateSelected);
        let draft = serde_json::to_string(
            state
                .workflow_recording
                .as_ref()
                .expect("recording")
                .recorder
                .draft(),
        )
        .expect("serialize draft");
        assert!(draft.contains("role=button;name=Save settings"));
        let stopped = state.stop_workflow_recording().expect("stop recording");
        assert!(stopped.contains("1 step"));
        let path = root.join(".glass/workflows/save-settings.draft.json");
        let persisted = std::fs::read_to_string(&path).expect("read draft");
        assert!(persisted.contains("role=button;name=Save settings"));
        assert!(persisted.contains("\"reviewRequired\": true"));
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
        assert_eq!(state.surface, DevSurface::Code);
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

    #[test]
    fn composer_queues_follow_up_while_a_send_is_in_flight() {
        let root = std::env::temp_dir().join(format!("glass-follow-up-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.surface = DevSurface::Agent;
        state.snapshot_trust_label = "trusted".into();
        state.agent_readiness = "✓ Ready · Node ✓ · SDK 0.84.3 · auth ✓".into();
        state.agent_send_job = Some(9);
        state.open_composer();
        state.composer_input = "keep going".into();
        state.composer_cursor = state.composer_input.len();
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);
        state.submit_composer(&mut worker);
        assert_ne!(
            state.status,
            "Background operation running · message kept in composer"
        );
        assert!(
            state.status.contains("Queued follow-up")
                || state.status.contains("thinking")
                || state.status.contains("retry")
                || state.status.contains("Sent")
        );
        drop(worker);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn composer_slash_compact_routes_to_pi_compact() {
        let root = std::env::temp_dir().join(format!("glass-slash-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.surface = DevSurface::Agent;
        state.snapshot_trust_label = "trusted".into();
        state.agent_readiness = "✓ Ready · Node ✓ · SDK 0.84.3 · auth ✓".into();
        state.open_composer();
        state.composer_input = "/compact keep the review".into();
        state.composer_cursor = state.composer_input.len();
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);
        state.submit_composer(&mut worker);
        assert!(
            state.status.contains("glass.agent.compact")
                || state.status.contains("unavailable")
                || state.status.contains("failed")
        );
        drop(worker);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn transcript_fork_uses_selected_entry_id() {
        let root = std::env::temp_dir().join(format!("glass-fork-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.conversation_items = vec![super::super::projection::ConversationEntry {
            kind: super::super::projection::ConversationKind::User,
            text: "ship the review".into(),
            streaming: false,
            entry_id: Some("entry-7".into()),
            tool_name: None,
        }];
        state.transcript_selection = 0;
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);
        state.fork_selected_transcript(&mut worker);
        assert!(
            state.status.contains("entry-7")
                || state.status.contains("glass.agent.fork")
                || state.status.contains("branch")
        );
        drop(worker);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn agent_chrome_and_session_picker_and_context_chips() {
        let root = std::env::temp_dir().join(format!("glass-chrome-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.agent_model = "xai/grok".into();
        state.agent_thinking = "medium".into();
        state.agent_session_name = "review".into();
        state.agent_token_summary = "12 in · 40 out".into();
        state.focused_editor_path = "src/lib.rs".into();
        state.composer_steer = true;
        let chrome = state.agent_chrome_line();
        assert!(chrome.contains("xai/grok"));
        assert!(chrome.contains("think medium"));
        assert!(chrome.contains("review"));
        assert!(chrome.contains("12 in"));
        assert!(state.composer_context_chips().contains("@src/lib.rs"));
        assert!(state.composer_context_chips().contains("steer"));
        state.open_session_picker(&serde_json::json!([
            {"path": "/tmp/alpha.jsonl", "name": "alpha"},
            {"path": "/tmp/beta.jsonl"}
        ]));
        assert!(state.session_picker_open);
        assert_eq!(state.session_picker_items[0].label, "alpha");
        assert_eq!(state.session_picker_items[1].label, "beta.jsonl");
        state.edit_last_user_message();
        state.conversation_items = vec![super::super::projection::ConversationEntry {
            kind: super::super::projection::ConversationKind::User,
            text: "previous prompt\n· sending…".into(),
            streaming: false,
            entry_id: Some("entry-1".into()),
            tool_name: None,
        }];
        state.edit_last_user_message();
        assert!(state.composer_mode);
        assert_eq!(state.composer_input, "previous prompt");
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn chat_dock_stays_on_code_and_app() {
        let root = std::env::temp_dir().join(format!("glass-dock-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.snapshot_trust_label = "trusted".into();
        state.agent_readiness = "✓ Ready · Node ✓ · SDK 0.84.3 · auth ✓".into();
        state.surface = DevSurface::Code;
        std::fs::create_dir_all(root.join("src")).expect("src");
        std::fs::write(root.join("src/lib.rs"), "fn main() {}\n").expect("write");
        state
            .ws_mut()
            .expect("lock")
            .project_mut()
            .open_buffer("src/lib.rs", crate::development::Actor::local())
            .expect("open buffer");
        state.refresh_editor_projection();
        state.prepare_editor_agent_prompt();
        assert_eq!(state.surface, DevSurface::Code);
        assert!(state.composer_mode);
        state.close_composer();
        state.surface = DevSurface::App;
        state.focus_composer_dock();
        assert_eq!(state.surface, DevSurface::App);
        assert!(state.composer_mode);
        state.cycle_composer_run_mode();
        assert_eq!(state.composer_run_mode, crate::AgentTurnMode::Ask);
        state.cycle_composer_run_mode();
        assert_eq!(state.composer_run_mode, crate::AgentTurnMode::Plan);
        state.capture_plan_from_goal("ship the checkout");
        state.pending_plan.as_mut().unwrap().body = "1. open App\n2. click Sign in".into();
        assert!(!state.pending_plan.as_ref().unwrap().accepted);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn watching_agent_browser_act_enables_live_view() {
        let root = std::env::temp_dir().join(format!("glass-watch-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.watch_agent_on_app(
            "glass.browser.act",
            &serde_json::json!({"target": "Sign in"}),
        );
        assert!(state.browser_visual_live);
        assert!(state.status.contains("Sign in"));
        assert!(state.status.contains("watching"));
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }
}
