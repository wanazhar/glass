//! Interactive TUI application state and rendering.
//!
//! Implements the Ratatui-based terminal interface with split-pane layout,
//! command input, observation display, and keyboard-driven interaction.

use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tokio::{
    sync::{mpsc, watch},
    task::{JoinHandle, LocalSet},
    time::{self, MissedTickBehavior},
};

use crate::browser::policy::BrowserPolicy;
use crate::browser::profile::ProfileManager;
use crate::browser::session::{
    ActionOutcome, BrowserResult, BrowserSession, KnowledgeStore, PageContext, PageInfo,
    SemanticIntentExecutionRequest, SemanticIntentExecutionResult, SemanticIntentRequest,
    SemanticIntentResult, SemanticObservation, SemanticObservationLevel, SessionOptions,
    WorkflowDefinition, default_knowledge_store_path,
};
use crate::capabilities::GlassCapabilityManifest;
use crate::cli::args::Cli;

const INPUT_CHANNEL_CAPACITY: usize = 64;
const BROWSER_COMMAND_CHANNEL_CAPACITY: usize = 8;
const BROWSER_EVENT_CHANNEL_CAPACITY: usize = 8;
const ACTIVITY_LIMIT: usize = 100;
const TUI_PAGE_MAX_BYTES: usize = 24 * 1024;
const TUI_HEADER_MAX_BYTES: usize = 512;
const TUI_ACTIVITY_MAX_BYTES: usize = 512;
const TUI_INPUT_MAX_BYTES: usize = 4 * 1024;
const BUSY_TICK: Duration = Duration::from_millis(120);
const INPUT_POLL: Duration = Duration::from_millis(50);
const WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

pub struct App {
    url: String,
    title: String,
    activity: VecDeque<String>,
    page_content: String,
    page_scroll: u16,
    input: String,
    cursor_pos: usize,
    should_quit: bool,
    error_msg: Option<String>,
    status: String,
    capability_summary: String,
    intent_request: Option<SemanticIntentRequest>,
    intent_result: Option<SemanticIntentResult>,
    intent_selection: usize,
    knowledge_path: PathBuf,
    browser_state: BrowserState,
    busy: Option<BusyState>,
    next_operation_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowserState {
    Connecting,
    Ready,
    Unavailable,
    Stopped,
}

#[derive(Debug, Clone)]
struct BusyState {
    id: u64,
    label: String,
    cancelling: bool,
    spinner: usize,
}

#[derive(Debug, PartialEq, Eq)]
enum UiIntent {
    None,
    Submit(String),
    Cancel(u64),
    Quit,
}

impl App {
    fn new() -> Self {
        let mut activity = VecDeque::new();
        activity.push_back("Glass started.".to_string());
        activity.push_back("Connecting to Chrome…".to_string());
        Self {
            url: String::new(),
            title: "Glass — Browser Agent".to_string(),
            activity,
            page_content: "No page loaded.".to_string(),
            page_scroll: 0,
            input: String::new(),
            cursor_pos: 0,
            should_quit: false,
            error_msg: None,
            status: "Connecting to Chrome…".to_string(),
            capability_summary: "Capabilities: loading".to_string(),
            intent_request: None,
            intent_result: None,
            intent_selection: 0,
            knowledge_path: default_knowledge_store_path("default"),
            browser_state: BrowserState::Connecting,
            busy: None,
            next_operation_id: 1,
        }
    }

    fn add_activity(&mut self, message: impl Into<String>) {
        let message = bounded_text(&message.into(), TUI_ACTIVITY_MAX_BYTES);
        if self.activity.len() == ACTIVITY_LIMIT {
            self.activity.pop_front();
        }
        self.activity.push_back(message);
    }

    fn set_error(&mut self, message: impl Into<String>) {
        self.error_msg = Some(bounded_text(&message.into(), TUI_ACTIVITY_MAX_BYTES));
    }

    fn report_error(&mut self, message: impl Into<String>) {
        let message = bounded_text(&message.into(), TUI_ACTIVITY_MAX_BYTES);
        self.set_error(message.clone());
        self.add_activity(format!("Error: {message}"));
    }

    fn clear_error(&mut self) {
        self.error_msg = None;
    }

    fn set_status(&mut self, status: impl Into<String>) {
        self.status = bounded_text(&status.into(), TUI_ACTIVITY_MAX_BYTES);
    }

    fn cursor_byte_index(&self) -> usize {
        self.input
            .char_indices()
            .nth(self.cursor_pos)
            .map(|(index, _)| index)
            .unwrap_or(self.input.len())
    }

    fn insert_char(&mut self, character: char) -> bool {
        if self.input.len().saturating_add(character.len_utf8()) > TUI_INPUT_MAX_BYTES {
            return false;
        }
        let index = self.cursor_byte_index();
        self.input.insert(index, character);
        self.cursor_pos += 1;
        true
    }

    fn remove_before_cursor(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        let end = self.cursor_byte_index();
        let start = self
            .input
            .char_indices()
            .nth(self.cursor_pos - 1)
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.input.drain(start..end);
        self.cursor_pos -= 1;
    }

    fn remove_at_cursor(&mut self) {
        let start = self.cursor_byte_index();
        let end = self
            .input
            .char_indices()
            .nth(self.cursor_pos + 1)
            .map(|(index, _)| index)
            .unwrap_or(self.input.len());
        if start < end {
            self.input.drain(start..end);
        }
    }

    fn reduce_key(&mut self, key: KeyEvent) -> UiIntent {
        if key.kind != KeyEventKind::Press {
            return UiIntent::None;
        }

        match key.code {
            KeyCode::Char('q' | 'c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                UiIntent::Quit
            }
            KeyCode::Char('q') if self.input.is_empty() => UiIntent::Quit,
            KeyCode::Esc => {
                let cancellation = self.busy.as_mut().and_then(|busy| {
                    if busy.cancelling {
                        None
                    } else {
                        busy.cancelling = true;
                        Some((busy.id, busy.label.clone()))
                    }
                });
                if let Some((id, label)) = cancellation {
                    self.set_status(format!("Cancelling: {label}"));
                    self.add_activity(format!("Cancellation requested: {label}"));
                    UiIntent::Cancel(id)
                } else if self.busy.is_some() {
                    UiIntent::None
                } else if self.error_msg.is_some() {
                    self.clear_error();
                    UiIntent::None
                } else {
                    UiIntent::Quit
                }
            }
            KeyCode::Enter if !self.input.trim().is_empty() => {
                let command = std::mem::take(&mut self.input);
                self.cursor_pos = 0;
                UiIntent::Submit(command)
            }
            KeyCode::Backspace => {
                self.remove_before_cursor();
                UiIntent::None
            }
            KeyCode::Delete => {
                self.remove_at_cursor();
                UiIntent::None
            }
            KeyCode::Left => {
                self.cursor_pos = self.cursor_pos.saturating_sub(1);
                UiIntent::None
            }
            KeyCode::Right => {
                self.cursor_pos = (self.cursor_pos + 1).min(self.input.chars().count());
                UiIntent::None
            }
            KeyCode::Home => {
                self.cursor_pos = 0;
                UiIntent::None
            }
            KeyCode::End => {
                self.cursor_pos = self.input.chars().count();
                UiIntent::None
            }
            KeyCode::PageUp => {
                self.page_scroll = self.page_scroll.saturating_sub(10);
                UiIntent::None
            }
            KeyCode::PageDown => {
                self.page_scroll = self.page_scroll.saturating_add(10);
                UiIntent::None
            }
            KeyCode::Up if self.input.is_empty() => {
                self.move_intent_selection(-1);
                UiIntent::None
            }
            KeyCode::Down if self.input.is_empty() => {
                self.move_intent_selection(1);
                UiIntent::None
            }
            KeyCode::Char(character) => {
                if !self.insert_char(character) {
                    self.report_error(format!(
                        "Command input is limited to {TUI_INPUT_MAX_BYTES} bytes."
                    ));
                }
                UiIntent::None
            }
            _ => UiIntent::None,
        }
    }

