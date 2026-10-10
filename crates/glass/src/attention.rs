//! On-demand attention targets keyed to durable raw-record positions.
use crate::session::{Record, Session};
use serde_json::{Value, json};

pub fn targets(session: &Session) -> Vec<Value> {
    let records=session.history_records();
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
    let mut turn=0;
    let mut targets=Vec::new();
    for (index,record) in records.iter().enumerate() {
        if matches!(record,Record::User(_)) { turn+=1; }
        let (kind,body)=match record {
            Record::Meta(text) if text.starts_with("error:") => ("provider failure",text.clone()),
            Record::Bash { command,output,exit_code:Some(code),.. } if *code!=0 =>
                ("command failure",format!("$ {command}\n\nExit {code}\n\n{output}")),
            Record::ToolResult { name,body,.. } if name!="bash" && body.starts_with("error:") =>
                ("tool failure",format!("{name}\n\n{body}")),
            Record::Custom { kind,data:Some(data) } if kind.starts_with("team/") =>
                ("teammate event",data.clone()),
            _=>continue,
        };
        let mut end=body.len().min(32768);
        while !body.is_char_boundary(end) { end-=1; }
        let mut entry=json!({"session":session.id,"raw_record":index,"turn":turn,"kind":kind,
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
                map.insert("safe_action".into(), json!(failure.recommendation.action().label()));
                map.insert("mutation_state".into(), json!(failure.mutation_state.label()));
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
    targets.into_iter().rev().take(128).collect()
}

pub fn view(keys: &crate::input::KeyReader,session:&Session) {
    let targets=targets(session);
    let rows=targets.iter().map(|t|{
        let class=t["failure_class"].as_str().map(|c|format!("{c} · ")).unwrap_or_default();
        let attempts=match t["attempts"].as_u64(){Some(n)=>format!("{n} attempts · "),None=>String::new()};
        format!("Turn {} · {}{attempts}{} · record {}",t["turn"],class,t["kind"].as_str().unwrap_or("attention"),t["raw_record"])
    }).collect::<Vec<_>>();
    let searchable=targets.iter().zip(&rows).map(|(t,row)|format!("{row} {}",t["preview"].as_str().unwrap_or(""))).collect::<Vec<_>>();
    if rows.is_empty() { crate::modal::info(keys,"Attention","No recorded failures or teammate events in this conversation."); return; }
    while let Some(index)=crate::modal::choose_search(keys,"Attention","Search recorded events · exact durable record targets",&rows,&searchable) {
        let target=&targets[index];
        crate::modal::markdown(keys,&rows[index],&format!("{}\n\n{}",target["preview"].as_str().unwrap_or(""),if target["preview_truncated"]==true {"Preview capped at 32 KiB; export the conversation for the retained full event."} else {""}));
    }
}

pub fn command(args:&[String])->Result<(),String> {
    if args.is_empty() || args.len()>2 || args.get(1).is_some_and(|a|a!="--json") { return Err("gc attention SESSION [--json]".into()); }
    let session=Session::open_readonly(&crate::config::data_dir(),&args[0]).map_err(|e|e.to_string())?;
    let value=json!({"targets":targets(&session),"ordering":"newest first","limit":128,"failure_fields":"failure_class, attempts and safe_action come from the recovery inspector and are null for records it does not classify; a null is unknown, never zero"});
    if args.iter().any(|a|a=="--json") {crate::emit_json(value);} else {crate::cli::report("Attention",&value);}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn attention_retains_exact_positions_and_bounded_unicode() {
        let root=std::env::temp_dir().join(format!("gc-attention-{}",std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut session=Session::create(&root).unwrap();
        session.append(Record::User("run it".into()));
        session.append(Record::Meta(format!("error: {}","界".repeat(20000))));
        let targets=targets(&session);
        assert_eq!(targets.len(),1);
        assert_eq!(targets[0]["raw_record"],1);
        assert_eq!(targets[0]["turn"],1);
        assert!(targets[0]["preview"].as_str().unwrap().len()<=32768);
        assert_eq!(targets[0]["preview_truncated"],true);
        drop(session);
        let _=std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_recorded_failure_carries_the_inspector_lineage_and_others_stay_unknown() {
        let root=std::env::temp_dir().join(format!("gc-attention-lineage-{}",std::process::id()));
        let _=std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut session=Session::create(&root).unwrap();
        session.append(Record::User("go".into()));
        crate::attempt_log::append(&session.path,1,2,"http 429: slow down",1,None).unwrap();
        session.append(Record::Meta("error: http 429: slow down".into()));
        session.append(Record::Bash{
            call_id:Some("b1".into()),
            command:"false".into(),
            output:"nope".into(),
            exit_code:Some(1),
            truncated:false,
            spill:None,
        });
        let id=session.id.clone();
        drop(session);
        let session=Session::open_readonly(&root,&id).unwrap();
        let targets=targets(&session);
        let failure=targets.iter().find(|t|t["kind"]=="provider failure").unwrap();
        assert_eq!(failure["failure_class"],"rate_limit");
        assert_eq!(failure["severity"],"degraded");
        assert_eq!(failure["attempts"],1);
        assert_eq!(failure["safe_action"],"wait");
        assert_eq!(failure["replays_completed_mutation"],false);
        // A failed command is not a provider failure, so it gets an explicit
        // unknown rather than a classification the harness cannot support.
        let command=targets.iter().find(|t|t["kind"]=="command failure").unwrap();
        assert!(command["failure_class"].is_null());
        assert!(command["attempts"].is_null());
        assert!(command["safe_action"].is_null());
        let _=std::fs::remove_dir_all(root);
    }
}