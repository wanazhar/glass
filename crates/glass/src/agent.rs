//! The agent loop.
//!
//! One shape, no exceptions: stream a turn, run whatever tools it asked for,
//! feed the results back, repeat until it stops asking. Everything else in
//! glass is a detail hanging off this.

use std::path::Path;

use glass_mod_api::{Message, Mod_, ToolCall, ToolOutput, TurnContext};

use crate::compact;
use crate::concurrency;
use crate::ext;
use crate::provider::{self, Event, Provider, Request};
use crate::readguard;
use crate::session::{Record, Session};
use crate::tools::{self, State};
use crate::verify;
use crate::warmer::{CacheSnapshot, Warmer};

/// Condense tool output to one line for the activity feed.
///
/// The full text is always in the log and always goes to the model; this is
/// only what scrolls past on screen, so it must stay short enough that a long
/// turn does not bury the conversation.
fn summarize_output(body: &str) -> String {
    const MAX: usize = 160;
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "(no output)".to_string();
    }
    let first = trimmed.lines().next().unwrap_or("").trim();
    if first.starts_with("[exit ") && !first.starts_with("[exit 0]") {
        let preview = trimmed.lines().skip(1).find(|line| !line.trim().is_empty());
        if let Some(preview) = preview {
            return format!(
                "{}\n{}",
                first.chars().take(48).collect::<String>(),
                preview.trim().chars().take(MAX).collect::<String>()
            );
        }
    }
    let count = trimmed.lines().count();
    let mut s: String = first.chars().take(MAX).collect();
    if count > 1 {
        s.push_str(&format!(" … ({} lines)", count));
    } else if first.chars().count() > MAX {
        s.push('…');
    }
    s
}
/// What one turn produced.
pub struct TurnOutput {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<crate::provider::Usage>,
    /// Latest model request input, separate from aggregated turn usage.
    pub last_input: Option<u32>,
    pub warnings: Vec<String>,
}

/// Why the loop stopped.
#[derive(Debug, PartialEq, Eq)]
pub enum Stop {
    /// The model finished without asking for tools.
    Finished,
    /// The tool-call budget for this turn was exhausted.
    ToolBudget,
    /// The user steered, so the turn stopped early to take the correction.
    Steered,
    Cancelled,
}