    fn browser_ready(&self) -> bool {
        self.browser_state == BrowserState::Ready
    }

    fn is_busy(&self) -> bool {
        self.busy.is_some()
    }

    fn allocate_operation_id(&mut self) -> u64 {
        let id = self.next_operation_id;
        self.next_operation_id = self.next_operation_id.checked_add(1).unwrap_or(1);
        id
    }

    fn begin_operation(&mut self, id: u64, label: impl Into<String>) {
        let label = bounded_text(&label.into(), TUI_ACTIVITY_MAX_BYTES);
        self.busy = Some(BusyState {
            id,
            label: label.clone(),
            cancelling: false,
            spinner: 0,
        });
        self.set_status(format!("Queued: {label}"));
        self.add_activity(format!("Queued: {label}"));
    }

    fn finish_operation(&mut self, id: u64) {
        if self.busy.as_ref().is_some_and(|busy| busy.id == id) {
            self.busy = None;
            if self.browser_ready() {
                self.set_status("Ready");
            }
        }
    }

    fn cancellation_enqueue_failed(&mut self, id: u64) {
        let label = self.busy.as_mut().filter(|busy| busy.id == id).map(|busy| {
            busy.cancelling = false;
            busy.label.clone()
        });
        if let Some(label) = label {
            self.set_status(format!("Working: {label}"));
        }
    }

    fn tick_busy(&mut self) {
        let status = self.busy.as_mut().map(|busy| {
            busy.spinner = busy.spinner.wrapping_add(1);
            if busy.cancelling {
                format!("Cancelling: {}", busy.label)
            } else {
                let frame = ['|', '/', '-', '\\'][busy.spinner % 4];
                format!("{frame} Working: {}", busy.label)
            }
        });
        if let Some(status) = status {
            self.set_status(status);
        }
    }

    fn apply_browser_event(
        &mut self,
        event: BrowserEvent,
    ) -> BrowserResult<Option<BrowserOperation>> {
        match event {
            BrowserEvent::Connecting => {
                self.browser_state = BrowserState::Connecting;
                self.set_status("Connecting to Chrome…");
                self.add_activity("Browser worker is connecting.");
            }
            BrowserEvent::Ready { port } => {
                self.browser_state = BrowserState::Ready;
                self.set_status(format!("Connected on port {port}"));
                self.add_activity("Connected to Chrome.");
                return Ok(Some(BrowserOperation::Observe { fresh: false }));
            }
            BrowserEvent::StartupFailed { message } => {
                self.browser_state = BrowserState::Unavailable;
                self.busy = None;
                self.set_status("Browser unavailable");
                self.report_error(message);
            }
            BrowserEvent::OperationStarted { id, label } => {
                if self.busy.as_ref().is_some_and(|busy| busy.id == id) {
                    self.set_status(format!("Working: {label}"));
                    self.add_activity(format!("Started: {label}"));
                }
            }
            BrowserEvent::OperationFinished { id, result } => {
                self.finish_operation(id);
                if let Some(update) = result.update {
                    self.apply_page_update(update)?;
                }
                self.add_activity(result.activity);
            }
            BrowserEvent::OperationFailed { id, message } => {
                if self.busy.as_ref().is_some_and(|busy| busy.id == id) {
                    self.finish_operation(id);
                    self.report_error(message);
                } else {
                    self.add_activity(format!("Rejected operation {id}: {message}"));
                }
            }
            BrowserEvent::OperationCancelled { id } => {
                if self.busy.as_ref().is_some_and(|busy| busy.id == id) {
                    self.finish_operation(id);
                    self.add_activity(format!("Cancelled operation {id}."));
                }
            }
            BrowserEvent::WorkerFailed { message } => {
                self.browser_state = BrowserState::Unavailable;
                self.busy = None;
                self.set_status("Browser worker failed");
                self.report_error(message);
            }
            BrowserEvent::WorkerStopped => {
                self.browser_state = BrowserState::Stopped;
                self.busy = None;
                self.set_status("Browser worker stopped");
                self.add_activity("Browser worker stopped.");
            }
        }
        Ok(None)
    }

    fn apply_page_update(&mut self, update: PageUpdate) -> BrowserResult<()> {
        match update {
            PageUpdate::Context(context) => self.apply_context(&context),
            PageUpdate::Semantic(observation) => self.apply_semantic(&observation),
            PageUpdate::IntentResolution { request, result } => {
                self.apply_intent_resolution(*request, *result)
            }
            PageUpdate::Text { page, text } => {
                self.apply_page_header(&page);
                self.set_page_content(text);
                Ok(())
            }
        }
    }

    fn apply_context(&mut self, context: &PageContext) -> BrowserResult<()> {
        if context.screenshot.is_some() {
            return Err("TUI worker must not retain screenshot data".into());
        }
        self.apply_page_header(&context.page);
        self.set_page_content(serde_json::to_string_pretty(context)?);
        Ok(())
    }

    fn apply_semantic(&mut self, observation: &SemanticObservation) -> BrowserResult<()> {
        self.url = bounded_text(&observation.page.url, TUI_HEADER_MAX_BYTES);
        self.title = bounded_text(
            &format!("Glass — {}", observation.page.title),
            TUI_HEADER_MAX_BYTES,
        );
        self.set_page_content(serde_json::to_string_pretty(observation)?);
        Ok(())
    }

    fn apply_intent_resolution(
        &mut self,
        request: SemanticIntentRequest,
        result: SemanticIntentResult,
    ) -> BrowserResult<()> {
        self.intent_request = Some(request);
        self.intent_selection = 0;
        self.intent_result = Some(result.clone());
        self.url = result
            .route
            .as_ref()
            .map(|route| bounded_text(&route.url, TUI_HEADER_MAX_BYTES))
            .unwrap_or_default();
        self.title = "Glass — Intent resolution".into();
        self.set_page_content(format_intent_debug(&result, self.intent_selection));
        Ok(())
    }

    fn move_intent_selection(&mut self, delta: isize) {
        let Some(result) = self.intent_result.as_ref() else {
            return;
        };
        if result.candidates.is_empty() {
            return;
        }
        let maximum = result.candidates.len() - 1;
        self.intent_selection = if delta.is_negative() {
            self.intent_selection.saturating_sub(delta.unsigned_abs())
        } else {
            self.intent_selection
                .saturating_add(delta as usize)
                .min(maximum)
        };
        let content = format_intent_debug(result, self.intent_selection);
        let candidate_id = result.candidates[self.intent_selection].id.clone();
        self.set_page_content(content);
        self.set_status(format!(
            "Selected {} — submit: intent execute.",
            candidate_id
        ));
    }

    fn apply_page_header(&mut self, page: &PageInfo) {
        self.url = bounded_text(&page.url, TUI_HEADER_MAX_BYTES);
        self.title = bounded_text(&format!("Glass — {}", page.title), TUI_HEADER_MAX_BYTES);
    }

    fn set_page_content(&mut self, content: impl Into<String>) {
        self.page_content = bounded_text(&content.into(), TUI_PAGE_MAX_BYTES);
        self.page_scroll = 0;
    }
}

#[derive(Debug)]
enum InputEvent {
    Key(KeyEvent),
    Redraw,
    Error(String),
}

struct InputWorker {
    shutdown: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
}

impl InputWorker {
    fn spawn(events: mpsc::Sender<InputEvent>) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);
        let join = thread::spawn(move || {
            while !worker_shutdown.load(Ordering::Relaxed) {
                match event::poll(INPUT_POLL) {
                    Ok(false) => {}
                    Ok(true) => match event::read() {
                        Ok(Event::Key(key)) => {
                            if events.blocking_send(InputEvent::Key(key)).is_err() {
                                break;
                            }
                        }
                        Ok(_) => {
                            if events.blocking_send(InputEvent::Redraw).is_err() {
                                break;
                            }
                        }
                        Err(error) => {
                            let _ = events.blocking_send(InputEvent::Error(error.to_string()));
                            break;
                        }
                    },
                    Err(error) => {
                        let _ = events.blocking_send(InputEvent::Error(error.to_string()));
                        break;
                    }
                }
            }
        });
        Self {
            shutdown,
            join: Some(join),
        }
    }

    fn stop(&mut self) -> BrowserResult<()> {
        self.shutdown.store(true, Ordering::Relaxed);
        if self.join.take().is_some_and(|join| join.join().is_err()) {
            return Err("TUI input worker panicked".into());
        }
        Ok(())
    }
}

