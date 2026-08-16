use super::command;
use crate::browser::BrowserStartConfig;
use crate::{ExperimentComparison, SharedDevelopmentWorkspace};
use glass_browser::browser_workspace::{
    BrowserConnectionPhase, BrowserWorkspaceAction, BrowserWorkspaceAdapterKind,
    BrowserWorkspaceController, BrowserWorkspaceEntity, BrowserWorkspaceIntent,
    BrowserWorkspaceLayout,
};
use glass_browser::cli::args::TuiLayout;
use glass_browser::terminal_graphics::AnsiCanvas;
use glass_browser::tui::live_view::AnsiPane;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub struct PendingConfirmation {
    pub call: crate::development::ToolCall,
    pub context: crate::tools::DevelopmentToolContext,
    pub summary: String,
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
        Self {
            reason: error.to_string(),
            port,
            compatible_endpoint: error.to_lowercase().contains("attach"),
        }
    }

    pub fn actions(&self) -> &'static [(&'static str, &'static str)] {
        if self.compatible_endpoint {
            &[
                ("1", "attach to the running Chrome on this port"),
                ("2", "launch on an automatic free port"),
                ("3", "try an explicit port"),
                ("Esc", "dismiss"),
            ]
        } else {
            &[
                ("1", "launch on an automatic free port"),
                ("2", "try an explicit port"),
                ("3", "retry the preferred port"),
                ("Esc", "dismiss"),
            ]
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

    pub const PRIMARY: [Self; 7] = [
        Self::Agent,
        Self::Code,
        Self::App,
        Self::Terminal,
        Self::Tasks,
        Self::Git,
        Self::Debug,
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
    pub quit: bool,
    pub command_mode: bool,
    pub command_input: String,
    pub command_cursor: usize,
    pub command_history: Vec<String>,
    pub command_history_index: Option<usize>,
    pub palette_error: Option<String>,
    pub menu_open: bool,
    pub menu_selection: usize,
    pub composer_mode: bool,
    pub composer_input: String,
    pub composer_cursor: usize,
    pub composer_steer: bool,
    pub selected_agent: Option<crate::AgentId>,
    pub pending_confirmation: Option<PendingConfirmation>,
    pub surface_scroll: std::collections::BTreeMap<DevSurface, usize>,
    pub terminal_width: u16,
    pub terminal_height: u16,
    pub status: String,
    /// Wall-clock cost of the last background refresh pass.
    pub refresh_latency_ms: u64,
    pub agents: String,
    pub agent_readiness: String,
    pub agent_conversation: String,
    pub tasks: String,
    pub editor: String,
    pub files: Vec<String>,
    pub selected_file: usize,
    pub code_edit_mode: bool,
    pub lsp: String,
    pub processes: String,
    pub git: String,
    pub tests: String,
    pub kernels: String,
    pub debugger: String,
    pub replay: String,
    pub browser: String,
    pub browser_detail: String,
    pub browser_workspace: BrowserWorkspaceController,
    pub browser_recovery: Option<BrowserRecoveryOffer>,
    pub browser_visual_live: bool,
    pub browser_ansi: AnsiCanvas,
    pub browser_pane: Option<AnsiPane>,
    pub workflow: String,
    pub workspace_status: String,
    pub experiment_comparison: Option<ExperimentComparison>,
    pub experiments: String,
}

impl DevTuiState {
    /// Lock the shared workspace, mapping errors to strings for the palette.
    pub fn ws(&self) -> Result<std::sync::MutexGuard<'_, crate::DevelopmentWorkspace>, String> {
        self.workspace.lock().map_err(|error| error.to_string())
    }

    /// Lock the shared workspace for mutation, mapping errors to strings.
    pub fn ws_mut(&self) -> Result<std::sync::MutexGuard<'_, crate::DevelopmentWorkspace>, String> {
        self.workspace.lock().map_err(|error| error.to_string())
    }

    /// Lock the workspace for mutation inside UI callbacks; reports lock
    /// failures through the status line instead of `?`.
    fn locked<F, T>(&mut self, operation: F) -> Option<T>
    where
        F: FnOnce(&mut crate::DevelopmentWorkspace) -> T,
    {
        match self.workspace.lock() {
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
        let workspace = SharedDevelopmentWorkspace::open(root)?;
        let locked = workspace.lock()?;
        let trust_prompt = locked.trust() == crate::WorkspaceTrust::Untrusted
            && locked
                .trust_inspection()
                .iter()
                .any(|item| item.trust_required);
        drop(locked);
        let mut state = Self {
            workspace,
            surface: if trust_prompt {
                DevSurface::Trust
            } else {
                DevSurface::Agent
            },
            layout,
            quit: false,
            command_mode: false,
            command_input: String::new(),
            command_cursor: 0,
            command_history: Vec::new(),
            command_history_index: None,
            palette_error: None,
            menu_open: false,
            menu_selection: 0,
            composer_mode: false,
            composer_input: String::new(),
            composer_cursor: 0,
            composer_steer: false,
            selected_agent: None,
            pending_confirmation: None,
            surface_scroll: std::collections::BTreeMap::new(),
            terminal_width: 80,
            terminal_height: 24,
            refresh_latency_ms: 0,
            status: if trust_prompt {
                "Workspace trust required · I inspect · O untrusted · 1 trust once · T trust project".into()
            } else {
                "Ready · a actions · : commands · q quits".into()
            },
            agents: String::new(),
            agent_readiness: crate::pi_runtime::pi_readiness()
                .map(|readiness| format_pi_readiness(&readiness))
                .unwrap_or_else(|error| format!("Agent unavailable · {error}")),
            agent_conversation: "No conversation yet. Press i to compose a message.".into(),
            tasks: String::new(),
            editor: String::new(),
            files: Vec::new(),
            selected_file: 0,
            code_edit_mode: false,
            lsp: String::new(),
            processes: String::new(),
            git: String::new(),
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
            browser_visual_live: false,
            browser_ansi: AnsiCanvas::default(),
            browser_pane: None,
            workflow: "No workflow evidence yet".into(),
            workspace_status: String::new(),
            experiment_comparison: None,
            experiments: "No experiments. :experiment create ID BRANCH [PORT]".into(),
        };
        state.refresh();
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

    /// Discoverable actions for the current surface. `a` opens this menu.
    pub fn surface_actions(&self) -> Vec<(&'static str, &'static str, &'static str)> {
        match self.surface {
            DevSurface::Agent => vec![
                ("Compose message", "i", ""),
                ("Setup Pi runtime", "agent setup", ":"),
                ("Authenticate", "agent setup login", ":"),
                ("Readiness doctor", "agent doctor", ":"),
                ("New conversation", "agent new", ":"),
            ],
            DevSurface::Code => vec![
                ("Open selected file", "Enter", ""),
                ("Edit file", "i", ""),
                ("Save buffer", "Ctrl-S", ""),
                ("Search project", "editor search QUERY", ":"),
                ("Diagnostics", "lsp diagnostics", ":"),
            ],
            DevSurface::App => vec![
                ("Start browser", "browser start", ":"),
                ("Observe page", "browser observe", ":"),
                ("Navigate", "n", ""),
                ("Type into selected", "t", ""),
                ("Targets", "browser targets", ":"),
            ],
            DevSurface::Terminal => vec![
                ("Start dev server", "process start dev COMMAND", ":"),
                ("View logs", "process logs NAME", ":"),
                ("Stop process", "process stop NAME", ":"),
            ],
            DevSurface::Tasks => vec![
                ("Create task", "task create TITLE PROMPT", ":"),
                ("Cancel task", "task cancel TASK_ID", ":"),
                ("Retry task", "task retry TASK_ID", ":"),
            ],
            DevSurface::Git => vec![
                ("Stage all changes", "git stage .", ":"),
                ("Commit", "git commit MESSAGE", ":"),
                ("View diff", "git diff", ":"),
                ("Branches", "git branches", ":"),
            ],
            DevSurface::Debug => vec![
                ("Start debug session", "debug start NAME COMMAND", ":"),
                ("Run tests", "test run RUN_ID SUITE_ID", ":"),
                ("Test results", "test results", ":"),
            ],
            DevSurface::More => vec![
                ("Experiments", "experiment create ID BRANCH", ":"),
                ("Kernels", "kernel start NAME KIND", ":"),
                ("Workspace state", "workspace", ":"),
            ],
            DevSurface::Trust => vec![
                ("Inspect configuration", "I", ""),
                ("Trust once", "1", ""),
                ("Trust project", "T", ""),
            ],
        }
    }

    pub fn open_menu(&mut self) {
        self.menu_open = true;
        self.menu_selection = 0;
        self.status = "Surface actions · Enter runs · Esc closes".into();
    }

    pub fn close_menu(&mut self) {
        self.menu_open = false;
    }

    /// Run the selected menu action. Keyboard hints apply directly; strings
    /// starting with ':' open the palette prefilled with the command.
    pub fn run_menu_action(&mut self) {
        let Some((_, hint, prefix)) = self.surface_actions().get(self.menu_selection).copied()
        else {
            return;
        };
        self.menu_open = false;
        if prefix == ":" {
            if hint.contains(' ') {
                // Commands with arguments open prefilled for completion.
                self.open_palette_with(&format!("{} ", hint));
            } else {
                self.open_palette_with(hint);
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
        }
        // Single-key hints (Ctrl-S, n, t, 1, T, I) apply in navigation mode;
        // the status line reminds the user which key to press.
        else if !hint.is_empty() && hint.len() <= 6 {
            self.status = format!("Press {hint} in navigation mode");
        }
    }

    pub fn move_menu_selection(&mut self, delta: i32) {
        let count = self.surface_actions().len() as i32;
        self.menu_selection =
            (self.menu_selection as i32 + delta).rem_euclid(count.max(1)) as usize;
    }

    pub fn open_palette(&mut self) {
        self.command_mode = true;
        self.command_input.clear();
        self.command_cursor = 0;
        self.command_history_index = None;
        self.palette_error = None;
        self.status = "Command palette · type help for routes".into();
    }

    pub fn close_palette(&mut self) {
        self.command_mode = false;
        self.command_input.clear();
        self.command_cursor = 0;
        self.command_history_index = None;
        self.palette_error = None;
        self.status = "Command cancelled".into();
    }

    pub fn submit_palette(&mut self) {
        let input = std::mem::take(&mut self.command_input);
        self.command_cursor = 0;
        self.command_history_index = None;
        self.command_mode = false;
        if !input.trim().is_empty() && self.command_history.last() != Some(&input) {
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
        self.refresh();
    }

    pub fn insert_palette_char(&mut self, character: char) {
        self.command_input.insert(self.command_cursor, character);
        self.command_cursor += character.len_utf8();
        self.palette_error = None;
        self.command_history_index = None;
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
    }

    pub fn navigate_palette_history(&mut self, older: bool) {
        if self.command_history.is_empty() {
            return;
        }
        let last = self.command_history.len() - 1;
        let index = match (self.command_history_index, older) {
            (None, true) => last,
            (None, false) => return,
            (Some(index), true) => index.saturating_sub(1),
            (Some(index), false) if index < last => index + 1,
            (Some(_), false) => {
                self.command_history_index = None;
                self.command_input.clear();
                self.command_cursor = 0;
                return;
            }
        };
        self.command_history_index = Some(index);
        self.command_input.clone_from(&self.command_history[index]);
        self.command_cursor = self.command_input.len();
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
        self.status = format!("Completed `{completion}` · Enter runs · ↑/↓ history");
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

    pub fn palette_matches(&self) -> Vec<&'static str> {
        const COMMANDS: [&str; 18] = [
            "agent",
            "browser",
            "debug",
            "editor",
            "experiment",
            "git",
            "help",
            "kernel",
            "lsp",
            "process",
            "quit",
            "replay",
            "task",
            "test",
            "trust",
            "view",
            "workflow",
            "workspace",
        ];
        let query = self
            .command_input
            .split_whitespace()
            .next()
            .unwrap_or_default();
        COMMANDS
            .into_iter()
            .filter(|command| fuzzy_contains(command, query))
            .take(6)
            .collect()
    }

    pub fn open_composer(&mut self) {
        self.composer_mode = true;
        self.composer_cursor = self.composer_input.len();
        self.status = "Agent composer · Enter sends · Esc cancels".into();
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

    pub fn abort_selected_agent(&mut self) {
        let Some(agent) = self.selected_agent.clone() else {
            self.status = "No active agent to abort".into();
            return;
        };
        let outcome = self
            .workspace
            .lock()
            .map_err(|e| e.to_string())
            .and_then(|mut workspace| workspace.agents().cancel(&agent).map_err(|e| e.to_string()));
        match outcome {
            Ok(()) => self.status = format!("Aborted {}", agent.as_str()),
            Err(error) => self.status = format!("Abort failed: {error}"),
        }
        self.refresh();
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

    pub fn submit_composer(&mut self) {
        let mut text = std::mem::take(&mut self.composer_input);
        let steer = self.composer_steer;
        self.composer_cursor = 0;
        self.composer_mode = false;
        self.composer_steer = false;
        if text.trim().is_empty() {
            self.status = "Message was empty".into();
            return;
        }
        let trusted = self
            .workspace
            .lock()
            .map(|workspace| workspace.trust().permits_project_execution())
            .unwrap_or(false);
        if !trusted {
            self.status = "Agent requires trusted project execution · open Trust".into();
            return;
        }
        if let Some(entity) = self.browser_workspace.state().selected() {
            text.push_str(&format!(
                "\n\n[Glass App context: {} `{}` at browser revision {}; use current browser evidence and preserve its lease/revision guards.]",
                entity.role, entity.reference, entity.revision
            ));
            self.browser_workspace.state_mut().input_owner =
                glass_browser::browser_workspace::BrowserInputOwner::Agent;
        }
        let current = self.selected_agent.clone().or_else(|| {
            self.workspace
                .agents_list()
                .into_iter()
                .find(|agent| {
                    !matches!(
                        agent.status,
                        crate::AgentStatus::Completed
                            | crate::AgentStatus::Failed
                            | crate::AgentStatus::Cancelled
                    )
                })
                .map(|agent| agent.id)
        });
        let result = if let Some(agent) = current {
            self.selected_agent = Some(agent.clone());
            let status = self.workspace.lock().and_then(|mut workspace| {
                workspace.agents().snapshot(&agent).map(|item| item.status)
            });
            let outcome = match status {
                Ok(crate::AgentStatus::Working) => {
                    self.workspace.lock().ok().and_then(|mut workspace| {
                        if steer {
                            workspace.agents().steer(&agent, text).ok()
                        } else {
                            workspace.agents().follow_up(&agent, text).ok()
                        }
                    })
                }
                Ok(_) => self
                    .workspace
                    .lock()
                    .ok()
                    .and_then(|mut workspace| workspace.agents().prompt(&agent, text).ok()),
                Err(error) => {
                    self.status = format!("Agent failed: {error}");
                    None
                }
            };
            outcome.map(|()| agent)
        } else {
            self.workspace.lock().ok().and_then(|mut workspace| {
                workspace
                    .agents()
                    .create(crate::AgentSpec::new("assistant", text))
                    .ok()
            })
        };
        match result {
            Some(agent) => {
                self.selected_agent = Some(agent);
                self.status = "Message queued".into();
            }
            None => self.status = "Message failed · workspace or agent unavailable".into(),
        }
        self.refresh();
    }

    pub fn approve_confirmation(&mut self) {
        let Some(pending) = self.pending_confirmation.take() else {
            return;
        };
        match self
            .workspace
            .lock()
            .and_then(|mut workspace| workspace.execute_tool(&pending.call, &pending.context))
        {
            Ok(_) => self.status = format!("Approved once · {}", pending.summary),
            Err(error) => self.status = format!("Approved action failed: {error}"),
        }
        self.refresh();
    }

    pub fn deny_confirmation(&mut self) {
        if let Some(pending) = self.pending_confirmation.take() {
            self.status = format!("Denied · {}", pending.summary);
        }
    }

    pub fn handle_printable(&mut self, character: char) {
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
                    .lock()
                    .and_then(|mut workspace| workspace.apply_local_trust_decision(decision))
                {
                    Ok(trust) => {
                        self.surface = DevSurface::Agent;
                        self.status = format!("Workspace opened with {} authority", trust.label());
                    }
                    Err(error) => self.status = format!("Trust decision failed: {error}"),
                }
                return;
            }
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
                self.status = format!("{} · : for actions", surface.label());
                return;
            }
        }
        let surface = match character {
            '1' | 'a' => Some(DevSurface::Agent),
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
            self.status = format!("{} · : for actions", surface.label());
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
        let content = match self.surface {
            DevSurface::Agent => &self.agent_conversation,
            DevSurface::Code => &self.editor,
            DevSurface::App => &self.browser,
            DevSurface::Terminal => &self.processes,
            DevSurface::Tasks => &self.tasks,
            DevSurface::Git => &self.git,
            DevSurface::Debug => &self.debugger,
            DevSurface::More | DevSurface::Trust => &self.workspace_status,
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

    pub fn open_selected_file(&mut self) {
        let Some(path) = self.files.get(self.selected_file).cloned() else {
            self.status = "No project file selected".into();
            return;
        };
        match self.workspace.lock().and_then(|mut workspace| {
            workspace
                .project_mut()
                .open_buffer(&path, crate::development::Actor::local())
        }) {
            Ok(_) => {
                self.status = format!("Opened {path} · press i to edit");
                self.refresh();
            }
            Err(error) => self.status = format!("Open failed: {error}"),
        }
    }

    pub fn enter_code_edit(&mut self) {
        let has_buffer = self
            .locked(|workspace| workspace.project().buffers().next().is_some())
            .unwrap_or(false);
        if !has_buffer {
            self.open_selected_file();
        }
        let has_buffer = self
            .locked(|workspace| workspace.project().buffers().next().is_some())
            .unwrap_or(false);
        if has_buffer {
            self.code_edit_mode = true;
            self.status =
                "EDIT · arrows move · Ctrl-S save · Ctrl-Z/Y undo/redo · Esc close".into();
        }
    }

    pub fn close_code_edit(&mut self) {
        self.code_edit_mode = false;
        self.status = "Code navigation".into();
    }

    pub fn edit_code_key(
        &mut self,
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) {
        let Some(buffer) = self
            .locked(|workspace| workspace.project().buffers().next().cloned())
            .flatten()
        else {
            self.status = "No open buffer".into();
            return;
        };
        let path = buffer.path.clone();
        let result = match (code, modifiers) {
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
            (crossterm::event::KeyCode::Left, _) => self.move_editor_cursor(&path, 0, -1),
            (crossterm::event::KeyCode::Right, _) => self.move_editor_cursor(&path, 0, 1),
            (crossterm::event::KeyCode::Up, _) => self.move_editor_cursor(&path, -1, 0),
            (crossterm::event::KeyCode::Down, _) => self.move_editor_cursor(&path, 1, 0),
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
        self.refresh();
    }

    fn move_editor_cursor(
        &mut self,
        path: &str,
        line_delta: i32,
        column_delta: i32,
    ) -> crate::development::DevelopmentResult<()> {
        let buffer = self
            .workspace
            .lock()?
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
        self.workspace
            .lock()?
            .project_mut()
            .set_buffer_cursor(path, line, column)
    }

    fn insert_editor_text(
        &mut self,
        path: &str,
        text: &str,
    ) -> crate::development::DevelopmentResult<()> {
        let buffer = self
            .workspace
            .lock()?
            .project()
            .buffer(path)
            .cloned()
            .ok_or_else(|| {
                crate::development::DevelopmentError::NotFound(format!("buffer {path}"))
            })?;
        let offset = editor_offset(&buffer.content, buffer.cursor_line, buffer.cursor_column);
        let mut content = buffer.content.clone();
        content.insert_str(offset, text);
        self.workspace.lock()?.project_mut().edit_buffer(
            path,
            content,
            crate::development::Actor::local(),
        )?;
        let (line, column) = if text == "\n" {
            (buffer.cursor_line + 1, 1)
        } else {
            (
                buffer.cursor_line,
                buffer.cursor_column + text.chars().count() as u32,
            )
        };
        self.workspace
            .lock()?
            .project_mut()
            .set_buffer_cursor(path, line, column)
    }

    fn backspace_editor(&mut self, path: &str) -> crate::development::DevelopmentResult<()> {
        let buffer = self
            .workspace
            .lock()?
            .project()
            .buffer(path)
            .cloned()
            .ok_or_else(|| {
                crate::development::DevelopmentError::NotFound(format!("buffer {path}"))
            })?;
        let offset = editor_offset(&buffer.content, buffer.cursor_line, buffer.cursor_column);
        if offset == 0 {
            return Ok(());
        }
        let previous = buffer.content[..offset]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
        let removed_newline = &buffer.content[previous..offset] == "\n";
        let mut content = buffer.content.clone();
        content.drain(previous..offset);
        self.workspace.lock()?.project_mut().edit_buffer(
            path,
            content.clone(),
            crate::development::Actor::local(),
        )?;
        let (line, column) = if removed_newline {
            let line = buffer.cursor_line.saturating_sub(1).max(1);
            let column = content
                .split('\n')
                .nth(line.saturating_sub(1) as usize)
                .map(|line| line.chars().count() + 1)
                .unwrap_or(1) as u32;
            (line, column)
        } else {
            (
                buffer.cursor_line,
                buffer.cursor_column.saturating_sub(1).max(1),
            )
        };
        self.workspace
            .lock()?
            .project_mut()
            .set_buffer_cursor(path, line, column)
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
    }

    /// Apply one background snapshot without touching UI-only fields.
    pub fn apply_snapshot(&mut self, snapshot: &super::snapshot::DisplaySnapshot) {
        self.refresh_latency_ms = snapshot.duration.as_millis() as u64;
        self.agents = snapshot.agents.clone();
        self.agent_conversation = snapshot.agent_conversation.clone();
        self.tasks = snapshot.tasks.clone();
        self.editor = snapshot.editor.clone();
        self.lsp = snapshot.lsp.clone();
        self.processes = snapshot.processes.clone();
        self.git = snapshot.git.clone();
        self.tests = snapshot.tests.clone();
        self.kernels = snapshot.kernels.clone();
        self.debugger = snapshot.debugger.clone();
        self.replay = snapshot.replay.clone();
        self.workflow = snapshot.workflow.clone();
        self.workspace_status = snapshot.workspace_status.clone();
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
    }

    pub fn refresh(&mut self) {
        let Ok(mut workspace) = self.workspace.lock() else {
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
        self.agents = match workspace.agents().list() {
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
                "No conversation yet. Press i to compose a message.".into()
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
            "No file open. Select a file below and press Enter, then i to edit.".into()
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
        self.git = workspace
            .git()
            .map(|git| match git.status() {
                Ok(status) => {
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
                Err(error) => format!("Git state failed: {error}"),
            })
            .unwrap_or_else(|| "Not a Git repository".into());
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
        let agent_hint = if self.agent_readiness.starts_with("✓") {
            "✓ Glass Agent ready"
        } else {
            "○ Glass Agent needs setup (More → agent setup)"
        };
        self.workspace_status = format!(
            "WELCOME · {}\n{}\n\n{}\n{}\n\nNEXT ACTIONS\n◆ [a] actions menu · per-surface flows\n◆ [i] talk to Glass Agent\n◆ [Enter] open the selected file\n{}\n{}\n\nSTATE\nroot {}\ngeneration {} · project revision {} · trust {}\nresident: {} agents · {} tasks · {} kernels · {} debuggers",
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
    }

    pub fn apply_browser_result(&mut self, tool: &str, result: &serde_json::Value) {
        if tool == "glass.browser.observe" {
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
            if let Some(revision) = revision {
                self.browser_workspace
                    .update_page(title, url, false, Some(revision));
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
                            actionable: true,
                            revision,
                        })
                    })
                    .collect();
                self.browser_workspace.replace_entities(revision, entities);
            }
        } else if tool == "glass.browser.stop" {
            self.browser_workspace.state_mut().connection = BrowserConnectionPhase::Detached;
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
        format!(
            "{} · rev {} · {} · owner {}\n{}\n{}\n\nUNDERSTANDING\n{}",
            browser.connection_label(),
            browser
                .browser_revision
                .map_or_else(|| "—".into(), |revision| revision.to_string()),
            browser.presentation_label(),
            browser.input_owner_label(),
            browser.title,
            browser.url,
            entities,
        )
    }

    pub fn highlight_app_selection(&mut self) {
        let Some(entity) = self.browser_workspace.state().selected().cloned() else {
            return;
        };
        let highlight = self.workspace.lock().and_then(|workspace| {
            workspace
                .browser()
                .highlight(entity.reference, entity.revision)
        });
        if let Err(error) = highlight {
            let stale = error.to_string().to_lowercase().contains("stale");
            self.browser_workspace.fail_action(error.to_string(), stale);
            self.status = format!("App highlight failed: {error}");
        }
    }

    pub fn execute_app_intent(&mut self, intent: BrowserWorkspaceIntent) {
        let action = match self.browser_workspace.reduce(intent) {
            Ok(Some(action)) => action,
            Ok(None) => return,
            Err(error) => {
                self.status = format!("App action unavailable: {error}");
                return;
            }
        };
        let result = match action {
            BrowserWorkspaceAction::Click {
                target,
                expected_revision,
            } => self
                .workspace
                .lock()
                .and_then(|workspace| workspace.browser().click(target, expected_revision)),
            BrowserWorkspaceAction::Scroll {
                dx,
                dy,
                expected_revision,
            } => self
                .workspace
                .lock()
                .and_then(|workspace| workspace.browser().scroll(dx, dy, expected_revision)),
            _ => return,
        };
        let observed = result.and_then(|_| {
            self.workspace
                .lock()
                .and_then(|workspace| workspace.browser().observe())
        });
        match observed {
            Ok(observation) => {
                self.apply_browser_result("glass.browser.observe", &observation);
                self.browser_detail =
                    super::projection::browser_result("glass.browser.observe", &observation);
                self.status = "App action complete · semantic revision refreshed".into();
            }
            Err(error) => {
                let stale = error.to_string().to_lowercase().contains("stale");
                self.browser_workspace.fail_action(error.to_string(), stale);
                self.status = format!("App action failed: {error}");
            }
        }
    }

    /// Offer in-TUI recovery whenever a browser tool fails with a launch or
    /// connection error, so a port collision never strands the session.
    pub fn note_browser_failure(&mut self, tool: &str, error: &str) {
        if tool.contains("browser")
            && (error.contains("occupied")
                || error.contains("attach")
                || error.contains("connect")
                || error.contains("launch")
                || error.contains("timeout"))
        {
            self.browser_recovery = Some(BrowserRecoveryOffer::from_error(
                error,
                self.browser_recovery
                    .as_ref()
                    .map_or(9222, |offer| offer.port),
            ));
            self.browser_workspace.disconnected(error.to_string(), true);
        }
    }

    /// Execute a recovery choice from the offer sheet.
    pub fn accept_browser_recovery(&mut self, choice: usize) {
        let Some(offer) = self.browser_recovery.take() else {
            return;
        };
        let result = match (offer.compatible_endpoint, choice) {
            (true, 0) => self.workspace.lock().and_then(|workspace| {
                workspace.browser().start(BrowserStartConfig {
                    port: offer.port,
                    attach: true,
                    ..Default::default()
                })
            }),
            (true, 1) | (false, 0) | (true, 2) | (false, 1) => {
                let port = free_local_port().unwrap_or(0);
                if port == 0 {
                    self.status = "No free local port was available".into();
                    return;
                }
                self.workspace.lock().and_then(|workspace| {
                    workspace.browser().start(BrowserStartConfig {
                        port,
                        ..Default::default()
                    })
                })
            }
            (true, 3) | (false, 2) => self.workspace.lock().and_then(|workspace| {
                workspace.browser().start(BrowserStartConfig {
                    port: offer.port,
                    ..Default::default()
                })
            }),
            _ => {
                self.status = "Recovery dismissed".into();
                return;
            }
        };
        match result {
            Ok(state) => {
                self.apply_browser_result("glass.browser.start", &state);
                self.browser_detail =
                    super::projection::browser_result("glass.browser.start", &state);
                self.status = "Browser recovered · workspace state preserved".into();
            }
            Err(error) => {
                let error = error.to_string();
                self.note_browser_failure("glass.browser.start", &error);
                self.status = format!("Recovery failed: {error}");
            }
        }
    }

    pub fn refresh_agent_readiness(&mut self) -> Result<bool, String> {
        let readiness = crate::pi_runtime::pi_readiness().map_err(|error| error.to_string())?;
        let ready = readiness.ready;
        self.agent_readiness = format_pi_readiness(&readiness);
        Ok(ready)
    }

    /// Capture one browser frame into the ANSI pane for the App surface.
    pub fn refresh_app_visual(&mut self, columns: u16, rows: u16) {
        let screenshot = self
            .workspace
            .lock()
            .and_then(|workspace| workspace.browser().screenshot());
        let png = match screenshot {
            Ok(value) => value
                .get("base64")
                .and_then(serde_json::Value::as_str)
                .and_then(|encoded| {
                    use base64::Engine as _;
                    base64::engine::general_purpose::STANDARD
                        .decode(encoded)
                        .ok()
                }),
            Err(_) => None,
        };
        let Some(png) = png else {
            self.browser_visual_live = false;
            self.browser_workspace.state_mut().presentation_reason =
                Some("screenshot unavailable; start or observe the browser first".into());
            return;
        };
        let columns = columns.clamp(8, 80);
        let rows = rows.clamp(4, 40);
        match AnsiPane::from_png(
            &mut self.browser_ansi,
            &png,
            columns,
            rows,
            glass_browser::terminal_graphics::FrameFit::Contain,
        ) {
            Ok(pane) => {
                self.browser_pane = Some(pane);
                self.browser_workspace.state_mut().presentation =
                    glass_browser::browser_workspace::BrowserPresentationPath::Ansi;
                self.browser_workspace.state_mut().presentation_reason = None;
            }
            Err(error) => {
                self.browser_visual_live = false;
                self.browser_workspace.state_mut().presentation_reason =
                    Some(format!("ANSI renderer failed: {error}"));
            }
        }
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
            "{state_line}\nprovider {provider} · {session}{remediation}\n\n[E] `agent setup` installs the managed runtime · `agent setup login` authenticates"
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
