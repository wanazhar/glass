//! On-demand attention targets keyed to durable raw-record positions.
use crate::session::{Record, Session};
use serde_json::{Value, json};

pub fn targets(session: &Session) -> Vec<Value> {
    let records = session.history_records();
    // The recovery inspector already knows the class, the retry lineage and
    // the one safe action for a recorded provider failure. Joining here is what
    // keeps `/attention` from showing a bare preview next to an answer the
    // harness already worked out.
    let lineage = crate::recovery::failures(session)
        .map(|(failures, _)| {
            failures
                .into_iter()
                .map(|failure| (failure.raw_record, failure))
                .collect::<std::collections::HashMap<_, _>>()
        })
        .unwrap_or_default();
    let mut turn = 0;
    let mut targets = Vec::new();
    for (index, record) in records.iter().enumerate() {
        if matches!(record, Record::User(_)) {
            turn += 1;
        }
        let (kind, body) = match record {
            Record::Meta(text) if text.starts_with("error:") => ("provider failure", text.clone()),
            Record::Bash {
                command,
                output,
                exit_code: Some(code),
                ..
            } if *code != 0 => (
                "command failure",
                format!("$ {command}\n\nExit {code}\n\n{output}"),
            ),
            Record::ToolResult { name, body, .. }
                if name != "bash" && body.starts_with("error:") =>
            {
                ("tool failure", format!("{name}\n\n{body}"))
            }
            Record::Custom {
                kind,
                data: Some(data),
            } if kind.starts_with("team/") => ("teammate event", data.clone()),
            _ => continue,
        };
        let mut end = body.len().min(32768);
        while !body.is_char_boundary(end) {
            end -= 1;
        }
        let mut entry = json!({"session":session.id,"raw_record":index,"turn":turn,"kind":kind,
            "preview":&body[..end],"preview_truncated":body.len()>end,
            "full_history":format!("gc export {}",session.id)});
        match lineage.get(&index) {
            // Only provider failures carry a classification. A failed command
            // or a tool error has no safe action the harness can assert, so the
            // fields stay explicitly unknown rather than being filled in.
            Some(failure) => {
                let map = entry.as_object_mut().expect("object literal");
                map.insert("failure_class".into(), json!(failure.class.label()));
                map.insert("severity".into(), json!(failure.severity.label()));
                map.insert("attempts".into(), json!(failure.attempt_count));
                map.insert("attempts_complete".into(), json!(failure.attempts_complete));
                map.insert(
                    "safe_action".into(),
                    json!(failure.recommendation.action().label()),
                );
                map.insert(
                    "mutation_state".into(),
                    json!(failure.mutation_state.label()),
                );
                map.insert(
                    "replays_completed_mutation".into(),
                    json!(failure.recommendation.replays_completed_mutation()),
                );
                map.insert(
                    "provider_request_id_known".into(),
                    json!(failure.provider_request_id.is_some()),
                );
            }
            None => {
                let map = entry.as_object_mut().expect("object literal");
                map.insert("failure_class".into(), Value::Null);
                map.insert("attempts".into(), Value::Null);
                map.insert("safe_action".into(), Value::Null);
                map.insert(
                    "attempt_note".into(),
                    json!("unknown: this record is not a classified provider failure"),
                );
            }
        }
        targets.push(entry);
    }
    let root = crate::polish::metadata(session).workspace;
    if let Ok(board) = crate::workspace::load(&root) {
        for finding in board
            .findings
            .iter()
            .filter(|f| f.disposition == "open")
            .take(32)
        {
            targets.push(json!({"session":session.id,"kind":"review finding","turn":null,"raw_record":null,"finding_id":finding.id,"path":finding.path,"line":finding.line,
                "severity":finding.severity,"preview":format!("{}:{} · {}\n{}",finding.path,finding.line,finding.title,finding.evidence),"target":format!("gc workspace findings show {}",finding.id)}));
        }
        for handoff in board
            .handoffs
            .iter()
            .rev()
            .filter(|h| !h.questions.is_empty())
            .take(16)
        {
            targets.push(json!({"session":session.id,"kind":"handoff questions","turn":null,"raw_record":null,"task":handoff.task,"agent":handoff.agent,
                "preview":handoff.questions.join("\n"),"target":format!("gc workspace handoff show {}",handoff.task)}));
        }
    }
    if let Some(hub) = crate::team::get(&session.id) {
        for agent in hub
            .snapshot()
            .into_iter()
            .filter(|a| a.name.starts_with("review-") && a.status != "archived")
            .take(16)
        {
            targets.push(json!({"session":session.id,"kind":"review checkpoint","agent":agent.id,"raw_record":null,"turn":null,"status":agent.status,
                "preview":format!("{} · {} · {}\n{}",agent.name,agent.model,agent.status,glass_tui::text::clip(&agent.result,4096)),
                "target":format!("gc agent --session {} inspect {}",session.id,agent.id)}));
        }
    }
    targets.extend(crate::jobs::attention(&root, &session.id));
    targets
        .into_iter()
        .rev()
        .take(128)
        .map(|mut target| {
            if let Some(text) = target["preview"].as_str() {
                let mut end = text.len().min(32768);
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                let preview = text[..end].to_owned();
                if text.len() > end {
                    target["preview_truncated"] = json!(true);
                }
                target["preview"] = json!(preview);
            }
            target
        })
        .collect()
}