impl Drop for InputWorker {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LocalCommand {
    Help,
    Profiles,
    Knowledge(Option<String>),
    Daemon(DaemonView),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DaemonView {
    Status,
    Doctor,
    Logs,
    Recovery,
}

#[derive(Debug, Clone, PartialEq)]
enum BrowserOperation {
    Navigate(String),
    Screenshot(String),
    Text,
    Dom,
    Observe {
        fresh: bool,
    },
    Semantic {
        level: SemanticObservationLevel,
        region: Option<String>,
    },
    Click(String),
    DoubleClick(String),
    Hover(String),
    Clear(String),
    Check(String),
    Uncheck(String),
    Select {
        target: String,
        value: String,
    },
    Type(String),
    KeyPress(String),
    Shortcut(String),
    Scroll {
        dx: f64,
        dy: f64,
    },
    AcceptDialog,
    DismissDialog,
    DismissConsent,
    Evaluate(String),
    Workflow(String),
    ResolveIntent(String),
    ExecuteIntent(Box<SemanticIntentExecutionRequest>),
}

impl BrowserOperation {
    fn label(&self) -> &'static str {
        match self {
            Self::Navigate(_) => "Navigate",
            Self::Screenshot(_) => "Screenshot",
            Self::Text => "Text",
            Self::Dom => "Compact DOM",
            Self::Observe { .. } => "Observe",
            Self::Semantic { .. } => "Semantic observe",
            Self::Click(_) => "Click",
            Self::DoubleClick(_) => "Double-click",
            Self::Hover(_) => "Hover",
            Self::Clear(_) => "Clear",
            Self::Check(_) => "Check",
            Self::Uncheck(_) => "Uncheck",
            Self::Select { .. } => "Select",
            Self::Type(_) => "Type",
            Self::KeyPress(_) => "Key press",
            Self::Shortcut(_) => "Shortcut",
            Self::Scroll { .. } => "Scroll",
            Self::AcceptDialog => "Accept dialog",
            Self::DismissDialog => "Dismiss dialog",
            Self::DismissConsent => "Dismiss consent",
            Self::Evaluate(_) => "Evaluate",
            Self::Workflow(_) => "Workflow",
            Self::ResolveIntent(_) => "Resolve intent",
            Self::ExecuteIntent(_) => "Execute intent",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ParsedCommand {
    Local(LocalCommand),
    Browser(BrowserOperation),
}

fn parse_command(input: &str) -> Result<ParsedCommand, String> {
    let command = input.trim();
    if command.is_empty() {
        return Err("command cannot be empty".to_string());
    }
    if command.eq_ignore_ascii_case("help") {
        return Ok(ParsedCommand::Local(LocalCommand::Help));
    }
    if command.eq_ignore_ascii_case("profiles") {
        return Ok(ParsedCommand::Local(LocalCommand::Profiles));
    }
    if command.eq_ignore_ascii_case("knowledge") {
        return Ok(ParsedCommand::Local(LocalCommand::Knowledge(None)));
    }
    if let Some(record_id) = strip_ascii_prefix(command, "knowledge show ") {
        return required_command_argument(record_id, "knowledge record ID")
            .map(|record_id| ParsedCommand::Local(LocalCommand::Knowledge(Some(record_id))));
    }
    if command.eq_ignore_ascii_case("daemon") || command.eq_ignore_ascii_case("daemon doctor") {
        return Ok(ParsedCommand::Local(LocalCommand::Daemon(
            DaemonView::Doctor,
        )));
    }
    if command.eq_ignore_ascii_case("daemon status") {
        return Ok(ParsedCommand::Local(LocalCommand::Daemon(
            DaemonView::Status,
        )));
    }
    if command.eq_ignore_ascii_case("daemon logs") {
        return Ok(ParsedCommand::Local(LocalCommand::Daemon(DaemonView::Logs)));
    }
    if command.eq_ignore_ascii_case("daemon recovery") {
        return Ok(ParsedCommand::Local(LocalCommand::Daemon(
            DaemonView::Recovery,
        )));
    }
    for prefix in ["navigate ", "go to ", "go "] {
        if let Some(url) = strip_ascii_prefix(command, prefix) {
            return required_command_argument(url, "URL")
                .map(BrowserOperation::Navigate)
                .map(ParsedCommand::Browser);
        }
    }
    if let Some(target) = strip_ascii_prefix(command, "double click ") {
        return required_command_argument(target, "double-click target")
            .map(BrowserOperation::DoubleClick)
            .map(ParsedCommand::Browser);
    }
    if let Some(target) = strip_ascii_prefix(command, "click ") {
        return required_command_argument(target, "click target")
            .map(BrowserOperation::Click)
            .map(ParsedCommand::Browser);
    }
    if let Some(target) = strip_ascii_prefix(command, "hover ") {
        return required_command_argument(target, "hover target")
            .map(BrowserOperation::Hover)
            .map(ParsedCommand::Browser);
    }
    for (prefix, operation, name) in [
        (
            "clear ",
            BrowserOperation::Clear as fn(String) -> BrowserOperation,
            "clear target",
        ),
        (
            "check ",
            BrowserOperation::Check as fn(String) -> BrowserOperation,
            "check target",
        ),
        (
            "uncheck ",
            BrowserOperation::Uncheck as fn(String) -> BrowserOperation,
            "uncheck target",
        ),
    ] {
        if let Some(target) = strip_ascii_prefix(command, prefix) {
            return required_command_argument(target, name)
                .map(operation)
                .map(ParsedCommand::Browser);
        }
    }
    if let Some(values) = strip_ascii_prefix(command, "select ") {
        return parse_target_value(values, "select target and value").map(|(target, value)| {
            ParsedCommand::Browser(BrowserOperation::Select { target, value })
        });
    }
    if let Some(text) = strip_ascii_prefix(command, "type ") {
        return required_command_argument(text, "text")
            .map(BrowserOperation::Type)
            .map(ParsedCommand::Browser);
    }
    if let Some(key) = strip_ascii_prefix(command, "press ") {
        return required_command_argument(key, "key")
            .map(BrowserOperation::KeyPress)
            .map(ParsedCommand::Browser);
    }
    if let Some(shortcut) = strip_ascii_prefix(command, "shortcut ") {
        return required_command_argument(shortcut, "shortcut")
            .map(BrowserOperation::Shortcut)
            .map(ParsedCommand::Browser);
    }
    if let Some(path) = strip_ascii_prefix(command, "workflow ") {
        return required_command_argument(path, "workflow JSON path")
            .map(BrowserOperation::Workflow)
            .map(ParsedCommand::Browser);
    }
    if let Some(path) = strip_ascii_prefix(command, "resolve-intent ") {
        return required_command_argument(path, "intent JSON path")
            .map(BrowserOperation::ResolveIntent)
            .map(ParsedCommand::Browser);
    }
    if command.eq_ignore_ascii_case("screenshot") {
        return Ok(ParsedCommand::Browser(BrowserOperation::Screenshot(
            "screenshot.png".to_string(),
        )));
    }
    if let Some(output) = strip_ascii_prefix(command, "screenshot ") {
        let output = output.trim();
        return Ok(ParsedCommand::Browser(BrowserOperation::Screenshot(
            if output.is_empty() {
                "screenshot.png".to_string()
            } else {
                output.to_string()
            },
        )));
    }
    if ["text", "content", "get text", "page text"]
        .iter()
        .any(|candidate| command.eq_ignore_ascii_case(candidate))
    {
        return Ok(ParsedCommand::Browser(BrowserOperation::Text));
    }
    if ["dom", "snapshot", "get dom"]
        .iter()
        .any(|candidate| command.eq_ignore_ascii_case(candidate))
    {
        return Ok(ParsedCommand::Browser(BrowserOperation::Dom));
    }
    if ["observe", "context"]
        .iter()
        .any(|candidate| command.eq_ignore_ascii_case(candidate))
    {
        return Ok(ParsedCommand::Browser(BrowserOperation::Observe {
            fresh: false,
        }));
    }
    if command.eq_ignore_ascii_case("semantic") {
        return Ok(ParsedCommand::Browser(BrowserOperation::Semantic {
            level: SemanticObservationLevel::Summary,
            region: None,
        }));
    }
    if let Some(values) = strip_ascii_prefix(command, "semantic ") {
        return parse_semantic_observation(values).map(ParsedCommand::Browser);
    }
    if command.eq_ignore_ascii_case("scroll") {
        return Ok(ParsedCommand::Browser(BrowserOperation::Scroll {
            dx: 0.0,
            dy: 600.0,
        }));
    }
    if let Some(values) = strip_ascii_prefix(command, "scroll ") {
        return parse_scroll(values).map(ParsedCommand::Browser);
    }
    if command.eq_ignore_ascii_case("accept-dialog") {
        return Ok(ParsedCommand::Browser(BrowserOperation::AcceptDialog));
    }
    if command.eq_ignore_ascii_case("dismiss-dialog") {
        return Ok(ParsedCommand::Browser(BrowserOperation::DismissDialog));
    }
    if command.eq_ignore_ascii_case("dismiss-consent") {
        return Ok(ParsedCommand::Browser(BrowserOperation::DismissConsent));
    }
    Ok(ParsedCommand::Browser(BrowserOperation::Evaluate(
        command.to_string(),
    )))
}

fn strip_ascii_prefix<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))?;
    Some(&value[prefix.len()..])
}

fn required_command_argument(value: &str, name: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{name} cannot be empty"))
    } else {
        Ok(value.to_string())
    }
}

