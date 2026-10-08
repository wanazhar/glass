//! Session export: the conversation as one Markdown document.
//!
//! Export reads the raw log, not the compaction projection: the point of an
//! export is the record of what actually happened, including turns compaction
//! later dropped from the model's view. Reasoning is excluded unless asked
//! for — it is private working thought — and tool output is folded into
//! `<details>` so the document reads as a conversation rather than a log.

use crate::session::{Record, Session};

/// The raw log, not the compaction projection.
pub fn raw_records(session: &Session) -> Vec<Record> {
    std::fs::read_to_string(&session.path)
        .ok()
        .map(|text| {
            text.lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .filter_map(|v| Record::from_json(&v))
                .collect::<Vec<_>>()
        })
        .filter(|records| !records.is_empty())
        .unwrap_or_else(|| session.records.clone())
}

/// Render one session as Markdown.
pub fn markdown(session: &Session, thinking: bool, records: &[Record]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# glass-core session {}\n\n", session.id));
    out.push_str(&format!(
        "Exported {} · {} records.\n\n---\n\n",
        crate::format_time(std::time::SystemTime::now()),
        records.len()
    ));
    for record in records {
        match record {
            Record::User(text) => section(&mut out, "User", text),
            Record::Assistant(text) => section(&mut out, "Assistant", text),
            Record::Reasoning(text) => {
                if thinking {
                    out.push_str("<details><summary>reasoning</summary>\n\n");
                    out.push_str(text.trim_end());
                    out.push_str("\n\n</details>\n\n");
                }
            }
            Record::Bash {
                command,
                output,
                exit_code,
                truncated,
                spill,
            } => {
                out.push_str(&fence(&format!("$ {command}"), Some("sh")));
                out.push('\n');
                let mut body = output.clone();
                if *truncated {
                    body.push_str("\n[output truncated in the record]");
                }
                if let Some(path) = spill {
                    body.push_str(&format!("\n[full output spilled to {path}]"));
                }
                let status = exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "?".into());
                out.push_str(&format!(
                    "<details><summary>output (exit {status})</summary>\n\n{}\n</details>\n\n",
                    fence(&body, None)
                ));
            }
            Record::ToolResult { name, body, .. } if name != "bash" => {
                out.push_str(&format!(
                    "<details><summary>{name}</summary>\n\n{}\n</details>\n\n",
                    fence(body, None)
                ));
            }
            Record::State(items) => {
                out.push_str("<details><summary>state</summary>\n\n");
                for item in items {
                    out.push_str(&format!("- {item}\n"));
                }
                out.push_str("\n</details>\n\n");
            }
            Record::System(text) => {
                out.push_str("<details><summary>system</summary>\n\n");
                out.push_str(text.trim_end());
                out.push_str("\n\n</details>\n\n");
            }
            Record::Meta(text) => out.push_str(&format!("> {text}\n\n")),
            // A bash tool result duplicates the Bash record above; log
            // metadata and the call/result plumbing are not content.
            Record::ToolResult { .. }
            | Record::Compacted { .. }
            | Record::ToolCall { .. } => {}
        }
    }
    out
}

fn section(out: &mut String, title: &str, text: &str) {
    out.push_str(&format!("## {title}\n\n{}\n\n", text.trim_end()));
}

/// A fenced block whose fence is one backtick longer than the longest run in
/// the body, so embedded fences cannot break the document.
fn fence(body: &str, language: Option<&str>) -> String {
    let longest = body
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let ticks = "`".repeat((longest + 1).max(3));
    let lang = language.unwrap_or("");
    format!("{ticks}{lang}\n{}\n{ticks}", body.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn records() -> Vec<Record> {
        vec![
            Record::User("fix the parser".into()),
            Record::Reasoning("private thinking".into()),
            Record::ToolCall {
                id: "c1".into(),
                name: "bash".into(),
                arguments: "{}".into(),
            },
            Record::Bash {
                command: "cargo test".into(),
                output: "ok 3 passed".into(),
                exit_code: Some(0),
                truncated: false,
                spill: None,
            },
            Record::Assistant("done".into()),
            Record::State(vec!["parser fixed".into()]),
            Record::Meta("error: old attempt".into()),
            Record::Compacted {
                kept: 2,
                dropped: 9,
                summary: Some("earlier work".into()),
                tail_start: 3,
            },
        ]
    }

    fn session_with(records: Vec<Record>) -> (Session, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "gc-export-{}-{}",
            std::process::id(),
            records.len()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let mut session = Session::create(&dir).unwrap();
        for r in records {
            session.append(r);
        }
        (session, dir)
    }

    #[test]
    fn renders_conversation_commands_and_folded_output() {
        let (session, dir) = session_with(vec![]);
        let md = markdown(&session, false, &records());
        assert!(md.starts_with("# glass-core session "));
        assert!(md.contains("## User\n\nfix the parser"));
        assert!(md.contains("## Assistant\n\ndone"));
        assert!(md.contains("```sh\n$ cargo test\n```"), "{md}");
        assert!(md.contains("<summary>output (exit 0)</summary>"));
        assert!(md.contains("ok 3 passed"));
        assert!(md.contains("- parser fixed"));
        assert!(md.contains("> error: old attempt"));
        assert!(
            !md.contains("private thinking"),
            "reasoning is private unless requested"
        );
        assert!(
            !md.contains("earlier work"),
            "compaction markers are log metadata, not conversation"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn thinking_is_included_only_when_asked_for() {
        let (session, dir) = session_with(vec![]);
        let md = markdown(&session, true, &records());
        assert!(md.contains("<summary>reasoning</summary>"));
        assert!(md.contains("private thinking"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_body_with_fences_escalates_the_outer_fence() {
        let (session, dir) = session_with(vec![]);
        let md = markdown(
            &session,
            false,
            &[Record::ToolResult {
                name: "hl_edit".into(),
                call_id: String::new(),
                body: "```\ninner\n```".into(),
            }],
        );
        assert!(md.contains("````\n```\ninner\n```\n````"), "{md}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn raw_records_fall_back_to_the_projection_for_a_missing_log() {
        let (session, dir) = session_with(vec![Record::User("only record".into())]);
        let records = raw_records(&session);
        assert!(records.iter().any(|r| matches!(r, Record::User(t) if t == "only record")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
