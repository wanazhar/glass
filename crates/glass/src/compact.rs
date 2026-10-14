//! Two-tier context management.
//!
//! Tier 1 is deterministic and free: drop the middle of the conversation but
//! keep the opening task, the model's own recorded state, and the recent tail.
//! This mirrors what crush does, and it covers almost every real session
//! because a model that has been maintaining `state` has already written down
//! what matters.
//!
//! Tier 2 spends an API call: when tier 1 still cannot get under the limit, ask
//! the model to summarize the transcript. Quality is higher because nuance
//! survives, but it is non-deterministic and needs a prompt we now maintain,
//! so it is strictly the fallback.
//!
//! A third safety net runs underneath both: deterministic eviction of the
//! largest tool outputs. Tool results are the bulk of the context and the most
//! lossy thing to drop, which is why it is the last resort and why dropped
//! results are replaced with an explicit placeholder rather than silently
//! removed.

use glass_mod_api::Message;

use crate::provider::{self, Event, Provider, Request};
use crate::session::{Record, Session};
use crate::tools::State;

/// Fraction of the context window above which compaction is attempted.
pub const COMPACT_AT: f32 = 0.78;

/// How many recent messages survive a deterministic compaction.
pub const KEEP_RECENT: usize = 6;

/// A resumed projection hiding more than this many raw records between the
/// last user message and the compaction boundary earns a branch summary
/// line (pi branch summarization): the gap is where context went. Extractive
/// only — it must never spend a model request just to explain a gap.
pub const BRANCH_SUMMARY_GAP: usize = 60;

