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
        name: &'a str,
        command: Option<&'a str>,
    },
    /// A tool finished, with the tail of its output.
    ToolEnd { summary: String, output: &'a str },
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
    pub max_tokens: u32,
    pub reasoning: Option<u32>,
    /// Cap on tool calls per turn, so a confused model cannot loop forever.
    pub max_tool_rounds: usize,
    /// Base system prompt, with mod contributions appended per request.
    base_prompt: String,
    /// Skill catalogue, appended after the prompt.
    catalogue: Option<String>,
    /// Project instructions from AGENTS.md / CLAUDE.md.
    project_instructions: Option<String>,
    /// Called as the loop makes progress. `None` disables reporting entirely.
    pub on_progress: Option<ProgressSink<'a>>,
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
    /// Circuit breaker for paid summaries: three consecutive failures stop
    /// the paid path for the session while deterministic compaction continues.
    breaker: compact::Breaker,
    /// Whether this turn already spent its one context-overflow recovery.
    overflow_recovered: bool,
}

/// Lines of a written file echoed back to the model.
///
/// Bounded because a generated file can be thousands of lines; the model can
/// always read more with `bash`, and the point is to show that the write
/// landed, not to replace the file in the context.
const VERIFY_TAIL_LINES: usize = 40;

/// A progress callback.
pub type ProgressSink<'a> = Box<dyn Fn(Progress<'_>) + 'a>;

impl<'a> Agent<'a> {
    /// Report a progress event if an observer is attached.
    fn emit(&self, p: Progress<'_>) {
        if let Some(f) = &self.on_progress {
            f(p);
        }
    }

