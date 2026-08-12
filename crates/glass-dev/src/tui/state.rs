use super::command;
use crate::{DevelopmentWorkspace, ExperimentComparison};
use glass_browser::browser_workspace::{
    BrowserConnectionPhase, BrowserWorkspaceAdapterKind, BrowserWorkspaceController,
    BrowserWorkspaceEntity, BrowserWorkspaceLayout,
};
use glass_browser::cli::args::TuiLayout;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub struct PendingConfirmation {
    pub call: crate::development::ToolCall,
    pub context: crate::tools::DevelopmentToolContext,
    pub summary: String,
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

pub struct DevTuiState {
    pub workspace: DevelopmentWorkspace,
    pub surface: DevSurface,
    pub layout: TuiLayout,
    pub quit: bool,
    pub command_mode: bool,
    pub command_input: String,
    pub command_cursor: usize,
    pub command_history: Vec<String>,
    pub palette_error: Option<String>,
    pub composer_mode: bool,
    pub composer_input: String,
    pub composer_cursor: usize,
    pub selected_agent: Option<crate::AgentId>,
    pub pending_confirmation: Option<PendingConfirmation>,
    pub surface_scroll: std::collections::BTreeMap<DevSurface, usize>,
    pub terminal_width: u16,
    pub terminal_height: u16,
    pub status: String,
    pub agents: String,
    pub agent_readiness: String,
    pub agent_conversation: String,
    pub tasks: String,
    pub editor: String,
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
    pub workflow: String,
    pub workspace_status: String,
    pub experiment_comparison: Option<ExperimentComparison>,
    pub experiments: String,
}

impl DevTuiState {
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
        let workspace = DevelopmentWorkspace::open(root)?;
        let trust_prompt = workspace.trust() == crate::WorkspaceTrust::Untrusted
            && workspace
                .trust_inspection()
                .iter()
                .any(|item| item.trust_required);
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
            palette_error: None,
            composer_mode: false,
            composer_input: String::new(),
            composer_cursor: 0,
            selected_agent: None,
            pending_confirmation: None,
            surface_scroll: std::collections::BTreeMap::new(),
            terminal_width: 80,
            terminal_height: 24,
            status: if trust_prompt {
                "Workspace trust required · I inspect · O untrusted · 1 trust once · T trust project".into()
            } else {
                "Ready · : opens the command palette · q quits".into()
            },
            agents: String::new(),
            agent_readiness: crate::pi_runtime::pi_readiness()
                .map(|readiness| format_pi_readiness(&readiness))
                .unwrap_or_else(|error| format!("Agent unavailable · {error}")),
            agent_conversation: "No conversation yet. Press i to compose a message.".into(),
            tasks: String::new(),
            editor: String::new(),
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

    pub fn open_palette(&mut self) {
        self.command_mode = true;
        self.command_input.clear();
        self.command_cursor = 0;
        self.palette_error = None;
        self.status = "Command palette · type help for routes".into();
    }

    pub fn close_palette(&mut self) {
        self.command_mode = false;
        self.command_input.clear();
        self.command_cursor = 0;
        self.palette_error = None;
        self.status = "Command cancelled".into();
    }

    pub fn submit_palette(&mut self) {
        let input = std::mem::take(&mut self.command_input);
        self.command_cursor = 0;
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
        self.status = "Composer closed".into();
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
        self.composer_cursor = 0;
        self.composer_mode = false;
        if text.trim().is_empty() {
            self.status = "Message was empty".into();
            return;
        }
        if !self.workspace.trust().permits_project_execution() {
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
                .agents()
                .list()
                .ok()?
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
            let status = self
                .workspace
                .agents()
                .snapshot(&agent)
                .map(|item| item.status);
            match status {
                Ok(crate::AgentStatus::Working) => self.workspace.agents().follow_up(&agent, text),
                Ok(_) => self.workspace.agents().prompt(&agent, text),
                Err(error) => Err(error),
            }
            .map(|()| agent)
        } else {
            self.workspace
                .agents()
                .create(crate::AgentSpec::new("assistant", text))
        };
        match result {
            Ok(agent) => {
                self.selected_agent = Some(agent);
                self.status = "Message queued".into();
            }
            Err(error) => self.status = format!("Message failed: {error}"),
        }
        self.refresh();
    }

    pub fn approve_confirmation(&mut self) {
        let Some(pending) = self.pending_confirmation.take() else {
            return;
        };
        match self.workspace.execute_tool(&pending.call, &pending.context) {
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
                match self.workspace.apply_local_trust_decision(decision) {
                    Ok(trust) => {
                        self.surface = DevSurface::Agent;
                        self.status = format!("Workspace opened with {trust:?} authority");
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

    pub fn current_scroll(&self) -> u16 {
        self.surface_scroll
            .get(&self.surface)
            .copied()
            .unwrap_or(0)
            .min(u16::MAX as usize) as u16
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

    pub fn set_terminal_size(&mut self, width: u16, height: u16) {
        self.terminal_width = width;
        self.terminal_height = height;
    }

    pub fn refresh(&mut self) {
        self.agents = match self.workspace.agents().list() {
            Ok(agents) if agents.is_empty() => "No agents. :agent spawn ROLE TASK".into(),
            Ok(agents) => agents
                .iter()
                .map(|agent| {
                    format!(
                        "{}  {:?}  {} · {}\n  target {} · model {} · thinking {} · events {} · dropped {}{}\n  evidence {}",
                        agent.id.as_str(),
                        agent.status,
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
        self.agent_conversation = match self.workspace.agents().history(0) {
            Ok(events) if events.is_empty() => {
                "No conversation yet. Press i to compose a message.".into()
            }
            Ok(events) => events
                .iter()
                .rev()
                .take(32)
                .rev()
                .map(format_agent_event)
                .collect::<Vec<_>>()
                .join("\n\n"),
            Err(error) => format!("Conversation unavailable · {error}"),
        };
        self.tasks = match self.workspace.tasks() {
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
                        "{} {}  {:?}  {}\n  goal {}\n  agent {} · attempt {} · model {} · thinking {}\n  depends {}\n  verification {}{}\n  evidence {}",
                        glyph,
                        task.id.as_str(),
                        task.state,
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
                            .map(|evidence| format!("{}={:?}", evidence.kind, evidence.passed))
                            .collect::<Vec<_>>()
                            .join(" · ")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
            Err(error) => format!("Task scheduler failed: {error}"),
        };
        self.processes = self
            .workspace
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
                                "{} {:?} · health {:?} · pid {} · {}\n  {}",
                                if matches!(item.health, crate::development::ProcessHealth::Healthy)
                                {
                                    "●"
                                } else {
                                    "○"
                                },
                                item.name,
                                item.health,
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
        let buffers = self
            .workspace
            .project()
            .buffers()
            .cloned()
            .collect::<Vec<_>>();
        self.editor = if buffers.is_empty() {
            "No file open. Use :editor open PATH or select a file.".into()
        } else {
            buffers
                .iter()
                .map(|buffer| {
                    let viewport = buffer
                        .content
                        .lines()
                        .take(24)
                        .enumerate()
                        .map(|(index, line)| format!("{:>4} {}", index + 1, line))
                        .collect::<Vec<_>>()
                        .join("\n");
                    format!(
                        "{}{} · cursor {}:{} · actor {}\n{}",
                        if buffer.dirty { "● " } else { "○ " },
                        buffer.path,
                        buffer.cursor_line,
                        buffer.cursor_column,
                        buffer.actor.id,
                        viewport
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        self.lsp = {
            let language = self.workspace.language();
            let servers = language.names().collect::<Vec<_>>();
            let event_count = language.events(0).len();
            if servers.is_empty() {
                "No language server active · diagnostics unavailable".into()
            } else {
                format!("● {} · {} recent events", servers.join(" · "), event_count)
            }
        };
        self.git = self
            .workspace
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
        let _ = self.workspace.tests_mut().poll();
        let test_runs = self
            .workspace
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
                        "{} {} · {:?} · {} ms · {} cases",
                        if run.exit_code == Some(0) {
                            "✓"
                        } else {
                            "×"
                        },
                        run.suite_id,
                        run.state,
                        run.duration_ms.unwrap_or_default(),
                        run.cases.len()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let kernels = self.workspace.kernels().snapshots().collect::<Vec<_>>();
        self.kernels = if kernels.is_empty() {
            "No persistent kernels".into()
        } else {
            kernels
                .iter()
                .map(|kernel| {
                    format!(
                        "{} {} · {:?} · {} executions · rev {}",
                        if matches!(kernel.state, crate::kernels::KernelState::Ready) {
                            "●"
                        } else {
                            "○"
                        },
                        kernel.name,
                        kernel.kind,
                        kernel.executions,
                        kernel.workspace_revision
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let debugger_names = self
            .workspace
            .debugger_names()
            .map(str::to_string)
            .collect::<Vec<_>>();
        self.debugger = if debugger_names.is_empty() {
            "No debugger sessions. :debug start NAME COMMAND [ARGS...]".into()
        } else {
            let snapshots = debugger_names
                .iter()
                .map(|name| {
                    self.workspace
                        .debugger_mut(name)
                        .and_then(|debugger| debugger.snapshot())
                        .map(|snapshot| (name, snapshot))
                })
                .collect::<Result<Vec<_>, _>>();
            match snapshots {
                Ok(snapshots) => snapshots.iter().map(|(name, snapshot)| format!("● {} · {:?} · pid {} · {} breakpoints · {} watches · {} threads/processes", name, snapshot.state, snapshot.adapter_process_id, snapshot.breakpoints.values().map(Vec::len).sum::<usize>(), snapshot.watches.len(), snapshot.debuggee_processes.len())).collect::<Vec<_>>().join("\n"),
                Err(error) => format!("Debugger state failed: {error}"),
            }
        };
        self.replay = self
            .workspace
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
        self.browser = match self.workspace.browser().state() {
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
        self.workflow = self
            .workspace
            .browser()
            .list_workflows()
            .and_then(|state| serde_json::to_string_pretty(&state).map_err(Into::into))
            .unwrap_or_else(|error| format!("Workflow state failed: {error}"));
        let root = self.workspace.root().display().to_string();
        let generation = self.workspace.generation();
        let project_revision = self.workspace.project().revision();
        let trust = self.workspace.trust();
        let detection = self.workspace.project().detection().clone();
        let agent_count = self
            .workspace
            .agents()
            .list()
            .map(|items| items.len())
            .unwrap_or(0);
        let task_count = self.workspace.tasks().map(|items| items.len()).unwrap_or(0);
        let kernel_count = self.workspace.kernels().snapshots().count();
        let debugger_count = self.workspace.debugger_names().count();
        self.workspace_status = format!(
            "root {}\n{} project · {} · branch {}\ngeneration {} · project revision {} · trust {:?}\nresident: {} agents · {} tasks · {} kernels · {} debuggers\nnext actions: Agent · Code · {} · {}",
            root,
            detection.languages.join("/"),
            detection.build_system.as_deref().unwrap_or("unknown build"),
            detection.git_branch.as_deref().unwrap_or("no Git"),
            generation,
            project_revision,
            trust,
            agent_count,
            task_count,
            kernel_count,
            debugger_count,
            detection
                .dev_command
                .as_deref()
                .map(|command| format!("run `{command}`"))
                .unwrap_or_else(|| "configure a dev command".into()),
            detection
                .browser_url
                .as_deref()
                .map(|url| format!("open App {url}"))
                .unwrap_or_else(|| "start App after a URL is detected".into()),
        );
        if self.workspace.trust().permits_project_execution()
            && let Ok(experiments) = self.workspace.experiments()
        {
            let snapshots = experiments.snapshots();
            self.experiments = if snapshots.is_empty() {
                "No experiments".into()
            } else {
                snapshots
                    .iter()
                    .map(|experiment| {
                        format!(
                            "{} {} · {:?} · port {} · agent {}",
                            if experiment.evidence.tests_failed == 0
                                && experiment.evidence.tests_passed > 0
                            {
                                "✓"
                            } else {
                                "○"
                            },
                            experiment.id,
                            experiment.state,
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
            "{} · rev {} · {} · owner {:?}\n{}\n{}\n\nUNDERSTANDING\n{}",
            browser.connection_label(),
            browser
                .browser_revision
                .map_or_else(|| "—".into(), |revision| revision.to_string()),
            browser.presentation_label(),
            browser.input_owner,
            browser.title,
            browser.url,
            entities,
        )
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
    format!(
        "{} · Node {:?} · SDK {} · auth {:?} · provider {} · {}{}",
        if readiness.ready {
            "Ready"
        } else {
            "Needs setup"
        },
        readiness.node.state,
        readiness.sdk.version.as_deref().unwrap_or("missing"),
        readiness.authentication.state,
        provider,
        session,
        remediation,
    )
}

fn format_agent_event(event: &crate::AgentEvent) -> String {
    let actor = event.agent_id.as_str();
    let text = event
        .payload
        .pointer("/message/content")
        .or_else(|| event.payload.pointer("/result/text"))
        .or_else(|| event.payload.get("text"))
        .and_then(serde_json::Value::as_str);
    match text {
        Some(text) => format!("{actor} · {}\n{}", event.kind, text),
        None if event.payload.is_null() => format!("{actor} · {}", event.kind),
        None => format!("{actor} · {} · details available in Inspect", event.kind),
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
