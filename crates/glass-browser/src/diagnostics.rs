//! Reader for Glass Core's local, redacted actionable-error journal.

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Value};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_RECORDS: usize = 256;
const MAX_CATALOG_ENTRIES: usize = 128;

#[derive(Debug)]
pub struct DiagnosticLog {
    pub path: PathBuf,
    pub entries: Vec<Value>,
}

/// Read the newest sanitized entries from the current workspace's local log.
pub fn read_local(root: &Path, limit: usize) -> io::Result<DiagnosticLog> {
    let root = fs::canonicalize(root)?;
    let directory = root.join(".glass");
    reject_symlink(&directory)?;
    let path = directory.join("diagnostics.jsonl");
    let backup = directory.join("diagnostics.jsonl.1");
    reject_symlink(&path)?;
    reject_symlink(&backup)?;

    let mut entries = Vec::new();
    for candidate in [&backup, &path] {
        let Ok(file) = File::open(candidate) else {
            continue;
        };
        let mut content = Vec::new();
        file.take(MAX_FILE_BYTES + 1).read_to_end(&mut content)?;
        if content.len() as u64 > MAX_FILE_BYTES {
            content.truncate(MAX_FILE_BYTES as usize);
        }
        for line in content.split(|byte| *byte == b'\n') {
            if line.is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_slice::<Value>(line) else {
                continue;
            };
            if let Some(value) = sanitize_record(value) {
                entries.push(value);
            }
        }
    }
    let keep = limit.clamp(1, MAX_RECORDS);
    if entries.len() > keep {
        entries.drain(..entries.len() - keep);
    }
    Ok(DiagnosticLog { path, entries })
}

fn reject_symlink(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "diagnostic log path contains a symbolic link",
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn safe_identifier(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(*character, '.' | '_' | '-')
        })
        .take(96)
        .collect()
}

fn is_hex_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sanitize_record(value: Value) -> Option<Value> {
    let object = value.as_object()?;
    if object.get("status")?.as_str()? != "error" {
        return None;
    }
    let timestamp = object.get("timestampUnixMs")?.as_u64()?;
    let component = object.get("component")?.as_str()?;
    if !matches!(component, "glass-core-agent" | "glass-runtime-host") {
        return None;
    }
    let tool_name = safe_identifier(object.get("toolName")?.as_str()?);
    let error_class = object.get("errorClass")?.as_str()?;
    if !matches!(
        error_class,
        "tool_validation"
            | "tool_unavailable"
            | "policy_denied"
            | "timeout"
            | "network_or_decode"
            | "tool_execution"
    ) {
        return None;
    }
    let backend = object.get("backend").and_then(Value::as_str);
    let backend = backend.filter(|value| matches!(*value, "native" | "chromium" | "core"));
    let summary = object.get("summary")?.as_str()?;
    let summary = summary
        .chars()
        .filter(|character| !character.is_control())
        .take(256)
        .collect::<String>();
    let session_hash = object.get("sessionHash")?.as_str()?;
    let correlation_hash = object.get("correlationHash")?.as_str()?;
    if !is_hex_digest(session_hash) || !is_hex_digest(correlation_hash) {
        return None;
    }
    let mut record = Map::new();
    record.insert("timestampUnixMs".into(), Value::from(timestamp));
    record.insert("component".into(), Value::from(component));
    record.insert("status".into(), Value::from("error"));
    record.insert("toolName".into(), Value::from(tool_name));
    record.insert("errorClass".into(), Value::from(error_class));
    record.insert("backend".into(), backend.map_or(Value::Null, Value::from));
    record.insert("summary".into(), Value::from(summary));
    record.insert("sessionHash".into(), Value::from(session_hash));
    record.insert("correlationHash".into(), Value::from(correlation_hash));
    if let Some(catalog) = sanitize_catalog(object.get("toolCatalog")) {
        record.insert("toolCatalog".into(), catalog);
    }
    Some(Value::Object(record))
}

