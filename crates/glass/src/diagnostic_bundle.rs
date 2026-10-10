//! Local diagnostic previews contain metadata only, never prompts, shell commands or credentials.
use crate::{config, session::{Record, Session}};
use serde_json::{json, Value};
use std::path::Path;

pub fn snapshot(session: &Session) -> Result<Value, String> {
    let records = Session::raw_records(&session.path).map_err(|e| e.to_string())?;
    let events = records.iter().rev().take(64).map(|record| {
        let (kind, bytes) = match record {
            Record::User(text) => ("user", text.len()),
            Record::Assistant(text) => ("answer", text.len()),
            Record::Reasoning(text) => ("reasoning", text.len()),
            Record::Bash { output, .. } => ("shell_result", output.len()),
            Record::ToolCall { arguments, .. } => ("tool_call", arguments.len()),
            Record::ToolResult { body, .. } => ("tool_result", body.len()),
            Record::Compacted { .. } => ("compaction", 0),
            Record::Meta(text) if text.starts_with("error:") => ("failure", text.len()),
            _ => ("metadata", 0),
        };
        json!({"kind":kind,"bytes":bytes})
    }).collect::<Vec<_>>();
    let reports = crate::requests::reports(&session.path).unwrap_or_default();
    let requests = reports.iter().rev().take(16).map(|report| {
        let outcome = report["outcome"].as_str().unwrap_or("");
        let failure = if outcome.contains("output limit") { "output_limit" }
            else if outcome.contains("reasoning") { "reasoning_without_answer" }
            else if outcome.contains("429") || outcome.contains("rate") { "rate_limit" }
            else if outcome.contains("context") { "context_limit" }
            else if outcome.contains("auth") || outcome.contains("401") { "authentication" }
            else if outcome.contains("error") || outcome.contains("failed") { "provider_or_transport" }
            else { "none_or_unspecified" };
        let mut safe = serde_json::Map::new();
        for key in ["elapsed_ms", "first_event_ms", "text_bytes", "reasoning_bytes", "tool_calls", "transport_retries", "usage", "cost_usd"] {
            safe.insert(key.into(), report[key].clone());
        }
        safe.insert("failure_class".into(), json!(failure));
        // Hashes disclose change boundaries without copying instruction paths or content.
        safe.insert("prefix_fingerprint".into(), report["prefix_fingerprint"].clone());
        for key in ["model_changed", "system_changed", "tools_changed", "prefix_changed", "unchanged_message_prefix", "estimated_token_delta"] {
            safe.insert(key.into(), report["context_delta"][key].clone());
        }
        Value::Object(safe)
    }).collect::<Vec<_>>();
    let dimensions = glass_tui::crossterm::terminal::size().ok().map(|(columns, rows)| json!({"columns":columns,"rows":rows}));
    Ok(json!({"schema_version":1,"application":"glass-core","version":env!("CARGO_PKG_VERSION"),"terminal":dimensions,"managed_surface":crate::surface::managed(),"event_count":records.len(),"recent_events_newest_first":events,"recent_requests_newest_first":requests,"redaction":"Allowlisted numeric metadata, event kinds and prefix hashes only. Prompts, reasoning text, commands, output, paths, model IDs, endpoint URLs, response IDs and credentials are omitted.","sharing":"Review locally before sharing. Nothing is uploaded."}))
}

pub fn command(args: &[String]) -> Result<(), String> {
    let usage = "gc diagnostics SESSION [--json] [--save FILE]";
    let id = args.first().ok_or(usage)?;
    let session = Session::open_readonly(&config::data_dir(), id).map_err(|e| e.to_string())?;
    let value = snapshot(&session)?;
    let mut save = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {},
            "--save" if save.is_none() => { i += 1; save = Some(args.get(i).ok_or(usage)?); },
            _ => return Err(usage.into()),
        }
        i += 1;
    }
    if let Some(path) = save {
        let path = Path::new(path);
        if path.exists() { return Err("Choose a new bundle filename; existing files are preserved".into()); }
        let bytes = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
        // create_new protects against a competing writer and symlink replacement.
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path).map_err(|e| e.to_string())?;
        file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
    }
    if args.iter().any(|a| a == "--json") { crate::emit_json(value); }
    else { crate::cli::report("Diagnostic bundle preview", &value); }
    Ok(())
}
