use super::command;
use crate::{DevelopmentWorkspace, ExperimentComparison};
use glass_browser::cli::args::TuiLayout;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DevSurface {
    Dashboard,
    Editor,
    Agent,
    Agents,
    Processes,
    Debugger,
    Git,
    Tests,
    Experiments,
    Graph,
    Replay,
    Browser,
}

impl DevSurface {
    pub const ALL: [Self; 12] = [
        Self::Dashboard,
        Self::Editor,
        Self::Agent,
        Self::Agents,
        Self::Processes,
        Self::Debugger,
        Self::Git,
        Self::Tests,
        Self::Experiments,
        Self::Graph,
        Self::Replay,
        Self::Browser,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Editor => "Editor",
            Self::Agent => "Glass Agent",
            Self::Agents => "Agents",
            Self::Processes => "Processes",
            Self::Debugger => "Debugger",
            Self::Git => "Git",
            Self::Tests => "Tests",
            Self::Experiments => "Experiments",
            Self::Graph => "Graph",
            Self::Replay => "Replay",
            Self::Browser => "Browser",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponsiveClass {
    Desktop,
    Compact,
    Phone,
}

pub struct DevTuiState {
    pub workspace: DevelopmentWorkspace,
    pub surface: DevSurface,
    pub layout: TuiLayout,
    pub quit: bool,
    pub command_mode: bool,
    pub command_input: String,
    pub status: String,
    pub agents: String,
    pub processes: String,
    pub git: String,
    pub tests: String,
    pub kernels: String,
    pub debugger: String,
    pub replay: String,
    pub experiment_comparison: Option<ExperimentComparison>,
}

impl DevTuiState {
    pub fn open(
        root: impl AsRef<Path>,
        layout: TuiLayout,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let workspace = DevelopmentWorkspace::open(root)?;
        let mut state = Self {
            workspace,
            surface: DevSurface::Dashboard,
            layout,
            quit: false,
            command_mode: false,
            command_input: String::new(),
            status: "Ready · : opens the command palette · q quits".into(),
            agents: String::new(),
            processes: String::new(),
            git: String::new(),
            tests: String::new(),
            kernels: String::new(),
            debugger: String::new(),
            replay: String::new(),
            experiment_comparison: None,
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
        self.status = "Command palette · type help for routes".into();
    }

    pub fn close_palette(&mut self) {
        self.command_mode = false;
        self.command_input.clear();
        self.status = "Command cancelled".into();
    }

    pub fn submit_palette(&mut self) {
        let input = std::mem::take(&mut self.command_input);
        self.command_mode = false;
        match command::execute(self, &input) {
            Ok(message) => self.status = message,
            Err(error) => self.status = format!("Error: {error}"),
        }
        self.refresh();
    }

    pub fn handle_printable(&mut self, character: char) {
        let surface = match character {
            'h' => Some(DevSurface::Dashboard),
            'e' => Some(DevSurface::Editor),
            'a' => Some(DevSurface::Agent),
            'm' => Some(DevSurface::Agents),
            'p' => Some(DevSurface::Processes),
            'd' => Some(DevSurface::Debugger),
            'g' => Some(DevSurface::Git),
            't' => Some(DevSurface::Tests),
            'x' => Some(DevSurface::Experiments),
            'n' => Some(DevSurface::Graph),
            'r' => Some(DevSurface::Replay),
            'b' => Some(DevSurface::Browser),
            _ => None,
        };
        if let Some(surface) = surface {
            self.surface = surface;
            self.status = format!("{} · : for actions", surface.label());
        }
    }

    pub fn next_surface(&mut self) {
        let index = DevSurface::ALL
            .iter()
            .position(|surface| *surface == self.surface)
            .unwrap_or(0);
        self.surface = DevSurface::ALL[(index + 1) % DevSurface::ALL.len()];
    }

    pub fn previous_surface(&mut self) {
        let index = DevSurface::ALL
            .iter()
            .position(|surface| *surface == self.surface)
            .unwrap_or(0);
        self.surface = DevSurface::ALL[(index + DevSurface::ALL.len() - 1) % DevSurface::ALL.len()];
    }

    pub fn refresh(&mut self) {
        self.agents = match self.workspace.agents().list() {
            Ok(agents) if agents.is_empty() => "No agents. :agent spawn ROLE TASK".into(),
            Ok(agents) => agents
                .iter()
                .map(|agent| {
                    format!(
                        "{}  {:?}  {} · {}\n  target {} · model {} · events {}{}",
                        agent.id.as_str(),
                        agent.status,
                        agent.role,
                        agent.task,
                        agent.worktree.display(),
                        agent.model.as_deref().unwrap_or("default"),
                        agent.event_count,
                        agent
                            .last_error
                            .as_deref()
                            .map(|error| format!(" · {error}"))
                            .unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
            Err(error) => format!("Agent registry failed: {error}"),
        };
        self.processes = self
            .workspace
            .project_mut()
            .processes()
            .list_checked()
            .and_then(|items| serde_json::to_string_pretty(&items).map_err(Into::into))
            .unwrap_or_else(|error| format!("Process state failed: {error}"));
        self.git = self
            .workspace
            .git()
            .map(|git| match git.status() {
                Ok(status) => serde_json::to_string_pretty(&status)
                    .unwrap_or_else(|error| format!("Git serialization failed: {error}")),
                Err(error) => format!("Git state failed: {error}"),
            })
            .unwrap_or_else(|| "Not a Git repository".into());
        let _ = self.workspace.tests_mut().poll();
        self.tests = serde_json::to_string_pretty(
            &self
                .workspace
                .tests()
                .results()
                .rev()
                .take(32)
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|error| error.to_string());
        self.kernels =
            serde_json::to_string_pretty(&self.workspace.kernels().snapshots().collect::<Vec<_>>())
                .unwrap_or_else(|error| error.to_string());
        self.debugger = if self.workspace.debugger_names().next().is_none() {
            "No debugger sessions. :debug start NAME COMMAND [ARGS...]".into()
        } else {
            self.workspace
                .debugger_names()
                .map(str::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        };
        self.replay = self
            .workspace
            .intelligence()
            .replay(0, 128)
            .and_then(|events| serde_json::to_string_pretty(&events).map_err(Into::into))
            .unwrap_or_else(|error| format!("Replay failed: {error}"));
    }
}
