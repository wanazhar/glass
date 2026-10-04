//! The agent loop.
//!
//! One shape, no exceptions: stream a turn, run whatever tools it asked for,
//! feed the results back, repeat until it stops asking. Everything else in
//! glass is a detail hanging off this.

use std::path::Path;

use glass_mod_api::{Message, Mod_, ToolCall, ToolOutput, TurnContext};

use crate::compact;
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
pub enum Stop {
    /// The model finished without asking for tools.
    Finished,
    /// The tool-call budget for this turn was exhausted.
    ToolBudget,
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
}

pub struct Agent<'a> {
    pub provider: &'a dyn Provider,
    pub session: &'a mut Session,
    pub state: &'a mut State,
    pub mods: &'a [Box<dyn Mod_>],
    pub workspace: &'a Path,
    pub max_tokens: u32,
    pub reasoning: Option<u32>,
    /// Cap on tool calls per turn, so a confused model cannot loop forever.
    pub max_tool_rounds: usize,
    /// Base system prompt, with mod contributions appended per request.
    base_prompt: String,
    /// Skill catalogue, appended after the prompt.
    catalogue: Option<String>,
    /// Called as the loop makes progress. `None` disables reporting entirely.
    pub on_progress: Option<ProgressSink<'a>>,
}

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
        workspace: &'a Path,
    ) -> Self {
        Self {
            provider,
            session,
            state,
            mods,
            workspace,
            max_tokens: 16_000,
            reasoning: None,
            max_tool_rounds: 24,
            base_prompt: String::new(),
            catalogue: None,
            on_progress: None,
        }
    }

    /// Attach a progress observer.
    pub fn with_progress(mut self, f: impl Fn(Progress<'_>) + 'a) -> Self {
        self.on_progress = Some(Box::new(f));
        self
    }

    pub fn with_prompt(mut self, base: String, catalogue: Option<String>) -> Self {
        self.base_prompt = base;
        self.catalogue = catalogue;
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
    pub fn turn(&mut self) -> Result<TurnOutput, provider::Error> {
        let messages = self.session.messages();
        self.ensure_context_fits(&messages)?;

        let tools = provider::collect_tools(self.mods);
        let system = self.system_prompt();
        let model = self.provider.id().to_string();

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
    /// asking for them or the round budget runs out.
    pub fn run_turn(&mut self) -> Result<(TurnOutput, Stop), provider::Error> {
        let mut last = TurnOutput {
            text: String::new(),
            tool_calls: Vec::new(),
            usage: None,
            warnings: Vec::new(),
        };

        for round in 0..self.max_tool_rounds {
            let out = self.turn()?;
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

            if !out.wants_tools() {
                return Ok((last, Stop::Finished));
            }

            for call in &out.tool_calls {
                self.run_tool(call);
            }

            if round + 1 == self.max_tool_rounds {
                return Ok((last, Stop::ToolBudget));
            }
        }
        Ok((last, Stop::ToolBudget))
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
        let known = call.name == "bash" || call.name == "state" || self.mods.iter().any(|m| {
            m.tools().iter().any(|(n, _, _)| *n == call.name)
        });

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
        } else if let Some(denied) = self.mods.iter().find_map(|m| m.authorize(call).err()) {
            outcome.body = format!("blocked: {denied}");
        } else if call.name == "bash" {
            let r = tools::bash(call, self.workspace);
            display_bash = Some((command.clone(), r.body.clone()));
            outcome = r;
        } else if call.name == "state" {
            outcome = self.state.apply(call);
        } else {
            // A mod-supplied tool has no built-in implementation; the mod is
            // expected to handle it through its own hooks.
            outcome.body = format!("`{}` is not implemented by the loaded mods", call.name);
        }

        let mut output = ToolOutput::new(outcome.body);
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