fn sanitize_catalog(value: Option<&Value>) -> Option<Value> {
    let object = value?.as_object()?;
    let count = object.get("count")?.as_u64()?;
    let digest = object.get("sha256")?.as_str()?;
    if !is_hex_digest(digest) {
        return None;
    }
    let entries = object.get("entries")?.as_array()?;
    if entries.len() > MAX_CATALOG_ENTRIES || entries.len() as u64 != count {
        return None;
    }
    let entries = entries
        .iter()
        .filter_map(|entry| {
            let entry = entry.as_object()?;
            let wire_name = safe_identifier(entry.get("wireName")?.as_str()?);
            let route_name = safe_identifier(entry.get("routeName")?.as_str()?);
            let schema_hash = entry.get("schemaSha256")?.as_str()?;
            if !is_hex_digest(schema_hash) {
                return None;
            }
            Some(serde_json::json!({
                "wireName": wire_name,
                "routeName": route_name,
                "schemaSha256": schema_hash,
            }))
        })
        .collect::<Vec<_>>();
    Some(serde_json::json!({"count": count, "sha256": digest, "entries": entries}))
}

/// Print one local diagnostic log without exposing free-form errors or call data.
pub fn print_local(root: &Path, limit: usize, json: bool, show_catalog: bool) -> io::Result<()> {
    let log = read_local(root, limit)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "path": log.path,
                "entries": log.entries,
            }))
            .map_err(io::Error::other)?
        );
        return Ok(());
    }
    if log.entries.is_empty() {
        println!("No diagnostic entries found at {}", log.path.display());
        return Ok(());
    }
    println!(
        "Showing {} sanitized diagnostic entr{} from {}",
        log.entries.len(),
        if log.entries.len() == 1 { "y" } else { "ies" },
        log.path.display()
    );
    for entry in &log.entries {
        let timestamp = entry
            .get("timestampUnixMs")
            .and_then(Value::as_i64)
            .and_then(DateTime::<Utc>::from_timestamp_millis)
            .map(|time| time.to_rfc3339_opts(SecondsFormat::Millis, true))
            .unwrap_or_else(|| "unknown time".into());
        println!(
            "{timestamp}  {component}  {backend}  {tool}  {class}",
            component = entry["component"].as_str().unwrap_or("unknown"),
            backend = entry["backend"].as_str().unwrap_or("unknown backend"),
            tool = entry["toolName"].as_str().unwrap_or("unknown tool"),
            class = entry["errorClass"].as_str().unwrap_or("unknown error"),
        );
        println!("  {}", entry["summary"].as_str().unwrap_or_default());
        if show_catalog && let Some(catalog) = entry.get("toolCatalog") {
            println!(
                "  Provider tool catalogue: {} entries (sha256 {})",
                catalog["count"].as_u64().unwrap_or_default(),
                catalog["sha256"].as_str().unwrap_or("unknown")
            );
            if let Some(entries) = catalog["entries"].as_array() {
                for tool in entries {
                    println!(
                        "    {} → {}  schema sha256 {}",
                        tool["wireName"].as_str().unwrap_or(""),
                        tool["routeName"].as_str().unwrap_or(""),
                        tool["schemaSha256"].as_str().unwrap_or(""),
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn reader_filters_untrusted_fields_and_returns_newest_bounded_entries() {
        let root = std::env::temp_dir().join(format!(
            "glass-diagnostics-read-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join(".glass")).unwrap();
        let valid = serde_json::json!({
            "timestampUnixMs": 1_798_000_000_000_u64,
            "component": "glass-core-agent",
            "status": "error",
            "toolName": "glass.browser.observe",
            "errorClass": "tool_validation",
            "backend": "native",
            "summary": "Schema mismatch",
            "sessionHash": "a".repeat(64),
            "correlationHash": "b".repeat(64),
            "arguments": {"text":"must not be printed"}
        });
        let invalid = serde_json::json!({"status":"error","toolName":"private-value"});
        fs::write(
            root.join(".glass/diagnostics.jsonl"),
            format!("{valid}\n{invalid}\nnot-json\n"),
        )
        .unwrap();
        let log = read_local(&root, 5).unwrap();
        assert_eq!(log.entries.len(), 1);
        assert_eq!(log.entries[0]["toolName"], "glass.browser.observe");
        assert!(log.entries[0].get("arguments").is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
