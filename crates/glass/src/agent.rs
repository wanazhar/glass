//! The agent loop.
//!
//! One shape, no exceptions: stream a turn, run whatever tools it asked for,
//! feed the results back, repeat until it stops asking. Everything else in
//! glass is a detail hanging off this.

use std::path::Path;

use glass_mod_api::{Message, Mod_, ToolCall, ToolOutput, TurnContext};

use crate::compact;
use crate::ext;
use crate::verify;
use crate::provider::{self, Event, Provider, Request};
use crate::session::{Record, Session};
use crate::tools::{self, State};

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
    pub usage: Option<(u32, u32)>,
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
    /// Assistant text arriving, chunk by chunk.
    Text(&'a str),
    /// A tool is about to run.
    ToolStart { name: &'a str, command: Option<&'a str> },
    /// A tool finished, with the tail of its output.
    ToolEnd { name: &'a str, summary: String },
    /// Context was compacted.
    Compacted { kept: usize, dropped: usize },
    /// The user interrupted the turn with a correction.
    Steered(&'a str),
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
        }
    }

    /// Fetch extension prompt text once and cache it.
    ///
    /// Called before the first request; afterwards the cache is reused, because
    /// a pipe round-trip per turn would be latency for a value that does not
    /// change.
    pub fn prime_extensions(mut self) -> Self {
        self.ext_prompt = self.extensions.system_prompt();
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

        let mut tools = provider::collect_tools(self.mods);
        // Extension tools are appended and can never shadow a builtin, so a
        // prompt written against `bash` and `state` keeps working with any
        // extension set loaded.
        for (n, d, s) in self.extensions.tools() {
            if !tools.iter().any(|(existing, _, _)| *existing == n) {
                tools.push((n, d, s));
            }
        }

        let system = self.system_prompt();
        let model = self.provider.id().to_string();

        // Size of the prefix a provider would cache: system prompt plus tool
        // schemas. Reported to extensions so a cache warmer knows whether a
        // refresh write is worth its cost.
        self.prefix_bytes =
            system.len() + tools.iter().map(|(_, d, s)| d.len() + s.len()).sum::<usize>();

        let req = Request {
            model: &model,
            system: &system,
            messages: &messages,
            tools: &tools,
            max_tokens: self.max_tokens,
            reasoning: self.reasoning,
        };

        let stream = self.provider.stream(&req)?;
        let mut out = TurnOutput {
            text: String::new(),
            tool_calls: Vec::new(),
            usage: None,
            warnings: Vec::new(),
        };

        for ev in stream {
            // Drain input on every event: streaming is the one place the loop
            // is busy but not blocked, so it is the right place to notice a
            // steer without spinning when there is nothing to read.
            pump(&mut self.steer);
            match ev {
                Event::Text(t) => {
                    // Emit before accumulating so the UI can paint the delta
                    // as it arrives rather than after the whole turn.
                    self.emit(Progress::Text(&t));
                    out.text.push_str(&t);
                }
                // Reasoning is captured but not rendered by default: it is
                // noise in a terminal and costs context to replay.
                Event::Reasoning(_) => {}
                Event::ToolCall { id, name, arguments } => {
                    out.tool_calls.push(ToolCall { id, name, arguments })
                }
                Event::Usage { input, output } => out.usage = Some((input, output)),
                Event::Warning(w) => out.warnings.push(w),
                Event::Done => break,
            }
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

        for round in 0..self.max_tool_rounds {
            let out = self.turn(&mut pump)?;
            last.text.clone_from(&out.text);
            last.usage = out.usage;
            last.warnings.clone_from(&out.warnings);
            last.tool_calls = out.tool_calls.clone();

            if !out.text.trim().is_empty() {
                self.session.append(Record::Assistant(out.text.clone()));
            }
            for w in &out.warnings {
                self.session.append(Record::System(format!("provider: {w}")));
            }

            // Give mods a look at the finished turn.
            let model = self.provider.id().to_string();
            let current = self.session.messages();
            let ctx = TurnContext {
                messages: &current,
                model: &model,
                turn_index: round,
            };
            for m in self.mods {
                m.on_turn_end(&ctx, &out.text);
            }
            self.extensions.turn_end(&out.text, self.prefix_bytes);

            if !out.wants_tools() {
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

            for call in &out.tool_calls {
                // A tool can run for minutes; the user must be able to steer
                // out of it, so the wait is pumped rather than blocking.
                self.run_tool_pumped(call, &mut |slot| pump(slot));
            }

            if round + 1 == self.max_tool_rounds {
                return Ok((last, Stop::ToolBudget));
            }
        }
        Ok((last, Stop::ToolBudget))
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

        let args = call.arguments.clone();
        let workspace = self.workspace.to_path_buf();
        let (tx, rx) = std::sync::mpsc::channel();
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let flag = std::sync::Arc::clone(&done);
        std::thread::spawn(move || {
            let inner = ToolCall { id: "worker".into(), name: "bash".into(), arguments: args };
            let _ = tx.send(tools::bash(&inner, &workspace));
            flag.store(true, std::sync::atomic::Ordering::Release);
        });

        loop {
            if let Ok(result) = rx.try_recv() {
                self.apply_bash_result(call, result);
                return;
            }
            if done.load(std::sync::atomic::Ordering::Acquire)
                && rx.try_recv().is_err()
            {
                return;
            }
            pump(&mut self.steer);
            // The user corrected course while this command was running. Its
            // output is now irrelevant to what they asked for, and feeding it
            // back would push the model toward the plan they just rejected, so
            // the result is dropped and the loop moves on to the correction.
            if self.steer.is_some() {
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

        let rewritten = self.extensions.rewrite_tool_result(&call.id, &call.name, &mut body);
        let mut output = ToolOutput::new(body);
        if rewritten {
            output.rewritten = true;
        }
        for m in self.mods {
            m.on_tool_result(call, &mut output);
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
            body: output.body,
        });
    }

    /// Execute one tool call, letting mods veto or rewrite the result.
    fn run_tool(&mut self, call: &ToolCall) {
        let command = call.arg_str("command").unwrap_or_default();
        self.emit(Progress::ToolStart {
            name: &call.name,
            command: (!command.is_empty()).then_some(command.as_str()),
        });

        // Mods may add tools and may block them; the two built-ins are always
        // present but a mod can still refuse to run one.
        let known = call.name == "bash"
            || call.name == "state"
            || self.extensions.has_tool(&call.name)
            || self.mods.iter().any(|m| m.tools().iter().any(|(n, _, _)| *n == call.name));

        let mut outcome = tools::Outcome {
            body: String::new(),
            truncated: false,
            exit_code: None,
            spill: None,
        };
        let mut display_bash: Option<(String, String)> = None;

        if !known {
            let extra: Vec<String> =
                self.mods.iter().flat_map(|m| m.tools()).map(|(n, _, _)| n).collect();
            let mut avail = vec!["bash".to_string(), "state".to_string()];
            avail.extend(extra);
            outcome.body =
                format!("error: no tool named `{}`. Available tools: {}.", call.name, avail.join(", "));
        } else if let Err(denied) = self.extensions.authorize(&call.id, &call.name, &call.arguments) {
            outcome.body = format!("blocked by extension: {denied}");
        } else if let Some(denied) = self.mods.iter().find_map(|m| m.authorize(call).err()) {
            outcome.body = format!("blocked: {denied}");
        } else if call.name == "bash" {
            let r = tools::bash(call, self.workspace);

            // Verification happens in apply_bash_result, which every bash
            // result passes through.
            display_bash = Some((command.clone(), r.body.clone()));
            outcome = r;
        } else if call.name == "state" {
            outcome = self.state.apply(call);
        } else if self.extensions.has_tool(&call.name) {
            // The bridge lets an extension call tools on the host while it
            // runs. Only `bash` and `state` are exposed: those are the tools
            // the model already has, so an extension cannot widen its own
            // reach by nesting something deeper.
            let workspace = self.workspace.to_path_buf();
            let state = &mut *self.state;
            let mut bridge = |tool: &str, args: &str| -> String {
                let inner = ToolCall { id: format!("{}-nested", call.id), name: tool.to_string(), arguments: args.to_string() };
                match tool {
                    "bash" => tools::bash(&inner, &workspace).body,
                    "state" => state.apply(&inner).body,
                    other => format!("error: extensions may only call bash or state, not `{other}`"),
                }
            };
            match self.extensions.run_tool(&call.name, &call.arguments, &mut bridge) {
                Some(out) => outcome.body = out,
                None => outcome.body = format!("`{}` did not return a result", call.name),
            }
        } else {
            // A tool was declared but nothing answered: report that honestly
            // rather than returning a silent empty success.
            outcome.body = format!("`{}` did not return a result", call.name);
        }

        let mut body = outcome.body;
        let rewritten = self.extensions.rewrite_tool_result(&call.id, &call.name, &mut body);
        let mut output = ToolOutput::new(body);
        if rewritten {
            output.rewritten = true;
        }
        for m in self.mods {
            m.on_tool_result(call, &mut output);
        }

        self.emit(Progress::ToolEnd {
            name: &call.name,
            summary: summarize_output(&output.body),
        });

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
            self.session.append(Record::State(self.state.items().to_vec()));
        }

        self.session.append(Record::ToolResult {
            name: call.name.clone(),
            call_id: call.id.clone(),
            body: output.body,
        });
    }

    /// Assemble the system prompt: base, then mod contributions, then skills.
    fn system_prompt(&self) -> String {
        let mut p = self.base_prompt.clone();
        let model = self.provider.id().to_string();
        let ctx = TurnContext { messages: &[], model: &model, turn_index: 0 };
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
                self.session.append(Record::Compacted { kept, dropped, summary: None });
                Ok(())
            }
            compact::Action::NeedsSummary { prompt } => self.summarize(&prompt),
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
            });
            let after = provider::pressure(self.provider, &compact::project(&self.session.records));
            if after < compact::COMPACT_AT {
                return Ok(());
            }
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

        let stream = self.provider.stream(&req)?;
        let mut summary = String::new();
        for ev in stream {
            match ev {
                Event::Text(t) => summary.push_str(&t),
                Event::Done => break,
                _ => {}
            }
        }
        let summary = summary.trim().to_string();
        if summary.is_empty() {
            // Leave the conversation alone rather than destroying context on a
            // failed summarization.
            return Ok(());
        }

        let before = self.session.records.len();
        self.session.records = compact::apply_summary(&self.session.records, &summary);
        self.session.append(Record::Compacted {
            kept: self.session.records.len(),
            dropped: before.saturating_sub(self.session.records.len()),
            summary: Some(summary),
        });
        Ok(())
    }
}