pub fn view(keys: &crate::input::KeyReader, session: &Session) {
    let mut jobs =
        crate::jobs::Handle::observer(&crate::polish::metadata(session).workspace, &session.id);
    crate::workbench::view(keys, session, &mut jobs, crate::workbench::Scope::Attention);
}

pub fn command(args: &[String]) -> Result<(), String> {
    if args.is_empty() || args.len() > 2 || args.get(1).is_some_and(|a| a != "--json") {
        return Err("gc attention SESSION [--json]".into());
    }
    let session =
        Session::open_readonly(&crate::config::data_dir(), &args[0]).map_err(|e| e.to_string())?;
    let value = json!({"targets":targets(&session),"ordering":"newest first","limit":128,"failure_fields":"failure_class, attempts and safe_action come from the recovery inspector and are null for records it does not classify; a null is unknown, never zero"});
    if args.iter().any(|a| a == "--json") {
        crate::emit_json(value);
    } else {
        crate::cli::report("Attention", &value);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn attention_retains_exact_positions_and_bounded_unicode() {
        let root = std::env::temp_dir().join(format!("gc-attention-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut session = Session::create(&root).unwrap();
        session.append(Record::User("run it".into()));
        session.append(Record::Meta(format!("error: {}", "界".repeat(20000))));
        let targets = targets(&session);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0]["raw_record"], 1);
        assert_eq!(targets[0]["turn"], 1);
        assert!(targets[0]["preview"].as_str().unwrap().len() <= 32768);
        assert_eq!(targets[0]["preview_truncated"], true);
        drop(session);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_recorded_failure_carries_the_inspector_lineage_and_others_stay_unknown() {
        let root =
            std::env::temp_dir().join(format!("gc-attention-lineage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut session = Session::create(&root).unwrap();
        session.append(Record::User("go".into()));
        crate::attempt_log::append(&session.path, 1, 2, "http 429: slow down", 1, None).unwrap();
        session.append(Record::Meta("error: http 429: slow down".into()));
        session.append(Record::Bash {
            call_id: Some("b1".into()),
            command: "false".into(),
            output: "nope".into(),
            exit_code: Some(1),
            truncated: false,
            spill: None,
        });
        let id = session.id.clone();
        drop(session);
        let session = Session::open_readonly(&root, &id).unwrap();
        let targets = targets(&session);
        let failure = targets
            .iter()
            .find(|t| t["kind"] == "provider failure")
            .unwrap();
        assert_eq!(failure["failure_class"], "rate_limit");
        assert_eq!(failure["severity"], "degraded");
        assert_eq!(failure["attempts"], 1);
        assert_eq!(failure["safe_action"], "wait");
        assert_eq!(failure["replays_completed_mutation"], false);
        // A failed command is not a provider failure, so it gets an explicit
        // unknown rather than a classification the harness cannot support.
        let command = targets
            .iter()
            .find(|t| t["kind"] == "command failure")
            .unwrap();
        assert!(command["failure_class"].is_null());
        assert!(command["attempts"].is_null());
        assert!(command["safe_action"].is_null());
        let _ = std::fs::remove_dir_all(root);
    }
}
