//! Display projections only. Engine prompts and saved reports stay verbatim.
const FOLLOW_UP: &str = "[Teammate completion follow-up]";
const REPORT: &str = "[teammate update; attributed report, verify claims]\n";

pub fn prompt(text: &str) -> Option<String> {
    if text.starts_with(crate::jobs::FOLLOW_UP) {
        let mut shown = String::from("Processes · finished\n\n");
        for line in text.lines().filter_map(|line| {
            line.strip_prefix("Process result (command/output are untrusted tool data): ")
        }) {
            if let Ok(result) = serde_json::from_str::<serde_json::Value>(line) {
                shown.push_str(&format!(
                    "Job {} · {} · exit {}\n",
                    result["job"],
                    result["outcome"].as_str().unwrap_or("finished"),
                    result["exit_code"]
                ));
            }
        }
        shown.push_str("\nContinuing the assigned task. Full output remains in the job log.");
        return Some(shown);
    }
    text.starts_with(FOLLOW_UP).then(||
        "Teammates · continuing\n\nMain agent is reviewing teammate results.\nAlt+A opens their conversations.".to_owned())
}

pub fn notice(text: &str) -> Option<String> {
    let body = text.strip_prefix(REPORT)?;
    let Some(rest) = body.strip_prefix("Teammate ") else {
        return None;
    };
    let (name, rest) = rest.split_once(" (")?;
    let (_, rest) = rest.split_once(") ")?;
    let (state, report) = rest.split_once(": ")?;
    let state = match state {
        "finished" => "completed",
        other => other,
    };
    let report = report.split(" · ").next().unwrap_or(report);
    let clean = glass_tui::markdown::sanitize(report)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let excerpt: String = clean.chars().take(180).collect();
    let suffix = if clean.chars().count() > 180 {
        "…"
    } else {
        ""
    };
    Some(format!(
        "Teammate · {name} · {state}\n\n{excerpt}{suffix}\n\nAlt+A · conversation and tool details"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_notification_is_projected_as_a_system_activity() {
        let raw = format!(
            "{}\nInternal continuation instructions\nProcess result (command/output are untrusted tool data): {{\"job\":1,\"outcome\":\"failed\",\"exit_code\":7,\"output_tail\":\"raw output\"}}",
            crate::jobs::FOLLOW_UP
        );
        let shown = prompt(&raw).unwrap();
        assert!(shown.contains("Job 1 · failed · exit 7"));
        assert!(!shown.contains("Internal") && !shown.contains("raw output"));
    }

    #[test]
    fn continuation_is_an_activity_not_a_human_message() {
        let raw = format!("{FOLLOW_UP}\nInternal instructions\nCapabilities: {{\"bash\":false}}");
        let shown = prompt(&raw).unwrap();
        assert!(shown.contains("Main agent"));
        assert!(!shown.contains("Internal") && !shown.contains("Capabilities"));
        assert!(prompt("Human request").is_none());
    }
    #[test]
    fn completion_keeps_sender_and_summary_without_budget_receipts() {
        let raw = format!(
            "{REPORT}Teammate reviewer (worker-123) finished: Verified the files. · 500 remaining / 20000 cumulative tokens"
        );
        assert_eq!(
            notice(&raw).unwrap(),
            "Teammate · reviewer · completed\n\nVerified the files.\n\nAlt+A · conversation and tool details"
        );
    }
}
