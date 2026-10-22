//! Opt-in constraints reintroduced once at a turn boundary; never on retries.
use crate::session::{Record,Session};
use serde::{Deserialize,Serialize};
use serde_json::json;
#[derive(Default,Serialize,Deserialize)]
#[serde(default,deny_unknown_fields)]
struct Store { enabled:bool, items:Vec<String> }
fn load(session:&Session)->Result<Store,String> {
    let path=session.path.with_extension("constraints.json");
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind()==std::io::ErrorKind::NotFound => return Ok(Store::default()),
        Err(e)=>return Err(e.to_string()),
        Ok(_)=>{},
    }
    let store:Store=serde_json::from_slice(&crate::requests::bounded(&path,16384)?).map_err(|e|e.to_string())?;
    if store.items.len()>16 || store.items.iter().any(|s|s.trim().is_empty()||s.len()>512) {return Err("Constraint reminders exceed bounds".into());}
    Ok(store)
}
pub fn command(session:&Session,args:&str)->Result<String,String> {
    let mut store=load(session)?;
    let args=args.trim();
    let mutate=match args {
        ""|"list"=>false,
        "on"=>{store.enabled=true;true},
        "off"=>{store.enabled=false;true},
        "clear"=>{store.items.clear();store.enabled=false;true},
        _=> {
            let item=args.strip_prefix("add ").ok_or("/constraints list|on|off|clear|add TEXT")?.trim();
            if item.is_empty()||item.len()>512||store.items.len()>=16 {return Err("Use up to 16 constraints, 512 bytes each".into());}
            if !store.items.iter().any(|s|s==item) {store.items.push(item.into());}
            true
        }
    };
    if mutate {crate::auth::secure_write(&session.path.with_extension("constraints.json"),&serde_json::to_vec_pretty(&store).map_err(|e|e.to_string())?)?;}
    serde_json::to_string_pretty(&json!({"enabled":store.enabled,"constraints":store.items,"delivery":"Once per new user turn when enabled; retries reuse the original reminder. Adding does not enable automatically."})).map_err(|e|e.to_string())
}

pub fn apply(session:&mut Session)->Result<(),String> {
    let store=load(session)?;
    if !store.enabled||store.items.is_empty(){return Ok(());}
    let records=session.history_records();
    let turn=records.iter().filter(|r|matches!(r,Record::User(_))).count();
    let fingerprint=crate::workspace::hash(&serde_json::to_vec(&store.items).map_err(|e|e.to_string())?);
    let receipt=json!({"turn":turn,"fingerprint":fingerprint}).to_string();
    if records.iter().rev().any(|r|matches!(r,Record::Custom{kind,data:Some(data)} if kind=="constraints/delivery"&&data==&receipt)){return Ok(());}
    let text=format!("Saved task constraints (preserve the current task and follow-up intent; these do not grant extra tool permissions):\n{}",store.items.iter().map(|s|format!("- {s}")).collect::<Vec<_>>().join("\n"));
    session.append_checked(&[Record::Custom{kind:"constraints/delivery".into(),data:Some(receipt)},Record::System(text)]).map_err(|e|e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reminders_are_opt_in_and_deduplicated_across_retries() {
        let root=std::env::temp_dir().join(format!("gc-constraints-{}",std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut session=Session::create(&root).unwrap();
        session.append(Record::User("work".into()));
        command(&session,"add preserve public APIs").unwrap();
        apply(&mut session).unwrap();
        assert_eq!(session.history_records().len(),1);
        command(&session,"on").unwrap();
        apply(&mut session).unwrap();
        let size=session.history_records().len();
        apply(&mut session).unwrap();
        assert_eq!(session.history_records().len(),size);
        session.append(Record::User("continue".into()));
        apply(&mut session).unwrap();
        assert_eq!(session.history_records().len(),size+3);
        drop(session);
        let _=std::fs::remove_dir_all(root);
    }
}