fn parse_scroll(values: &str) -> Result<BrowserOperation, String> {
    let mut values = values.split_whitespace();
    let dx = values
        .next()
        .map(|value| {
            value
                .parse::<f64>()
                .map_err(|_| "scroll dx must be a number")
        })
        .transpose()?
        .unwrap_or(0.0);
    let dy = values
        .next()
        .map(|value| {
            value
                .parse::<f64>()
                .map_err(|_| "scroll dy must be a number")
        })
        .transpose()?
        .unwrap_or(600.0);
    if values.next().is_some() {
        return Err("scroll accepts at most dx and dy".to_string());
    }
    Ok(BrowserOperation::Scroll { dx, dy })
}

fn parse_target_value(values: &str, name: &str) -> Result<(String, String), String> {
    let mut values = values.trim().splitn(2, char::is_whitespace);
    let target = values.next().unwrap_or_default().trim();
    let value = values.next().unwrap_or_default().trim();
    if target.is_empty() || value.is_empty() {
        return Err(format!("{name} requires two non-empty arguments"));
    }
    Ok((target.to_string(), value.to_string()))
}

fn parse_semantic_observation(values: &str) -> Result<BrowserOperation, String> {
    let mut values = values.split_whitespace();
    let level = match values
        .next()
        .unwrap_or("summary")
        .to_ascii_lowercase()
        .as_str()
    {
        "summary" => SemanticObservationLevel::Summary,
        "interactive" => SemanticObservationLevel::Interactive,
        "structured" => SemanticObservationLevel::Structured,
        "detailed" => SemanticObservationLevel::Detailed,
        "raw" => SemanticObservationLevel::Raw,
        _ => {
            return Err(
                "semantic level must be summary, interactive, structured, detailed, or raw".into(),
            );
        }
    };
    let region = values.next().map(str::to_string);
    if values.next().is_some() {
        return Err("semantic accepts a level and optional region ID".into());
    }
    Ok(BrowserOperation::Semantic { level, region })
}

fn format_intent_activity(result: &SemanticIntentResult) -> String {
    format!(
        "Intent {:?}: {} (policy={:?}, candidates={}, revision={}).",
        result.resolution,
        result.normalized_intent,
        result.policy_decision,
        result.candidates.len(),
        result
            .revision
            .map(|revision| revision.to_string())
            .unwrap_or_else(|| "unknown".into())
    )
}

fn format_intent_execution_activity(result: &SemanticIntentExecutionResult) -> String {
    match result.status {
        crate::browser::session::SemanticIntentExecutionStatus::Executed => format!(
            "Intent executed: candidate={} resolution={} execution={}.",
            result.candidate_id,
            result.resolution_id,
            result.execution_id.as_deref().unwrap_or("unknown")
        ),
        crate::browser::session::SemanticIntentExecutionStatus::NotExecuted => format!(
            "Intent not executed: candidate={} resolution={:?}; {}.",
            result.candidate_id,
            result.resolution.resolution,
            result
                .reason
                .as_deref()
                .unwrap_or("policy did not authorize dispatch")
        ),
    }
}

fn format_intent_debug(result: &SemanticIntentResult, selected: usize) -> String {
    let mut output = vec![
        format!("Normalized intent: {}", result.normalized_intent),
        format!("Resolution: {:?}", result.resolution),
        format!("Policy: {:?}", result.policy_decision),
        format!(
            "Revision: {}",
            result
                .revision
                .map(|revision| revision.to_string())
                .unwrap_or_else(|| "unknown".into())
        ),
        String::new(),
        "Candidates:".into(),
    ];
    if result.candidates.is_empty() {
        output.push("  (none)".into());
    } else {
        for (index, candidate) in result.candidates.iter().enumerate() {
            let evidence = candidate
                .evidence
                .iter()
                .map(|item| format!("{:?}: {}", item.category, item.detail))
                .collect::<Vec<_>>()
                .join("; ");
            output.push(format!(
                "  {}{} [{}] {} — {:?}",
                if index == selected { "> " } else { "  " },
                candidate.id,
                candidate.role,
                candidate.name,
                candidate.confidence
            ));
            if !evidence.is_empty() {
                output.push(format!("    evidence: {evidence}"));
            }
        }
    }
    if !result.excluded_candidates.is_empty() {
        output.push(format!(
            "Excluded candidates: {}",
            result.excluded_candidates.len()
        ));
        for candidate in &result.excluded_candidates {
            output.push(format!(
                "  {} — {:?}: {}",
                candidate.id, candidate.reason.category, candidate.reason.detail
            ));
        }
    }
    if let Some(reason) = &result.reason {
        output.push(format!("Reason: {reason}"));
    }
    output.join("\n")
}

#[derive(Debug)]
enum BrowserCommand {
    Execute {
        id: u64,
        operation: BrowserOperation,
    },
    Cancel {
        id: u64,
    },
    Shutdown,
}

#[derive(Debug)]
enum BrowserEvent {
    Connecting,
    Ready {
        port: u16,
    },
    StartupFailed {
        message: String,
    },
    OperationStarted {
        id: u64,
        label: String,
    },
    OperationFinished {
        id: u64,
        result: Box<OperationResult>,
    },
    OperationFailed {
        id: u64,
        message: String,
    },
    OperationCancelled {
        id: u64,
    },
    WorkerFailed {
        message: String,
    },
    WorkerStopped,
}

#[derive(Debug)]
enum PageUpdate {
    Context(Box<PageContext>),
    Semantic(Box<SemanticObservation>),
    IntentResolution {
        request: Box<SemanticIntentRequest>,
        result: Box<SemanticIntentResult>,
    },
    Text {
        page: PageInfo,
        text: String,
    },
}

#[derive(Debug)]
struct OperationResult {
    activity: String,
    update: Option<PageUpdate>,
}

enum ActiveOperationState {
    Completed(BrowserResult<Box<OperationResult>>),
    Cancelled,
    Shutdown,
}