impl TurnOutput {
    /// True when the model asked for at least one tool.
    pub fn wants_tools(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

/// Something the loop wants shown to the user while it is working.
///
/// The agent never writes to the terminal itself: streaming is a UI concern and
/// a headless caller should not have output appearing under it. The observer is
/// how the loop reports progress without depending on a renderer.
pub enum Progress<'a> {
    /// Waiting for the provider to begin its response.
    Waiting,
    /// Assistant text arriving, chunk by chunk.
    Text(&'a str),
    /// Provider-exposed reasoning text or public summary.
    Reasoning(&'a str),
    /// A tool is about to run.
    ToolStart {
        call_id: &'a str,
        name: &'a str,
        command: Option<&'a str>,
    },
    /// A tool finished, with the tail of its output.
    ToolEnd {
        call_id: &'a str,
        summary: String,
        output: &'a str,
    },
    /// Context was compacted.
    Compacted { kept: usize, dropped: usize },
    /// The user interrupted the turn with a correction.
    Steered(&'a str),
    /// A write was refused because the file had not been read.
    BlockedByGuard(&'a str),
    /// A transient provider failure is being retried.
    Retrying {
        attempt: u8,
        delay_s: u64,
        reason: String,
    },
    /// A provider notice such as an automatic model fallback.
    ProviderNotice(&'a str),
    /// A failed provider stream with a caller-safe cause.
    ProviderFailure(&'a str),
}

/// Run `f` with the host-tool bridge an extension may use during a hook.
///
/// Only `bash` and `state` are exposed: those are the tools the model already
/// has, so nesting cannot widen an extension's reach beyond the base surface.
/// Used by `run_tool` and by `rewrite_tool_result` (auto-verify runs the
/// project's verifier from inside that hook). Callback-shaped so the mutable
/// borrow of `state` cannot outlive the call.
fn with_tool_bridge<R>(
    workspace: &std::path::Path,
    state: &mut State,
    call_id: &str,
    f: impl FnOnce(&mut dyn FnMut(&str, &str) -> String) -> R,
) -> R {
    let workspace = workspace.to_path_buf();
    let id = format!("{call_id}-nested");
    let mut bridge = move |tool: &str, args: &str| -> String {
        let inner = ToolCall {
            id: id.clone(),
            name: tool.to_string(),
            arguments: args.to_string(),
        };
        match tool {
            "bash" => tools::bash(&inner, &workspace).body,
            "state" => state.apply(&inner).body,
            other => format!("error: extensions may only call bash or state, not `{other}`"),
        }
    };
    f(&mut bridge)
}

enum StreamUpdate {
    Event(Event),
    Pricing(String, Option<crate::pricing::RateCard>),
    Retry {
        attempt: u8,
        delay_s: u64,
        reason: String,
    },
}
struct StreamWorker {
    receiver: std::sync::mpsc::Receiver<StreamUpdate>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Drop for StreamWorker {
    fn drop(&mut self) {
        self.cancel
            .store(true, std::sync::atomic::Ordering::Release);
    }
}

pub struct Agent<'a> {
    pub provider: &'a dyn Provider,
    pub session: &'a mut Session,
    pub state: &'a mut State,
    pub mods: &'a [Box<dyn Mod_>],
    /// External extensions. Separate from `mods` because they are child
    /// processes on a pipe rather than in-process trait objects.
    pub extensions: &'a mut ext::Registry,
    pub workspace: &'a Path,
    pub image_input: bool,
    pub max_tokens: u32,
    pub reasoning: Option<u32>,
    /// Cap on tool calls per turn, so a confused model cannot loop forever.
    pub max_tool_rounds: usize,
    /// Model-invoked context tools (checkpoint/rewind/wait): off by default,
    /// enabled by `checkpoint_tools = true` in config. Appending is done in
    /// `tool_set` so a disabled feature costs zero schema bytes and an
    /// enabled one joins the frozen declared list at session start.
    pub checkpoint_tools: bool,
    /// Base system prompt, with mod contributions appended per request.
    base_prompt: String,
    /// Skill catalogue, appended after the prompt.
    catalogue: Option<String>,
    /// Project instructions from AGENTS.md / CLAUDE.md.
    project_instructions: Option<String>,
    /// Called as the loop makes progress. `None` disables reporting entirely.
    pub on_progress: Option<ProgressSink<'a>>,
    /// Interactive user-input channel, borrowed from the existing terminal reader.
    /// Headless/ACP agents leave this unset and never advertise elicitation support.
    input_keys: Option<&'a crate::input::KeyReader>,
    /// Cached system-prompt contribution from extensions.
    ext_prompt: String,
    /// Size of the cacheable prefix, recomputed each request.
    prefix_bytes: usize,
    /// A correction the user typed mid-turn, applied before the next request.
    steer: Option<String>,
    /// Files the model has demonstrably read. Feeds the read-before-write guard.
    reads: readguard::ReadSet,
    /// Cache warmer, when the REPL provides one. The Agent owns
    /// no credentials, so the warmer is injected rather than built.
    pub warmer: Option<&'a Warmer>,
    stream_factory: Option<crate::warmer::ProviderFactory>,
    recovering: bool,
    read_only: bool,
    /// Pre-tool-use shell hook (wave E); None = no hook configured.
    pre_tool_hook: Option<crate::hooks_shell::PreToolUse>,
    /// Circuit breaker for paid summaries: three consecutive failures stop
    /// the paid path for the session while deterministic compaction continues.
    breaker: compact::Breaker,
    /// Whether this turn already spent its one context-overflow recovery.
    overflow_recovered: bool,
    empty_recovered: bool,
    empty_correction: bool,
    /// Throttled partial-output sidecar, when the session wants one.
    live: Option<crate::partial::Writer>,
    /// Repeated-tool-call detection across the current turn.
    loop_window: crate::loop_detect::LoopWindow,
    /// Count of tool calls run in this turn, for the read-staleness hint.
    tool_call_index: u64,
    /// The cancellation token every extension tool call this turn runs
    /// under. Tripped when the user steers, so a slow extension stops
    /// waiting for results the turn no longer needs.
    turn_cancel: crate::ext::Cancel,
}

/// Lines of a written file echoed back to the model.
///
/// Bounded because a generated file can be thousands of lines; the model can
/// always read more with `bash`, and the point is to show that the write
/// landed, not to replace the file in the context.
const VERIFY_TAIL_LINES: usize = 40;

/// A progress callback.
pub type ProgressSink<'a> = Box<dyn Fn(Progress<'_>) + 'a>;

impl Drop for Agent<'_> {
    /// The turn is over however it ended: the partial-output sidecar had a
    /// reason to exist only while the turn was running. A file that survives
    /// this is therefore evidence of a kill, which is exactly the signal the
    /// next open acts on.
    fn drop(&mut self) {
        if let Some(live) = self.live.as_mut() {
            live.finish();
        }
    }
}

impl<'a> Agent<'a> {
    /// Report a progress event if an observer is attached.
    fn emit(&self, p: Progress<'_>) {
        if let Some(f) = &self.on_progress {
            f(p);
        }
    }

    /// Open a tool chip in the default view.
    fn emit_tool_start(&mut self, call: &ToolCall) {
        if let Some(live) = self.live.as_mut() {
            live.tool(&call.id, &call.name);
        }
        let command = call.arg_str("command").or_else(|| call.arg_str("path")).or_else(|| {
            call.arg_raw("job")
                .and_then(|raw| serde_json::from_str(&raw).ok())
                .map(|v| crate::jobs::label(&v))
        });
        self.emit(Progress::ToolStart {
            call_id: &call.id,
            name: &call.name,
            command: command.as_deref().filter(|c| !c.is_empty()),
        });
    }

    pub fn new(
        provider: &'a dyn Provider,
        session: &'a mut Session,
        state: &'a mut State,
        mods: &'a [Box<dyn Mod_>],
        extensions: &'a mut ext::Registry,
        workspace: &'a Path,
    ) -> Self {
        extensions.bind_session_path(session.path.clone());
        Self {
            provider,
            session,
            state,
            mods,
            extensions,
            workspace,
            image_input: true,
            max_tokens: 16_000,
            reasoning: None,
            max_tool_rounds: usize::MAX,
            base_prompt: String::new(),
            catalogue: None,
            project_instructions: None,
            on_progress: None,
            input_keys: None,
            ext_prompt: String::new(),
            prefix_bytes: 0,
            steer: None,
            reads: readguard::ReadSet::new(),
            warmer: None,
            stream_factory: None,
            recovering: false,
            read_only: false,
            checkpoint_tools: false,
            pre_tool_hook: None,
            breaker: compact::Breaker::default(),
            overflow_recovered: false,
            empty_recovered: false,
            empty_correction: false,
            live: None,
            loop_window: crate::loop_detect::LoopWindow::new(),
            tool_call_index: 0,
            turn_cancel: crate::ext::Cancel::new(),
        }
    }

    /// Commit streamed partial output to a sidecar between writes.
    ///
    /// `interval_ms == 0` disables it. The writer is dropped with the agent,
    /// which deletes the sidecar: a file left behind therefore means the
    /// process died mid-turn, not that cleanup was forgotten.
    pub fn with_live(mut self, interval_ms: u64) -> Self {
        self.live = crate::partial::Writer::new(&self.session.path, interval_ms);
        self
    }

    /// Shared isolated-task setup. Callers still own providers, sessions,
    /// metering, cancellation and persistence; no permissions are broadened.
    pub fn with_task_context(self, prompt: String, read_only: bool) -> Self {
        self.with_prompt(prompt, None, None).with_plan_mode(read_only)
    }

    pub fn with_plan_mode(mut self, enabled: bool) -> Self {
        self.read_only = enabled;
        self
    }

    /// Register the pre-tool-use shell hook; fires before every tool call.
    pub fn with_pre_tool_hook(mut self, hook: Option<crate::hooks_shell::PreToolUse>) -> Self {
        self.pre_tool_hook = hook;
        self
    }
    fn tool_set(&self) -> Vec<(String, String, String)> {
        if self.read_only {
            crate::inspect::schema()
        } else {
            let mut tools = provider::collect_tools(self.mods);
            if !self.image_input { tools.retain(|(name, _, _)| name != "view_image"); }
            if self.checkpoint_tools {
                tools.extend(crate::provider::checkpoint_tool_schemas());
            }
            tools
        }
    }

    pub fn with_recovery(mut self, recovering: bool) -> Self {
        self.recovering = recovering;
        self
    }
    pub fn with_stream_factory(mut self, factory: crate::warmer::ProviderFactory) -> Self {
        self.stream_factory = Some(factory);
        self
    }

    /// Attach the cache warmer.
    pub fn with_warmer(mut self, warmer: &'a Warmer) -> Self {
        self.warmer = Some(warmer);
        self
    }

    /// Refresh the prompt cache across the idle gap that is
    /// about to begin, if an extension asked for one and the
    /// prefix is large enough to be worth the write.
    ///
    /// Called when a turn ends and control returns to the
    /// prompt: that is the moment the harness goes idle, and
    /// therefore the moment a cold cache would be felt.
    pub fn schedule_warm(&mut self) {
        if self.read_only {
            return;
        }
        let Some(warmer) = self.warmer else { return };
        let Some(policy) = self.extensions.cache_warmer() else {
            return;
        };
        if self.prefix_bytes < policy.min_prefix_bytes {
            return;
        }

        // Rebuild exactly what the last request sent, so the
        // refresh lands on the same cache entries a real turn
        // would read: the declared (frozen) extension tools
        // plus the built-ins, and the checkpointed system
        // prompt, are what the last request actually sent.
        let (declared, _) = self.extensions.declared_tools();
        let mut tools = self.tool_set();
        for (n, d, sc) in declared {
            if !tools.iter().any(|(existing, _, _)| *existing == n) {
                tools.push((n, d, sc));
            }
        }
        let snapshot = CacheSnapshot {
            session_path: Some(self.session.path.clone()),
            system: self.effective_system(),
            messages: self.session.messages(),
            tools,
        };
        warmer.schedule(policy, snapshot);
    }

    /// Fetch extension prompt text once and cache it.
    ///
    /// Called before the first request; afterwards the cache is reused, because
    /// a pipe round-trip per turn would be latency for a value that does not
    /// change.
    pub fn prime_extensions(mut self) -> Self {
        if !self.read_only {
            self.ext_prompt = self.extensions.system_prompt();
        }
        self
    }

    /// Attach a progress observer.
    pub fn with_progress(mut self, f: impl Fn(Progress<'_>) + 'a) -> Self {
        self.on_progress = Some(Box::new(f));
        self
    }

    /// Bind MCP elicitation to the already-running UI key reader for this turn.
    pub fn with_input_keys(mut self, keys: &'a crate::input::KeyReader) -> Self {
        self.input_keys = Some(keys);
        self
    }

    /// Enable the model-invoked checkpoint/rewind/wait tools.
    pub fn with_checkpoint_tools(mut self, on: bool) -> Self {
        self.checkpoint_tools = on;
        self
    }

    fn command_approvals(&self, call: &ToolCall) -> crate::sandbox::SessionApprovals {
        // Headless and delegated agents never inherit the interactive parent
        // session's exceptions. Snapshot explicit grants before worker dispatch.
        if self.input_keys.is_some() {
            call.arg_str("command")
                .and_then(|command| crate::permissions::approvals_for_command(&command).ok())
                .unwrap_or_default()
        } else {
            crate::sandbox::SessionApprovals::default()
        }
    }

    pub fn with_prompt(
        mut self,
        base: String,
        catalogue: Option<String>,
        project_instructions: Option<String>,
    ) -> Self {
        self.base_prompt = base;
        self.catalogue = catalogue;
        self.project_instructions = project_instructions;
        self
    }

    pub fn with_image_input(mut self, enabled: bool) -> Self { self.image_input = enabled; self }

    pub fn with_limits(mut self, max_tokens: u32, reasoning: Option<u32>, rounds: usize) -> Self {
        self.max_tokens = max_tokens;
        self.reasoning = reasoning;
        self.max_tool_rounds = rounds.max(1);
        self
    }

    /// Stream exactly one model turn, without running any tools.
    ///
    /// Takes `&mut self` because context compaction rewrites the session,
    /// which is not something a read-only borrow can express.
    pub fn turn(
        &mut self,
        mut pump: impl FnMut(&mut Option<String>),
    ) -> Result<TurnOutput, provider::Error> {
        self.session.validate_writes();
        if let Some(error) = self.session.write_error() {
            return Err(provider::Error::Config(format!(
                "session log is not writable; no provider request was made: {error}"
            )));
        }
        let mut messages = self.session.messages();
        if !self.image_input && messages.iter().any(|m| matches!(m, glass_mod_api::Message::Image {..})) {
            return Err(provider::Error::Config("Image input is disabled for this model (image_input=false). Remove image attachments in /context or select a vision-capable model.".into()));
        }
        let context_epoch = self.session.log_len();
        self.ensure_context_fits(&messages)?;
        // Compaction updates records; send that projection, not the stale
        // pre-compaction request. Avoid rebuilding on ordinary requests.
        if self.session.log_len() != context_epoch {
            messages = self.session.messages();
        }

        let mut tools = self.tool_set();
        // Extension tools are appended and can never shadow a builtin, so a
        // prompt written against `bash` and `state` keeps working with any
        // extension set loaded. The extension list is frozen for the session
        // (see `Registry::declared_tools`): it is part of the cacheable
        // prefix, so changing it mid-session would turn every later turn
        // into a cache miss. A change is noted once, in the transcript, and
        // applies to the next session.
        let (declared, diff) = if self.read_only {
            (Vec::new(), crate::ext::FreezeDiff::default())
        } else {
            self.extensions.declared_tools()
        };
        for (n, d, s) in declared {
            if !tools.iter().any(|(existing, _, _)| *existing == n) {
                tools.push((n, d, s));
            }
        }
        if !diff.added.is_empty() || !diff.removed.is_empty() {
            let mut note = String::from("tool set frozen for prompt-cache stability");
            if !diff.added.is_empty() {
                note.push_str(&format!(
                    "; not loaded this session: {}",
                    diff.added.join(", ")
                ));
            }
            if !diff.removed.is_empty() {
                note.push_str(&format!("; still declared: {}", diff.removed.join(", ")));
            }
            note.push_str("; extension changes apply to the next session");
            self.session.append(Record::Meta(note));
        }

        let system = self.effective_system();
        if self.empty_correction {
            messages.push(Message::User("Continue the current task from the existing tool results. Your last response contained reasoning but no answer. Produce an answer or valid tool call now; do not repeat completed actions. Keep reasoning brief.".into()));
        }
        let model = self.provider.model().to_string();

        // Size of the prefix a provider would cache: system prompt plus tool
        // schemas. Reported to extensions so a cache warmer knows whether a
        // refresh write is worth its cost.
        self.prefix_bytes = system.len()
            + tools
                .iter()
                .map(|(_, d, s)| d.len() + s.len())
                .sum::<usize>();

        let req = Request {
            model: &model,
            system: &system,
            messages: &messages,
            tools: &tools,
            max_tokens: self.max_tokens,
            reasoning: if self.empty_correction { Some(0) } else { self.reasoning },
        };

        let manifest = crate::context_audit::capture(
            &req,
            self.catalogue.as_deref(),
            &self.ext_prompt,
            latest_system_checkpoint(&self.session.history_records()).is_some() && !self.read_only,
        );
        crate::context_audit::save(self.session, &manifest).map_err(|e| {
            provider::Error::Config(format!(
                "cannot record request context; no provider request was made: {e}"
            ))
        })?;

        let mut trace = crate::requests::Trace::begin(self.session, &req);
        trace.pricing(self.provider.rate_card());
        self.emit(Progress::Waiting);
        // Durable coordinate for the retries of this request. Captured before
        // the request goes out so every retry of it records the same index,
        // which is what lets the inspector attach a retry to the failure it
        // belongs to without needing a clock.
        let request_anchor = self.session.log_len();
        let attempt_log = self.session.path.clone();
        let receiver = if let Some(factory) = self.stream_factory.clone() {
            let (tx, rx) = std::sync::mpsc::sync_channel(64);
            let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let worker_cancel = cancel.clone();
            let system = system.clone();
            let model = model.clone();
            let messages = messages.clone();
            let tools = tools.clone();
            let max_tokens = self.max_tokens;
            let reasoning = req.reasoning;
            std::thread::spawn(move || {
                let notice_tx = tx.clone();
                let result = provider::with_pacing_notice(move |notice| { let _ = notice_tx.send(StreamUpdate::Event(Event::Warning(notice))); }, || provider::with_request_cancel(worker_cancel.clone(), || -> Result<(), String> {
                    let provider = factory()?;
                    let req = Request {
                        model: &model,
                        system: &system,
                        messages: &messages,
                        tools: &tools,
                        max_tokens,
                        reasoning,
                    };
                    let mut attempt = 0u8;
                    let stream = loop {
                        if worker_cancel.load(std::sync::atomic::Ordering::Acquire) {
                            return Ok(());
                        }
                        match provider.stream(&req) {
                            Ok(stream) => break stream,
                            Err(e)
                                if provider::is_transient(&e)
                                    && attempt + 1 < provider::RETRY_ATTEMPTS =>
                            {
                                let delay_s = provider::RETRY_DELAYS
                                    .get(attempt as usize)
                                    .copied()
                                    .unwrap_or(0);
                                attempt += 1;
                                if tx
                                    .send(StreamUpdate::Retry {
                                        attempt,
                                        delay_s,
                                        reason: e.to_string(),
                                    })
                                    .is_err()
                                {
                                    return Ok(());
                                }
                                let deadline = std::time::Instant::now()
                                    + std::time::Duration::from_secs(delay_s);
                                while std::time::Instant::now() < deadline {
                                    if worker_cancel.load(std::sync::atomic::Ordering::Acquire) {
                                        return Ok(());
                                    }
                                    std::thread::sleep(std::time::Duration::from_millis(30));
                                }
                            }
                            Err(e) => return Err(e.to_string()),
                        }
                    };
                    if tx.send(StreamUpdate::Pricing(provider.model().to_owned(), provider.rate_card())).is_err() { return Ok(()) }
                    let mut completed = false;
                    for event in stream {
                        if matches!(event, Event::Done) {
                            completed = true;
                            break;
                        }
                        if worker_cancel.load(std::sync::atomic::Ordering::Acquire)
                            || tx.send(StreamUpdate::Event(event)).is_err()
                        {
                            return Ok(());
                        }
                    }
                    // Dropping the stream commits metered usage before the UI observes Done.
                    if completed {
                        let _ = tx.send(StreamUpdate::Event(Event::Done));
                    }
                    Ok(())
                }));
                if let Err(e) = result {
                    let _ = tx.send(StreamUpdate::Event(Event::Error(e)));
                }
            });
            Some(StreamWorker {
                receiver: rx,
                cancel,
            })
        } else {
            None
        };
        let mut stream = if receiver.is_none() {
            Some(provider::stream_with_retry(
                self.provider,
                &req,
                provider::RETRY_ATTEMPTS,
                &provider::RETRY_DELAYS,
                |attempt, delay, why| {
                    trace.retry();
                    // One durable line per retry: attempt number, the backoff
                    // actually applied, and the caller-safe cause. Diagnostics
                    // only — a failed write never fails the turn.
                    let _ = crate::attempt_log::append(
                        &attempt_log,
                        attempt,
                        delay,
                        why,
                        request_anchor,
                        crate::transport::request_id(why),
                    );
                    if let Some(live) = self.live.as_mut() {
                        live.retry(attempt as u32, delay);
                    }
                    self.emit(Progress::Retrying {
                        attempt,
                        delay_s: delay,
                        reason: why.to_string(),
                    });
                },
            )?)
        } else {
            None
        };
        if receiver.is_none() { trace.serving(self.provider.model(), self.provider.rate_card()); }
        let mut out = TurnOutput {
            text: String::new(),
            tool_calls: Vec::new(),
            usage: None,
            last_input: None,
            warnings: Vec::new(),
        };

        loop {
            let ev = if let Some(rx) = &receiver {
                match rx
                    .receiver
                    .recv_timeout(std::time::Duration::from_millis(40))
                {
                    Ok(StreamUpdate::Event(ev)) => Some(ev),
                    Ok(StreamUpdate::Pricing(model, rate)) => { trace.serving(&model, rate); None }
                    Ok(StreamUpdate::Retry {
                        attempt,
                        delay_s,
                        reason,
                    }) => {
                        trace.retry();
                        // Same durable lineage as the in-process retry loop:
                        // the streaming worker reports the attempt back here,
                        // which is the only place that still has the session.
                        let _ = crate::attempt_log::append(
                            &attempt_log,
                            attempt,
                            delay_s,
                            &reason,
                            request_anchor,
                            crate::transport::request_id(&reason),
                        );
                        if let Some(live) = self.live.as_mut() {
                            live.retry(attempt as u32, delay_s);
                        }
                        self.emit(Progress::Retrying {
                            attempt,
                            delay_s,
                            reason,
                        });
                        None
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
                    Err(_) => break,
                }
            } else {
                match stream.as_mut().and_then(|s| s.next()) {
                    Some(ev) => Some(ev),
                    None => break,
                }
            };
            // Drain input on every event: streaming is the one place the loop
            // is busy but not blocked, so it is the right place to notice a
            // steer without spinning when there is nothing to read.
            pump(&mut self.steer);
            if self.steer.as_deref().is_some_and(|text| !text.is_empty()) {
                // Chat Completions cannot accept input into this HTTP request.
                // Preserve received prose and resume from history with the
                // correction; incomplete/unexecuted tool calls must not run.
                out.tool_calls.clear();
                return Ok(out);
            }
            if self.steer.as_deref() == Some("") {
                if !out.text.is_empty() {
                    self.session.append(Record::Assistant(out.text.clone()));
                }
                self.session.append(Record::Meta("Response cancelled; streamed output was preserved and pending tools were not executed.".into()));
                return Ok(out);
            }
            let Some(ev) = ev else { continue };
            trace.event(&ev);
            match ev {
                Event::Text(t) => {
                    // Emit before accumulating so the UI can paint the delta
                    // as it arrives rather than after the whole turn.
                    if let Some(live) = self.live.as_mut() {
                        live.text(&t);
                    }
                    self.emit(Progress::Text(&t));
                    out.text.push_str(&t);
                }
                // Reasoning is captured but not rendered by default: it is
                // noise in a terminal and costs context to replay.
                Event::Reasoning(t) => {
                    self.session.append(Record::Reasoning(t.clone()));
                    self.emit(Progress::Reasoning(&t));
                }
                Event::ToolCall {
                    id,
                    name,
                    arguments,
                } => out.tool_calls.push(ToolCall {
                    id,
                    name,
                    arguments,
                }),
                Event::Usage(u) => {
                    out.last_input = Some(u.prompt_total());
                    out.usage = Some(u);
                }
                Event::Warning(w) => {
                    let w = crate::live::sanitize_provider_detail(&w);
                    self.emit(Progress::ProviderNotice(&w));
                    out.warnings.push(w);
                }
                Event::Error(e) => {
                    let e = crate::live::sanitize_provider_detail(&e);
                    if !out.text.is_empty() {
                        self.session.append(Record::Assistant(out.text.clone()));
                    }
                    if !out.text.trim().is_empty() || !out.tool_calls.is_empty() {
                        // A Meta record is only honest when something survived;
                        // a bare setup refusal (an overflow, a bad key) leaves
                        // no partial response to preserve.
                        self.session.append(Record::Meta(
                            "Partial response preserved; provider stream failed before completion."
                                .into(),
                        ));
                    }
                    self.emit(Progress::ProviderFailure(&e));
                    return Err(provider::Error::Transport(e));
                }
                Event::Done => break,
            }
        }
        let mut identities = std::collections::HashSet::new();
        if out.tool_calls.iter().any(|call| {
            call.id.is_empty() || call.name.is_empty() || !identities.insert(call.id.as_str())
        }) {
            return Err(provider::Error::Transport(
                "provider returned missing or duplicate tool call identity; no tools executed"
                    .into(),
            ));
        }
        if out.text.trim().is_empty() && out.tool_calls.is_empty() {
            trace.outcome("empty response: no text or tool calls");
            self.emit(Progress::ProviderFailure(
                "provider returned no text or tool calls",
            ));
            return Err(provider::Error::Transport(
                "provider returned no text or tool calls; check /status and /login".into(),
            ));
        }
        Ok(out)
    }

    /// Run a full turn: stream, execute tools, repeat until the model stops
    /// asking for them, the round budget runs out, or the user steers.
    ///
    /// `steer` receives a correction the user typed mid-turn, and `pump` is
    /// called repeatedly while waiting so the caller can drain pending input.
    /// Without `pump` the loop blocks and a steer can never arrive, because the
    /// key event just sits in the channel while we are busy.
    pub fn run_turn(
        &mut self,
        mut pump: impl FnMut(&mut Option<String>),
    ) -> Result<(TurnOutput, Stop), provider::Error> {
        self.session.validate_writes();
        if let Some(error) = self.session.write_error() {
            return Err(provider::Error::Config(format!(
                "session log is not writable; turn refused: {error}"
            )));
        }
        // Freeze completed results before recovery starts. Reuse is a one-time
        // replay of the interrupted batch, never a cache for newly run tools.
        let mut recovery_results = Vec::new();
        if self.recovering {
            let start = self.session.records.iter()
                .rposition(|r| matches!(r, Record::User(_))).unwrap_or(0);
            let completed: std::collections::HashMap<_, _> = self.session.records[start..].iter()
                .filter_map(|record| match record {
                    Record::ToolResult { call_id, body, .. } => Some((call_id.as_str(), body)),
                    _ => None,
                }).collect();
            for record in self.session.records[start..].iter().rev() {
                if let Record::ToolCall { id, name, arguments } = record
                    && let Ok(arguments) = serde_json::from_str::<serde_json::Value>(arguments)
                    && let Some(body) = completed.get(id.as_str())
                {
                    recovery_results.push((name.clone(), arguments, (*body).clone()));
                }
            }
        }
        self.extensions.clear_scope();
        crate::reminders::apply(self.session).map_err(provider::Error::Config)?;
        let mut last = TurnOutput {
            text: String::new(),
            tool_calls: Vec::new(),
            usage: None,
            last_input: None,
            warnings: Vec::new(),
        };

        if let Some(live) = self.live.as_mut() {
            let turn = self.session.raw_turns().unwrap_or_else(|_| {
                self.session
                    .records
                    .iter()
                    .filter(|r| matches!(r, Record::User(_)))
                    .count()
            }) as u64;
            live.begin_turn(turn);
        }
        self.overflow_recovered = false;
        self.empty_recovered = false;
        self.loop_window.clear();
        let mut repairs = 0;
        let mut repair_tools = None;
        for round in 0..self.max_tool_rounds {
            crate::team::record_notices(self.session).map_err(provider::Error::Config)?;
            let out = match self.turn(&mut pump) {
                Ok(out) => out,
                // A refusal that names an overflow can be recovered once: the
                // harness caused it by under-estimating, and the right answer
                // is to truncate — never to re-send the history that just did
                // not fit (the Crush #3620 cascade). Recovery only fires when
                // something actually got freed; otherwise the original error
                // is the honest report.
                Err(e)
                    if !self.overflow_recovered && (compact::looks_like_overflow(&e.to_string()) || e.to_string().contains("teammate request context exceeds remaining")) =>
                {
                    self.overflow_recovered = true;
                    if !self.force_compact() {
                        return Err(e);
                    }
                    self.turn(&mut pump)?
                }
                Err(e)
                    if !self.empty_recovered
                        && e.to_string()
                            .contains("reasoning arrived without an answer")
                        || !self.empty_recovered && e.to_string().contains("output limit reached before an answer") =>
                {
                    self.empty_recovered = true;
                    let detail = "No answer received; requesting an answer with reduced reasoning. Completed tools are preserved.".to_string();
                    self.session.append(Record::Meta(detail.clone()));
                    self.emit(Progress::ProviderNotice(&detail));
                    self.empty_correction = true;
                    let recovered = self.turn(&mut pump);
                    self.empty_correction = false;
                    recovered?
                }
                Err(e) => return Err(e),
            };
            // A successful response advances the task. Recovery allowance is
            // per response chain, so a later tool round can recover as well.
            self.empty_recovered = false;
            self.overflow_recovered = false;
            if self.steer.as_deref() == Some("") {
                return Ok((out, Stop::Cancelled));
            }
            last.text.clone_from(&out.text);
            last.last_input = out.last_input;
            if let Some(u) = out.usage {
                let total = last
                    .usage
                    .get_or_insert_with(crate::provider::Usage::default);
                total.input = total.input.saturating_add(u.input);
                total.output = total.output.saturating_add(u.output);
                total.cache_read = total.cache_read.saturating_add(u.cache_read);
                total.cache_write = total.cache_write.saturating_add(u.cache_write);
            }
            last.warnings.clone_from(&out.warnings);
            last.tool_calls = out.tool_calls.clone();

            if !out.text.trim().is_empty() {
                self.session.append(Record::Assistant(out.text.clone()));
            }
            for w in &out.warnings {
                self.session
                    .append(Record::System(format!("provider: {w}")));
            }

            // Give mods a look at the finished turn.
            let model = self.provider.model().to_string();
            let current = self.session.messages();
            let ctx = TurnContext {
                messages: &current,
                model: &model,
                turn_index: round,
            };
            if !self.read_only {
                for m in self.mods {
                    m.on_turn_end(&ctx, &out.text);
                }
                self.extensions.turn_end(&out.text, self.prefix_bytes);
                // An extension may have queued durable entries while
                // handling the turn-end event; land them now rather
                // than at the next tool call, which may never come.
                for (kind, data) in self.extensions.drain_custom() {
                    self.session.append(Record::Custom { kind, data });
                }
            }

            // A steer typed while this turn was running lands here. Checking
            // *before* the tools run is the whole point: the user is objecting
            // to what the model is about to do, so the correction has to arrive
            // before the command executes rather than after it finishes.
            if let Some(steer) = self.steer.take() {
                // Everything still running in this turn stops mattering;
                // extension calls watching the shared token stop waiting.
                self.turn_cancel.cancel();
                if steer.is_empty() { return Ok((last, Stop::Cancelled)); }
                self.session.append(Record::User(format!(
                    "Follow-up received before the requested actions: {steer}"
                )));
                self.emit(Progress::Steered(&steer));
                return Ok((last, Stop::Steered));
            }

            if !out.wants_tools() {
                self.schedule_warm();
                return Ok((last, Stop::Finished));
            }

            // Validation is separate from execution errors: reject a malformed
            // batch before any side effect, resolve every call, then re-ask.
            let schemas = repair_tools.get_or_insert_with(|| {
                let mut tools = self.tool_set();
                for (name, description, schema) in self.extensions.declared_tools().0 {
                    if !tools.iter().any(|(n, _, _)| n == &name) {
                        tools.push((name, description, schema));
                    }
                }
                tools
                    .into_iter()
                    .map(|(n, _, schema)| {
                        (
                            n,
                            serde_json::from_str(&schema).unwrap_or(serde_json::Value::Bool(false)),
                        )
                    })
                    .collect::<Vec<_>>()
            });
            let errors: Vec<_> = out
                .tool_calls
                .iter()
                .map(|call| crate::tool_repair::validate(call, schemas).err())
                .collect();
            if errors.iter().any(Option::is_some) {
                repairs += 1;
                let exhausted = repairs > crate::tool_repair::MAX_REPAIRS;
                let notice = if exhausted {
                    "Tool repair exhausted; rejected batch did not execute".to_string()
                } else {
                    format!(
                        "Tool repair {repairs}/{} · rejected batch did not execute",
                        crate::tool_repair::MAX_REPAIRS
                    )
                };
                self.emit(Progress::ProviderNotice(&notice));
                for (call, error) in out.tool_calls.iter().zip(errors) {
                    self.session
                        .append_checked(&[Record::ToolCall {
                            id: call.id.clone(),
                            name: call.name.clone(),
                            arguments: call.arguments.clone(),
                        }])
                        .map_err(|e| {
                            provider::Error::Config(format!(
                                "cannot record rejected tool call: {e}"
                            ))
                        })?;
                    self.emit_tool_start(call);
                    let error = error.unwrap_or_else(|| "another call in this batch was invalid; resubmit this call with the corrected batch".into());
                    self.finish_tool(
                        call,
                        tools::Outcome::err(crate::tool_repair::feedback(
                            &error,
                            repairs.min(crate::tool_repair::MAX_REPAIRS),
                        )),
                    );
                }
                self.session.append(Record::Meta(notice));
                if exhausted {
                    return Err(provider::Error::Config("tool call repair exhausted after two correction requests; no calls in the rejected batch executed; use /retry or switch model".into()));
                }
                continue;
            }

            // Independent calls run together. Correctness comes from the
            // plan, not from hope: same-path writes are serialized, destructive
            // commands run alone, and results are reassembled in call order so
            // the transcript is identical to a sequential run.
            let mut executable = Vec::new();
            for call in &out.tool_calls {
                let cached = if recovery_results.is_empty() { None } else {
                    serde_json::from_str::<serde_json::Value>(&call.arguments).ok()
                        .and_then(|arguments| recovery_results.iter()
                            .position(|(name, value, _)| name == &call.name && value == &arguments))
                        .map(|index| recovery_results.remove(index).2)
                };
                if let Err(error) = self.session.append_checked(&[Record::ToolCall {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                }]) {
                    return Err(provider::Error::Config(format!(
                        "cannot record tool intent; action was not run: {error}"
                    )));
                }
                if let Some(body) = cached {
                    self.session.append(Record::ToolResult {
                        name: call.name.clone(),
                        call_id: call.id.clone(),
                        body,
                    });
                    self.session.append(Record::Meta(format!(
                        "Reused completed {} result during recovery; action was not run again.",
                        call.name
                    )));
                } else {
                    let mut executable_call = call.clone();
                    // Execute the same parsed object that validation examined.
                    // Canonical keys also resolve escaped/duplicate JSON keys
                    // consistently; raw arguments remain in the intent record.
                    executable_call.arguments = crate::tool_repair::wire_arguments(&call.arguments);
                    executable.push(executable_call);
                }
            }
            // Beyond this first valid batch, identical calls are intentional
            // fresh reads/checks and must observe current workspace state.
            recovery_results.clear();
            self.recovering = false;
            self.run_tool_batch(&executable, &mut |slot| pump(slot));
            if let Some(steer) = self.steer.take() {
                // Still-running extension tools (and bash workers) stop
                // waiting; their results report the cancellation honestly.
                self.turn_cancel.cancel();
                for call in &out.tool_calls {
                    if !self
                        .session
                        .records
                        .iter()
                        .any(|r| matches!(r,Record::ToolResult{call_id,..} if call_id==&call.id))
                    {
                        self.session.append(Record::ToolResult {
                            name: call.name.clone(),
                            call_id: call.id.clone(),
                            body: "Cancelled before execution; no tool result.".into(),
                        });
                    }
                }
                if steer.is_empty() {
                    self.session.append(Record::Meta(
                        "Cancelled: tool execution stopped; remaining tools skipped."
                            .into(),
                    ));
                    return Ok((last, Stop::Cancelled));
                }
                self.session
                    .append(Record::User(format!("Follow-up: {steer}")));
                self.emit(Progress::Steered(&steer));
                return Ok((last, Stop::Steered));
            }

            if round + 1 == self.max_tool_rounds {
                return Ok((last, Stop::ToolBudget));
            }
        }
        Ok((last, Stop::ToolBudget))
    }

    /// Record one call in the loop window and emit the single reminder when
    /// the repeat threshold is crossed. Called at every execution site so
    /// the count matches what actually runs, independent of batching mode.
    fn loop_guard(&mut self, call: &ToolCall) {
        self.tool_call_index = self.tool_call_index.saturating_add(1);
        if self.loop_window.record(&call.name, &call.arguments) {
            let reminder = crate::loop_detect::reminder(&call.name);
            self.emit(Progress::ProviderNotice(&reminder));
            self.session.append(Record::Meta(reminder));
        }
    }

    /// Execute a batch of tool calls, fanning out where it is safe.
    ///
    /// Results are re-inserted in call order, so the model sees the same
    /// transcript it would have seen one call at a time. A `bash` call still
    /// runs on a worker thread so the UI stays live and a steer can land.
    fn run_tool_batch(&mut self, calls: &[ToolCall], pump: &mut impl FnMut(&mut Option<String>)) {
        if self.read_only {
            for call in calls {
                pump(&mut self.steer);
                if self.steer.is_some() {
                    break;
                }
                let outcome = if call.name == "inspect" {
                    crate::inspect::run(call, self.workspace)
                } else {
                    tools::Outcome::err(
                        "Plan mode blocks execution: only read-only inspect is available",
                    )
                };
                self.finish_tool(call, outcome);
            }
            return;
        }
        if calls.is_empty() {
            return;
        }

        if calls.iter().any(|call|crate::files::is_tool(&call.name))
            || !self.mods.is_empty()
            || !self.extensions.is_empty()
            || calls
                .iter()
                .any(|c| c.name == "bash" && self.read_before_write_refusal(c).is_some())
        {
            for call in calls {
                if self.steer.is_some() {
                    break;
                }
                self.loop_guard(call);
                self.run_tool_pumped_or_inline(call, pump);
            }
            return;
        }

        let bash_indices: Vec<usize> = calls
            .iter()
            .enumerate()
            .filter(|(_, c)| c.name == "bash")
            .map(|(i, _)| i)
            .collect();

        if calls.len() == 1 || bash_indices.len() == calls.len() {
            // Nothing to overlap: run in order.
            for call in calls {
                if self.steer.is_some() {
                    break;
                }
                self.loop_guard(call);
                self.run_tool_pumped_or_inline(call, pump);
            }
            return;
        }

        // Plan only the bash calls; the rest are microseconds and run inline.
        let cmds: Vec<(usize, String)> = bash_indices
            .iter()
            .filter_map(|&i| {
                calls
                    .get(i)
                    .and_then(|c| c.arg_str("command"))
                    .map(|cmd| (i, cmd))
            })
            .collect();
        let mut plan = concurrency::plan_bash(&cmds, self.workspace);

        // Chips open up front for the fanned-out commands: they are all
        // issued before any of them can finish, and the results are
        // reported in call order below.
        for call in calls.iter().filter(|c| c.name == "bash") {
            self.emit_tool_start(call);
        }

        // An extension can declare that its own tool must not overlap, which is
        // the only way it can protect shared state without knowing about the
        // built-in tools. Everything else keeps the class we inferred.
        for call in calls.iter().filter(|c| c.name != "bash") {
            if self.extensions.tool_concurrency(&call.name)
                == Some(ext::protocol::ExtConcurrency::Serial)
            {
                plan = plan.with_serial(calls.iter().position(|c| std::ptr::eq(c, call)));
            }
        }

        if !plan.needs_parallelism() {
            for call in calls {
                if self.steer.is_some() {
                    break;
                }
                self.loop_guard(call);
                self.run_tool_pumped_or_inline(call, pump);
            }
            return;
        }

        let workspace = self.workspace.to_path_buf();
        let by_index: std::collections::HashMap<usize, ToolCall> = calls
            .iter()
            .enumerate()
            .map(|(i, c)| (i, c.clone()))
            .collect();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let owner = self.session.id.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let approvals: std::collections::HashMap<_, _> = calls
            .iter()
            .enumerate()
            .map(|(i, call)| (i, self.command_approvals(call)))
            .collect();
        std::thread::spawn(move || {
            let results = concurrency::execute(&plan, concurrency::MAX_PARALLEL, |i| {
                let inner = by_index.get(&i).unwrap();
                if worker_cancel.load(std::sync::atomic::Ordering::Acquire) {
                    return tools::Outcome::err("Cancelled before execution");
                }
                tools::bash_owned(
                    inner,
                    &workspace,
                    &worker_cancel,
                    &approvals[&i],
                    Some(&owner),
                )
            });
            let _ = tx.send(results);
        });
        let results = loop {
            match rx.recv_timeout(std::time::Duration::from_millis(20)) {
                Ok(r) => break r,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    pump(&mut self.steer);
                    if self.steer.is_some() {
                        cancel.store(true, std::sync::atomic::Ordering::Release);
                    }
                }
                Err(_) => return,
            }
        };

        // Report in call order. `_` is the batch index; the results are already
        // sorted, so this is the same order the model would have seen inline.
        let mut by_batch: std::collections::HashMap<usize, tools::Outcome> =
            results.into_iter().collect();
        for (i, call) in calls.iter().enumerate() {
            if let Some(out) = by_batch.remove(&i) {
                self.apply_bash_result(call, out);
            } else if self.steer.is_none() {
                self.loop_guard(call);
                self.run_tool_pumped_or_inline(call, pump);
            }
        }
    }

    /// Run one call, on a worker thread when it is a bash command that could
    /// block, inline otherwise.
    fn run_tool_pumped_or_inline(
        &mut self,
        call: &ToolCall,
        pump: &mut impl FnMut(&mut Option<String>),
    ) {
        if call.name == "view_image" && !self.image_input {
            self.finish_tool(call, tools::Outcome::err("Image input is disabled for this model"));
        } else if crate::files::is_tool(&call.name) {
            self.run_file_pumped(call,pump);
        } else if call.name == "bash" || self.extensions.has_tool(&call.name) {
            self.run_tool_pumped(call, pump);
        } else {
            self.run_tool(call);
        }
    }

    /// Native IO runs off the input thread. Preserve results even when steered:
    /// atomic commits must never become unobserved background mutations.
    fn run_file_pumped(&mut self,call:&ToolCall,pump:&mut impl FnMut(&mut Option<String>)) {
        self.emit_tool_start(call);
        if let Some(verdict)=self.pre_tool_verdict(call) {
            match verdict {
                crate::hooks_shell::Verdict::Block(reason)=>return self.finish_tool(call,tools::Outcome::err(reason)),
                crate::hooks_shell::Verdict::Halt(reason)=>{
                    self.steer = Some(String::new());
                    self.session.append(Record::Meta("error: pre-tool-use hook cancelled the rest of the turn".into()));
                    return self.finish_tool(call,tools::Outcome::err(reason));
                },
                crate::hooks_shell::Verdict::Warn(warning)=>self.session.append(Record::Meta(format!("hook warning: {warning}"))),
                crate::hooks_shell::Verdict::Allow=>{},
            }
        }
        if let Some(reason)=self.authorization_refusal(call) {return self.finish_tool(call,tools::Outcome::err(reason));}
        if let Some(reason)=self.read_before_write_refusal(call) {return self.finish_tool(call,tools::Outcome::err(reason));}
        let workspace=self.workspace.to_path_buf();let worker=call.clone();
        let (tx,rx)=std::sync::mpsc::channel();
        let cancel=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_cancel=cancel.clone();
        std::thread::spawn(move || {let _=tx.send(crate::files::tool_cancellable(&worker,&workspace,&worker_cancel));});
        let outcome=loop {
            match rx.recv_timeout(std::time::Duration::from_millis(10)) {
                Ok(result)=>break result,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)=>{
                    pump(&mut self.steer);
                    if self.steer.is_some() {cancel.store(true,std::sync::atomic::Ordering::Release);}
                },
                Err(_)=>break tools::Outcome::err("Native file worker failed; re-read before retrying a mutation"),
            }
        };
        if call.name=="read" && outcome.exit_code==Some(0)
            && let Some(path)=call.arg_str("path") {self.reads.mark_at(&self.workspace.join(path),self.tool_call_index);}
        let mut body=outcome.body;
        if matches!(call.name.as_str(), "edit" | "write")
            && let Some(path) = call.arg_str("path")
            && let Some(hint) = self.reads.staleness_hint(&self.workspace.join(path), self.tool_call_index) {
            body.push_str(&format!("\n{hint}"));
        }
        let before=self.state.clone();
        with_tool_bridge(self.workspace,&mut *self.state,&call.id,|bridge| {
            self.extensions.rewrite_tool_result(&call.id,&call.name,&mut body,bridge)
        });
        if let Err(error)=self.persist_state_change(before) {body.push_str(&format!("\n{error}"));}
        self.finish_tool(call,tools::Outcome {body,truncated:outcome.truncated,exit_code:outcome.exit_code,spill:outcome.spill});
        for (kind,data) in self.extensions.drain_custom() {self.session.append(Record::Custom {kind,data});}
    }

    fn authorization_refusal(&mut self, call: &ToolCall) -> Option<String> {
        if let Err(reason) = self
            .extensions
            .authorize(&call.id, &call.name, &call.arguments)
        {
            return Some(reason);
        }
        self.mods.iter().find_map(|m| m.authorize(call).err())
    }

    /// Run a tool while continuing to service input.
    ///
    /// The tool itself is blocking — a `cargo build` can take minutes — so it
    /// runs on a worker thread and the main thread pumps input meanwhile. That
    /// keeps the UI live and lets a steer be observed, though it does not
    /// preempt the command itself: `bash` has its own timeout for that.
    fn run_tool_pumped(&mut self, call: &ToolCall, pump: &mut impl FnMut(&mut Option<String>)) {
        // `bash` and `state` are the only built-ins that can block, and only
        // `bash` is worth a worker thread. Everything else is microseconds.
        if call.name != "bash" && !self.extensions.has_tool(&call.name) {
            self.run_tool(call);
            return;
        }

        // The hook gate runs here for bash (this path skips `run_tool`)
        // and inside `run_tool` for everything else.
        if let Some(verdict) = self.pre_tool_verdict(call) {
            match verdict {
                crate::hooks_shell::Verdict::Block(reason) => {
                    self.session.append(Record::Meta(format!(
                        "hook blocked {name}: {reason}", name = call.name
                    )));
                    return self.finish_tool(
                        call,
                        tools::Outcome::err(format!("blocked by pre-tool-use hook: {reason}")),
                    );
                }
                crate::hooks_shell::Verdict::Halt(reason) => {
                    self.steer = Some(String::new());
                    self.session.append(Record::Meta(format!(
                        "hook halting turn at {name}: {reason}", name = call.name
                    )));
                    self.session.append(Record::Meta(
                        "error: pre-tool-use hook cancelled the rest of the turn".into(),
                    ));
                    return self.finish_tool(
                        call,
                        tools::Outcome::err(format!(
                            "blocked by pre-tool-use hook (turn cancelled): {reason}"
                        )),
                    );
                }
                crate::hooks_shell::Verdict::Warn(warning) => {
                    self.session
                        .append(Record::Meta(format!("hook warning: {warning}")));
                }
                crate::hooks_shell::Verdict::Allow => {}
            }
        }

        if let Some(reason) = self.authorization_refusal(call) {
            self.finish_tool(call, tools::Outcome::err(format!("Blocked: {reason}")));
            return;
        }

        // The chip opens here rather than in `run_tool`, because a bash
        // call on this path never reaches it: it goes straight to a
        // worker thread. Without this the harness's primary tool is
        // invisible in the default view.
        self.emit_tool_start(call);

        if self.extensions.has_tool(&call.name) {
            self.run_ext_observably(call, pump);
            return;
        }
        // Read-before-write is checked here rather than in `run_tool`, because a
        // bash call on this path never passes through `run_tool`: it goes
        // straight to a worker thread. Checking in one place and dispatching
        // from another is how a guard ends up silently not applying.
        if let Some(refusal) = self.read_before_write_refusal(call) {
            self.emit(Progress::BlockedByGuard(&refusal));
            self.finish_tool(call, tools::Outcome::err(refusal));
            return;
        }

        let args = call.arguments.clone();
        let workspace = self.workspace.to_path_buf();
        let (tx, rx) = std::sync::mpsc::channel();
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let flag = std::sync::Arc::clone(&done);
        let approvals = self.command_approvals(call);
        let owner = self.session.id.clone();
        std::thread::spawn(move || {
            let inner = ToolCall {
                id: "worker".into(),
                name: "bash".into(),
                arguments: args,
            };
            let _ = tx.send(tools::bash_owned(
                &inner,
                &workspace,
                &worker_cancel,
                &approvals,
                Some(&owner),
            ));
            flag.store(true, std::sync::atomic::Ordering::Release);
        });

        loop {
            if let Ok(result) = rx.try_recv() {
                self.apply_bash_result(call, result);
                return;
            }
            if done.load(std::sync::atomic::Ordering::Acquire) && rx.try_recv().is_err() {
                return;
            }
            pump(&mut self.steer);
            // The user corrected course while this command was running. Its
            // output is now irrelevant to what they asked for, and feeding it
            // back would push the model toward the plan they just rejected, so
            // the result is dropped and the loop moves on to the correction.
            if self.steer.is_some() {
                cancel.store(true, std::sync::atomic::Ordering::Release);
                // Wait for the owned process group to stop and preserve its result.
                if let Ok(result) = rx.recv_timeout(std::time::Duration::from_secs(3)) {
                    self.apply_bash_result(call, result);
                } else {
                    self.finish_tool(
                        call,
                        tools::Outcome::err(
                            "Cancellation requested; worker did not return before timeout",
                        ),
                    );
                }
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// Keep input live while an extension runs. Scoped borrowing retains the
    /// registry and the actual state even if the worker fails; one channel
    /// carries the result, and interactive requests return to the UI thread.
    fn run_ext_observably(&mut self, call: &ToolCall, pump: &mut impl FnMut(&mut Option<String>)) {
        let scope = crate::ext::CallScope::from_call(&call.id, self.turn_cancel.clone());
        let cancel = self.turn_cancel.clone();
        let keys = self.input_keys;
        let workspace = self.workspace;
        let owner = self.session.id.clone();
        let registry = &mut *self.extensions;
        let state = &mut *self.state;
        let session = &mut *self.session;
        let steer = &mut self.steer;
        type InputRequest = (
            String,
            serde_json::Value,
            std::sync::mpsc::Sender<serde_json::Value>,
        );
        let (inputs_tx, inputs_rx) = std::sync::mpsc::channel::<InputRequest>();
        let (state_tx, state_rx) =
            std::sync::mpsc::channel::<(String, std::sync::mpsc::Sender<String>)>();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let interactive = keys.is_some();
        let mut body = std::thread::scope(|threads| {
            let worker = threads.spawn(move || {
                let mut bridge = |tool: &str, arguments: &str| {
                    if tool == "state" {
                        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
                        if state_tx.send((arguments.to_owned(), reply_tx)).is_err() {
                            return "error: state writer unavailable".into();
                        }
                        return reply_rx
                            .recv()
                            .unwrap_or_else(|_| "error: state writer unavailable".into());
                    }
                    if tool == "bash" {
                        let nested = ToolCall { id: format!("{}-nested", call.id), name: "bash".into(), arguments: arguments.to_owned() };
                        return tools::bash_owned(&nested, workspace, &scope.cancel.shared_flag(), &crate::sandbox::SessionApprovals::default(), Some(&owner)).body;
                    }
                    format!("error: extensions may only call bash or state, not `{tool}`")
                };
                let output = {
                    if interactive {
                        let mut input = |server: &str, params: &serde_json::Value| {
                            let (reply_tx, reply_rx) = std::sync::mpsc::channel();
                            if inputs_tx
                                .send((server.to_owned(), params.clone(), reply_tx))
                                .is_err()
                            {
                                return serde_json::json!({"action":"decline"});
                            }
                            loop {
                                match reply_rx.recv_timeout(std::time::Duration::from_millis(25)) {
                                    Ok(value) => return value,
                                    Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                                        if !scope.cancel.is_cancelled() => {}
                                    _ => return serde_json::json!({"action":"decline"}),
                                }
                            }
                        };
                        registry
                            .run_tool_with_input_scoped(
                                &scope,
                                &call.name,
                                &call.arguments,
                                &mut bridge,
                                &mut input,
                            )
                            .describe(&call.name)
                    } else {
                        registry
                            .run_tool_scoped(&scope, &call.name, &call.arguments, &mut bridge)
                            .describe(&call.name)
                    }
                };
                let _ = done_tx.send(output);
            });
            let output = loop {
                match done_rx.try_recv() {
                    Ok(body) => break body,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        break "error: extension worker stopped without a result".into();
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
                if let Ok((arguments, reply)) = state_rx.try_recv() {
                    let previous = state.clone();
                    let nested = ToolCall {
                        id: format!("{}-nested", call.id),
                        name: "state".into(),
                        arguments,
                    };
                    let mut output = state.apply(&nested).body;
                    if previous.items() != state.items()
                        && let Err(error) = session.append_checked(&[Record::State(state.items().to_vec())])
                    {
                        *state = previous;
                        output = format!("error: nested state was rolled back: {error}");
                    }
                    let _ = reply.send(output);
                }
                if let Ok((server, params, reply)) = inputs_rx.try_recv() {
                    let value = if cancel.is_cancelled() {
                        serde_json::json!({"action":"decline"})
                    } else if let Some(keys) = keys {
                        crate::input::mcp_input_callback(keys)(&server, &params)
                    } else {
                        serde_json::json!({"action":"decline"})
                    };
                    let _ = reply.send(value);
                }
                pump(steer);
                if steer.is_some() {
                    cancel.cancel();
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            };
            if worker.join().is_err() {
                "error: extension worker panicked; partial state retained for persistence".into()
            } else {
                output
            }
        });
        let before = self.state.clone();
        with_tool_bridge(self.workspace, &mut *self.state, &call.id, |bridge| {
            self.extensions
                .rewrite_tool_result(&call.id, &call.name, &mut body, bridge)
        });
        if let Err(error) = self.persist_state_change(before) {
            body = error;
        }
        self.finish_tool(
            call,
            tools::Outcome {
                body,
                truncated: false,
                exit_code: None,
                spill: None,
            },
        );
        for (kind, data) in self.extensions.drain_custom() {
            self.session.append(Record::Custom { kind, data });
        }
    }

    /// Record a bash result produced on a worker thread.
    fn apply_bash_result(&mut self, call: &ToolCall, outcome: tools::Outcome) {
        let state_before = self.state.clone();
        let mut outcome = outcome;
        let command = call
            .arg_str("command")
            .or_else(|| {
                call.arg_raw("job")
                    .and_then(|raw| serde_json::from_str(&raw).ok())
                    .map(|v| crate::jobs::label(&v))
            })
            .unwrap_or_default();
        let mut body = outcome.body.clone();

        // Verification belongs here, not at the call site: this is the single
        // place every bash result passes through, whether it ran inline or on a
        // worker thread.
        let cwd = tools::bash_cwd(call, self.workspace).ok();
        let touched = cwd.as_ref().map(|cwd| verify::written_paths(&command, cwd)).unwrap_or_default();
        for path in touched.iter().take(16) {
            if let Some(hint) = self.reads.staleness_hint(path, self.tool_call_index) {
                body.push_str(&format!("\n{}: {hint}", path.display()));
            }
        }
        if !touched.is_empty() {
            let note = verify::report(&touched, self.workspace, VERIFY_TAIL_LINES);
            if !note.is_empty() {
                body.push_str(&note);
            }
        }
        // Record what the command read, so a later write can be allowed.
        if outcome.exit_code == Some(0) && let Some(cwd) = &cwd {
            self.reads.observe_at(&command, &outcome.body, cwd, self.tool_call_index);
        }

        let rewritten = with_tool_bridge(self.workspace, &mut *self.state, &call.id, |bridge| {
            self.extensions
                .rewrite_tool_result(&call.id, &call.name, &mut body, bridge)
        });
        if let Err(error) = self.persist_state_change(state_before) {
            body.push_str(&format!("\n{error}"));
            outcome.exit_code = None;
        }
        let mut output = ToolOutput::new(body);
        if rewritten {
            output.rewritten = true;
        }
        if !self.read_only {
            for m in self.mods {
                m.on_tool_result(call, &mut output);
            }
        }
        self.session.append(Record::Bash {
            call_id: Some(call.id.clone()),
            command,
            output: output.body.clone(),
            exit_code: outcome.exit_code,
            truncated: outcome.truncated,
            // Spill paths carry their content address when one exists:
            // a later session re-reads through `blob:<sha256>` (wave D).
            spill: outcome.spill.as_ref().map(|p| {
                crate::tools::blob_reference_for(p).unwrap_or_else(|| p.display().to_string())
            }),
        });
        self.session.append(Record::ToolResult {
            name: call.name.clone(),
            call_id: call.id.clone(),
            body: output.body.clone(),
        });
        // The chip closes here because this is the funnel every bash
        // result passes through — inline, pumped, or fanned out.
        self.emit(Progress::ToolEnd {
            call_id: &call.id,
            summary: summarize_output(&output.body),
            output: &output.body,
        });
    }

    fn persist_state_change(&mut self, before: State) -> Result<(), String> {
        if before.items() == self.state.items() {
            return Ok(());
        }
        if let Err(error) = self.session.append_checked(&[Record::State(self.state.items().to_vec())]) {
            *self.state = before;
            return Err(format!("error: state update was not persisted and was rolled back: {error}"));
        }
        Ok(())
    }

    /// Refuse a write to a file the model has not read.
    ///
    /// A `sed -i` against an unseen file succeeds silently, and the damage only
    /// surfaces turns later at build time, when the cause is no longer obvious.
    /// Creating a new file is never blocked, so this only ever gets in the way
    /// of a genuinely blind overwrite.
    fn read_before_write_refusal(&self, call: &ToolCall) -> Option<String> {
        let targets = if matches!(call.name.as_str(),"edit"|"write") {
            if call.arg_raw("preview").as_deref()==Some("true") {return None;}
            vec![self.workspace.join(call.arg_str("path")?)]
        } else {
            let command=call.arg_str("command")?;
            verify::written_paths(&command,&tools::bash_cwd(call, self.workspace).ok()?)
        };
        let blocked = self.reads.blocked(&targets);
        if blocked.is_empty() {
            return None;
        }
        let names: Vec<String> = blocked.iter().map(|p| p.display().to_string()).collect();
        Some(format!(
            "refused: {} has not been read. Read it first (read, or rg with the path), \
             then retry. Overwriting a file you have not looked at is how changes get lost.",
            names.join(", ")
        ))
    }

    /// Execute one tool call, letting mods veto or rewrite the result.
    /// Shared hook gate: Some(verdict) when a hook is configured and
    /// read-only mode is off, None otherwise. Used by both bash's pumped
    /// path and the ordinary `run_tool` path so every tool call sees one
    /// identical gate.
    fn pre_tool_verdict(&self, call: &ToolCall) -> Option<crate::hooks_shell::Verdict> {
        if self.read_only {
            return None;
        }
        let hook = self.pre_tool_hook.as_ref()?;
        let call_json = serde_json::json!({
            "name": call.name,
            "arguments": serde_json::from_str::<serde_json::Value>(&call.arguments)
                .unwrap_or(serde_json::Value::Null),
        })
        .to_string();
        Some(crate::hooks_shell::run(hook, &call_json))
    }

    fn run_tool(&mut self, call: &ToolCall) {
        self.emit_tool_start(call);
        let state_before = self.state.clone();

        // Pre-tool-use hook (wave E): fires before ANY tool execution —
        // built-ins and extension tools alike — outside read-only mode.
        // Its stdout can only block or pass; it never injects tool output.
        if let Some(verdict) = self.pre_tool_verdict(call) {
            match verdict {
                crate::hooks_shell::Verdict::Block(reason) => {
                    self.session.append(Record::Meta(format!(
                        "hook blocked {name}: {reason}", name = call.name
                    )));
                    return self.finish_tool(
                        call,
                        tools::Outcome::err(format!("blocked by pre-tool-use hook: {reason}")),
                    );
                }
                crate::hooks_shell::Verdict::Halt(reason) => {
                    self.steer = Some(String::new());
                    self.session.append(Record::Meta(format!(
                        "hook halting turn at {name}: {reason}", name = call.name
                    )));
                    self.session.append(Record::Meta(
                        "error: pre-tool-use hook cancelled the rest of the turn".into(),
                    ));
                    return self.finish_tool(
                        call,
                        tools::Outcome::err(format!(
                            "blocked by pre-tool-use hook (turn cancelled): {reason}"
                        )),
                    );
                }
                crate::hooks_shell::Verdict::Warn(warning) => {
                    self.session
                        .append(Record::Meta(format!("hook warning: {warning}")));
                }
                crate::hooks_shell::Verdict::Allow => {}
            }
        }

        // Mods may add tools and may block them; core built-ins are always
        // present but a mod can still refuse to run one.
        let known = crate::files::is_tool(&call.name) || call.name == "bash"
            || call.name == "state"
            || (self.checkpoint_tools
                && matches!(
                    call.name.as_str(),
                    "checkpoint" | "rewind" | "wait"
                ))
            || self.extensions.has_tool(&call.name)
            || self
                .mods
                .iter()
                .any(|m| m.tools().iter().any(|(n, _, _)| *n == call.name));

        let mut outcome = tools::Outcome {
            body: String::new(),
            truncated: false,
            exit_code: None,
            spill: None,
        };

        if !known {
            let extra: Vec<String> = self
                .mods
                .iter()
                .flat_map(|m| m.tools())
                .map(|(n, _, _)| n)
                .collect();
            let mut avail = vec!["read".into(),"edit".into(),"write".into(),"view_image".into(),"bash".into(), "state".into()];
            avail.extend(extra);
            outcome.body = format!(
                "error: no tool named `{}`. Available tools: {}.",
                call.name,
                avail.join(", ")
            );
        } else if let Err(denied) = self
            .extensions
            .authorize(&call.id, &call.name, &call.arguments)
        {
            outcome.body = format!("blocked by extension: {denied}");
        } else if let Some(denied) = self.mods.iter().find_map(|m| m.authorize(call).err()) {
            outcome.body = format!("blocked: {denied}");
        } else if call.name == "view_image" && !self.image_input {
            outcome = tools::Outcome::err("Image input is disabled for this model");
        } else if crate::files::is_tool(&call.name) {
            if let Some(refusal)=self.read_before_write_refusal(call) {return self.finish_tool(call,tools::Outcome::err(refusal));}
            outcome=crate::files::tool(call,self.workspace);
            if call.name=="read" && outcome.exit_code==Some(0) && let Some(path)=call.arg_str("path") {self.reads.mark_at(&self.workspace.join(path),self.tool_call_index);}
        } else if call.name == "bash" {
            if let Some(refusal) = self.read_before_write_refusal(call) {
                self.emit(Progress::BlockedByGuard(&refusal));
                return self.finish_tool(call, tools::Outcome::err(refusal));
            }
            let r = tools::bash_owned(
                call,
                self.workspace,
                &std::sync::atomic::AtomicBool::new(false),
                &self.command_approvals(call),
                Some(&self.session.id),
            );

            // Verification happens in apply_bash_result, which every bash
            // result passes through.
            outcome = r;
        } else if self.checkpoint_tools
            && matches!(call.name.as_str(), "checkpoint" | "rewind" | "wait")
        {
            // Model-invoked context tools run through their own module.
            // Wait polls the attention summary rather than the model:
            // the same bounded rows /attention shows, no second taxonomy.
            if call.name == "wait" {
                // Bounded one-shot summaries of the same rows /attention
                // renders: at most a handful, no transcript bodies.
                let rows = crate::attention::targets(self.session)
                    .into_iter()
                    .take(8)
                    .map(|v| {
                        v.get("kind").and_then(|k| k.as_str()).unwrap_or("event").to_string()
                    })
                    .collect::<Vec<_>>();
                let mut poll = move || rows.first().cloned();
                outcome = crate::checkpoint_tools::run(call, self.session, &mut poll);
            } else {
                let mut noop = || None;
                outcome = crate::checkpoint_tools::run(call, self.session, &mut noop);
            }
        } else if call.name == "state" {
            let namespaces = ["guidance", "neuralnets", "capabilities", "items"].iter().filter(|key| call.arg_raw(key).is_some()).count();
            if namespaces > 1 {
                outcome = tools::Outcome::err("Use one state namespace per call");
            } else if let Some(raw) = call.arg_raw("guidance") {
                outcome = match serde_json::from_str(&raw).map_err(|e|e.to_string()).and_then(|value|crate::guidance::request(&value)) { Ok(value)=>tools::Outcome::ok(value.to_string()), Err(error)=>tools::Outcome::err(error) };
            } else if let Some(raw) = call.arg_raw("neuralnets") {
                let source = format!("session:{} tool:{}",self.session.id,call.id);
                outcome = match serde_json::from_str(&raw).map_err(|e|e.to_string()).and_then(|value|crate::neuralnets::request(self.workspace,&value,Some(&source),self.read_only)) { Ok(value)=>tools::Outcome::ok(value.to_string()), Err(error)=>tools::Outcome::err(error) };
            } else if let Some(capabilities)=call.arg_raw("capabilities") {
                outcome=match serde_json::from_str(&capabilities).map_err(|e|e.to_string()).and_then(|value|self.extensions.capabilities(&value)) {
                    Ok(value)=>tools::Outcome::ok(value.to_string()),Err(error)=>tools::Outcome::err(error),
                };
            } else {
                outcome = self.state.apply(call);

            }
        } else if self.extensions.has_tool(&call.name) {
            // The bridge lets an extension call tools on the host while it
            // runs — only `bash` and `state`, the model's own tools.
            //
            // The call runs under a scope the agent loop owns: the token is
            // tripped when the user steers mid-turn, so a slow extension
            // stops waiting instead of outliving the turn it belongs to.
            // The outcome is reported exactly — completed, cooperatively
            // cancelled, killed past its own stop — not flattened into a
            // generic error.
            let scope = crate::ext::CallScope::from_call(&call.id, self.turn_cancel.clone());
            let input_keys = self.input_keys;
            let out = with_tool_bridge(self.workspace, &mut *self.state, &call.id, |bridge| {
                if let Some(keys) = input_keys {
                    let mut host_input = crate::input::mcp_input_callback(keys);
                    self.extensions
                        .run_tool_with_input_scoped(&scope, &call.name, &call.arguments, bridge, &mut host_input)
                        .describe(&call.name)
                } else {
                    self.extensions
                        .run_tool_scoped(&scope, &call.name, &call.arguments, bridge)
                        .describe(&call.name)
                }
            });
            outcome.body = out;
        } else {
            // A tool was declared but nothing answered: report that honestly
            // rather than returning a silent empty success.
            outcome.body = format!("error: `{}` did not return a result", call.name);
        }

        let mut body = outcome.body;
        let rewritten = with_tool_bridge(self.workspace, &mut *self.state, &call.id, |bridge| {
            self.extensions
                .rewrite_tool_result(&call.id, &call.name, &mut body, bridge)
        });
        // A clear is a mutation too. Persist before acknowledging it, and
        // roll back the in-memory notes if the durable append fails.
        if let Err(error) = self.persist_state_change(state_before) {
            body = error;
            outcome.exit_code = None;
        }
        self.finish_tool(
            call,
            tools::Outcome {
                body,
                truncated: outcome.truncated,
                exit_code: outcome.exit_code,
                spill: outcome.spill,
            },
        );
        // Durable entries the extension queued while running (or
        // authorizing, or rewriting) this call land after its
        // result, in causal order.
        for (kind, data) in self.extensions.drain_custom() {
            self.session.append(Record::Custom { kind, data });
        }
        let _ = rewritten;
    }

    /// Record a tool result and notify mods. The single path every tool
    /// outcome takes, including the ones blocked by a guard.
    fn finish_tool(&mut self, call: &ToolCall, outcome: tools::Outcome) {
        if let Some(live) = self.live.as_mut() {
            live.tool_done();
        }
        let mut output = ToolOutput::new(outcome.body);
        if !self.read_only {
            for m in self.mods {
                m.on_tool_result(call, &mut output);
            }
        }

        self.emit(Progress::ToolEnd {
            call_id: &call.id,
            summary: summarize_output(&output.body),
            output: &output.body,
        });

        let display_bash = if call.name == "bash" {
            Some((
                call.arg_str("command").unwrap_or_default(),
                output.body.clone(),
            ))
        } else {
            None
        };

        // Log before the result so the transcript reads in causal order.
        if let Some((cmd, shown)) = display_bash {
            self.session.append(Record::Bash {
                call_id: Some(call.id.clone()),
                command: cmd,
                output: shown,
                exit_code: outcome.exit_code,
                truncated: outcome.truncated,
                spill: outcome.spill.as_ref().map(|p| {
                    crate::tools::blob_reference_for(p).unwrap_or_else(|| p.display().to_string())
                }),
            });
        }
        self.session.append(Record::ToolResult {
            name: call.name.clone(),
            call_id: call.id.clone(),
            body: output.body,
        });
    }

    /// The system prompt to send with a request.
    ///
    /// The last compaction's checkpoint when there is one, else the
    /// freshly assembled prompt. Within a session the two are
    /// identical — the prompt's inputs are fixed at construction —
    /// so the checkpoint only changes behaviour when a session is
    /// resumed after its prompt config changed: the compacted
    /// history keeps the prompt it was made under, until the next
    /// compaction re-captures the current one.
    fn effective_system(&self) -> String {
        if self.read_only {
            return self.system_prompt();
        }
        let system = latest_system_checkpoint(&self.session.history_records())
            .unwrap_or_else(|| self.system_prompt());
        crate::neuralnets::refresh_context(system, self.project_instructions.as_deref())
    }

    /// Assemble the system prompt: base, then mod contributions, then skills.
    fn system_prompt(&self) -> String {
        if self.read_only {
            let preferences = if self.base_prompt == crate::config::BASE_SYSTEM_PROMPT
                || self.base_prompt == crate::inspect::PROMPT
            {
                ""
            } else {
                self.base_prompt.as_str()
            };
            return format!(
                "{}\nUser preferences (planning tools remain read-only): {}\n{}",
                crate::inspect::PROMPT,
                preferences,
                self.project_instructions.as_deref().unwrap_or("")
            );
        }
        let mut p = self.base_prompt.clone();
        let model = self.provider.model().to_string();
        let ctx = TurnContext {
            messages: &[],
            model: &model,
            turn_index: 0,
        };
        for m in self.mods {
            if let Some(extra) = m.system_prompt(&ctx) {
                p.push_str("\n\n");
                p.push_str(&extra);
            }
        }
        // Extension prompt text is fetched once per session, not per request:
        // it is static config in practice and a pipe round-trip per turn would
        // be pure latency.
        let ext_prompt = self.ext_prompt.clone();
        if !ext_prompt.is_empty() {
            p.push_str(&ext_prompt);
        }
        if let Some(c) = &self.project_instructions {
            p.push_str(c);
        }
        if let Some(c) = &self.catalogue {
            p.push_str("\n\n");
            p.push_str(c);
        }
        p
    }

    /// Compact when the conversation approaches the window limit.
    fn ensure_context_fits(&mut self, messages: &[Message]) -> Result<(), provider::Error> {
        let pressure = provider::pressure(self.provider, messages);
        if pressure < compact::COMPACT_AT {
            return Ok(());
        }

        match compact::plan(&self.session.records, self.state, pressure) {
            compact::Action::None => Ok(()),
            compact::Action::Dropped { kept, dropped } => {
                let rebuilt = compact::rebuild(&self.session.records, self.state);
                self.session.records = rebuilt;
                self.emit(Progress::Compacted { kept, dropped });
                let system = self.system_prompt();
                self.session.append(Record::Compacted {
                    kept,
                    dropped,
                    summary: None,
                    tail_start: 0,
                    system: Some(system),
                });
                Ok(())
            }
            compact::Action::NeedsSummary { prompt } => {
                if let Err(error) = self.summarize(&prompt) {
                    compact::fallback(self.session, self.state, &error.to_string())
                        .map_err(provider::Error::Config)?;
                }
                Ok(())
            },
        }
    }

    /// Free context after a provider refused the request as too large.
    ///
    /// Deterministic only: evict oversized tool results anywhere in the
    /// conversation and drop the middle if there is one. Returns whether
    /// anything changed, because a recovery that freed nothing must report
    /// the original error rather than looping.
    fn force_compact(&mut self) -> bool {
        let evicted =
            compact::evict_oversized(&mut self.session.records, compact::SUMMARY_RECORD_CAP);
        let mut changed = evicted > 0;
        if let compact::Action::Dropped { kept, dropped } =
            compact::plan(&self.session.records, self.state, 1.0)
        {
            self.session.records = compact::rebuild(&self.session.records, self.state);
            let system = self.system_prompt();
            self.session.append(Record::Compacted {
                kept,
                dropped,
                summary: None,
                tail_start: 0,
                system: Some(system),
            });
            self.emit(Progress::Compacted { kept, dropped });
            changed = true;
        } else if evicted > 0 {
            // No structural drop happened, so record the eviction as a meta
            // note rather than a compaction epoch: replay must not treat the
            // conversation as truncated when the records are still here.
            self.session.append(Record::Meta(format!(
                "context recovery: {evicted} oversized tool results evicted"
            )));
            self.emit(Progress::Compacted {
                kept: self.session.records.len(),
                dropped: evicted,
            });
        }
        if provider::pressure(self.provider, &self.session.messages()) >= compact::COMPACT_AT {
            changed |= compact::fallback(self.session, self.state, "context overflow").is_ok();
        }
        changed
    }

    /// Count a tier-2 failure; announce it when the breaker opens.
    fn note_summary_failure(&mut self) {
        if self.breaker.record_failure() {
            self.session.append(Record::Meta(
                "compaction: model summarization paused after 3 consecutive failures; \
                 deterministic compaction continues for this session"
                    .into(),
            ));
        }
    }

    /// Tier two: spend an API call to summarize the transcript.
    fn summarize(&mut self, prompt: &str) -> Result<(), provider::Error> {
        // Safety net first: shrinking oversized tool results often frees enough
        // room to skip the paid summarization entirely.
        let pressure = provider::pressure(self.provider, &compact::project(&self.session.records));
        let shrunk = compact::shrink_tool_results(&mut self.session.records, pressure);
        if shrunk > 0 {
            self.emit(Progress::Compacted {
                kept: self.session.records.len(),
                dropped: shrunk,
            });
            let system = self.system_prompt();
            self.session.append(Record::Compacted {
                kept: self.session.records.len(),
                dropped: 0,
                summary: Some(format!("{shrunk} tool outputs shrunk")),
                tail_start: 0,
                system: Some(system),
            });
            let after = provider::pressure(self.provider, &compact::project(&self.session.records));
            if after < compact::COMPACT_AT {
                return Ok(());
            }
        }

        // Paid summarization is off for this session after repeated failures;
        // the deterministic passes above still ran, and they are free.
        if self.breaker.is_open() {
            return Err(provider::Error::Transport("model compaction is paused after repeated failures".into()));
        }

        // Ask for a summary using a bare request with no tools, so the model
        // cannot start calling tools during what should be a read-only turn.
        let tools: Vec<(String, String, String)> = Vec::new();
        let model = self.provider.model().to_string();
        let req = Request {
            model: &model,
            system: "You write precise technical summaries. Follow the instructions exactly.",
            messages: &[Message::User(compact::fit_summary_prompt(self.provider, prompt))],
            tools: &tools,
            max_tokens: compact::summary_output_limit(self.provider),
            reasoning: Some(0),
        };

        let manifest = format!(
            "Compaction summary request\n{}",
            crate::context_audit::capture(&req, None, "", false)
        );
        crate::context_audit::save(self.session, &manifest)
            .map_err(|e| provider::Error::Config(format!("cannot record summary context: {e}")))?;

        let mut trace=crate::requests::Trace::compaction(self.session,&req);
        let stream = match provider::stream_with_retry(
            self.provider,
            &req,
            provider::RETRY_ATTEMPTS,
            &provider::RETRY_DELAYS,
            |_, _, _| {},
        ) {
            Ok(stream) => stream,
            Err(e) => {
                self.note_summary_failure();
                return Err(e);
            }
        };
        let mut summary = String::new();
        for ev in stream {
            trace.event(&ev);
            match ev {
                Event::Text(t) => summary.push_str(&t),
                Event::Error(e) => {
                    self.note_summary_failure();
                    return Err(provider::Error::Transport(e));
                }
                Event::Done => break,
                _ => {}
            }
        }
        let summary = summary.trim().to_string();
        if summary.is_empty() {
            // Leave the conversation alone rather than destroying context on a
            // failed summarization.
            self.note_summary_failure();
            return Err(provider::Error::Transport("provider returned an empty compaction summary".into()));
        }
        self.breaker.record_success();

        let before = self.session.records.len();
        // The surviving tail is the last KEEP_RECENT records; replay needs the
        // index to rebuild this projection instead of reverting to raw history.
        let tail_start = self.session.log_len().saturating_sub(compact::KEEP_RECENT);
        self.session.records = compact::apply_summary(&self.session.records, &summary);
        let kept = self.session.records.len();
        let system = self.system_prompt();
        self.session.append(Record::Compacted {
            kept,
            dropped: before.saturating_sub(kept),
            tail_start,
            summary: Some(summary),
            system: Some(system),
        });
        Ok(())
    }
}

fn latest_system_checkpoint(records: &[Record]) -> Option<String> {
    records
        .iter()
        .rev()
        .find_map(|record| match record {
            Record::Compacted { system, .. } => Some(system.clone()),
            _ => None,
        })
        .flatten()
}

#[cfg(test)]
mod warm_integration {
    use super::*;
    use crate::ext::Registry;
    use crate::provider::Error;
    use crate::warmer::{ProviderFactory, Warmer};
    use glass_mod_api::Message;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn latest_checkpoint_including_none_is_authoritative() {
        let checkpoint = |system| Record::Compacted {
            kept: 0,
            dropped: 0,
            summary: None,
            tail_start: 0,
            system,
        };
        assert_eq!(
            latest_system_checkpoint(&[checkpoint(Some("old".into()))]),
            Some("old".into())
        );
        assert_eq!(
            latest_system_checkpoint(&[checkpoint(Some("old".into())), checkpoint(None)]),
            None
        );
        assert_eq!(
            latest_system_checkpoint(&[
                checkpoint(Some("saved".into())),
                Record::Assistant("tail".into())
            ]),
            Some("saved".into())
        );
    }

    /// Does nothing with input. A named function rather
    /// than a closure so it is generic over the borrow
    /// lifetime, which `run_turn` requires.
    fn no_pump(_: &mut Option<String>) {}

    /// A provider that answers immediately and asks for no
    /// tools, so a turn finishes in one round.
    struct MockTurnProvider;

    #[test]
    fn bash_rewrite_nested_state_is_persisted() {
        let Some(py) = python() else { return };
        let dir = std::env::temp_dir().join(format!("gc-nested-state-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("nested.py");
        std::fs::write(&script, r#"
import json,sys
def send(value):
 print(json.dumps(value),flush=True)
for line in sys.stdin:
 r=json.loads(line); kind=r.get('type'); ident=r.get('id')
 if kind=='hello': send({'id':ident,'protocol':1,'name':'nested-state','version':'1','tools':[]})
 elif kind=='tool_result':
  send({'type':'call_tool','call_id':'nested-state','name':'state','arguments':json.dumps({'items':['from rewrite']})})
  reply=json.loads(sys.stdin.readline())
  assert reply['type']=='call_tool_result'
  send({'id':ident,'body':r.get('body','')})
 elif kind=='goodbye': break
 else: send({'id':ident})
"#).unwrap();
        let mut session = Session::create(&dir).unwrap();
        let mut state = State::new();
        let mods = Vec::new();
        let mut extensions = Registry::load(&[format!("{py} {}", script.display())], &dir, "mock");
        assert_eq!(extensions.len(), 1);
        {
            let mut agent = Agent::new(&MockTurnProvider, &mut session, &mut state, &mods, &mut extensions, &dir);
            agent.apply_bash_result(&ToolCall { id: "bash".into(), name: "bash".into(), arguments: r#"{"command":"true"}"#.into() }, tools::Outcome { body: "[exit 0]\n(no output)".into(), truncated:false, exit_code:Some(0), spill:None });
        }
        assert_eq!(state.items(), &["from rewrite".to_string()]);
        assert!(session.history_records().iter().any(|r| matches!(r, Record::State(items) if items == &["from rewrite".to_string()])));
        drop(extensions);
        drop(session);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn state_clear_is_durable_and_failed_persistence_rolls_back() {
        let dir = std::env::temp_dir().join(format!("gc-state-receipt-{}", std::process::id()));
        let mut session = Session::create(&dir).unwrap();
        let id = session.id.clone();
        let mut state = State::new();
        let mods = Vec::new();
        let mut extensions = Registry::load(&[], &dir, "mock");
        let set = ToolCall { id: "set".into(), name: "state".into(), arguments: r#"{"items":["remember"]}"#.into() };
        let clear = ToolCall { id: "clear".into(), name: "state".into(), arguments: r#"{"items":[]}"#.into() };
        {
            let mut agent = Agent::new(&MockTurnProvider, &mut session, &mut state, &mods, &mut extensions, &dir);
            agent.run_tool(&set);
            agent.run_tool(&clear);
        }
        assert!(state.items().is_empty());
        let states = session.history_records().into_iter().filter_map(|r| match r { Record::State(items) => Some(items), _ => None }).collect::<Vec<_>>();
        assert_eq!(states, vec![vec!["remember".to_string()], vec![]]);
        drop(session);
        let mut readonly = Session::open_readonly(&dir, &id).unwrap();
        {
            let mut agent = Agent::new(&MockTurnProvider, &mut readonly, &mut state, &mods, &mut extensions, &dir);
            agent.run_tool(&set);
        }
        assert!(state.items().is_empty(), "failed persistence must not change notes");
        drop(readonly);
        std::fs::remove_dir_all(dir).unwrap();
    }

    impl Provider for MockTurnProvider {
        fn id(&self) -> &str {
            "mock"
        }
        fn stream(
            &self,
            _req: &Request<'_>,
        ) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
            Ok(Box::new(
                vec![Event::Text("ok".into()), Event::Done].into_iter(),
            ))
        }
        fn context_window(&self) -> u32 {
            200_000
        }
    }

    #[test]
    fn bash_read_guards_and_observation_use_effective_cwd() {
        let dir = std::env::temp_dir().join(format!("gc-cwd-guard-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/file.txt"), "sub contents").unwrap();
        std::fs::write(dir.join("file.txt"), "root contents").unwrap();
        let mut session = Session::create(&dir.join("sessions")).unwrap();
        let mut state = State::new();
        let mut extensions = Registry::load(&[], &dir, "mock");
        let mods = Vec::new();
        let call = |command: &str, cwd: Option<&str>| ToolCall { id:"cwd".into(), name:"bash".into(), arguments:serde_json::json!({"command":command,"cwd":cwd}).to_string() };
        {
            let mut agent = Agent::new(&MockTurnProvider, &mut session, &mut state, &mods, &mut extensions, &dir);
            let write = call("printf changed > file.txt", Some("sub"));
            assert!(agent.read_before_write_refusal(&write).is_some());
            agent.apply_bash_result(&call("cat file.txt", Some("sub")), tools::Outcome::err("permission denied"));
            assert!(agent.read_before_write_refusal(&write).is_some(), "failed reads cannot authorize writes");
            agent.apply_bash_result(&call("cat file.txt", Some("sub")), tools::Outcome::ok("sub contents".into()));
            assert!(agent.read_before_write_refusal(&write).is_none());
            let root_write = ToolCall { id:"root".into(), name:"bash".into(), arguments:r#"{"command":"printf changed > file.txt"}"#.into() };
            assert!(agent.read_before_write_refusal(&root_write).is_some());
        }
        drop(session);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn live_followups_keep_user_role_and_empty_interrupts_cancel() {
        for text in ["[main → teammate] review the patch", ""] {
            let dir = std::env::temp_dir().join(format!("gc-steer-role-{}-{}", std::process::id(), text.len()));
            let mut session = Session::create(&dir).unwrap();
            session.append(Record::User("original task".into()));
            let mut state = State::new();
            let mods = Vec::new();
            let mut extensions = Registry::load(&[], &dir, "mock");
            let stop = {
                let mut agent = Agent::new(&MockTurnProvider, &mut session, &mut state, &mods, &mut extensions, &dir);
                agent.run_turn(|steer| { if steer.is_none() { *steer = Some(text.into()); } }).unwrap().1
            };
            if text.is_empty() { assert_eq!(stop, Stop::Cancelled); }
            else {
                assert_eq!(stop, Stop::Steered);
                assert!(session.messages().iter().any(|m| matches!(m, Message::User(t) if t.contains(text))));
                assert!(!session.messages().iter().any(|m| matches!(m, Message::System(t) if t.contains(text))));
            }
            drop(session);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn async_reasoning_recovery_preserves_effective_budget() {
        struct ReasoningProvider(std::sync::Arc<std::sync::Mutex<Vec<Option<u32>>>>, bool);
        impl Provider for ReasoningProvider {
            fn id(&self) -> &str { "reasoning" }
            fn rate_card(&self) -> Option<crate::pricing::RateCard> {
                self.1.then_some(crate::pricing::RateCard { input:1.0, output:1.0, cache_read:1.0, cache_write:1.0 })
            }
            fn context_window(&self) -> u32 { 200_000 }
            fn stream(&self, request: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
                let mut calls = self.0.lock().unwrap();
                calls.push(request.reasoning);
                let events = if calls.len() == 1 { vec![Event::Error("reasoning arrived without an answer".into())] }
                    else { vec![Event::Text("recovered".into()), Event::Usage(provider::Usage { input:1, output:1, cache_read:0, cache_write:0 }), Event::Done] };
                Ok(Box::new(events.into_iter()))
            }
        }
        let dir = std::env::temp_dir().join(format!("gc-async-reasoning-{}", std::process::id()));
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let provider = ReasoningProvider(calls.clone(), false);
        let mut session = Session::create(&dir).unwrap();
        session.append(Record::User("continue".into()));
        let mut state = State::new();
        let mods = Vec::new();
        let mut extensions = Registry::load(&[], &dir, "reasoning");
        {
            let mut agent = Agent::new(&provider, &mut session, &mut state, &mods, &mut extensions, &dir);
            agent.reasoning = Some(8192);
            let factory_calls = calls.clone();
            agent = agent.with_stream_factory(std::sync::Arc::new(move || Ok(Box::new(ReasoningProvider(factory_calls.clone(), true)))));
            let (output, stop) = agent.run_turn(no_pump).unwrap();
            assert_eq!(output.text, "recovered");
            assert_eq!(stop, Stop::Finished);
        }
        assert_eq!(*calls.lock().unwrap(), vec![Some(8192), Some(0)]);
        let trace = std::fs::read_to_string(session.path.with_extension("requests.jsonl")).unwrap();
        let last: serde_json::Value = serde_json::from_str(trace.lines().last().unwrap()).unwrap();
        assert_eq!(last["cost_usd"], 0.000002);
        assert_eq!(last["model_served"], "reasoning");
        drop(session);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn latest_request_input_is_separate_from_aggregated_tool_turn_usage() {
        struct Rounds(AtomicU64);
        impl Provider for Rounds {
            fn id(&self) -> &str {
                "rounds"
            }
            fn context_window(&self) -> u32 {
                200_000
            }
            fn stream(
                &self,
                _: &Request<'_>,
            ) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
                let first = self.0.fetch_add(1, Ordering::SeqCst) == 0;
                let mut events = if first {
                    vec![Event::ToolCall {
                        id: "call".into(),
                        name: "state".into(),
                        arguments: "{}".into(),
                    }]
                } else {
                    vec![Event::Text("done".into())]
                };
                events.push(Event::Usage(crate::provider::Usage {
                    input: if first { 90_000 } else { 1000 },
                    output: 10,
                    ..Default::default()
                }));
                events.push(Event::Done);
                Ok(Box::new(events.into_iter()))
            }
        }
        let dir = std::env::temp_dir().join(format!("gc-usage-rounds-{}", std::process::id()));
        let mut session = Session::create(&dir).unwrap();
        session.append(Record::User("task".into()));
        let mut state = State::new();
        let mods = Vec::new();
        let mut extensions = Registry::load(&[], &dir, "rounds");
        let provider = Rounds(AtomicU64::new(0));
        let (out, stop) = Agent::new(
            &provider,
            &mut session,
            &mut state,
            &mods,
            &mut extensions,
            &dir,
        )
        .run_turn(no_pump)
        .unwrap();
        assert_eq!(stop, Stop::Finished);
        assert_eq!(out.usage.unwrap().prompt_total(), 91_000);
        assert_eq!(out.last_input, Some(1000));
        assert_eq!(crate::context_audit::reports(&session).len(), 2);
        drop(session);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn counting_factory(calls: Arc<AtomicU64>) -> ProviderFactory {
        Arc::new(move || {
            struct Counting {
                calls: Arc<AtomicU64>,
            }
            impl Provider for Counting {
                fn id(&self) -> &str {
                    "counting"
                }
                fn stream(
                    &self,
                    _req: &Request<'_>,
                ) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
                    self.calls.fetch_add(1, Ordering::SeqCst);
                    Ok(Box::new(vec![Event::Done].into_iter()))
                }
                fn context_window(&self) -> u32 {
                    200_000
                }
            }
            Ok(Box::new(Counting {
                calls: Arc::clone(&calls),
            }))
        })
    }

    /// Path to the real cache-warmer extension, which declares
    /// the policy the Agent reads.
    fn warmer_extension() -> String {
        let root = env!("CARGO_MANIFEST_DIR");
        format!(
            "python3 {}/../../extensions/cache-warmer/cache-warmer.py",
            root
        )
    }

    #[test]
    fn a_finished_turn_schedules_a_cache_refresh() {
        // The extension declares a 4000-byte minimum, so the
        // system prompt has to be large enough to clear it —
        // a tiny prompt must not schedule a refresh, because
        // warming a few hundred bytes is pure cost.
        let big_prompt = "x".repeat(5000);

        let dir = std::env::temp_dir().join(format!("glass-warm-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut session = Session::create(&dir).unwrap();
        let mut state = State::new();
        let no_mods: Vec<Box<dyn Mod_>> = Vec::new();

        let mut extensions = Registry::load(&[warmer_extension()], Path::new("."), "mock");

        let calls = Arc::new(AtomicU64::new(0));
        let warmer = Warmer::new(counting_factory(Arc::clone(&calls)));

        {
            let mut agent = Agent::new(
                &MockTurnProvider,
                &mut session,
                &mut state,
                &no_mods,
                &mut extensions,
                Path::new("."),
            )
            .with_prompt(big_prompt, None, None)
            .with_warmer(&warmer);

            assert_eq!(warmer.scheduled(), 0);
            let result = agent.run_turn(no_pump);
            assert!(result.is_ok(), "turn should finish cleanly");
        }

        // The turn ended with no tool calls, so the harness
        // went idle and a refresh was scheduled for the gap.
        assert_eq!(
            warmer.scheduled(),
            1,
            "a finished turn must schedule exactly one refresh"
        );

        // Wait past the extension's refresh delay only if it
        // was tuned short; with the default 240s we assert on
        // the schedule, not the firing, so do not block here.
        let _ = calls;

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_small_prefix_schedules_no_refresh() {
        // Below the extension's minimum, warming is a net
        // spend for no latency win, so nothing is scheduled.
        let dir = std::env::temp_dir().join(format!("glass-warm-small-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut session = Session::create(&dir).unwrap();
        let mut state = State::new();
        let no_mods: Vec<Box<dyn Mod_>> = Vec::new();
        let mut extensions = Registry::load(&[format!("env GLASS_CACHE_WARMER_MIN_PREFIX_BYTES=16000 {}",warmer_extension())], Path::new("."), "mock");

        let calls = Arc::new(AtomicU64::new(0));
        let warmer = Warmer::new(counting_factory(Arc::clone(&calls)));

        {
            let mut agent = Agent::new(
                &MockTurnProvider,
                &mut session,
                &mut state,
                &no_mods,
                &mut extensions,
                Path::new("."),
            )
            // Short prompt: well under the 16000-byte test minimum.
            .with_prompt("you are glass".into(), None, None)
            .with_warmer(&warmer);

            let result = agent.run_turn(no_pump);
            assert!(result.is_ok());
        }

        assert_eq!(
            warmer.scheduled(),
            0,
            "a small prefix must not schedule a refresh"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_extension_schedules_no_refresh() {
        // With no cache-warmer extension loaded, nothing
        // schedules, which is the default configuration.
        let dir = std::env::temp_dir().join(format!("glass-warm-none-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut session = Session::create(&dir).unwrap();
        let mut state = State::new();
        let no_mods: Vec<Box<dyn Mod_>> = Vec::new();
        let mut extensions = Registry::empty();

        let calls = Arc::new(AtomicU64::new(0));
        let warmer = Warmer::new(counting_factory(Arc::clone(&calls)));

        {
            let mut agent = Agent::new(
                &MockTurnProvider,
                &mut session,
                &mut state,
                &no_mods,
                &mut extensions,
                Path::new("."),
            )
            .with_prompt("x".repeat(5000), None, None)
            .with_warmer(&warmer);

            let result = agent.run_turn(no_pump);
            assert!(result.is_ok());
        }

        assert_eq!(
            warmer.scheduled(),
            0,
            "no extension means no warmer, even on a big prefix"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    struct RecoveryProvider { requests: std::sync::Mutex<Vec<(String, Option<u32>)>> }
    impl Provider for RecoveryProvider {
        fn id(&self) -> &str { "recovery" }
        fn context_window(&self) -> u32 { 200_000 }
        fn stream(&self, req: &Request<'_>) -> Result<Box<dyn Iterator<Item=Event> + '_>, Error> {
            let mut requests = self.requests.lock().unwrap();
            let index = requests.len();
            requests.push((req.system.to_string(), req.reasoning));
            let events = if index == 0 || index == 20 {
                vec![Event::Reasoning("partial thought".into()), Event::Error("reasoning arrived without an answer".into())]
            } else if index <= 30 {
                if index == 1 { assert!(format!("{:?}", req.messages).contains("Keep reasoning brief")); }
                vec![Event::ToolCall { id:format!("state-{index}"), name:"state".into(), arguments:"{}".into() }, Event::Done]
            } else { vec![Event::Text("finished task".into()), Event::Done] };
            Ok(Box::new(events.into_iter()))
        }
    }
    #[test]
    fn recovery_preserves_cached_system_prefix_and_main_exceeds_24_rounds() {
        let dir = std::env::temp_dir().join(format!("gc-recovery-prefix-{}", std::process::id()));
        let mut session = Session::create(&dir).unwrap();
        session.append(Record::User("complete the task".into()));
        let mut state = State::new();
        let mut extensions = Registry::empty();
        let provider = RecoveryProvider { requests: std::sync::Mutex::new(Vec::new()) };
        let mods = Vec::new();
        let mut agent = Agent::new(&provider, &mut session, &mut state, &mods, &mut extensions, &dir);
        let (output, stop) = agent.run_turn(no_pump).unwrap();
        assert_eq!(output.text, "finished task");
        assert_eq!(stop, Stop::Finished);
        let requests = provider.requests.lock().unwrap();
        assert_eq!(requests.len(), 32);
        assert!(requests.iter().all(|(system, _)| system == &requests[0].0));
        assert_eq!(requests[1].1, Some(0));
        assert_eq!(requests[21].1, Some(0));
        drop(agent); drop(session);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A provider that records the tool names each request carried,
    /// so a test can see exactly what the model was offered.
    struct CapturingProvider {
        tools: std::sync::Mutex<Vec<Vec<String>>>,
    }

    impl Provider for CapturingProvider {
        fn id(&self) -> &str {
            "capturing"
        }
        fn stream(&self, req: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
            self.tools
                .lock()
                .unwrap()
                .push(req.tools.iter().map(|(n, _, _)| n.clone()).collect());
            Ok(Box::new(
                vec![Event::Text("ok".into()), Event::Done].into_iter(),
            ))
        }
        fn context_window(&self) -> u32 {
            200_000
        }
    }

    #[test]
    fn poisoned_session_refuses_before_provider_request() {
        let dir = std::env::temp_dir().join(format!("glass-poisoned-agent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut session = Session::create(&dir).unwrap();
        session.append(Record::User("original".into()));
        std::fs::write(&session.path, "{\"k\":\"user\",\"t\":\"external\"}\n").unwrap();
        let provider = CapturingProvider {
            tools: std::sync::Mutex::new(Vec::new()),
        };
        let mut state = State::new();
        let no_mods: Vec<Box<dyn Mod_>> = Vec::new();
        let mut extensions = Registry::empty();
        let mut agent = Agent::new(
            &provider,
            &mut session,
            &mut state,
            &no_mods,
            &mut extensions,
            Path::new("."),
        );
        assert!(agent.run_turn(no_pump).is_err());
        assert!(provider.tools.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A provider with a tiny window, so a short conversation still
    /// trips compaction pressure.
    struct CrampedProvider;

    impl Provider for CrampedProvider {
        fn id(&self) -> &str {
            "cramped"
        }
        fn stream(
            &self,
            _req: &Request<'_>,
        ) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
            Ok(Box::new(
                vec![Event::Text("ok".into()), Event::Done].into_iter(),
            ))
        }
        fn context_window(&self) -> u32 {
            60
        }
    }

    fn python() -> Option<String> {
        for c in ["python3", "python"] {
            if std::process::Command::new(c)
                .arg("--version")
                .output()
                .is_ok_and(|o| o.status.success())
            {
                return Some(c.to_string());
            }
        }
        None
    }

    /// An extension whose manifest declares one tool.
    fn tool_extension() -> String {
        format!(
            r#"
import json, sys
def send(o): sys.stdout.write(json.dumps(o)+"\n"); sys.stdout.flush()
for line in sys.stdin:
    line=line.strip()
    if not line: continue
    r=json.loads(line); t=r.get("type"); i=r.get("id")
    if t=="hello":
        send({{"id":i,"protocol":{},"name":"extra","version":"1",
              "tools":[{{"name":"extra_tool","description":"d","schema":{{"type":"object"}}}}]}})
    elif t=="turn_end": send({{"id":i}})
    elif t=="goodbye": break
"#,
            crate::ext::protocol::PROTOCOL_VERSION
        )
    }

    #[test]
    fn the_declared_tool_list_is_frozen_for_the_session() {
        let Some(py) = python() else { return }; // no interpreter, skip
        let dir = std::env::temp_dir().join(format!("glass-freeze-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let script = dir.join("ext.py");
        std::fs::write(&script, tool_extension()).unwrap();
        let mut extensions = Registry::load(
            &[format!("{py} {}", script.display())],
            Path::new("."),
            "mock",
        );
        assert!(extensions.has_tool("extra_tool"), "the helper must load");

        let capturing = CapturingProvider {
            tools: std::sync::Mutex::new(Vec::new()),
        };
        let mut session = Session::create(&dir).unwrap();
        let mut state = State::new();
        let no_mods: Vec<Box<dyn Mod_>> = Vec::new();

        // Turn one fixes the declared list.
        {
            let mut agent = Agent::new(
                &capturing,
                &mut session,
                &mut state,
                &no_mods,
                &mut extensions,
                Path::new("."),
            )
            .with_prompt("prompt".into(), None, None);
            agent.run_turn(no_pump).unwrap();
        }
        // The extension goes away mid-session. A second agent — the
        // REPL builds one per turn — must keep declaring its tool:
        // the list is part of the cacheable prefix.
        extensions.disable("extra");
        {
            let mut agent = Agent::new(
                &capturing,
                &mut session,
                &mut state,
                &no_mods,
                &mut extensions,
                Path::new("."),
            )
            .with_prompt("prompt".into(), None, None);
            agent.run_turn(no_pump).unwrap();
        }

        let seen = capturing.tools.lock().unwrap().clone();
        assert_eq!(seen.len(), 2, "one request per turn: {seen:?}");
        assert!(
            seen[0].iter().any(|n| n == "extra_tool"),
            "the extension tool is declared on turn one: {:?}",
            seen[0]
        );
        assert!(
            seen[1].iter().any(|n| n == "extra_tool"),
            "the frozen list must survive a mid-session change: {:?}",
            seen[1]
        );
        assert!(
            session
                .records
                .iter()
                .any(|r| matches!(r, Record::Meta(note) if note.starts_with("tool set frozen"))),
            "the change must be noted in the transcript once"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_compaction_epoch_checkpoints_the_system_prompt() {
        let dir = std::env::temp_dir().join(format!("glass-checkpoint-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let mut session = Session::create(&dir).unwrap();
        for i in 0..30 {
            session.append(Record::User(format!("question number {i} with some words")));
            session.append(Record::Assistant(format!(
                "answer number {i} with some words"
            )));
        }
        let mut state = State::new();
        let no_mods: Vec<Box<dyn Mod_>> = Vec::new();
        let mut extensions = Registry::empty();
        let marker = format!("MARKER-{}", std::process::id());
        {
            let mut agent = Agent::new(
                &CrampedProvider,
                &mut session,
                &mut state,
                &no_mods,
                &mut extensions,
                Path::new("."),
            )
            .with_prompt(format!("You are a test assistant. {marker}"), None, None);
            let _ = agent.run_turn(no_pump);
        }
        assert!(
            session.records.iter().any(|r| matches!(
                r,
                Record::Compacted {
                    system: Some(system),
                    ..
                } if system.contains(&marker)
            )),
            "the compaction epoch must checkpoint the system prompt in effect"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Silence the unused-import warning for Message, which the
    // request shape references but this test constructs indirectly.
    #[allow(dead_code)]
    fn _message_type_is_used(_: Option<Message>) {}
}

/// Context-overflow recovery: refuse once, truncate, retry once.
#[cfg(test)]
mod recovery_tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn no_pump(_: &mut Option<String>) {}

    /// Refuses the first request with the configured message, then answers.
    struct RefusesOnce {
        calls: Arc<AtomicUsize>,
        message: &'static str,
    }

    impl Provider for RefusesOnce {
        fn id(&self) -> &str {
            "mock"
        }

        fn stream(
            &self,
            _: &Request<'_>,
        ) -> Result<Box<dyn Iterator<Item = Event> + '_>, provider::Error> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err(provider::Error::Status(400, self.message.into()));
            }
            Ok(Box::new(
                vec![Event::Text("recovered".into()), Event::Done].into_iter(),
            ))
        }

        fn context_window(&self) -> u32 {
            200_000
        }
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("glass-recovery-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn overflow() -> &'static str {
        "prompt is too long: 250000 tokens > 200000 maximum"
    }

    #[test]
    fn proactive_compaction_sends_the_rebuilt_context_on_the_first_request() {
        struct Capture(std::cell::Cell<usize>);
        impl Provider for Capture {
            fn id(&self) -> &str { "mock" }
            fn context_window(&self) -> u32 { 1000 }
            fn stream(&self, req: &Request<'_>) -> Result<Box<dyn Iterator<Item=Event> + '_>, provider::Error> {
                self.0.set(req.messages.len());
                Ok(Box::new(vec![Event::Text("done".into()), Event::Done].into_iter()))
            }
        }
        let dir=scratch("fresh-projection");
        let mut session=Session::create(&dir).unwrap();
        for i in 0..12 {
            session.append(if i%2==0 { Record::User("u".repeat(1000)) } else { Record::Assistant("a".repeat(1000)) });
        }
        let mut state=State::new();
        let mods:Vec<Box<dyn Mod_>>=Vec::new();
        let mut registry=ext::Registry::empty();
        let provider=Capture(std::cell::Cell::new(0));
        let mut agent=Agent::new(&provider,&mut session,&mut state,&mods,&mut registry,Path::new("."));
        agent.turn(no_pump).unwrap();
        assert!(provider.0.get()<12,"request must use the compacted records immediately");
        drop(agent);
        assert!(session.records.iter().any(|r|matches!(r,Record::Compacted{..})));
        let _=std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_overflow_refusal_evicts_the_hog_and_retries_once() {
        let dir = scratch("once");
        let mut session = Session::create(&dir).unwrap();
        session.append(Record::User("do work".into()));
        session.append(Record::ToolResult {
            name: "bash".into(),
            call_id: "c1".into(),
            body: "x".repeat(40_000),
        });
        let mut state = State::new();
        let no_mods: Vec<Box<dyn Mod_>> = Vec::new();
        let mut extensions = ext::Registry::empty();
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = RefusesOnce {
            calls: Arc::clone(&calls),
            message: overflow(),
        };
        {
            let mut agent = Agent::new(
                &provider,
                &mut session,
                &mut state,
                &no_mods,
                &mut extensions,
                Path::new("."),
            );
            let (out, stop) = agent.run_turn(no_pump).expect("recovery must succeed");
            assert!(matches!(stop, Stop::Finished));
            assert_eq!(out.text, "recovered");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2, "one refusal, one retry");
        assert!(
            session.records.iter().any(|r| matches!(
                r,
                Record::ToolResult { body, .. } if body.contains("dropped to recover")
            )),
            "the oversized result must be evicted"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_redacted_teammate_allowance_refusal_compacts_without_repeating_tools() {
        let dir = scratch("teammate-allowance");
        let mut session = Session::create(&dir).unwrap();
        session.append(Record::User("do work".into()));
        session.append(Record::ToolResult {
            name: "bash".into(),
            call_id: "c1".into(),
            body: "x".repeat(40_000),
        });
        let mut state = State::new();
        let no_mods: Vec<Box<dyn Mod_>> = Vec::new();
        let mut extensions = ext::Registry::empty();
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = RefusesOnce {
            calls: Arc::clone(&calls),
            message: "teammate request context exceeds remaining token budget: next_input_estimate=9000, remaining=3000",
        };
        {
            let mut agent = Agent::new(
                &provider,
                &mut session,
                &mut state,
                &no_mods,
                &mut extensions,
                Path::new("."),
            );
            let (out, stop) = agent.run_turn(no_pump).expect("recovery must succeed");
            assert!(matches!(stop, Stop::Finished));
            assert_eq!(out.text, "recovered");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2, "one refusal, one retry");
        assert!(
            session.records.iter().any(|r| matches!(
                r,
                Record::ToolResult { body, .. } if body.contains("dropped to recover")
            )),
            "the oversized result must be evicted"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovery_that_frees_nothing_reports_the_original_error() {
        let dir = scratch("nothing");
        let mut session = Session::create(&dir).unwrap();
        session.append(Record::User("hi".into()));
        session.append(Record::Assistant("hello".into()));
        let mut state = State::new();
        let no_mods: Vec<Box<dyn Mod_>> = Vec::new();
        let mut extensions = ext::Registry::empty();
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = RefusesOnce {
            calls: Arc::clone(&calls),
            message: overflow(),
        };
        {
            let mut agent = Agent::new(
                &provider,
                &mut session,
                &mut state,
                &no_mods,
                &mut extensions,
                Path::new("."),
            );
            let err = match agent.run_turn(no_pump) {
                Err(e) => e,
                Ok(_) => panic!("nothing to free, nothing to recover"),
            };
            assert!(err.to_string().contains("prompt is too long"), "{err}");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "no pointless retry");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_non_overflow_failure_is_not_recovered() {
        let dir = scratch("other");
        let mut session = Session::create(&dir).unwrap();
        session.append(Record::User("hi".into()));
        let mut state = State::new();
        let no_mods: Vec<Box<dyn Mod_>> = Vec::new();
        let mut extensions = ext::Registry::empty();
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = RefusesOnce {
            calls: Arc::clone(&calls),
            message: "invalid request: bad tool schema",
        };
        {
            let mut agent = Agent::new(
                &provider,
                &mut session,
                &mut state,
                &no_mods,
                &mut extensions,
                Path::new("."),
            );
            assert!(agent.run_turn(no_pump).is_err());
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "no recovery for a 400");
        assert!(
            session
                .records
                .iter()
                .all(|r| !matches!(r, Record::Meta(_))),
            "a bare refusal leaves no partial-response meta behind"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The agent loop's per-turn cancel token reaches extension tool calls:
/// a steer mid-turn stops a slow extension from waiting out its exchange,
/// and the transcript reports the stop as a cancellation, not an error.
#[cfg(test)]
mod ext_cancel_tests {
    use super::*;
    use crate::ext::Registry;
    use std::path::Path;

    fn python() -> Option<String> {
        for c in ["python3", "python"] {
            if std::process::Command::new(c)
                .arg("--version")
                .output()
                .is_ok_and(|o| o.status.success())
            {
                return Some(c.to_string());
            }
        }
        None
    }

    #[test]
    fn a_steer_mid_turn_cancels_a_slow_extension_call() {
        let Some(py) = python() else { return };
        let dir = std::env::temp_dir().join(format!("glass-ext-steer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let script = dir.join("ext.py");
        // A tool that answers only after a long sleep.
        std::fs::write(&script, format!(r#"
import json, sys, time
def send(o): sys.stdout.write(json.dumps(o)+"\n"); sys.stdout.flush()
for line in sys.stdin:
    line=line.strip()
    if not line: continue
    r=json.loads(line); t=r.get("type"); i=r.get("id")
    if t=="hello":
        send({{"id":i,"protocol":{},"name":"slow","version":"1",
              "tools":[{{"name":"slow","description":"d","schema":{{"type":"object"}}}}]}})
    elif t=="run_tool":
        time.sleep(30)
        send({{"id":i,"output":"finished"}})
    elif t=="goodbye": break
    else: send({{"id":i,"allow":True}})
"#, crate::ext::protocol::PROTOCOL_VERSION)).unwrap();
        let mut extensions =
            Registry::load(&[format!("{py} {}", script.display())], Path::new("."), "mock");

        let provider = CapturingProviderWithToolCall::default();
        let mut session = Session::create(&dir).unwrap();
        let mut state = State::new();
        let no_mods: Vec<Box<dyn Mod_>> = Vec::new();
        let mut agent = Agent::new(
            &provider,
            &mut session,
            &mut state,
            &no_mods,
            &mut extensions,
            Path::new("."),
        )
        .with_prompt("turn".into(), None, None);
        // The provider asks for the slow tool; the steer arrives while the
        // tool is blocked, so the turn stops waiting for the extension.
        let started = std::time::Instant::now();
        let tool = ToolCall { id: "call_slow".into(), name: "slow".into(), arguments: "{}".into() };
        agent.run_tool_pumped_or_inline(&tool, &mut |steer| {
            if started.elapsed() >= std::time::Duration::from_millis(100) {
                *steer = Some("do something else".into());
            }
        });
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        assert!(agent.extensions.has_tool("slow"), "registry retained after cancellation");
        assert_eq!(agent.session.records.iter().filter(|r| matches!(r, Record::ToolResult {call_id, ..} if call_id == "call_slow")).count(), 1);
        assert!(!agent.session.records.iter().any(|r| matches!(r, Record::Bash {..})), "extension results are not shell results");

        // The token carried into the extension host is tripped, so the
        // extension's exchange is abandoned rather than waited out.
        assert!(
            agent.turn_cancel.is_cancelled(),
            "the turn cancel token must be tripped by the steer"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Provider that answers the first request with one tool call, then
    /// stops; enough for the extension dispatch path to run.
    #[derive(Default)]
    struct CapturingProviderWithToolCall {
        calls: std::sync::atomic::AtomicUsize,
    }

    impl Provider for CapturingProviderWithToolCall {
        fn id(&self) -> &str {
            "toolcall-mock"
        }
        fn stream(
            &self,
            _req: &Request<'_>,
        ) -> Result<Box<dyn Iterator<Item = Event> + '_>, crate::provider::Error> {
            let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if n == 0 {
                return Ok(Box::new(
                    vec![
                        Event::ToolCall {
                            id: "call_1".into(),
                            name: "slow".into(),
                            arguments: "{}".into(),
                        },
                        Event::Done,
                    ]
                    .into_iter(),
                ));
            }
            Ok(Box::new(vec![Event::Text("ok".into()), Event::Done].into_iter()))
        }
        fn context_window(&self) -> u32 {
            200_000
        }
    }
}
