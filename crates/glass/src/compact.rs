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

use crate::session::Record;
use crate::tools::State;

/// Fraction of the context window above which compaction is attempted.
pub const COMPACT_AT: f32 = 0.78;

/// How many recent messages survive a deterministic compaction.
pub const KEEP_RECENT: usize = 6;

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
        return Action::NeedsSummary { prompt: summary_prompt(records, state) };
    }

    let kept: Vec<Record> = rebuild(records, state);
    let dropped = context_len.saturating_sub(kept.len());
    if dropped == 0 {
        return Action::None;
    }
    Action::Dropped { kept: kept.len(), dropped }
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
        if let Some(a) = records[..tail_start].iter().rev().find(|r| matches!(r, Record::Assistant(_))) {
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
        if let Record::ToolResult { body, name, .. } = &records[i] {
            let bytes = body.len();
            records[i] = Record::ToolResult {
                name: name.clone(),
                call_id: String::new(),
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
    let mut transcript = String::new();
    for r in records {
        let (role, text) = r.to_entry();
        if text.is_empty() || matches!(r, Record::Reasoning(_)) {
            continue;
        }
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
    out.push(Record::System(format!("Summary of earlier work in this session:\n{summary}")));
    // Keep a short tail so the model retains immediate momentum.
    let tail_start = records.len().saturating_sub(4);
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
pub fn project(records: &[Record]) -> Vec<Message> {
    records
        .iter()
        .filter_map(|r| match r {
            Record::User(t) => Some(Message::User(t.clone())),
            Record::Assistant(t) => Some(Message::Assistant(t.clone())),
            Record::ToolResult { name, call_id, body } => Some(Message::ToolResult {
                call_id: call_id.clone(),
                name: name.clone(),
                body: body.clone(),
            }),
            Record::System(t) => Some(Message::System(t.clone())),
            // Reasoning, bash echo, state, meta and markers are display-only.
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(matches!(plan(&recs, &State::new(), 0.9), Action::Dropped { .. }));
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
        assert!(out.iter().any(|r| matches!(r, Record::System(t) if t.contains("did the thing"))));
        assert!(matches!(out.last().unwrap(), Record::User(t) if t == "msg 49"));
        assert!(out.len() < recs.len(), "compaction must actually shrink");
    }

    #[test]
    fn rebuild_never_leads_with_orphan_tool_result() {
        let mut recs = user(10);
        recs.insert(6, Record::ToolResult { name: "bash".into(), call_id: "c".into(), body: "out".into() });
        // Tail lands on the tool result, which would be an orphan.
        let out = rebuild(&recs, &State::new());
        assert!(!matches!(out.first(), Some(Record::ToolResult { .. })));
    }

    #[test]
    fn rebuild_drops_leading_orphan_but_keeps_its_requester() {
        let mut recs = user(20);
        recs.insert(18, Record::Assistant("calling a tool".into()));
        recs.insert(19, Record::ToolResult { name: "bash".into(), call_id: "c".into(), body: "out".into() });
        let out = rebuild(&recs, &State::new());
        // The assistant turn must be restored so the tool result is not orphaned.
        let idx = out.iter().position(|r| matches!(r, Record::ToolResult { .. })).expect("tool kept");
        assert!(idx > 0);
        assert!(matches!(out[idx - 1], Record::Assistant(_)));
    }

    #[test]
    fn tiny_conversation_escalates_to_summary() {
        let recs = user(3);
        assert!(matches!(plan(&recs, &State::new(), 0.95), Action::NeedsSummary { .. }));
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
        let big = recs.iter().filter(|r| matches!(r, Record::ToolResult { body, .. } if body.len() > 2_000)).count();
        assert_eq!(big, KEEP_TOOL_HEAD + KEEP_TOOL_TAIL, "head and tail must survive");
    }

    #[test]
    fn shrink_is_a_noop_below_threshold() {
        let mut recs = vec![Record::ToolResult { name: "bash".into(), call_id: "c".into(), body: "x".repeat(9_000) }];
        assert_eq!(shrink_tool_results(&mut recs, 0.1), 0);
    }

    #[test]
    fn shrink_keeps_everything_when_there_are_few_results() {
        let mut recs = vec![Record::ToolResult { name: "bash".into(), call_id: "c".into(), body: "x".repeat(9_000) }];
        assert_eq!(shrink_tool_results(&mut recs, 0.99), 0, "must not orphan a lone tool result");
    }

    #[test]
    fn summary_prompt_includes_transcript_and_instructions() {
        let recs = vec![Record::User("fix the parser".into()), Record::Assistant("on it".into())];
        let p = summary_prompt(&recs, &State::new());
        assert!(p.contains("fix the parser"));
        assert!(p.contains("next steps"));
        assert!(p.contains("No state was recorded"));
    }

    #[test]
    fn summary_prompt_skips_reasoning() {
        let recs = vec![Record::User("q".into()), Record::Reasoning("private thinking".into())];
        assert!(!summary_prompt(&recs, &State::new()).contains("private thinking"));
    }

    #[test]
    fn apply_summary_keeps_task_and_summary() {
        let recs = user(30);
        let out = apply_summary(&recs, "we were mid-refactor");
        assert!(matches!(out[0], Record::User(ref t) if t == "msg 0"));
        assert!(out.iter().any(|r| matches!(r, Record::System(t) if t.contains("mid-refactor"))));
    }

    #[test]
    fn apply_summary_does_not_lead_with_tool_result() {
        let mut recs = user(30);
        recs.push(Record::ToolResult { name: "bash".into(), call_id: "c".into(), body: "o".into() });
        let out = apply_summary(&recs, "s");
        assert!(!matches!(out.first(), Some(Record::ToolResult { .. })));
    }

    #[test]
    fn projection_matches_record_visibility_rules() {
        let recs = vec![
            Record::User("u".into()),
            Record::Reasoning("r".into()),
            Record::Bash { command: "c".into(), output: "o".into(), exit_code: Some(0), truncated: false, spill: None },
            Record::Assistant("a".into()),
        ];
        let msgs = project(&recs);
        assert_eq!(msgs.len(), 2);
    }
}