/// Bounded branch summary over the dropped region: the last assistant action
/// and the files the dropped commands named. Extractive, from raw records;
/// never writes cache (one-off, prompt-cache hostile by construction).
pub fn branch_summary(dropped: &[Record]) -> Option<String> {
    let mut files: Vec<String> = Vec::new();
    let mut last_action = None;
    for r in dropped {
        match r {
            Record::Assistant(text) if !text.trim().is_empty() => {
                last_action = Some(text.lines().next().unwrap_or(text).to_string());
            }
            Record::ToolCall {name,arguments,..} if crate::files::is_tool(name) => {
                if let Ok(value)=serde_json::from_str::<serde_json::Value>(arguments)
                    && let Some(path)=value["path"].as_str()
                    && !files.iter().any(|p|p==path) && files.len()<4 {
                    files.push(glass_tui::text::clip(path,512));
                }
            }
            // A plausible path literal from a shell command: same heuristic
            // the inspector uses; false positives cost one bounded word.
            Record::Bash { command, .. }
                if last_action.is_none() || files.is_empty() =>
            {
                for token in command.split_whitespace() {
                    if token.starts_with('-') || token.len() < 3 {
                        continue;
                    }
                    if token.contains('/')
                        || token.contains(".rs")
                        || token.contains(".py")
                        || token.contains(".toml")
                        || token.contains(".md")
                        || token.contains(".js")
                        || token.contains(".ts")
                    {
                        let path: String = token
                            .trim_matches(|c: char| "\"'`(),;".contains(c))
                            .to_string();
                        if !path.is_empty() && !files.contains(&path) && files.len() < 4 {
                            files.push(path);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    last_action.map(|action| {
        let action = if action.chars().count() > 200 {
            format!("{}…", action.chars().take(200).collect::<String>())
        } else {
            action
        };
        if files.is_empty() {
            format!("Branch summary of the skipped work: last action — {action}")
        } else {
            format!(
                "Branch summary of the skipped work: last action — {action}; files touched: {}",
                files.join(", ")
            )
        }
    })
}

/// How many messages are protected when evicting oversized tool outputs.
pub const KEEP_TOOL_HEAD: usize = 2;
pub const KEEP_TOOL_TAIL: usize = 4;

/// Outcome of a compaction attempt.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Nothing needed doing.
    None,
    /// Deterministically dropped middle messages, no API call.
    Dropped { kept: usize, dropped: usize },
    /// Needs a model summary call before the conversation can be replaced.
    NeedsSummary { prompt: String },
}

/// What tier 1 produced: either a rebuilt conversation, or a signal that a
/// summary is required.
pub fn plan(records: &[Record], state: &State, pressure: f32) -> Action {
    if pressure < COMPACT_AT {
        return Action::None;
    }

    let context_len = records.len();
    if context_len <= KEEP_RECENT + 2 {
        // Nothing meaningful to drop; the tail alone is the problem.
        return Action::NeedsSummary {
            prompt: summary_prompt(records, state),
        };
    }

    let kept: Vec<Record> = rebuild(records, state);
    let dropped = context_len.saturating_sub(kept.len());
    if dropped == 0 {
        return Action::None;
    }
    Action::Dropped {
        kept: kept.len(),
        dropped,
    }
}

/// Build the post-compaction record list: first user task, recorded state, tail.
///
/// The opening user message is what keeps the model working toward the original
/// goal; the tail is what it was just doing. Everything between them is
/// recoverable from the state's own notes.
pub fn rebuild(records: &[Record], state: &State) -> Vec<Record> {
    let mut out: Vec<Record> = Vec::new();

    // The first user message anchors the task.
    if let Some(first) = records.iter().find(|r| matches!(r, Record::User(_))) {
        out.push(first.clone());
    }

    // The model's own state, injected as a system message so it reads as
    // durable knowledge rather than something the model just said.
    if let Some(ctx) = state.as_context() {
        out.push(Record::System(ctx));
    }
    if !state.items().is_empty() {
        out.push(Record::State(state.items().to_vec()));
    }

    if state.todo != crate::todo::Board::default() { out.push(state.todo.record()); }

    // Recent tail, skipping a leading orphan tool result whose requesting
    // assistant turn was dropped. Providers reject that pairing.
    let tail_start = records.len().saturating_sub(KEEP_RECENT);
    let mut tail: Vec<Record> = records[tail_start..].to_vec();
    let mut dropped_lead = false;
    while matches!(tail.first(), Some(Record::ToolResult { .. })) {
        tail.remove(0);
        dropped_lead = true;
    }
    if dropped_lead && out.is_empty() {
        // Nothing anchors the tail, so preserve one assistant turn before it
        // rather than sending a tool result with no request behind it.
        if let Some(a) = records[..tail_start]
            .iter()
            .rev()
            .find(|r| matches!(r, Record::Assistant(_)))
        {
            out.push(a.clone());
        }
    }
    out.extend(tail);

    // Second pass: if the rebuild still begins with a tool result, drop it.
    while matches!(out.first(), Some(Record::ToolResult { .. })) {
        out.remove(0);
    }
    out
}

/// Replace oversized tool results with a placeholder.
///
/// Tool output is where the tokens are, and the model rarely needs the middle
/// of a `rg` sweep twice. Head and tail are preserved because they carry the
/// command and the conclusion.
pub fn shrink_tool_results(records: &mut [Record], pressure: f32) -> usize {
    if pressure < COMPACT_AT {
        return 0;
    }

    let indices: Vec<usize> = records
        .iter()
        .enumerate()
        .filter(|(_, r)| matches!(r, Record::ToolResult { body, .. } if body.len() > 2_000))
        .map(|(i, _)| i)
        .collect();

    if indices.len() <= KEEP_TOOL_HEAD + KEEP_TOOL_TAIL {
        return 0;
    }

    let protected: Vec<usize> = indices
        .iter()
        .take(KEEP_TOOL_HEAD)
        .chain(indices.iter().skip(indices.len() - KEEP_TOOL_TAIL))
        .copied()
        .collect();

    let mut shrunk = 0;
    for i in indices {
        if protected.contains(&i) {
            continue;
        }
        if let Record::ToolResult { body, name, call_id } = &records[i] {
            let bytes = body.len();
            records[i] = Record::ToolResult {
                name: name.clone(),
                call_id: call_id.clone(),
                body: format!("[{bytes} bytes of {name} output dropped to save context]"),
            };
            shrunk += 1;
        }
    }
    shrunk
}

/// Prompt for tier 2. Kept as a function so it is greppable and testable
/// rather than buried in the loop.
pub fn summary_prompt(records: &[Record], state: &State) -> String {
    summary_prompt_bounded(records, state, SUMMARY_RECORD_CAP)
}

/// Per-record ceiling when serializing a transcript for a summary.
///
/// The summary request has to fit the same window it is replacing: streaming
/// a 400k-token transcript to the summarizer is refused exactly like the turn
/// it was meant to rescue. Tool output is also where the tokens are, and the
/// summarizer needs to know what happened, not every line of it — the same
/// 2,000-char discipline pi serializes with.
pub const SUMMARY_RECORD_CAP: usize = 2_000;

/// `summary_prompt` with an explicit per-record ceiling.
pub fn summary_prompt_bounded(records: &[Record], state: &State, cap: usize) -> String {
    let mut transcript = String::new();
    for r in records {
        let (role, text) = r.to_entry();
        if text.is_empty() || matches!(r, Record::Reasoning(_)) {
            continue;
        }
        let text = clip_record(&text, cap);
        transcript.push_str(&format!("## {}\n{}\n\n", role_label(role), text));
    }

    let prior = state
        .as_context()
        .unwrap_or_else(|| "No state was recorded.".to_string());

    format!(
        "Summarize the work below so another agent can continue without seeing it.\n\n\
         Write for someone with no access to the transcript. Be specific and concrete.\n\
         Include, when present: the original goal, decisions made and why, files created \
         or modified with their paths, commands that worked, commands that failed, the \
         current state of the work, and the exact next steps.\n\
         Do not include reasoning narration, pleasantries, or restated code.\n\
         Do not invent anything not present below.\n\n\
         {prior}\n\n\
         # Transcript\n{transcript}"
    )
}

/// The prompt for a manual `/compact`, optionally focused on something.
pub fn manual_prompt(records: &[Record], state: &State, focus: Option<&str>) -> String {
    let mut prompt = summary_prompt(records, state);
    if let Some(focus) = focus.map(str::trim).filter(|f| !f.is_empty()) {
        prompt.push_str(&format!(
            "\n\nUser focus for this compaction: {focus}\n\
             Weight what matters for that focus. Do not omit anything load-bearing."
        ));
    }
    prompt
}

fn clip_record(text: &str, cap: usize) -> String {
    let total = text.chars().count();
    if total <= cap {
        return text.to_string();
    }
    let head: String = text.chars().take(cap).collect();
    format!("{head}\n[… {} chars truncated to fit the summary request]", total - cap)
}

/// Drop oversized tool results wherever they are.
///
/// `shrink_tool_results` protects the head and tail because it runs during
/// planned compaction; overflow recovery cannot afford that protection,
/// because the result that overflowed the window may well be the newest one.
pub fn evict_oversized(records: &mut [Record], cap: usize) -> usize {
    let mut evicted = 0;
    for r in records.iter_mut() {
        let (name, call_id, bytes) = match r {
            Record::ToolResult { body, name, call_id } if body.len() > cap => (name.clone(), call_id.clone(), body.len()),
            _ => continue,
        };
        *r = Record::ToolResult {
            name,
            call_id,
            body: format!(
                "[{bytes} bytes of output dropped to recover from a context overflow; \
                 re-run the command if it is still needed]"
            ),
        };
        evicted += 1;
    }
    evicted
}

/// Whether a provider error describes a context-window overflow.
///
/// Message matching rather than status codes: providers disagree about which
/// status carries the refusal (400, 413, 422), and a request that failed for
/// some other reason must not trigger a destructive compaction.
pub fn looks_like_overflow(message: &str) -> bool {
    const PATTERNS: &[&str] = &[
        "prompt is too long",
        "prompt too long",
        "context length",
        "context_length_exceeded",
        "maximum context",
        "max context",
        "too many tokens",
        "input is too long",
        "exceeds the maximum",
        "reduce the length",
    ];
    let m = message.to_ascii_lowercase();
    PATTERNS.iter().any(|p| m.contains(p))
}

/// How many consecutive failures disable paid summarization for the session.
///
/// One failure used to be permanent in other harnesses (qwen-code shipped
/// exactly that bug); a circuit breaker tolerates a flaky request without
/// retrying a broken one forever. Deterministic compaction keeps running
/// while the breaker is open.
pub const SUMMARY_FAILURE_LIMIT: u8 = 3;

/// Consecutive-failure guard for tier-2 summaries.
#[derive(Debug, Default, Clone, Copy)]
pub struct Breaker {
    failures: u8,
}

impl Breaker {
    /// Record a failed or empty summary. True when this failure opened it.
    pub fn record_failure(&mut self) -> bool {
        self.failures = self.failures.saturating_add(1);
        self.failures == SUMMARY_FAILURE_LIMIT
    }

    pub fn record_success(&mut self) {
        self.failures = 0;
    }

    pub fn is_open(&self) -> bool {
        self.failures >= SUMMARY_FAILURE_LIMIT
    }
}

/// Force a paid summary now, for `/compact`.
///
/// Works below the threshold: a manual compaction is a statement about what
/// the user wants, not a measurement. The conversation is left untouched if
/// the provider fails or returns nothing. No system-prompt checkpoint is
/// recorded here: the assembled prompt lives with the Agent, so a manual
/// epoch falls back to the current prompt on replay — the automatic path in
/// `agent.rs` is the one that captures it.
/// Keep summary requests comfortably below the window, retaining both the goal
/// and the latest work. This also bounds sessions with thousands of records.
pub fn summary_output_limit(provider: &dyn Provider) -> u32 {
    (provider.context_window() / 4).clamp(1, 4096)
}

pub fn fit_summary_prompt(provider: &dyn Provider, prompt: &str) -> String {
    let budget = provider.context_window().saturating_sub(summary_output_limit(provider)).saturating_sub(provider.context_window() / 16).max(1);
    let mut cap = prompt.len().min(96_000);
    loop {
        let fitted = excerpt(prompt, cap);
        if provider.estimate_tokens(&[Message::User(fitted.clone())]) <= budget || cap == 0 { return fitted; }
        cap /= 2;
    }
}

fn excerpt(text: &str, cap: usize) -> String {
    if text.len() <= cap { return text.to_string(); }
    let marker = "\n[Middle omitted; full transcript remains in the session log.]\n";
    let available = cap.saturating_sub(marker.len());
    let mut head = available / 3;
    while !text.is_char_boundary(head) { head -= 1; }
    let mut tail = text.len().saturating_sub(available - head);
    while !text.is_char_boundary(tail) { tail += 1; }
    if cap < marker.len() {
        let mut end = cap.min(text.len());
        while !text.is_char_boundary(end) { end -= 1; }
        return text[..end].to_string();
    }
    format!("{}{marker}{}", &text[..head], &text[tail..])
}

/// Extractive recovery: no invented model summary and no provider request.
/// Original records remain in the append-only session log.
pub fn fallback(session: &mut Session, state: &State, reason: &str) -> Result<(), String> {
    let summary = format!("Extractive checkpoint (model summary unavailable).\n{}", excerpt(&summary_prompt_bounded(&session.records, state, 400), 24_000));
    let system = session.history_records().iter().rev().find_map(|r| match r { Record::Compacted { system, .. } => Some(system.clone()), _ => None }).flatten();
    let before = session.records.len();
    let tail_start = session.log_len();
    let mut projected = Vec::new();
    if let Some(first) = session.records.iter().find(|r| matches!(r, Record::User(_))) { projected.push(first.clone()); }
    projected.push(Record::System(format!("Summary of earlier work in this session:\n{summary}")));
    projected.push(Record::State(state.items().to_vec()));
    if state.todo != crate::todo::Board::default() { projected.push(state.todo.record()); }
    let kept = projected.len();
    session.append_checked(&[Record::State(state.items().to_vec()), Record::Compacted {
        kept, dropped: before.saturating_sub(kept), summary: Some(summary),
        tail_start: tail_start + 1, system,
    }]).map_err(|e| e.to_string())?;
    session.records = projected;
    let _ = reason;
    session.append(Record::Meta("Compacted locally; model summary unavailable. Full transcript retained in the session log.".into()));
    Ok(())
}

pub fn run_manual(provider: &dyn Provider, session: &mut Session, state: &State, focus: Option<&str>) -> Result<(usize, usize), String> {
    let before = provider.estimate_tokens(&session.messages()) as usize;
    match run_manual_summary(provider, session, state, focus) {
        Ok(result) => Ok(result),
        Err(error) => {
            fallback(session, state, &error)?;
            Ok((before, provider.estimate_tokens(&session.messages()) as usize))
        }
    }
}

fn run_manual_summary(
    provider: &dyn Provider,
    session: &mut Session,
    state: &State,
    focus: Option<&str>,
) -> Result<(usize, usize), String> {
    let before = provider.estimate_tokens(&session.messages()) as usize;
    let prompt = manual_prompt(&session.records, state, focus);
    let tools: Vec<(String, String, String)> = Vec::new();
    let model = provider.model().to_string();
    let req = Request {
        model: &model,
        system: "You write precise technical summaries. Follow the instructions exactly.",
        messages: &[Message::User(fit_summary_prompt(provider, &prompt))],
        tools: &tools,
        max_tokens: summary_output_limit(provider),
        reasoning: Some(0),
    };
    let mut trace=crate::requests::Trace::compaction(session,&req);
    let stream = provider::stream_with_retry(
        provider,
        &req,
        provider::RETRY_ATTEMPTS,
        &provider::RETRY_DELAYS,
        |_, _, _| {},
    )
    .map_err(|e| e.to_string())?;
    trace.serving(provider.model(), provider.rate_card());
    let mut summary = String::new();
    for ev in stream {
        trace.event(&ev);
        match ev {
            Event::Text(t) => summary.push_str(&t),
            Event::Error(e) => return Err(format!("summary stream failed: {e}")),
            Event::Done => break,
            _ => {}
        }
    }
    let summary = summary.trim();
    if summary.is_empty() {
        return Err("provider returned an empty summary; conversation left unchanged".into());
    }
    let before_len = session.records.len();
    // The surviving tail is the last KEEP_RECENT records; replay needs the
    // index to rebuild this projection instead of reverting to raw history.
    let tail_start = session.log_len().saturating_sub(KEEP_RECENT);
    session.records = apply_summary(&session.records, summary);
    let kept = session.records.len();
    session.append(Record::Compacted {
        kept,
        dropped: before_len.saturating_sub(kept),
        summary: Some(summary.to_string()),
        tail_start,
        system: None,
    });
    let after = provider.estimate_tokens(&session.messages()) as usize;
    Ok((before, after))
}

fn role_label(role: glass_tui::Role) -> &'static str {
    match role {
        glass_tui::Role::User => "user",
        glass_tui::Role::Assistant => "assistant",
        glass_tui::Role::Bash => "command",
        glass_tui::Role::Tool => "tool output",
        glass_tui::Role::System => "system",
        glass_tui::Role::Meta => "meta",
    }
}

/// Turn a model summary into the replacement conversation.
pub fn apply_summary(records: &[Record], summary: &str) -> Vec<Record> {
    let mut out: Vec<Record> = Vec::new();
    if let Some(first) = records.iter().find(|r| matches!(r, Record::User(_))) {
        out.push(first.clone());
    }
    out.push(Record::System(format!(
        "Summary of earlier work in this session:\n{summary}"
    )));
    if let Some(snapshot)=records.iter().rev().find(|r|matches!(r,Record::Custom{kind,..}if kind==crate::todo::KIND)){out.push(snapshot.clone());}
    // Keep a short tail so the model retains immediate momentum.
    let tail_start = records.len().saturating_sub(KEEP_RECENT);
    let mut tail: Vec<Record> = records[tail_start..].to_vec();
    while matches!(tail.first(), Some(Record::ToolResult { .. })) {
        tail.remove(0);
    }
    out.extend(tail);
    out
}

/// Messages the model should see, from a record list.
///
/// Mirrors `Session::messages`, but takes a slice so compaction can project a
/// rebuilt list before deciding to commit it.
pub fn project(records: &[Record]) -> Vec<Message> { project_filtered(records, None) }

pub(crate) fn project_filtered(records: &[Record], active: Option<&std::collections::HashSet<&str>>) -> Vec<Message> {
    let calls = records
        .iter()
        .filter_map(|r| {
            if let Record::ToolCall { id, .. } = r {
                Some(id.as_str())
            } else {
                None
            }
        })
        .collect::<std::collections::HashSet<_>>();
    let results = records
        .iter()
        .filter_map(|r| {
            if let Record::ToolResult { call_id, .. } = r {
                Some(call_id.as_str())
            } else {
                None
            }
        })
        .collect::<std::collections::HashSet<_>>();
    let mut earlier = Vec::new();
    let mut messages = Vec::new();
    let latest: std::collections::HashMap<&str, usize> = records.iter().enumerate().filter_map(|(index, r)| match r { Record::Image {data, ..} => Some((data.as_str(), index)), _ => None }).collect();
    for (index, record) in records.iter().enumerate() {
        if let (Some(active), Record::Image {data, ..}) = (active, record)
            && (!active.contains(data.as_str()) || latest.get(data.as_str()) != Some(&index)) { continue; }
        let Some(m) = record.to_message() else { continue };
        if matches!(record, Record::Checkpoint { rewind_summary: Some(_), .. }) {
            // A rewind happens inside a tool batch. Its report must not split
            // the assistant calls from their following tool results.
            earlier.push(m);
            continue;
        }
        match &m {
            Message::ToolResult {
                call_id,
                name,
                body,
            } if !calls.contains(call_id.as_str()) => {
                // Put old-log and truncated-tail facts before the conversation, never between a call and its result.
                earlier.push(Message::System(format!("Earlier {name} result:\n{body}")));
            }
            Message::ToolCall { id, name, .. } if !results.contains(id.as_str()) => {
                earlier.push(Message::System(format!(
                    "An earlier {name} call was interrupted before a result was recorded."
                )));
            }
            Message::User(text) => {
                // In-message citation markers dedupe at the single context
                // projection point: paste/backspace round-trips and an
                // auto-stamp meeting a hand-typed marker both collapse here,
                // for every consumer of the projection (task six-wave-e-001).
                let deduped = crate::session::dedupe_citation_markers(text);
                if deduped.is_empty() {
                    messages.push(m);
                } else {
                    messages.push(Message::User(deduped));
                }
            }
            _ => messages.push(m),
        }
    }
    earlier.extend(messages);
    if records.iter().any(|r|matches!(r,Record::Custom{kind,..}if kind==crate::todo::KIND))
        && let Ok(board)=crate::todo::Board::restore(records)
        && let Some(context)=board.context() { earlier.push(Message::System(context)); }
    crate::images::expand(earlier)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailedSummary;
    impl Provider for FailedSummary {
        fn id(&self) -> &str { "transport" }
        fn model(&self) -> &str { "actual-model" }
        fn context_window(&self) -> u32 { 128_000 }
        fn stream(&self, req: &Request<'_>) -> Result<Box<dyn Iterator<Item=Event> + '_>, provider::Error> {
            assert_eq!(req.model, "actual-model");
            assert_eq!(req.reasoning, Some(0));
            Ok(Box::new(vec![Event::Error("reasoning arrived without an answer".into())].into_iter()))
        }
    }
    #[test]
    fn failed_manual_summary_uses_durable_extractive_checkpoint() {
        let dir = std::env::temp_dir().join(format!("gc-summary-fallback-{}", std::process::id()));
        let mut session = Session::create(&dir).unwrap();
        session.append(Record::User("preserve original goal".into()));
        session.append(Record::ToolResult { name:"bash".into(), call_id:"large".into(), body:"x".repeat(100_000) });
        session.append(Record::User("latest task instruction".into()));
        let (before, after) = run_manual(&FailedSummary, &mut session, &State::new(), None).unwrap();
        assert!(after < before);
        let text = format!("{:?}", session.messages());
        assert!(text.contains("latest task instruction"));
        assert!(text.contains("Extractive checkpoint"));
        let id = session.id.clone();
        drop(session);
        let resumed = Session::open(&dir, &id).unwrap();
        assert!(format!("{:?}", resumed.messages()).contains("latest task instruction"));
        assert!(resumed.records.iter().all(|r| !matches!(r, Record::ToolResult { body, .. } if body.len() > 2000)));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn branch_summary_names_files_and_bounds_the_action() {
        let dropped = vec![
            Record::Assistant("Refactored the cache module after profiling.".into()),
            Record::Bash { call_id: None, command: "cargo build crates/glass/src/cache.rs".into(), output: "".into(), exit_code: Some(0), truncated: false, spill: None },
            Record::Bash { call_id: None, command: "--verbose --flag".into(), output: "".into(), exit_code: Some(0), truncated: false, spill: None },
        ];
        let line = branch_summary(&dropped).unwrap();
        assert!(line.contains("last action — Refactored"), "{line}");
        assert!(line.contains("crates/glass/src/cache.rs"), "{line}");
        // Flag-looking tokens are never "files".
        assert!(!line.contains("--verbose"), "{line}");

        // A long action is bounded.
        let dropped = vec![Record::Assistant("x".repeat(5_000))];
        let line = branch_summary(&dropped).unwrap();
        assert!(line.chars().count() < 260, "bounded: {}", line.chars().count());

        // No assistant content in the gap: no summary, no invention.
        let dropped = vec![Record::Bash { call_id: None, command: "ls".into(), output: "".into(), exit_code: Some(0), truncated: false, spill: None }];
        assert!(branch_summary(&dropped).is_none());
    }

    #[test]
    fn summary_prompt_bounds_total_bytes_and_preserves_latest_work() {
        let text = format!("original goal{}latest work", "🦀".repeat(100_000));
        let fitted = fit_summary_prompt(&FailedSummary, &text);
        assert!(fitted.len() < 96_100);
        assert!(fitted.starts_with("original goal"));
        assert!(fitted.ends_with("latest work"));
    }

    fn user(n: usize) -> Vec<Record> {
        (0..n).map(|i| Record::User(format!("msg {i}"))).collect()
    }

    #[test]
    fn no_action_below_threshold() {
        let recs = user(30);
        assert_eq!(plan(&recs, &State::new(), 0.10), Action::None);
    }

    #[test]
    fn action_triggers_above_threshold() {
        let recs = user(60);
        assert!(matches!(
            plan(&recs, &State::new(), 0.9),
            Action::Dropped { .. }
        ));
    }

    #[test]
    fn rebuild_keeps_first_task_and_tail() {
        let recs = user(50);
        let mut st = State::new();
        st.apply(&glass_mod_api::ToolCall {
            id: "t".into(),
            name: "state".into(),
            arguments: r#"{"items":["did the thing"]}"#.into(),
        });

        let out = rebuild(&recs, &st);
        assert!(matches!(out[0], Record::User(ref t) if t == "msg 0"));
        assert!(
            out.iter()
                .any(|r| matches!(r, Record::System(t) if t.contains("did the thing")))
        );
        assert!(matches!(out.last().unwrap(), Record::User(t) if t == "msg 49"));
        assert!(out.len() < recs.len(), "compaction must actually shrink");
    }

    #[test]
    fn rebuild_never_leads_with_orphan_tool_result() {
        let mut recs = user(10);
        recs.insert(
            6,
            Record::ToolResult {
                name: "bash".into(),
                call_id: "c".into(),
                body: "out".into(),
            },
        );
        // Tail lands on the tool result, which would be an orphan.
        let out = rebuild(&recs, &State::new());
        assert!(!matches!(out.first(), Some(Record::ToolResult { .. })));
    }

    #[test]
    fn rebuild_drops_leading_orphan_but_keeps_its_requester() {
        let mut recs = user(20);
        recs.insert(18, Record::Assistant("calling a tool".into()));
        recs.insert(
            19,
            Record::ToolResult {
                name: "bash".into(),
                call_id: "c".into(),
                body: "out".into(),
            },
        );
        let out = rebuild(&recs, &State::new());
        // The assistant turn must be restored so the tool result is not orphaned.
        let idx = out
            .iter()
            .position(|r| matches!(r, Record::ToolResult { .. }))
            .expect("tool kept");
        assert!(idx > 0);
        assert!(matches!(out[idx - 1], Record::Assistant(_)));
    }

    #[test]
    fn tiny_conversation_escalates_to_summary() {
        let recs = user(3);
        assert!(matches!(
            plan(&recs, &State::new(), 0.95),
            Action::NeedsSummary { .. }
        ));
    }

    #[test]
    fn drop_count_matches_records_removed() {
        let recs = user(40);
        match plan(&recs, &State::new(), 0.9) {
            Action::Dropped { kept, dropped } => assert_eq!(kept + dropped, recs.len()),
            other => panic!("expected drop, got {other:?}"),
        }
    }

    #[test]
    fn shrink_replaces_large_results_with_placeholder() {
        let mut recs = user(2);
        for i in 0..10 {
            recs.push(Record::ToolResult {
                name: "bash".into(),
                call_id: format!("c{i}"),
                body: "x".repeat(5_000),
            });
        }
        let shrunk = shrink_tool_results(&mut recs, 0.9);
        assert_eq!(shrunk, 10 - KEEP_TOOL_HEAD - KEEP_TOOL_TAIL);
        let big = recs
            .iter()
            .filter(|r| matches!(r, Record::ToolResult { body, .. } if body.len() > 2_000))
            .count();
        assert_eq!(
            big,
            KEEP_TOOL_HEAD + KEEP_TOOL_TAIL,
            "head and tail must survive"
        );
    }

    #[test]
    fn shrink_is_a_noop_below_threshold() {
        let mut recs = vec![Record::ToolResult {
            name: "bash".into(),
            call_id: "c".into(),
            body: "x".repeat(9_000),
        }];
        assert_eq!(shrink_tool_results(&mut recs, 0.1), 0);
    }

    #[test]
    fn shrink_keeps_everything_when_there_are_few_results() {
        let mut recs = vec![Record::ToolResult {
            name: "bash".into(),
            call_id: "c".into(),
            body: "x".repeat(9_000),
        }];
        assert_eq!(
            shrink_tool_results(&mut recs, 0.99),
            0,
            "must not orphan a lone tool result"
        );
    }

    #[test]
    fn summary_prompt_includes_transcript_and_instructions() {
        let recs = vec![
            Record::User("fix the parser".into()),
            Record::Assistant("on it".into()),
        ];
        let p = summary_prompt(&recs, &State::new());
        assert!(p.contains("fix the parser"));
        assert!(p.contains("next steps"));
        assert!(p.contains("No state was recorded"));
    }

    #[test]
    fn summary_prompt_skips_reasoning() {
        let recs = vec![
            Record::User("q".into()),
            Record::Reasoning("private thinking".into()),
        ];
        assert!(!summary_prompt(&recs, &State::new()).contains("private thinking"));
    }

    #[test]
    fn summary_prompt_caps_each_record() {
        let big = "x".repeat(10_000);
        let recs = vec![Record::User("small".into()), Record::Assistant(big.clone())];
        let prompt = summary_prompt(&recs, &State::new());
        assert!(prompt.contains("small"));
        assert!(prompt.contains("chars truncated"), "truncation must be marked");
        assert!(
            !prompt.contains(&big),
            "the full oversized record must not be re-streamed to the summarizer"
        );
        assert!(prompt.len() < 5_000, "prompt should stay near the cap");
    }

    #[test]
    fn evict_oversized_replaces_even_the_newest_result() {
        let mut recs = vec![
            Record::ToolResult {
                name: "bash".into(),
                call_id: "c1".into(),
                body: "y".repeat(9_000),
            },
            Record::ToolResult {
                name: "bash".into(),
                call_id: "c2".into(),
                body: "small".into(),
            },
        ];
        assert_eq!(evict_oversized(&mut recs, 2_000), 1);
        assert!(matches!(
            &recs[0],
            Record::ToolResult { body, call_id, .. } if body.contains("dropped to recover") && call_id == "c1"
        ));
        assert!(matches!(
            &recs[1],
            Record::ToolResult { body, .. } if body == "small"
        ));
    }

    #[test]
    fn overflow_detection_matches_refusals_not_other_errors() {
        for msg in [
            "http 400: prompt is too long: 250000 tokens > 200000 maximum",
            "context_length_exceeded",
            "This model's maximum context length is 128000 tokens",
            "input is too long for this model",
            "too many tokens",
        ] {
            assert!(looks_like_overflow(msg), "{msg}");
        }
        for msg in [
            "http 401: invalid api key",
            "http 429: rate limit exceeded, retry after 30s",
            "transport: connection reset by peer",
        ] {
            assert!(!looks_like_overflow(msg), "{msg}");
        }
    }

    #[test]
    fn breaker_opens_on_the_third_consecutive_failure_and_a_success_resets() {
        let mut b = Breaker::default();
        assert!(!b.record_failure());
        assert!(!b.record_failure());
        assert!(!b.is_open());
        assert!(b.record_failure(), "the third failure opens the breaker");
        assert!(b.is_open());
        b.record_success();
        assert!(!b.is_open());
    }

    #[test]
    fn manual_prompt_appends_the_focus_when_present() {
        let recs = vec![Record::User("fix the parser".into())];
        let plain = manual_prompt(&recs, &State::new(), None);
        assert!(!plain.contains("User focus"));
        let focused = manual_prompt(&recs, &State::new(), Some("  keep test failures  "));
        assert!(focused.contains("User focus for this compaction: keep test failures"));
        assert!(focused.contains("fix the parser"));
    }

    #[test]
    fn apply_summary_keeps_task_and_summary() {
        let recs = user(30);
        let out = apply_summary(&recs, "we were mid-refactor");
        assert!(matches!(out[0], Record::User(ref t) if t == "msg 0"));
        assert!(
            out.iter()
                .any(|r| matches!(r, Record::System(t) if t.contains("mid-refactor")))
        );
    }

    #[test]
    fn apply_summary_does_not_lead_with_tool_result() {
        let mut recs = user(30);
        recs.push(Record::ToolResult {
            name: "bash".into(),
            call_id: "c".into(),
            body: "o".into(),
        });
        let out = apply_summary(&recs, "s");
        assert!(!matches!(out.first(), Some(Record::ToolResult { .. })));
    }

    #[test]
    fn projection_matches_record_visibility_rules() {
        let recs = vec![
            Record::User("u".into()),
            Record::Reasoning("r".into()),
            Record::Bash {
                call_id: None,
                command: "c".into(),
                output: "o".into(),
                exit_code: Some(0),
                truncated: false,
                spill: None,
            },
            Record::Assistant("a".into()),
        ];
        let msgs = project(&recs);
        assert_eq!(msgs.len(), 2);
    }
}
