//! Local, rotating diagnostic records for actionable tool failures.
//!
//! Records intentionally contain classifications and schema fingerprints, not
//! prompts, tool arguments, page contents, URLs, or raw error text.

use fs2::FileExt;
use serde_json::{Value, json};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_RECORD_BYTES: usize = 16 * 1024;

pub fn record_tool_failure(
    workspace: &Path,
    session_id: &str,
    call_id: &str,
    tool_name: &str,
    backend: Option<&str>,
    error_text: &str,
    tool_catalog: Option<&Value>,
) {
    let session_hash = crate::hashing::sha256(session_id.as_bytes());
    let correlation_hash = crate::hashing::sha256(format!("{session_id}\0{call_id}").as_bytes());
    let error_class = classify_error(error_text);
    let summary = safe_summary(error_class);
    let mut event = json!({
        "timestampUnixMs": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64,
        "component": "glass-core-agent",
        "status": "error",
        "toolName": safe_identifier(tool_name),
        "errorClass": error_class,
        "backend": backend.filter(|value| matches!(*value, "native" | "chromium" | "core")),
        "summary": summary,
        "sessionHash": session_hash,
        "correlationHash": correlation_hash,
    });
    if let Some(catalog) = tool_catalog {
        event["toolCatalog"] = catalog.clone();
    }
    let _ = append(workspace, &event);
}

pub fn classify_error(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("tool call validation") || lower.contains("expected a json boolean") {
        "tool_validation"
    } else if lower.contains("unknown tool") || lower.contains("not found") {
        "tool_unavailable"
    } else if lower.contains("permission") || lower.contains("policy") || lower.contains("denied") {
        "policy_denied"
    } else if lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("did not settle")
    {
        "timeout"
    } else if lower.contains("charset")
        || lower.contains("decoding")
        || lower.contains("network")
        || lower.contains("http")
    {
        "network_or_decode"
    } else {
        "tool_execution"
    }
}

fn safe_summary(class: &str) -> &'static str {
    match class {
        "tool_validation" => "The returned tool call did not match its advertised schema.",
        "tool_unavailable" => "The requested tool was unavailable in this session's catalogue.",
        "policy_denied" => "The operation was rejected by the active permission policy.",
        "timeout" => "The operation did not settle before its deadline.",
        "network_or_decode" => "The request or document could not be processed successfully.",
        _ => "The tool returned an error; inspect the session transcript for its result.",
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

pub fn provider_tool_catalog(
    tools: &[(String, String, String)],
    aliases: &std::collections::HashMap<String, String>,
) -> Value {
    let entries = tools
        .iter()
        .take(128)
        .map(|(wire_name, _, schema)| {
            let route = aliases
                .get(wire_name)
                .map(String::as_str)
                .unwrap_or(wire_name);
            json!({
                "wireName": safe_identifier(wire_name),
                "routeName": safe_identifier(route),
                "schemaSha256": crate::hashing::sha256(schema.as_bytes()),
            })
        })
        .collect::<Vec<_>>();
    let digest = crate::hashing::sha256(&serde_json::to_vec(&entries).unwrap_or_default());
    json!({"count": entries.len(), "sha256": digest, "entries": entries})
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

#[cfg(unix)]
fn open_private(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut options = OpenOptions::new();
    options.create(true).append(true).read(true).mode(0o600);
    let file = options.open(path)?;
    let mut permissions = file.metadata()?.permissions();
    permissions.set_mode(0o600);
    file.set_permissions(permissions)?;
    Ok(file)
}

#[cfg(not(unix))]
fn open_private(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(path)
}

fn log_paths(workspace: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let directory = workspace.join(".glass");
    (
        directory.join("diagnostics.jsonl"),
        directory.join("diagnostics.jsonl.1"),
        directory.join("diagnostics.lock"),
    )
}

fn append(workspace: &Path, event: &Value) -> io::Result<()> {
    let (path, backup, lock_path) = log_paths(workspace);
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("diagnostic log has no parent directory"))?;
    reject_symlink(directory)?;
    fs::create_dir_all(directory)?;
    reject_symlink(&path)?;
    reject_symlink(&backup)?;
    reject_symlink(&lock_path)?;

    let lock = open_private(&lock_path)?;
    lock.lock_exclusive()?;
    let mut record = event.clone();
    if record
        .get("timestampUnixMs")
        .and_then(Value::as_u64)
        .is_none()
    {
        record["timestampUnixMs"] = json!(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64
        );
    }
    let mut bytes = serde_json::to_vec(&record).map_err(io::Error::other)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_RECORD_BYTES {
        let _ = FileExt::unlock(&lock);
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "diagnostic record exceeds its byte limit",
        ));
    }
    let current_len = fs::metadata(&path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if current_len.saturating_add(bytes.len() as u64) > MAX_FILE_BYTES {
        match fs::remove_file(&backup) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                let _ = FileExt::unlock(&lock);
                return Err(error);
            }
        }
        if current_len > 0 {
            fs::rename(&path, &backup)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&backup, fs::Permissions::from_mode(0o600))?;
            }
        }
    }
    let mut file = open_private(&path)?;
    file.write_all(&bytes)?;
    file.sync_data()?;
    FileExt::unlock(&lock)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn tool_failures_are_bounded_redacted_correlated_and_rotated() {
        let workspace = std::env::temp_dir().join(format!(
            "glass-session-diagnostics-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&workspace).unwrap();
        let catalog = provider_tool_catalog(
            &[(
                "glass_tool_0".into(),
                "Glass route: glass.browser.observe".into(),
                r#"{"type":"object","properties":{"browserRevision":{"type":"integer"}}}"#.into(),
            )],
            &std::collections::HashMap::from([(
                "glass_tool_0".into(),
                "glass.browser.observe".into(),
            )]),
        );
        let event = json!({
            "timestampUnixMs": 1_798_000_000_000_u64,
            "component": "glass-core-agent",
            "status": "error",
            "toolName": "glass.browser.observe",
            "errorClass": classify_error("error: tool call validation failed: $.browserRevision: expected integer"),
            "backend": "native",
            "summary": safe_summary("tool_validation"),
            "sessionHash": crate::hashing::sha256(b"session"),
            "correlationHash": crate::hashing::sha256(b"session\0call"),
            "toolCatalog": catalog,
        });
        append(&workspace, &event).unwrap();
        let (path, backup, _) = log_paths(&workspace);
        let file = fs::File::open(&path).unwrap();
        #[cfg(unix)]
        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        let line = fs::read_to_string(&path).unwrap();
        assert!(line.contains("glass.browser.observe"));
        assert!(line.contains("schemaSha256"));
        assert!(!line.contains("api_key"));
        assert!(!line.contains("raw arguments"));
        for _ in 0..40 {
            append(&workspace, &json!({"summary":"x".repeat(15_000)})).unwrap();
        }
        assert!(fs::metadata(&path).unwrap().len() <= MAX_FILE_BYTES);
        assert!(fs::metadata(&backup).unwrap().len() <= MAX_FILE_BYTES);
        fs::remove_dir_all(workspace).unwrap();
    }
}
