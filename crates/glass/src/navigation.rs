//! Small navigation receipts; never scan transcript bodies to draw a hub.
use crate::{config,session::Session};
use serde::{Serialize,Deserialize};
use serde_json::{Value,json};
use std::path::{Path,PathBuf};
#[derive(Default,Serialize,Deserialize)]
#[serde(default)]
pub struct Receipt {pub model:String,pub branch:String,pub status:String,pub parent:Option<String>,pub fork_turn:Option<usize>,pub updated:u64}
fn path(id:&str)->PathBuf {config::data_dir().join(format!("{id}.nav.json"))}
pub fn load(id:&str)->Receipt {crate::requests::bounded(&path(id),8192).ok().and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or_default()}
pub fn lineage(session:&Session,parent:&str,through:usize)->Result<(),String> {let r=Receipt{parent:Some(parent.into()),fork_turn:Some(through),..Default::default()};crate::auth::secure_write(&session.path.with_extension("nav.json"),&serde_json::to_vec(&r).map_err(|e|e.to_string())?)}
pub fn record(session:&Session,workspace:&Path,model:&str,status:&str)->Result<(),String> {
    let mut r=load(&session.id);r.model=model.into();r.status=status.into();r.updated=crate::loops::now();
    r.branch=std::process::Command::new("git").current_dir(workspace).args(["symbolic-ref","--quiet","--short","HEAD"]).output().ok().filter(|o|o.status.success()).map(|o|String::from_utf8_lossy(&o.stdout).trim().chars().take(128).collect()).unwrap_or_else(||"detached or no Git".into());
    crate::auth::secure_write(&path(&session.id),&serde_json::to_vec(&r).map_err(|e|e.to_string())?)
}
pub fn inspect(workspace:&Path)->Value {
    let rows=crate::resume_picker::all(&config::data_dir()).into_iter().take(512).map(|thread| {
        let r=load(&thread.id);
        use std::os::unix::fs::FileTypeExt;
        let live=std::fs::symlink_metadata(config::data_dir().join(format!("serve-{}.sock",thread.id))).is_ok_and(|m|m.file_type().is_socket());
        let team=crate::team::get(&thread.id).map(|h|h.snapshot().len());
        json!({"session":thread.id,"title":thread.title,"workspace":thread.workspace,"modified":thread.modified,"model":r.model,"branch":r.branch,"last_status":r.status,"status_freshness":"Last recorded turn status; live connection is shown separately.","server_socket_present":live,"server_liveness":"Not probed; socket presence may be stale.","teammates_in_this_process":team,"parent":r.parent,"fork_turn":r.fork_turn,"current_workspace":thread.workspace==workspace})
    }).collect::<Vec<_>>();
    json!({"threads":rows,"limit":512,"history_loaded":false,"unread":"Unknown until events are inspected; no invented unread counts."})
}
pub fn view(keys:&crate::input::KeyReader,session:&Session,workspace:&Path,branches:bool)->Option<String> {
    let value=inspect(workspace);let all=value["threads"].as_array()?;
    let root=load(&session.id).parent.unwrap_or_else(||session.id.clone());
    let rows=all.iter().filter(|r|!branches||r["session"]==session.id||r["session"]==root||r["parent"]==session.id||r["parent"]==root).collect::<Vec<_>>();
    let labels=rows.iter().map(|r|format!("{} · {} · {} · {}",r["title"].as_str().unwrap_or("Untitled"),r["workspace"].as_str().unwrap_or(""),r["model"].as_str().unwrap_or(""),r["last_status"].as_str().unwrap_or("unknown"))).collect::<Vec<_>>();
    loop {
        let i=crate::modal::choose(keys,if branches {"Conversation alternatives"}else{"Workspace conversations"},"Search project, model, status or title; history loads only on selection",&labels)?;
        let row=rows[i];let id=row["session"].as_str()?;
        let actions=["Inspect metadata","Resume conversation","Send background follow-up"].map(String::from);
        match crate::modal::choose(keys,"Conversation actions",&labels[i],&actions) {
            Some(0)=>crate::structured::view(keys,"Conversation metadata",&row.to_string()),
            Some(1) if row["current_workspace"]==true=>return Some(id.into()),
            Some(1)=>crate::modal::info(keys,"Other workspace",&format!("Open glass-core in {} and run gc resume {id}. The active workspace is not changed underneath running tools.",row["workspace"].as_str().unwrap_or(""))),
            Some(2)=>{
                if let Some(text)=crate::modal::field(keys,"Background follow-up","Durably queued in the target inbox; processed when the target runs and is idle","",false) {
                    match crate::inbox::send(id,&text,&session.id) {Ok(())=>crate::modal::info(keys,"Follow-up saved","Target inbox accepted the message. This does not claim provider completion."),Err(e)=>crate::modal::info(keys,"Follow-up unavailable",&e)}
                }
            },_=>{},
        }
    }
}
pub fn command(args:&[String])->Result<(),String> {if args.iter().any(|a|a!="--json")||args.len()>1{return Err("gc hub [--json]".into());}let value=inspect(&std::env::current_dir().map_err(|e|e.to_string())?.canonicalize().map_err(|e|e.to_string())?);if args.iter().any(|a|a=="--json"){crate::emit_json(value);}else{crate::cli::report("Workspace hub",&value);}Ok(())}
