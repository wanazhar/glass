//! Model-invoked checkpoint, rewind, and wait tools.
//!
//! The checkpoint/rewind pair ports OMP's `checkpoint`/`rewind` tools: the
//! model can mark the current conversation state before exploring, and end
//! an exploration by collapsing it into a retained report. The wait tool
//! (OMP `wait`) blocks a turn until a background event arrives instead of
//! the model polling with repeated no-op calls.
//!
//! Boundaries, stated rather than implpled:
//! - The pair never touches files. `/rewind` (the user command) owns file
//!   restoration; a completed mutation is never reported as reverted.
//! - Rewind is a projection change only — raw history, exports, and
//!   accounting keep every record.
//! - Wait reports what the attention/event source already holds; it never
//!   invents events and never busy-polls.

use crate::session::{Record, Session};
use crate::tools::Outcome;
use glass_mod_api::ToolCall;

/// Cap on the label a checkpoint stores, so a runaway label cannot bloat
/// the log.
pub const MAX_LABEL_BYTES: usize = 80;
/// Cap on the retained report a rewind keeps into context.
pub const MAX_REPORT_BYTES: usize = 4 * 1024;
/// Default and cap for the wait timeout, in seconds.
pub const WAIT_DEFAULT_S: u64 = 60;
pub const WAIT_MAX_S: u64 = 300;

/// Bound on how many trailing records the prewalk gate reads; the gate is a
/// heuristic over the last turn, never a full-log pass.
const MAX_GATE_SCAN: usize = 256;

/// Whether a session has a checkpoint to rewind that is not already
/// closed by that very record. Used to give a precise refusal message.
fn find_checkpoint_label(session: &Session) -> Option<String> {
    let mut open = Vec::new();
    for record in session.history_records() {
        if let Record::Checkpoint { label, rewind_summary, .. } = record {
            if rewind_summary.is_none() { open.push(label); }
            else if let Some(index) = open.iter().rposition(|name| name == &label) { open.truncate(index); }
        }
    }
    open.pop()
}

/// `checkpoint {label}`: mark the current state.
pub fn checkpoint(call: &ToolCall, session: &mut Session) -> Outcome {
    let label = match call.arg_str("label").filter(|l| !l.trim().is_empty()) {
        Some(l) => l.trim().to_string(),
        None => {
            return Outcome::err("error: checkpoint needs a short `label` naming what state you are preserving (e.g. \"before-cache-rework\")");
        }
    };
    if label.len() > MAX_LABEL_BYTES {
        return Outcome::err(format!(
            "error: label exceeds {MAX_LABEL_BYTES} bytes; keep it short and descriptive"
        ));
    }
    let log_len = session.log_len();
    if session
        .append_checked(&[Record::Checkpoint {
            label: label.clone(),
            log_len,
            rewind_summary: None,
        }])
        .is_err()
    {
        return Outcome::err("error: could not record the checkpoint; nothing was marked");
    }
    Outcome::ok(format!(
        "Checkpoint `{label}` marked. Explore freely from here; when you are done, call rewind with the retained report of what should survive. The user's `/rewind` restores files, not this tool — report file changes rather than reverting them."
    ))
}

/// `rewind {label? report}`: close the open checkpoint, hide the exploration
/// from context, and keep one bounded report.
pub fn rewind(call: &ToolCall, session: &mut Session) -> Outcome {
    let Some(label) = find_checkpoint_label(session) else {
        return Outcome::err(
            "error: no open checkpoint to rewind. Mark one with `checkpoint {label}` first; note this tool changes conversation context only — the user's /rewind owns file restoration.",
        );
    };
    if let Some(given) = call.arg_str("label") {
        let given = given.trim();
        if !given.is_empty() && given != label {
            return Outcome::err(format!(
                "error: `{given}` is not the open checkpoint (`{label}`). Rewind that one first, or ask the user to /rewind a different point."
            ));
        }
    }
    let report = call
        .arg_str("report")
        .filter(|r| !r.trim().is_empty())
        .map(|r| {
            // Elide the middle of an over-long report; the start and end are
            // what carry meaning, and the log keeps this bound visible.
            if r.len() > MAX_REPORT_BYTES {
                let mut head_end = MAX_REPORT_BYTES / 2;
                while !r.is_char_boundary(head_end) { head_end -= 1; }
                let mut tail_start = r.len() - MAX_REPORT_BYTES / 2;
                while !r.is_char_boundary(tail_start) { tail_start += 1; }
                let head = &r[..head_end];
                let tail = &r[tail_start..];
                format!("{head}\n…[{over} bytes elided; full text lives in the raw log]…\n{tail}", over = tail_start - head_end)
            } else {
                r
            }
        })
        .unwrap_or_else(|| "The model rewound a checkpoint without a report; the exploration is hidden from context and everything behind it stays on disk and in history.".to_string());
    if let Err(e) = session.append_checked(&[Record::Checkpoint {
        label,
        log_len: session.log_len(),
        rewind_summary: Some(report),
    }]) {
        return Outcome::err(format!("error: rewind could not be recorded; context unchanged: {e}"));
    }
    session.reproject_history();
    Outcome::ok("Rewound. The exploration after the checkpoint is hidden from your context; the retained report above is what survives. Files and raw history are unchanged.".to_string())
}