    /// Open a tool chip in the default view.
    fn emit_tool_start(&mut self, call: &ToolCall) {
        let command = call.arg_str("command");
        self.emit(Progress::ToolStart {
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
        Self {
            provider,
            session,
            state,
            mods,
            extensions,
            workspace,
            max_tokens: 16_000,
            reasoning: None,
            max_tool_rounds: 24,
            base_prompt: String::new(),
            catalogue: None,
            project_instructions: None,
            on_progress: None,
            ext_prompt: String::new(),
            prefix_bytes: 0,
            steer: None,
            reads: readguard::ReadSet::new(),
            warmer: None,
            stream_factory: None,
            recovering: false,
            read_only: false,
            breaker: compact::Breaker::default(),
            overflow_recovered: false,
        }
    }

    pub fn with_plan_mode(mut self, enabled: bool) -> Self {
        self.read_only = enabled;
        self
    }
    fn tool_set(&self) -> Vec<(String, String, String)> {
        if self.read_only {
            crate::inspect::schema()
        } else {
            provider::collect_tools(self.mods)
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
    pub fn schedule_warm(&self) {
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
        // would read.
        let mut tools = self.tool_set();
        for (n, d, sc) in if self.read_only {
            Vec::new()
        } else {
            self.extensions.tools()
        } {
            if !tools.iter().any(|(existing, _, _)| *existing == n) {
                tools.push((n, d, sc));
            }
        }
        let snapshot = CacheSnapshot {
            system: self.system_prompt(),
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
        let messages = self.session.messages();
        self.ensure_context_fits(&messages)?;

        let mut tools = self.tool_set();
        // Extension tools are appended and can never shadow a builtin, so a
        // prompt written against `bash` and `state` keeps working with any
        // extension set loaded.
        for (n, d, s) in if self.read_only {
            Vec::new()
        } else {
            self.extensions.tools()
        } {
            if !tools.iter().any(|(existing, _, _)| *existing == n) {
                tools.push((n, d, s));
            }
        }

        let system = self.system_prompt();
        let model = self.provider.id().to_string();

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
            reasoning: self.reasoning,
        };

        self.emit(Progress::Waiting);
        let receiver = if let Some(factory) = self.stream_factory.clone() {
            let (tx, rx) = std::sync::mpsc::sync_channel(64);
            let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let worker_cancel = cancel.clone();
            let system = system.clone();
            let model = model.clone();
            let messages = messages.clone();
            let tools = tools.clone();
            let max_tokens = self.max_tokens;
            let reasoning = self.reasoning;
            std::thread::spawn(move || {
                let result = (|| -> Result<(), String> {
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
                })();
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
        let mut out = TurnOutput {
            text: String::new(),
            tool_calls: Vec::new(),
            usage: None,
            warnings: Vec::new(),
        };

        loop {
            let ev = if let Some(rx) = &receiver {
                match rx
                    .receiver
                    .recv_timeout(std::time::Duration::from_millis(40))
                {
                    Ok(StreamUpdate::Event(ev)) => Some(ev),
                    Ok(StreamUpdate::Retry {
                        attempt,
                        delay_s,
                        reason,
                    }) => {
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
            if self.steer.as_deref() == Some("") {
                if !out.text.is_empty() {
                    self.session.append(Record::Assistant(out.text.clone()));
                }
                self.session.append(Record::Meta("Response cancelled; streamed output was preserved and pending tools were not executed.".into()));
                return Ok(out);
            }
            let Some(ev) = ev else { continue };
            match ev {
                Event::Text(t) => {
                    // Emit before accumulating so the UI can paint the delta
                    // as it arrives rather than after the whole turn.
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
                Event::Usage(u) => out.usage = Some(u),
                Event::Warning(w) => out.warnings.push(w),
                Event::Error(e) => {
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
                    return Err(provider::Error::Transport(e));
                }
                Event::Done => break,
            }
        }
        if out.text.trim().is_empty() && out.tool_calls.is_empty() {
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
        let mut last = TurnOutput {
            text: String::new(),
            tool_calls: Vec::new(),
            usage: None,
            warnings: Vec::new(),
        };

        self.overflow_recovered = false;
        for round in 0..self.max_tool_rounds {
            let out = match self.turn(&mut pump) {
                Ok(out) => out,
                // A refusal that names an overflow can be recovered once: the
                // harness caused it by under-estimating, and the right answer
                // is to truncate — never to re-send the history that just did
                // not fit (the Crush #3620 cascade). Recovery only fires when
                // something actually got freed; otherwise the original error
                // is the honest report.
                Err(e) if !self.overflow_recovered && compact::looks_like_overflow(&e.to_string()) => {
                    self.overflow_recovered = true;
                    if !self.force_compact() {
                        return Err(e);
                    }
                    self.turn(&mut pump)?
                }
                Err(e) => return Err(e),
            };
            if self.steer.as_deref() == Some("") {
                return Ok((out, Stop::Cancelled));
            }
            last.text.clone_from(&out.text);
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
            let model = self.provider.id().to_string();
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
            }

            if !out.wants_tools() {
                self.schedule_warm();
                return Ok((last, Stop::Finished));
            }

            // A steer typed while this turn was running lands here. Checking
            // *before* the tools run is the whole point: the user is objecting
            // to what the model is about to do, so the correction has to arrive
            // before the command executes rather than after it finishes.
            if let Some(steer) = self.steer.take() {
                self.session.append(Record::System(format!(
                    "user correction, interrupting before the requested actions: {steer}"
                )));
                self.emit(Progress::Steered(&steer));
                return Ok((last, Stop::Steered));
            }

            // Independent calls run together. Correctness comes from the
            // plan, not from hope: same-path writes are serialized, destructive
            // commands run alone, and results are reassembled in call order so
            // the transcript is identical to a sequential run.
            let mut executable = Vec::new();
            for call in &out.tool_calls {
                let cached = if self.recovering {
                    let start = self
                        .session
                        .records
                        .iter()
                        .rposition(|r| matches!(r, Record::User(_)))
                        .unwrap_or(0);
                    self.session.records[start..]
                        .iter()
                        .rev()
                        .find_map(|r| match r {
                            Record::ToolCall {
                                id,
                                name,
                                arguments,
                            } if name == &call.name
                                && serde_json::from_str::<serde_json::Value>(arguments).ok()
                                    == serde_json::from_str::<serde_json::Value>(
                                        &call.arguments,
                                    )
                                    .ok() =>
                            {
                                self.session.records.iter().find_map(|r| {
                                    if let Record::ToolResult { call_id, body, .. } = r {
                                        (call_id == id).then(|| body.clone())
                                    } else {
                                        None
                                    }
                                })
                            }
                            _ => None,
                        })
                } else {
                    None
                };
                self.session.append(Record::ToolCall {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                });
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
                    executable.push(call.clone());
                }
            }
            self.run_tool_batch(&executable, &mut |slot| pump(slot));
            if let Some(steer) = self.steer.take() {
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
                        "Cancelled: running shell process group stopped; remaining tools skipped."
                            .into(),
                    ));
                    return Ok((last, Stop::Cancelled));
                }
                self.session
                    .append(Record::System(format!("User correction: {steer}")));
                self.emit(Progress::Steered(&steer));
                return Ok((last, Stop::Steered));
            }

            if round + 1 == self.max_tool_rounds {
                return Ok((last, Stop::ToolBudget));
            }
        }
        Ok((last, Stop::ToolBudget))
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

        if !self.mods.is_empty()
            || !self.extensions.is_empty()
            || calls
                .iter()
                .any(|c| c.name == "bash" && self.read_before_write_refusal(c).is_some())
        {
            for call in calls {
                if self.steer.is_some() {
                    break;
                }
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
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let results = concurrency::execute(&plan, concurrency::MAX_PARALLEL, |i| {
                let inner = by_index.get(&i).unwrap();
                if worker_cancel.load(std::sync::atomic::Ordering::Acquire) {
                    return tools::Outcome::err("Cancelled before execution");
                }
                tools::bash_cancellable(inner, &workspace, &worker_cancel)
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
        if call.name == "bash" {
            self.run_tool_pumped(call, pump);
        } else {
            self.run_tool(call);
        }
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
        if call.name != "bash" {
            self.run_tool(call);
            return;
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
        std::thread::spawn(move || {
            let inner = ToolCall {
                id: "worker".into(),
                name: "bash".into(),
                arguments: args,
            };
            let _ = tx.send(tools::bash_cancellable(&inner, &workspace, &worker_cancel));
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

    /// Record a bash result produced on a worker thread.
    fn apply_bash_result(&mut self, call: &ToolCall, outcome: tools::Outcome) {
        let command = call.arg_str("command").unwrap_or_default();
        let mut body = outcome.body.clone();

        // Verification belongs here, not at the call site: this is the single
        // place every bash result passes through, whether it ran inline or on a
        // worker thread.
        let touched = verify::written_paths(&command, self.workspace);
        if !touched.is_empty() {
            let note = verify::report(&touched, self.workspace, VERIFY_TAIL_LINES);
            if !note.is_empty() {
                body.push_str(&note);
            }
        }
        // Record what the command read, so a later write can be allowed.
        self.reads.observe(&command, &outcome.body, self.workspace);

        let rewritten = with_tool_bridge(self.workspace, &mut *self.state, &call.id, |bridge| {
            self.extensions
                .rewrite_tool_result(&call.id, &call.name, &mut body, bridge)
        });
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
            command,
            output: output.body.clone(),
            exit_code: outcome.exit_code,
            truncated: outcome.truncated,
            spill: outcome.spill.as_ref().map(|p| p.display().to_string()),
        });
        self.session.append(Record::ToolResult {
            name: call.name.clone(),
            call_id: call.id.clone(),
            body: output.body.clone(),
        });
        // The chip closes here because this is the funnel every bash
        // result passes through — inline, pumped, or fanned out.
        self.emit(Progress::ToolEnd {
            summary: summarize_output(&output.body),
            output: &output.body,
        });
    }

    /// Refuse a write to a file the model has not read.
    ///
    /// A `sed -i` against an unseen file succeeds silently, and the damage only
    /// surfaces turns later at build time, when the cause is no longer obvious.
    /// Creating a new file is never blocked, so this only ever gets in the way
    /// of a genuinely blind overwrite.
    fn read_before_write_refusal(&self, call: &ToolCall) -> Option<String> {
        let command = call.arg_str("command")?;
        let targets = verify::written_paths(&command, self.workspace);
        let blocked = self.reads.blocked(&targets);
        if blocked.is_empty() {
            return None;
        }
        let names: Vec<String> = blocked.iter().map(|p| p.display().to_string()).collect();
        Some(format!(
            "refused: {} has not been read. Read it first (cat, or rg with the path), \
             then retry. Overwriting a file you have not looked at is how changes get lost.",
            names.join(", ")
        ))
    }

    /// Execute one tool call, letting mods veto or rewrite the result.
    fn run_tool(&mut self, call: &ToolCall) {
        self.emit_tool_start(call);

        // Mods may add tools and may block them; the two built-ins are always
        // present but a mod can still refuse to run one.
        let known = call.name == "bash"
            || call.name == "state"
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
            let mut avail = vec!["bash".to_string(), "state".to_string()];
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
        } else if call.name == "bash" {
            if let Some(refusal) = self.read_before_write_refusal(call) {
                self.emit(Progress::BlockedByGuard(&refusal));
                return self.finish_tool(call, tools::Outcome::err(refusal));
            }
            let r = tools::bash(call, self.workspace);

            // Verification happens in apply_bash_result, which every bash
            // result passes through.
            outcome = r;
        } else if call.name == "state" {
            outcome = self.state.apply(call);
        } else if self.extensions.has_tool(&call.name) {
            // The bridge lets an extension call tools on the host while it
            // runs — only `bash` and `state`, the model's own tools.
            let out = with_tool_bridge(self.workspace, &mut *self.state, &call.id, |bridge| {
                self.extensions.run_tool(&call.name, &call.arguments, bridge)
            });
            match out {
                Some(out) => outcome.body = out,
                None => outcome.body = format!("`{}` did not return a result", call.name),
            }
        } else {
            // A tool was declared but nothing answered: report that honestly
            // rather than returning a silent empty success.
            outcome.body = format!("`{}` did not return a result", call.name);
        }

        let mut body = outcome.body;
        let rewritten = with_tool_bridge(self.workspace, &mut *self.state, &call.id, |bridge| {
            self.extensions
                .rewrite_tool_result(&call.id, &call.name, &mut body, bridge)
        });
        self.finish_tool(
            call,
            tools::Outcome {
                body,
                truncated: outcome.truncated,
                exit_code: outcome.exit_code,
                spill: outcome.spill,
            },
        );
        let _ = rewritten;
    }

    /// Record a tool result and notify mods. The single path every tool
    /// outcome takes, including the ones blocked by a guard.
    fn finish_tool(&mut self, call: &ToolCall, outcome: tools::Outcome) {
        let mut output = ToolOutput::new(outcome.body);
        if !self.read_only {
            for m in self.mods {
                m.on_tool_result(call, &mut output);
            }
        }

        self.emit(Progress::ToolEnd {
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
                command: cmd,
                output: shown,
                exit_code: outcome.exit_code,
                truncated: outcome.truncated,
                spill: outcome.spill.as_ref().map(|p| p.display().to_string()),
            });
        }
        if call.name == "state" && !self.state.items().is_empty() {
            self.session
                .append(Record::State(self.state.items().to_vec()));
        }

        self.session.append(Record::ToolResult {
            name: call.name.clone(),
            call_id: call.id.clone(),
            body: output.body,
        });
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
        let model = self.provider.id().to_string();
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
                self.session.append(Record::Compacted {
                    kept,
                    dropped,
                    summary: None,
                    tail_start: 0,
                });
                Ok(())
            }
            compact::Action::NeedsSummary { prompt } => self.summarize(&prompt),
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
            self.session.append(Record::Compacted {
                kept,
                dropped,
                summary: None,
                tail_start: 0,
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
            self.session.append(Record::Compacted {
                kept: self.session.records.len(),
                dropped: 0,
                summary: Some(format!("{shrunk} tool outputs shrunk")),
                tail_start: 0,
            });
            let after = provider::pressure(self.provider, &compact::project(&self.session.records));
            if after < compact::COMPACT_AT {
                return Ok(());
            }
        }

        // Paid summarization is off for this session after repeated failures;
        // the deterministic passes above still ran, and they are free.
        if self.breaker.is_open() {
            return Ok(());
        }

        // Ask for a summary using a bare request with no tools, so the model
        // cannot start calling tools during what should be a read-only turn.
        let tools: Vec<(String, String, String)> = Vec::new();
        let model = self.provider.id().to_string();
        let req = Request {
            model: &model,
            system: "You write precise technical summaries. Follow the instructions exactly.",
            messages: &[Message::User(prompt.to_string())],
            tools: &tools,
            max_tokens: 4096,
            reasoning: None,
        };

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
            return Ok(());
        }
        self.breaker.record_success();

        let before = self.session.records.len();
        // The surviving tail is the last KEEP_RECENT records; replay needs the
        // index to rebuild this projection instead of reverting to raw history.
        let tail_start = self.session.log_len().saturating_sub(compact::KEEP_RECENT);
        self.session.records = compact::apply_summary(&self.session.records, &summary);
        let kept = self.session.records.len();
        self.session.append(Record::Compacted {
            kept,
            dropped: before.saturating_sub(kept),
            tail_start,
            summary: Some(summary),
        });
        Ok(())
    }
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

    /// Does nothing with input. A named function rather
    /// than a closure so it is generic over the borrow
    /// lifetime, which `run_turn` requires.
    fn no_pump(_: &mut Option<String>) {}

    /// A provider that answers immediately and asks for no
    /// tools, so a turn finishes in one round.
    struct MockTurnProvider;

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
            // Short prompt: well under the 4000-byte minimum.
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
            session.records.iter().all(|r| !matches!(r, Record::Meta(_))),
            "a bare refusal leaves no partial-response meta behind"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