async fn browser_worker(
    options: SessionOptions,
    policy: BrowserPolicy,
    mut commands: mpsc::Receiver<BrowserCommand>,
    events: mpsc::Sender<BrowserEvent>,
    mut shutdown: watch::Receiver<bool>,
) {
    if !send_browser_event(&events, BrowserEvent::Connecting).await {
        return;
    }

    let Some(session) =
        start_browser_session(&options, policy, &mut commands, &events, &mut shutdown).await
    else {
        return;
    };
    if !send_browser_event(&events, BrowserEvent::Ready { port: options.port }).await {
        let _ = session.close().await;
        return;
    }

    worker_loop(session, &mut commands, &events, &mut shutdown).await;
}

async fn start_browser_session(
    options: &SessionOptions,
    policy: BrowserPolicy,
    commands: &mut mpsc::Receiver<BrowserCommand>,
    events: &mpsc::Sender<BrowserEvent>,
    shutdown: &mut watch::Receiver<bool>,
) -> Option<BrowserSession> {
    let start = BrowserSession::start_with_policy(options, policy);
    tokio::pin!(start);

    loop {
        if *shutdown.borrow() {
            let _ = send_browser_event(events, BrowserEvent::WorkerStopped).await;
            return None;
        }
        tokio::select! {
            biased;
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    let _ = send_browser_event(events, BrowserEvent::WorkerStopped).await;
                    return None;
                }
            }
            command = commands.recv() => match command {
                Some(BrowserCommand::Shutdown) | None => {
                    let _ = send_browser_event(events, BrowserEvent::WorkerStopped).await;
                    return None;
                }
                Some(BrowserCommand::Execute { id, .. }) => {
                    if !send_browser_event(events, BrowserEvent::OperationFailed {
                        id,
                        message: "browser is still starting".to_string(),
                    }).await {
                        return None;
                    }
                }
                Some(BrowserCommand::Cancel { id }) => {
                    if !send_browser_event(events, BrowserEvent::OperationCancelled { id }).await {
                        return None;
                    }
                }
            },
            result = &mut start => match result {
                Ok(session) => return Some(session),
                Err(error) => {
                    let message = error.to_string();
                    drop(error);
                    let _ = send_browser_event(events, BrowserEvent::StartupFailed {
                        message,
                    }).await;
                    return None;
                }
            },
        }
    }
}

async fn worker_loop(
    session: BrowserSession,
    commands: &mut mpsc::Receiver<BrowserCommand>,
    events: &mpsc::Sender<BrowserEvent>,
    shutdown: &mut watch::Receiver<bool>,
) {
    loop {
        if *shutdown.borrow() {
            break;
        }
        tokio::select! {
            biased;
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
            command = commands.recv() => match command {
                Some(BrowserCommand::Shutdown) | None => break,
                Some(BrowserCommand::Cancel { .. }) => {}
                Some(BrowserCommand::Execute { id, operation }) => {
                    let label = operation.label().to_string();
                    if !send_browser_event(events, BrowserEvent::OperationStarted { id, label }).await {
                        break;
                    }
                    match await_active_operation(
                        execute_browser_operation(&session, operation),
                        id,
                        commands,
                        shutdown,
                        events,
                    ).await {
                        ActiveOperationState::Completed(Ok(result)) => {
                            if !send_browser_event(events, BrowserEvent::OperationFinished { id, result }).await {
                                break;
                            }
                        }
                        ActiveOperationState::Completed(Err(error)) => {
                            let message = error.to_string();
                            drop(error);
                            if !send_browser_event(events, BrowserEvent::OperationFailed {
                                id,
                                message,
                            }).await {
                                break;
                            }
                        }
                        ActiveOperationState::Cancelled => {
                            if !send_browser_event(events, BrowserEvent::OperationCancelled { id }).await {
                                break;
                            }
                        }
                        ActiveOperationState::Shutdown => break,
                    }
                }
            },
        }
    }

    let close_error = session.close().await.err().map(|error| error.to_string());
    if let Some(message) = close_error {
        let _ = send_browser_event(events, BrowserEvent::WorkerFailed { message }).await;
    }
    let _ = send_browser_event(events, BrowserEvent::WorkerStopped).await;
}

async fn await_active_operation<F>(
    operation: F,
    id: u64,
    commands: &mut mpsc::Receiver<BrowserCommand>,
    shutdown: &mut watch::Receiver<bool>,
    events: &mpsc::Sender<BrowserEvent>,
) -> ActiveOperationState
where
    F: Future<Output = BrowserResult<Box<OperationResult>>>,
{
    tokio::pin!(operation);
    loop {
        if *shutdown.borrow() {
            return ActiveOperationState::Shutdown;
        }
        tokio::select! {
            biased;
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return ActiveOperationState::Shutdown;
                }
            }
            command = commands.recv() => match command {
                Some(BrowserCommand::Shutdown) | None => return ActiveOperationState::Shutdown,
                Some(BrowserCommand::Cancel { id: cancel_id }) if cancel_id == id => {
                    return ActiveOperationState::Cancelled;
                }
                Some(BrowserCommand::Execute { id: queued_id, .. }) => {
                    if !send_browser_event(events, BrowserEvent::OperationFailed {
                        id: queued_id,
                        message: "browser worker is already executing an operation".to_string(),
                    }).await {
                        return ActiveOperationState::Shutdown;
                    }
                }
                Some(BrowserCommand::Cancel { .. }) => {}
            },
            result = &mut operation => return ActiveOperationState::Completed(result),
        }
    }
}