/// Dispatch for the three wave-C tools by name; shared by the tool schema
/// registration and tests.
pub fn run(call: &ToolCall, session: &mut Session, wait_poll: &mut dyn FnMut() -> Option<String>) -> Outcome {
    match call.name.as_str() {
        "checkpoint" => checkpoint(call, session),
        "rewind" => rewind(call, session),
        "wait" => wait(call, wait_poll),
        _ => Outcome::err(format!("error: `{}` is not a checkpoint/rewind/wait tool", call.name)),
    }
}

/// `wait {seconds?}`: block until the event source yields, or the cap.
pub fn wait(call: &ToolCall, poll: &mut dyn FnMut() -> Option<String>) -> Outcome {
    let started = std::time::Instant::now();
    // Seconds may arrive as a JSON number (raw `1`) or a quoted string
    // (`"1"`): arg_str only unquotes, so accept the bare integer too.
    let seconds = call
        .arg_raw("seconds")
        .map(|s| s.trim().trim_matches('"').to_string())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(WAIT_DEFAULT_S)
        .min(WAIT_MAX_S);
    let deadline = started + std::time::Duration::from_secs(seconds);
    loop {
        if let Some(event) = poll() {
            return Outcome::ok(format!(
                "event after {}s: {event}",
                started.elapsed().as_secs().max(1)
            ));
        }
        if std::time::Instant::now() >= deadline {
            return Outcome::ok(format!(
                "no background result within {seconds}s; do something else or wait again with a longer timeout (cap {WAIT_MAX_S}s)"
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// Prewalk handoff (OMP prewalk): a one-shot, user-confirmed handoff after a
/// plan exists — the planning model's session continues under a cheaper
/// target model. Purely a routing note: the projection and log untouched.
pub fn prewalk_note(session: &mut Session, from_model: &str, target_model: &str) -> Result<String, String> {
    session.append(Record::System(format!(
        "Prewalk handoff: per the user, remaining work in this session routes to `{target_model}` (planned under `{from_model}`). Continue from the existing plan."
    )));
    Ok(format!(
        "prewalk armed: the next turn is executed by `{target_model}`; run the turn with the existing plan. `gc prewalk --off` restores the primary model"
    ))
}

/// Headless prewalk gate (`gc prewalk --check`): reports whether a session is
/// past planning and ready to hand off — pending-plan evidence in recent
/// assistant content, no failing tool state after it, and a configured
/// target. Read-only; the caller alone routes.
pub fn prewalk_gate(session: &Session, target_model: &str) -> serde_json::Value {
    let mut pending_plan = false;
    let mut failing_tool = false;
    let mut last_action = "none".to_string();
    let mut scanned = 0usize;
    for record in session.records.iter().rev() {
        match record {
            Record::User(_) => break, // Gate reads the final turn only.
            Record::Assistant(text)
                if text.contains("todo")
                    || text.contains("plan:")
                    || text.contains("next steps") =>
            {
                // Plan evidence ahead of any later failure is still evidence.
                if !failing_tool {
                    pending_plan = true;
                }
            }
            Record::Bash { command, exit_code, .. }
                if exit_code.is_some_and(|code| code != 0) =>
            {
                failing_tool = true;
                last_action = format!("failing bash: {command}");
            }
            Record::ToolResult { .. }
                if last_action.starts_with("failing bash") => {}
            Record::ToolCall { name, .. }
                if !last_action.starts_with("failing bash") =>
            {
                last_action = format!("tool call: {name}");
            }
            Record::ToolResult { .. } | Record::Bash { .. } => {
                scanned += 1;
            }
            _ => {}
        }
        scanned += 1;
        if scanned >= MAX_GATE_SCAN { break; }
    }
    let ready = pending_plan && !failing_tool && !target_model.is_empty();
    serde_json::json!({
        "ready": ready,
        "target": target_model,
        "pending_plan": pending_plan,
        "failing_tool": failing_tool,
        "last_tool_action": last_action,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, json: &str) -> ToolCall {
        ToolCall {
            id: "t1".into(),
            name: name.into(),
            arguments: json.to_string(),
        }
    }

    fn fresh_session() -> Session {
        // Each caller gets its own temp dir: `create` opens a real log file
        // with the writer lock, so appends and projections run the same path
        // production does.
        let dir = std::env::temp_dir().join(format!(
            "gc-ckpt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        Session::create(&dir).unwrap()
    }

    #[test]
    fn checkpoint_requires_a_label() {
        let mut s = fresh_session();
        let out = checkpoint(&call("checkpoint", "{}"), &mut s);
        assert!(out.body.starts_with("error:"), "{}", out.body);
        assert!(out.body.contains("label"));
    }

    #[test]
    fn checkpoint_marker_is_durable_and_replayed() {
        let mut s = fresh_session();
        let out = checkpoint(&call("checkpoint", r#"{"label":"before-rework"}"#), &mut s);
        assert!(!out.body.starts_with("error:"), "{}", out.body);
        let marker = s
            .records
            .iter()
            .find_map(|r| match r {
                Record::Checkpoint { label, rewind_summary: None, .. } if label == "before-rework" => Some(()),
                _ => None,
            })
            .is_some();
        assert!(marker, "marker must be appended to the log");
    }

    #[test]
    fn rewind_without_checkpoint_names_the_right_tool() {
        let mut s = fresh_session();
        let out = rewind(&call("rewind", "{}"), &mut s);
        assert!(out.body.starts_with("error:"), "{}", out.body);
        assert!(out.body.contains("checkpoint {label}"), "{}", out.body);
    }

    #[test]
    fn rewind_reports_the_retained_summary_and_hides_the_exploration() {
        let mut s = fresh_session();
        s.append(Record::User("explore the cache".into()));
        let marked = checkpoint(&call("checkpoint", r#"{"label":"explore"}"#), &mut s);
        assert!(!marked.body.starts_with("error:"), "{}", marked.body);
        s.append(Record::Assistant("messy exploration notes".into()));
        let out = rewind(
            &call(
                "rewind",
                r#"{"report":"Explored the cache; found the prefix is stable; skip the change."}"#,
            ),
            &mut s,
        );
        assert!(!out.body.starts_with("error:"), "{}", out.body);
        // The projection must carry the report as a system note.
        let messages = s.messages();
        let text = messages
            .iter()
            .filter_map(|m| match m {
                glass_mod_api::Message::System(t) => Some(t.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("retained report") && text.contains("skip the change"),
            "projection must retain the report: {text}"
        );
        assert!(
            !text.contains("messy exploration notes"),
            "exploration must be hidden: {text}"
        );
        assert!(!format!("{messages:?}").contains("messy exploration notes"));
        // ...but the raw log keeps it (transcript and exports are honest).
        assert!(s.history_records().iter().any(|r| matches!(r, Record::Assistant(t) if t.contains("messy exploration notes"))));
    }

    #[test]
    fn rewind_after_real_compaction_drops_exploration_summary_and_resumes() {
        let mut session = fresh_session();
        session.append(Record::User("original goal".into()));
        checkpoint(&call("checkpoint", r#"{"label":"before"}"#), &mut session);
        for _ in 0..30 { session.append(Record::Assistant("EXPLORATION_SECRET".into())); }
        crate::compact::fallback(&mut session, &crate::tools::State::new(), "test").unwrap();
        assert!(format!("{:?}", session.messages()).contains("EXPLORATION_SECRET"));
        session.append(Record::ToolCall { id:"rewind-call".into(), name:"rewind".into(), arguments:r#"{"report":"use retained finding"}"#.into() });
        let outcome = rewind(&call("rewind", r#"{"report":"use retained finding"}"#), &mut session);
        assert!(!outcome.body.starts_with("error:"), "{}", outcome.body);
        session.append(Record::ToolResult { name:"rewind".into(), call_id:"rewind-call".into(), body:outcome.body });
        let text = format!("{:?}", session.messages());
        assert!(!text.contains("EXPLORATION_SECRET"), "{text}");
        assert!(text.contains("retained finding") && text.contains("rewind-call"));
        let path = session.path.clone();
        let id = session.id.clone();
        drop(session);
        let resumed = Session::open(path.parent().unwrap(), &id).unwrap();
        assert_eq!(format!("{:?}", resumed.messages()), text);
    }

    #[test]
    fn nested_checkpoints_can_rewind_inner_then_outer() {
        let mut session = fresh_session();
        session.append(Record::User("goal".into()));
        checkpoint(&call("checkpoint", r#"{"label":"outer"}"#), &mut session);
        checkpoint(&call("checkpoint", r#"{"label":"inner"}"#), &mut session);
        let inner = rewind(&call("rewind", r#"{"label":"inner","report":"inner finding"}"#), &mut session);
        assert!(!inner.body.starts_with("error:"));
        assert_eq!(find_checkpoint_label(&session).as_deref(), Some("outer"));
        let outer = rewind(&call("rewind", r#"{"label":"outer","report":"outer finding"}"#), &mut session);
        assert!(!outer.body.starts_with("error:"), "{}", outer.body);
        assert!(find_checkpoint_label(&session).is_none());
        assert!(!format!("{:?}", session.messages()).contains("inner finding"));
    }

    #[test]
    fn rewind_of_a_wrong_label_is_named() {
        let mut s = fresh_session();
        s.append(Record::User("start".into()));
        checkpoint(&call("checkpoint", r#"{"label":"alpha"}"#), &mut s);
        let out = rewind(&call("rewind", r#"{"label":"beta","report":"x"}"#), &mut s);
        assert!(out.body.contains("`beta` is not the open checkpoint (`alpha`)"), "{}", out.body);
    }

    #[test]
    fn oversized_reports_keep_start_and_end() {
        let mut s = fresh_session();
        s.append(Record::User("start".into()));
        checkpoint(&call("checkpoint", r#"{"label":"probe"}"#), &mut s);
        let long = format!("HEAD{}😀\n{}\n😀TAIL", "x".repeat(2043), "界".repeat(3_000));
        let out = rewind(
            &call("rewind", &format!(r#"{{"report":{}}}"#, serde_json::to_string(&long).unwrap())),
            &mut s,
        );
        assert!(!out.body.starts_with("error:"), "{}", out.body);
        let report = s
            .records
            .iter()
            .rev()
            .find_map(|r| match r {
                Record::Checkpoint { rewind_summary: Some(w), .. } => Some(w.clone()),
                _ => None,
            })
            .expect("rewind stores the report");
        assert!(report.starts_with("HEAD"), "start kept: {report:.40}");
        assert!(report.ends_with("TAIL"), "end kept");
        assert!(report.contains("elided"));
    }

    #[test]
    fn prewalk_gate_reports_plan_and_refuses_a_failing_tail() {
        let mut s = fresh_session();
        s.append(Record::User("plan it".into()));
        s.append(Record::Assistant("plan: emoji width; next steps: implement".into()));
        s.append(Record::ToolCall { id: "t".into(), name: "edit".into(), arguments: "{}".into() });
        let ready = prewalk_gate(&s, "mock");
        assert_eq!(ready["pending_plan"], true);
        assert_eq!(ready["failing_tool"], false);
        assert_eq!(ready["ready"], true);

        s.append(Record::Bash {
            call_id: None,
            command: "cargo test".into(),
            output: " failures: 1".into(),
            exit_code: Some(1),
            truncated: false,
            spill: None,
        });
        let blocked = prewalk_gate(&s, "mock");
        assert_eq!(blocked["failing_tool"], true);
        assert_eq!(blocked["ready"], false);
        assert_eq!(blocked["last_tool_action"], "failing bash: cargo test");

        // No configured target: never ready, whatever the plan says.
        let unplanned = prewalk_gate(&s, "");
        assert_eq!(unplanned["ready"], false);
    }
}
