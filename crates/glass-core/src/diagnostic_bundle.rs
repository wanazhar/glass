//! Local diagnostic previews contain metadata only, never prompts, shell commands or credentials.
use crate::{
    config,
    session::{Record, Session},
};
use serde_json::{Value, json};
use std::path::Path;

pub fn snapshot(session: &Session) -> Result<Value, String> {
    let records = Session::raw_records(&session.path).map_err(|e| e.to_string())?;
    let events = records
        .iter()
        .rev()
        .take(64)
        .map(|record| {
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
        })
        .collect::<Vec<_>>();
    let reports = crate::requests::reports(&session.path).unwrap_or_default();
    let requests = reports
        .iter()
        .rev()
        .take(16)
        .map(|report| {
            let outcome = report["outcome"].as_str().unwrap_or("");
            // One classifier, shared with /recovery, so the shareable bundle and the
            // local inspector can never disagree about what a failure was.
            let failure = crate::recovery::FailureClass::classify(outcome);
            let mut safe = serde_json::Map::new();
            for key in [
                "elapsed_ms",
                "first_event_ms",
                "text_bytes",
                "reasoning_bytes",
                "tool_calls",
                "transport_retries",
                "usage",
                "cost_usd",
            ] {
                safe.insert(key.into(), report[key].clone());
            }
            safe.insert("failure_class".into(), json!(failure.label()));
            // Hashes disclose change boundaries without copying instruction paths or content.
            safe.insert(
                "prefix_fingerprint".into(),
                report["prefix_fingerprint"].clone(),
            );
            for key in [
                "model_changed",
                "system_changed",
                "tools_changed",
                "prefix_changed",
                "unchanged_message_prefix",
                "estimated_token_delta",
            ] {
                safe.insert(key.into(), report["context_delta"][key].clone());
            }
            Value::Object(safe)
        })
        .collect::<Vec<_>>();
    let dimensions = glass_tui::crossterm::terminal::size()
        .ok()
        .map(|(columns, rows)| json!({"columns":columns,"rows":rows}));
    Ok(
        json!({"schema_version":1,"application":"glass-core","version":env!("CARGO_PKG_VERSION"),"terminal":dimensions,"managed_surface":crate::surface::managed(),"event_count":records.len(),"recent_events_newest_first":events,"recent_requests_newest_first":requests,"redaction":"Allowlisted numeric metadata, event kinds and prefix hashes only. Prompts, reasoning text, commands, output, paths, model IDs, endpoint URLs, response IDs and credentials are omitted. Provider request identifiers are kept out of this bundle entirely, including the ones `gc diagnostics --inspector` shows locally.","sharing":"Review locally before sharing. Nothing is uploaded."}),
    )
}

/// What the caller asked `gc diagnostics` to produce.
///
/// Split out so the flag rules — in particular the one that keeps the local
/// inspector out of the save path — are testable without a session in the
/// data directory.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Request {
    /// The shareable redacted bundle.
    Bundle { save: Option<String> },
    /// The local failure inspector, which carries provider request ids.
    Inspector { json: bool },
}

const USAGE: &str =
    "gc diagnostics SESSION [--json] [--save FILE] | gc diagnostics SESSION --inspector [--json]";

fn parse(args: &[String]) -> Result<Request, String> {
    let mut save = None;
    let mut inspector = false;
    let mut json = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => json = true,
            "--inspector" => inspector = true,
            "--save" if save.is_none() => {
                i += 1;
                save = Some(args.get(i).ok_or(USAGE)?.clone());
            }
            _ => return Err(USAGE.into()),
        }
        i += 1;
    }
    if inspector {
        // The inspector is a local view, not an artifact: it carries provider
        // request ids. Refusing --save keeps that distinction enforceable
        // rather than a matter of remembering.
        if save.is_some() {
            return Err("The local recovery inspector is not a shareable bundle; save `gc diagnostics` without --inspector instead".into());
        }
        return Ok(Request::Inspector { json });
    }
    Ok(Request::Bundle { save })
}