async fn execute_browser_operation(
    session: &BrowserSession,
    operation: BrowserOperation,
) -> BrowserResult<Box<OperationResult>> {
    match operation {
        BrowserOperation::Navigate(url) => {
            let page = session.navigate(&url).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: format!("Page loaded: {}", page.title),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Screenshot(output) => {
            let output = session
                .policy()
                .require_output_path(std::path::Path::new(&output))?;
            tokio::fs::write(&output, session.screenshot_png().await?).await?;
            Ok(Box::new(OperationResult {
                activity: format!("Screenshot saved to {}", output.display()),
                update: None,
            }))
        }
        BrowserOperation::Text => {
            let context = session.observe().await?;
            Ok(Box::new(OperationResult {
                activity: "Page text refreshed.".to_string(),
                update: Some(PageUpdate::Text {
                    page: context.page,
                    text: context.text,
                }),
            }))
        }
        BrowserOperation::Dom => {
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: "Compact DOM and accessibility context refreshed.".to_string(),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Observe { fresh } => {
            let context = if fresh {
                session.observe_fresh().await?
            } else {
                session.observe().await?
            };
            Ok(Box::new(OperationResult {
                activity: "Compact observation refreshed.".to_string(),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Semantic { level, region } => {
            let observation = session.semantic_observe(level).await?;
            let observation = if let Some(region_id) = region {
                session
                    .semantic_expand_region(&region_id, observation.revision, level)
                    .await?
            } else {
                observation
            };
            Ok(Box::new(OperationResult {
                activity: format!(
                    "Semantic {} observation refreshed (revision {}).",
                    serde_json::to_value(level)?.as_str().unwrap_or("unknown"),
                    observation.revision
                ),
                update: Some(PageUpdate::Semantic(Box::new(observation))),
            }))
        }
        BrowserOperation::Click(target) => {
            let outcome = session.click(&target).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: action_activity("Clicked", &outcome),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::DoubleClick(target) => {
            let outcome = session.double_click(&target).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: action_activity("Double-clicked", &outcome),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Hover(target) => {
            let outcome = session.hover(&target).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: action_activity("Hovered", &outcome),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Clear(target) => {
            let outcome = session.clear(&target).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: action_activity("Cleared", &outcome),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Check(target) => {
            let outcome = session.check(&target).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: action_activity("Checked", &outcome),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Uncheck(target) => {
            let outcome = session.uncheck(&target).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: action_activity("Unchecked", &outcome),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Select { target, value } => {
            let outcome = session.select_option(&target, &value).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: action_activity("Selected", &outcome),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Type(text) => {
            let character_count = text.chars().count();
            let outcome = session.type_text(&text, None).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: format!(
                    "Typed {character_count} characters (revision {}).",
                    outcome.revision
                ),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::KeyPress(key) => {
            let outcome = session.key_press(&key).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: action_activity("Pressed", &outcome),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Shortcut(shortcut) => {
            let outcome = session.shortcut(&shortcut).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: action_activity("Ran shortcut", &outcome),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Scroll { dx, dy } => {
            let outcome = session.scroll(dx, dy).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: format!("Scrolled (revision {}).", outcome.revision),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::AcceptDialog => {
            session.accept_dialog().await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: "Accepted the JavaScript dialog.".into(),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::DismissDialog => {
            session.dismiss_dialog().await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: "Dismissed the JavaScript dialog.".into(),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::DismissConsent => {
            let outcome = session.dismiss_consent().await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: format!("Consent dismissal: {outcome:?}."),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Evaluate(expression) => {
            let result = session.evaluate(&expression).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: format!(
                    "Result: {}",
                    bounded_text(&result.to_string(), TUI_ACTIVITY_MAX_BYTES)
                ),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::Workflow(path) => {
            let payload = tokio::fs::read_to_string(&path).await?;
            let payload: serde_json::Value = serde_json::from_str(&payload)?;
            let workflow_value = payload
                .get("workflow")
                .cloned()
                .unwrap_or_else(|| payload.clone());
            let inputs_value = payload
                .get("inputs")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let workflow = WorkflowDefinition::from_value(workflow_value)?;
            let inputs: BTreeMap<String, serde_json::Value> = serde_json::from_value(inputs_value)?;
            let result = session.run_workflow(&workflow, &inputs).await?;
            let step_summary = result
                .steps
                .iter()
                .map(|step| format!("{}={:?}", step.id, step.state))
                .collect::<Vec<_>>()
                .join(", ");
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: bounded_text(
                    &format!(
                        "Workflow {} {:?}; trace={} [{}].",
                        result.name,
                        result.status,
                        result.trace.events.len(),
                        step_summary
                    ),
                    TUI_ACTIVITY_MAX_BYTES,
                ),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
        BrowserOperation::ResolveIntent(path) => {
            let payload = tokio::fs::read_to_string(&path).await?;
            let request = SemanticIntentRequest::from_json(&payload)?;
            let result = session.resolve_intent(&request).await?;
            Ok(Box::new(OperationResult {
                activity: format_intent_activity(&result),
                update: Some(PageUpdate::IntentResolution {
                    request: Box::new(request),
                    result: Box::new(result),
                }),
            }))
        }
        BrowserOperation::ExecuteIntent(execution) => {
            let result = session.execute_intent(&execution).await?;
            let context = session.observe_fresh().await?;
            Ok(Box::new(OperationResult {
                activity: format_intent_execution_activity(&result),
                update: Some(PageUpdate::Context(Box::new(context))),
            }))
        }
    }
}

fn action_activity(verb: &str, outcome: &ActionOutcome) -> String {
    let target = outcome
        .target
        .as_ref()
        .map(|target| target.label.as_str())
        .unwrap_or("page");
    let mut effects = Vec::new();
    if outcome.verification.url_changed {
        effects.push("url");
    }
    if outcome.verification.title_changed {
        effects.push("title");
    }
    if outcome.verification.popup_opened {
        effects.push("popup");
    }
    if outcome.verification.dialog_open {
        effects.push("dialog");
    }
    if outcome.verification.download_started {
        effects.push("download");
    }
    let effect_text = if effects.is_empty() {
        String::new()
    } else {
        format!(" effects={}", effects.join(","))
    };
    format!(
        "{verb} {target} ({}; revision {}{}).",
        outcome.execution_id, outcome.revision, effect_text
    )
}

async fn send_browser_event(events: &mpsc::Sender<BrowserEvent>, event: BrowserEvent) -> bool {
    events.send(event).await.is_ok()
}

fn dispatch_ui_intent(
    app: &mut App,
    commands: &mpsc::Sender<BrowserCommand>,
    policy: &BrowserPolicy,
    intent: UiIntent,
) {
    match intent {
        UiIntent::None => {}
        UiIntent::Submit(command) => handle_submission(app, commands, policy, command),
        UiIntent::Cancel(id) => {
            if commands.try_send(BrowserCommand::Cancel { id }).is_err() {
                app.cancellation_enqueue_failed(id);
                app.report_error(
                    "Browser worker is unavailable; cancellation could not be queued.",
                );
            }
        }
        UiIntent::Quit => app.should_quit = true,
    }
}

fn handle_submission(
    app: &mut App,
    commands: &mpsc::Sender<BrowserCommand>,
    policy: &BrowserPolicy,
    command: String,
) {
    app.add_activity(format!("> {command}"));
    if command.eq_ignore_ascii_case("intent execute")
        || strip_ascii_prefix(&command, "intent execute ").is_some()
    {
        let value = strip_ascii_prefix(&command, "intent execute ")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let Some(request) = app.intent_request.clone() else {
            app.report_error("Resolve an intent before executing a selected candidate.");
            return;
        };
        let Some(result) = app.intent_result.as_ref() else {
            app.report_error("No intent resolution is available for execution.");
            return;
        };
        let Some(candidate) = result.candidates.get(app.intent_selection) else {
            app.report_error("No candidate is selected.");
            return;
        };
        let execution = SemanticIntentExecutionRequest {
            request: SemanticIntentRequest {
                expected_revision: result.revision,
                ..request
            },
            candidate_id: candidate.id.clone(),
            value,
        };
        if let Err(error) = execution.validate() {
            app.report_error(error.to_string());
            return;
        }
        queue_browser_operation(
            app,
            commands,
            BrowserOperation::ExecuteIntent(Box::new(execution)),
        );
        return;
    }
    match parse_command(&command) {
        Ok(ParsedCommand::Local(LocalCommand::Help)) => {
            app.add_activity(
                "navigate URL | click TARGET | double click TARGET | hover TARGET | type TEXT | clear TARGET | check TARGET | uncheck TARGET | select TARGET VALUE",
            );
            app.add_activity(
                "press KEY | shortcut MOD+KEY | scroll [DX [DY]] | accept-dialog | dismiss-dialog | dismiss-consent | workflow FILE | resolve-intent FILE | Up/Down select candidate | intent execute [VALUE] | observe | semantic [LEVEL [REGION_ID]] | text | dom | screenshot [FILE] | profiles | knowledge [show RECORD_ID] | daemon [status|doctor|logs|recovery] | JavaScript",
            );
        }
        Ok(ParsedCommand::Local(LocalCommand::Profiles)) => {
            if let Err(error) =
                policy.require(crate::browser::policy::PolicyCapability::PersistentProfile)
            {
                app.report_error(error.to_string());
                return;
            }
            match ProfileManager::new().list_profiles() {
                Ok(profiles) if profiles.is_empty() => app.add_activity("No saved profiles."),
                Ok(profiles) => {
                    for profile in profiles {
                        app.add_activity(format!("  - {profile}"));
                    }
                }
                Err(error) => app.report_error(error.to_string()),
            }
        }
        Ok(ParsedCommand::Local(LocalCommand::Knowledge(record_id))) => {
            if let Err(error) =
                policy.require(crate::browser::policy::PolicyCapability::PersistentProfile)
            {
                app.report_error(error.to_string());
                return;
            }
            match KnowledgeStore::open(&app.knowledge_path) {
                Ok(store) => {
                    let content = match record_id {
                        Some(record_id) => store
                            .get(&record_id)
                            .map(|record| {
                                serde_json::to_string_pretty(record)
                                    .map_err(|error| error.to_string())
                            })
                            .unwrap_or_else(|| {
                                Err(format!("knowledge record not found: {record_id}"))
                            }),
                        None => match store.stats() {
                            Ok(stats) => serde_json::to_string_pretty(&serde_json::json!({
                                "path": store.path().display().to_string(),
                                "stats": stats,
                                "records": store.records().iter().map(|record| serde_json::json!({
                                    "recordId": &record.record_id,
                                    "kind": record.kind,
                                    "confidence": record.confidence,
                                    "origin": &record.scope.origin,
                                    "pathPattern": &record.scope.path_pattern,
                                })).collect::<Vec<_>>(),
                            }))
                            .map_err(|error| error.to_string()),
                            Err(error) => Err(error.to_string()),
                        },
                    };
                    match content {
                        Ok(content) => {
                            app.title = "Glass — Knowledge inspector".into();
                            app.set_page_content(content);
                            app.set_status("Knowledge inspector");
                            app.add_activity("Knowledge store inspected without browser startup.");
                        }
                        Err(error) => app.report_error(error.to_string()),
                    }
                }
                Err(error) => app.report_error(error.to_string()),
            }
        }
        Ok(ParsedCommand::Local(LocalCommand::Daemon(view))) => {
            let (socket, status) = crate::daemon::default_paths();
            let result: BrowserResult<serde_json::Value> = match view {
                DaemonView::Status => crate::daemon::status(Some(&socket), Some(&status))
                    .and_then(|value| serde_json::to_value(value).map_err(Into::into)),
                DaemonView::Doctor => crate::daemon::doctor(Some(&socket), Some(&status)),
                DaemonView::Logs => crate::daemon::logs(Some(&status)),
                DaemonView::Recovery => crate::daemon::recovery(Some(&status)),
            };
            match result {
                Ok(value) => {
                    app.title = "Glass — Daemon inspector".into();
                    match serde_json::to_string_pretty(&value) {
                        Ok(content) => app.set_page_content(content),
                        Err(error) => app.report_error(error.to_string()),
                    }
                    app.set_status("Daemon inspector");
                    app.add_activity(
                        "Daemon state inspected without starting a browser operation.",
                    );
                }
                Err(error) => app.report_error(error.to_string()),
            }
        }
        Ok(ParsedCommand::Browser(operation)) => queue_browser_operation(app, commands, operation),
        Err(error) => app.report_error(error),
    }
}

fn queue_browser_operation(
    app: &mut App,
    commands: &mpsc::Sender<BrowserCommand>,
    operation: BrowserOperation,
) {
    if !app.browser_ready() {
        app.report_error("Browser is not ready yet.");
        return;
    }
    if app.is_busy() {
        app.report_error("A browser operation is already running; press Esc to cancel it.");
        return;
    }

    let id = app.allocate_operation_id();
    let label = operation.label().to_string();
    match commands.try_send(BrowserCommand::Execute { id, operation }) {
        Ok(()) => app.begin_operation(id, label),
        Err(mpsc::error::TrySendError::Full(_)) => {
            app.report_error("Browser command queue is full.");
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            app.browser_state = BrowserState::Unavailable;
            app.report_error("Browser worker is unavailable.");
        }
    }
}

fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let header = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);

    let title = Paragraph::new(app.title.as_str())
        .block(Block::default().borders(Borders::ALL).title("Glass"))
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(title, header[0]);

    let url = Paragraph::new(app.url.as_str())
        .block(Block::default().borders(Borders::ALL).title("URL"))
        .style(Style::default().fg(Color::Yellow));
    frame.render_widget(url, header[1]);

    let content = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(chunks[1]);

    let activity = app
        .activity
        .iter()
        .map(|entry| ListItem::new(Line::from(entry.as_str())))
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(activity)
            .block(Block::default().borders(Borders::ALL).title("Activity"))
            .style(Style::default().fg(Color::Green)),
        content[0],
    );

    frame.render_widget(
        Paragraph::new(app.page_content.as_str())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Structured Observation"),
            )
            .scroll((app.page_scroll, 0))
            .wrap(Wrap { trim: true }),
        content[1],
    );

    frame.render_widget(
        Paragraph::new(app.input.as_str())
            .block(Block::default().borders(Borders::ALL).title("Command")),
        chunks[2],
    );
    let escape_hint = if app.is_busy() {
        "Esc: cancel"
    } else {
        "Esc: close error/quit"
    };
    frame.render_widget(
        Paragraph::new(format!(
            " {}   {}   PgUp/PgDn: observation   q/Ctrl-C: quit   Enter: execute   {escape_hint}   {}",
            app.status,
            app.capability_summary,
            app.input.chars().count()
        ))
        .style(Style::default().fg(Color::DarkGray)),
        chunks[3],
    );

    if let Some(error) = &app.error_msg {
        let popup = centered_popup(frame.area());
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new(error.as_str())
                .block(Block::default().borders(Borders::ALL).title("Error"))
                .style(Style::default().fg(Color::Red))
                .wrap(Wrap { trim: true }),
            popup,
        );
    }
}

fn centered_popup(area: Rect) -> Rect {
    let width = (area.width.saturating_mul(2) / 3).max(1).min(area.width);
    let height = 5.min(area.height);
    Rect {
        x: area.width.saturating_sub(width) / 2,
        y: area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, Hide) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self { active: true })
    }

    fn restore(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        let raw_result = disable_raw_mode();
        let mut stdout = io::stdout();
        let screen_result = execute!(stdout, LeaveAlternateScreen, Show);
        match (raw_result, screen_result) {
            (Err(error), _) | (_, Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

pub async fn run_tui(cli: &Cli) -> BrowserResult<()> {
    let mut terminal_guard = TerminalGuard::enter()?;
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (input_tx, mut input_events) = mpsc::channel(INPUT_CHANNEL_CAPACITY);
    let (browser_commands, browser_command_rx) = mpsc::channel(BROWSER_COMMAND_CHANNEL_CAPACITY);
    let (browser_event_tx, mut browser_events) = mpsc::channel(BROWSER_EVENT_CHANNEL_CAPACITY);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let options = SessionOptions {
        port: cli.port,
        chrome_path: cli.chrome_path.clone(),
        profile: cli.profile.clone(),
        incognito: cli.incognito,
        attach: cli.attach,
        target_id: cli.target_id.clone(),
        frame_id: cli.frame_id.clone(),
        headed: cli.headed,
        interaction_mode: cli.interaction,
        audit: cli.audit,
        policy: None,
    };
    let policy = crate::cli::runner::policy_from_cli(cli)?;
    let local = LocalSet::new();
    let browser_worker = local.spawn_local(browser_worker(
        options,
        policy.clone(),
        browser_command_rx,
        browser_event_tx,
        shutdown_rx,
    ));
    let mut input_worker = InputWorker::spawn(input_tx);
    let mut app = App::new();
    let manifest = GlassCapabilityManifest::for_policy(&policy);
    app.capability_summary = format!(
        "Capabilities: {} schemas, daemon {}",
        manifest.schemas.len(),
        if manifest.capabilities.get("localDaemon") == Some(&true) {
            "on"
        } else {
            "off"
        }
    );
    app.knowledge_path = cli
        .knowledge_store
        .clone()
        .unwrap_or_else(|| default_knowledge_store_path(&cli.profile));

    let loop_result = local
        .run_until(run_tui_loop(
            &mut terminal,
            &mut app,
            &browser_commands,
            &mut input_events,
            &mut browser_events,
            &policy,
        ))
        .await;

    drop(input_events);
    drop(browser_events);
    let _ = shutdown_tx.send(true);
    let _ = browser_commands.try_send(BrowserCommand::Shutdown);
    drop(browser_commands);
    let input_result = input_worker.stop();
    let cursor_result = terminal.show_cursor();
    let terminal_result = terminal_guard.restore();
    let worker_result = local.run_until(finish_browser_worker(browser_worker)).await;

    loop_result?;
    input_result?;
    cursor_result?;
    terminal_result?;
    worker_result
}

async fn run_tui_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    commands: &mpsc::Sender<BrowserCommand>,
    input_events: &mut mpsc::Receiver<InputEvent>,
    browser_events: &mut mpsc::Receiver<BrowserEvent>,
    policy: &BrowserPolicy,
) -> BrowserResult<()> {
    let mut redraw = true;
    let mut browser_events_open = true;
    let mut busy_tick = time::interval(BUSY_TICK);
    busy_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    while !app.should_quit {
        if redraw {
            terminal.draw(|frame| draw(frame, app))?;
        }

        redraw = tokio::select! {
            biased;
            input = input_events.recv() => match input {
                Some(InputEvent::Key(key)) => {
                    let intent = app.reduce_key(key);
                    dispatch_ui_intent(app, commands, policy, intent);
                    true
                }
                Some(InputEvent::Redraw) => true,
                Some(InputEvent::Error(error)) => return Err(error.into()),
                None => return Err("TUI input worker stopped".into()),
            },
            event = browser_events.recv(), if browser_events_open => match event {
                Some(event) => {
                    if let Some(operation) = app.apply_browser_event(event)? {
                        queue_browser_operation(app, commands, operation);
                    }
                    true
                }
                None => {
                    browser_events_open = false;
                    app.busy = None;
                    if !matches!(app.browser_state, BrowserState::Unavailable | BrowserState::Stopped) {
                        app.browser_state = BrowserState::Unavailable;
                        app.set_status("Browser worker unavailable");
                        app.report_error("Browser worker stopped unexpectedly.");
                    }
                    true
                }
            },
            _ = busy_tick.tick(), if app.is_busy() => {
                app.tick_busy();
                true
            },
        };
    }
    Ok(())
}

async fn finish_browser_worker(mut worker: JoinHandle<()>) -> BrowserResult<()> {
    match time::timeout(WORKER_SHUTDOWN_TIMEOUT, &mut worker).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(format!("browser worker failed: {error}").into()),
        Err(_) => {
            worker.abort();
            let _ = worker.await;
            Err("timed out waiting for browser worker shutdown".into())
        }
    }
}

fn bounded_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    if max_bytes == 0 {
        return String::new();
    }

    const MARKER: &str = "\n[truncated]";
    if max_bytes <= MARKER.len() {
        let mut end = max_bytes;
        while end > 0 && !value.is_char_boundary(end) {
            end -= 1;
        }
        return value[..end].to_string();
    }

    let mut end = max_bytes - MARKER.len();
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = value[..end].to_string();
    truncated.push_str(MARKER);
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn command_parser_preserves_browser_actions_and_rejects_bad_scroll() {
        assert!(matches!(
            parse_command("double click r7:b42"),
            Ok(ParsedCommand::Browser(BrowserOperation::DoubleClick(target))) if target == "r7:b42"
        ));
        assert!(matches!(
            parse_command("select ref=r7:b42 premium"),
            Ok(ParsedCommand::Browser(BrowserOperation::Select { target, value }))
                if target == "ref=r7:b42" && value == "premium"
        ));
        assert!(matches!(
            parse_command("shortcut Ctrl+K"),
            Ok(ParsedCommand::Browser(BrowserOperation::Shortcut(shortcut)))
                if shortcut == "Ctrl+K"
        ));
        assert!(matches!(
            parse_command("dismiss-consent"),
            Ok(ParsedCommand::Browser(BrowserOperation::DismissConsent))
        ));
        assert!(parse_command("select target").is_err());
        assert!(matches!(
            parse_command("scroll -4 120"),
            Ok(ParsedCommand::Browser(BrowserOperation::Scroll { dx, dy })) if dx == -4.0 && dy == 120.0
        ));
        assert!(parse_command("scroll nope").is_err());
        assert!(matches!(
            parse_command("workflow workflow.json"),
            Ok(ParsedCommand::Browser(BrowserOperation::Workflow(path))) if path == "workflow.json"
        ));
        assert!(matches!(
            parse_command("resolve-intent intent.json"),
            Ok(ParsedCommand::Browser(BrowserOperation::ResolveIntent(path))) if path == "intent.json"
        ));
        assert!(matches!(
            parse_command("semantic interactive region_search_1"),
            Ok(ParsedCommand::Browser(BrowserOperation::Semantic {
                level: SemanticObservationLevel::Interactive,
                region: Some(region),
            })) if region == "region_search_1"
        ));
        assert!(parse_command("semantic verbose").is_err());
        assert!(matches!(
            parse_command("profiles"),
            Ok(ParsedCommand::Local(LocalCommand::Profiles))
        ));
        assert!(matches!(
            parse_command("knowledge"),
            Ok(ParsedCommand::Local(LocalCommand::Knowledge(None)))
        ));
        assert!(matches!(
            parse_command("knowledge show record-1"),
            Ok(ParsedCommand::Local(LocalCommand::Knowledge(Some(record))))
                if record == "record-1"
        ));
        assert!(matches!(
            parse_command("daemon status"),
            Ok(ParsedCommand::Local(LocalCommand::Daemon(
                DaemonView::Status
            )))
        ));
        assert!(matches!(
            parse_command("daemon recovery"),
            Ok(ParsedCommand::Local(LocalCommand::Daemon(
                DaemonView::Recovery
            )))
        ));
    }

    #[test]
    fn reducer_edits_unicode_and_requests_matching_cancellation() {
        let mut app = App::new();
        assert_eq!(app.reduce_key(key(KeyCode::Char('日'))), UiIntent::None);
        assert_eq!(app.reduce_key(key(KeyCode::Char('本'))), UiIntent::None);
        app.reduce_key(key(KeyCode::Backspace));
        assert_eq!(app.input, "日");

        app.browser_state = BrowserState::Ready;
        app.begin_operation(4, "Observe");
        assert_eq!(app.reduce_key(key(KeyCode::Esc)), UiIntent::Cancel(4));
        assert!(app.busy.as_ref().unwrap().cancelling);
        assert_eq!(app.reduce_key(key(KeyCode::Esc)), UiIntent::None);
    }

    #[test]
    fn app_bounds_retained_page_state() {
        let mut app = App::new();
        app.set_page_content("界".repeat(TUI_PAGE_MAX_BYTES));

        assert!(app.page_content.len() <= TUI_PAGE_MAX_BYTES);
        assert!(app.page_content.contains("[truncated]"));
    }

    #[tokio::test]
    async fn matching_cancel_interrupts_an_active_operation() {
        let (command_tx, mut command_rx) = mpsc::channel(2);
        let (_shutdown_tx, mut shutdown_rx) = watch::channel(false);
        let (event_tx, _event_rx) = mpsc::channel(2);
        command_tx
            .send(BrowserCommand::Cancel { id: 9 })
            .await
            .unwrap();

        let result = await_active_operation(
            std::future::pending::<BrowserResult<Box<OperationResult>>>(),
            9,
            &mut command_rx,
            &mut shutdown_rx,
            &event_tx,
        )
        .await;

        assert!(matches!(result, ActiveOperationState::Cancelled));
    }

    #[tokio::test]
    async fn delayed_worker_event_does_not_block_input_reducer() {
        let (command_tx, mut command_rx) = mpsc::channel(2);
        let (event_tx, mut event_rx) = mpsc::channel(4);
        let worker = tokio::spawn(async move {
            let Some(BrowserCommand::Execute { id, .. }) = command_rx.recv().await else {
                return;
            };
            event_tx
                .send(BrowserEvent::OperationStarted {
                    id,
                    label: "Observe".to_string(),
                })
                .await
                .unwrap();
            time::sleep(Duration::from_millis(100)).await;
            event_tx
                .send(BrowserEvent::OperationFinished {
                    id,
                    result: Box::new(OperationResult {
                        activity: "Observation refreshed.".to_string(),
                        update: None,
                    }),
                })
                .await
                .unwrap();
        });

        let mut app = App::new();
        app.browser_state = BrowserState::Ready;
        app.begin_operation(1, "Observe");
        command_tx
            .send(BrowserCommand::Execute {
                id: 1,
                operation: BrowserOperation::Observe { fresh: false },
            })
            .await
            .unwrap();
        app.apply_browser_event(event_rx.recv().await.unwrap())
            .unwrap();

        assert_eq!(app.reduce_key(key(KeyCode::Char('x'))), UiIntent::None);
        assert_eq!(app.input, "x");
        assert!(
            time::timeout(Duration::from_millis(20), event_rx.recv())
                .await
                .is_err()
        );

        app.apply_browser_event(event_rx.recv().await.unwrap())
            .unwrap();
        assert!(!app.is_busy());
        worker.await.unwrap();
    }
}
