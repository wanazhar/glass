use super::command;
use crate::{DevelopmentWorkspace, ExperimentComparison};
use glass_browser::browser_workspace::{
    BrowserConnectionPhase, BrowserWorkspaceAction, BrowserWorkspaceAdapterKind,
    BrowserWorkspaceController, BrowserWorkspaceEntity, BrowserWorkspaceIntent,
    BrowserWorkspaceLayout,
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
    pub command_history_index: Option<usize>,
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
            command_history_index: None,
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
        match self
            .workspace
            .project_mut()
            .open_buffer(&path, crate::development::Actor::local())
        {
            Ok(_) => {
                self.status = format!("Opened {path} · press i to edit");
                self.refresh();
            }
            Err(error) => self.status = format!("Open failed: {error}"),
        }
    }

    pub fn enter_code_edit(&mut self) {
        if self.workspace.project().buffers().next().is_none() {
            self.open_selected_file();
        }
        if self.workspace.project().buffers().next().is_some() {
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
        let Some(buffer) = self.workspace.project().buffers().next().cloned() else {
            self.status = "No open buffer".into();
            return;
        };
        let path = buffer.path.clone();
        let result = match (code, modifiers) {
            (crossterm::event::KeyCode::Char('s'), value)
                if value.contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.workspace.project_mut().save_buffer(&path).map(|_| ())
            }
            (crossterm::event::KeyCode::Char('z'), value)
                if value.contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.workspace.project_mut().undo_buffer(&path).map(|_| ())
            }
            (crossterm::event::KeyCode::Char('y'), value)
                if value.contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.workspace.project_mut().redo_buffer(&path).map(|_| ())
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
            .project()
            .buffer(path)
            .cloned()
            .ok_or_else(|| {
                crate::development::DevelopmentError::NotFound(format!("buffer {path}"))
            })?;
        let offset = editor_offset(&buffer.content, buffer.cursor_line, buffer.cursor_column);
        let mut content = buffer.content.clone();
        content.insert_str(offset, text);
        self.workspace.project_mut().edit_buffer(
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
            .project_mut()
            .set_buffer_cursor(path, line, column)
    }

    fn backspace_editor(&mut self, path: &str) -> crate::development::DevelopmentResult<()> {
        let buffer = self
            .workspace
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
        self.workspace.project_mut().edit_buffer(
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

    pub fn set_terminal_size(&mut self, width: u16, height: u16) {
        self.terminal_width = width;
        self.terminal_height = height;
    }

    pub fn refresh(&mut self) {
        self.files = self
            .workspace
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
            .map(|state| super::projection::workflow(Some(&state)))
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

    pub fn highlight_app_selection(&mut self) {
        let Some(entity) = self.browser_workspace.state().selected().cloned() else {
            return;
        };
        if let Err(error) = self
            .workspace
            .browser()
            .highlight(entity.reference, entity.revision)
        {
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
            } => self.workspace.browser().click(target, expected_revision),
            BrowserWorkspaceAction::Scroll {
                dx,
                dy,
                expected_revision,
            } => self.workspace.browser().scroll(dx, dy, expected_revision),
            _ => return,
        };
        match result.and_then(|_| self.workspace.browser().observe()) {
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

    pub fn refresh_agent_readiness(&mut self) -> Result<bool, String> {
        let readiness = crate::pi_runtime::pi_readiness().map_err(|error| error.to_string())?;
        let ready = readiness.ready;
        self.agent_readiness = format_pi_readiness(&readiness);
        Ok(ready)
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