pub fn command(args: &[String]) -> Result<(), String> {
    let id = args.first().ok_or(USAGE)?;
    let session = Session::open_readonly(&config::data_dir(), id).map_err(|e| e.to_string())?;
    match parse(args)? {
        Request::Inspector { json } => {
            let value = crate::recovery::inspector(&session);
            if json {
                crate::emit_json(value);
            } else {
                crate::cli::report("Failure recovery inspector (local)", &value);
            }
        }
        Request::Bundle { save } => {
            let value = snapshot(&session)?;
            if let Some(path) = save {
                let path = Path::new(&path);
                if path.exists() {
                    return Err("Choose a new bundle filename; existing files are preserved".into());
                }
                let bytes = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
                // create_new protects against a competing writer and symlink replacement.
                use std::io::Write;
                use std::os::unix::fs::OpenOptionsExt;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(path)
                    .map_err(|e| e.to_string())?;
                file.write_all(&bytes)
                    .and_then(|_| file.sync_all())
                    .map_err(|e| e.to_string())?;
            }
            if args.iter().any(|a| a == "--json") {
                crate::emit_json(value);
            } else {
                crate::cli::report("Diagnostic bundle preview", &value);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> (std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("gc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut session = Session::create(&dir).unwrap();
        session.append(Record::User("BUNDLE_PROMPT_SENTINEL".into()));
        session.append(Record::Bash {
            call_id: Some("c1".into()),
            command: "curl https://endpoint.invalid/BUNDLE_PATH_SENTINEL".into(),
            output: "BUNDLE_OUTPUT_SENTINEL".into(),
            exit_code: Some(0),
            truncated: false,
            spill: None,
        });
        crate::attempt_log::append(
            &session.path,
            1,
            1,
            "stream diagnostics: events=9, finish=length, response=BUNDLE_RESPONSE_ID_SENTINEL",
            1,
            None,
        )
        .unwrap();
        session.append(Record::Meta(
            "error: stream diagnostics: events=9, finish=length, response=BUNDLE_RESPONSE_ID_SENTINEL".into(),
        ));
        let id = session.id.clone();
        drop(session);
        (dir, id)
    }

    /// The bundle's redaction is what makes it shareable, so it is asserted
    /// against a session that actually contains everything it must omit.
    #[test]
    fn the_bundle_omits_prompts_credentials_endpoints_and_provider_ids() {
        let (dir, id) = fixture("bundle-redact");
        let session = Session::open_readonly(&dir, &id).unwrap();
        let bundle = serde_json::to_string(&snapshot(&session).unwrap()).unwrap();
        for sentinel in [
            "BUNDLE_PROMPT_SENTINEL",
            "BUNDLE_PATH_SENTINEL",
            "BUNDLE_OUTPUT_SENTINEL",
            "BUNDLE_RESPONSE_ID_SENTINEL",
        ] {
            assert!(
                !bundle.contains(sentinel),
                "bundle leaked {sentinel}: {bundle}"
            );
        }
        // The local inspector is the only place the id belongs, and it says so.
        let local = serde_json::to_string(&crate::recovery::inspector(&session)).unwrap();
        assert!(local.contains("BUNDLE_RESPONSE_ID_SENTINEL"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_bundle_and_the_inspector_agree_on_failure_class() {
        for (outcome, expected) in [
            ("http 429: slow down", "rate_limit"),
            ("http 401: bad key", "authentication"),
            ("http 403: forbidden", "authentication"),
            ("http 503: overloaded", "provider_or_transport"),
            ("context length exceeded", "context_limit"),
            ("output limit reached", "output_limit"),
            (
                "reasoning arrived without an answer",
                "reasoning_without_answer",
            ),
            ("transport: connection reset", "provider_or_transport"),
            ("something else", "none_or_unspecified"),
        ] {
            assert_eq!(
                crate::recovery::FailureClass::classify(outcome).label(),
                expected,
                "{outcome}"
            );
        }
    }

    #[test]
    fn the_local_inspector_refuses_to_be_saved_as_a_shareable_bundle() {
        let refused = parse(&[
            "session".into(),
            "--inspector".into(),
            "--save".into(),
            "/tmp/bundle.json".into(),
        ])
        .expect_err("saving the local inspector must be refused");
        assert!(refused.contains("not a shareable bundle"), "{refused}");
        assert_eq!(
            parse(&["session".into(), "--inspector".into(), "--json".into()]).unwrap(),
            Request::Inspector { json: true }
        );
        assert_eq!(
            parse(&["session".into(), "--save".into(), "/tmp/b.json".into()]).unwrap(),
            Request::Bundle {
                save: Some("/tmp/b.json".into())
            }
        );
        assert!(parse(&["session".into(), "--nope".into()]).is_err());
        assert!(parse(&["session".into(), "--save".into()]).is_err());
    }
}
