//! On-demand attention targets keyed to durable raw-record positions.
use crate::session::{Record, Session};
use serde_json::{Value, json};

pub fn targets(session: &Session) -> Vec<Value> {
    let records=session.history_records();
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
        targets.push(json!({"session":session.id,"raw_record":index,"turn":turn,"kind":kind,
            "preview":&body[..end],"preview_truncated":body.len()>end,
            "full_history":format!("gc export {}",session.id)}));
    }
    targets.into_iter().rev().take(128).collect()
}

pub fn view(keys: &crate::input::KeyReader,session:&Session) {
    let targets=targets(session);
    let rows=targets.iter().map(|t|format!("Turn {} · {} · record {}",t["turn"],t["kind"].as_str().unwrap_or("attention"),t["raw_record"])).collect::<Vec<_>>();
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
    let value=json!({"targets":targets(&session),"ordering":"newest first","limit":128});
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
}
