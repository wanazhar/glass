//! Bounded, read-only suite support snapshot. Never include raw logs or config.
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

fn bounded_json(path: &Path) -> Option<Value> {
    if !fs::symlink_metadata(path).ok()?.is_file()
        || fs::symlink_metadata(path).ok()?.file_type().is_symlink()
    {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(65537)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 65536 {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}
fn revision(value: &Value) -> Value {
    value
        .as_str()
        .filter(|s| (7..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .map_or(Value::Null, |s| json!(s))
}
fn sanitize_event(v: &Value) -> Option<Value> {
    let component = v["component"].as_str()?;
    if !matches!(component, "glass-core-agent" | "glass-runtime-host") {
        return None;
    }
    let class = v["errorClass"].as_str()?;
    if !matches!(
        class,
        "tool_validation"
            | "tool_unavailable"
            | "policy_denied"
            | "timeout"
            | "network_or_decode"
            | "tool_execution"
    ) {
        return None;
    }
    let backend = v["backend"]
        .as_str()
        .filter(|s| matches!(*s, "native" | "chromium" | "core"));
    let hash = |key: &str| {
        v[key]
            .as_str()
            .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .map(str::to_owned)
    };
    Some(
        json!({"timestampUnixMs":v["timestampUnixMs"].as_u64()?,"component":component,"errorClass":class,"backend":backend,"sessionHash":hash("sessionHash"),"correlationHash":hash("correlationHash"),"catalogCount":v["toolCatalog"]["count"].as_u64()}),
    )
}
fn journal(workspace: &Path) -> Value {
    let directory = workspace.join(".glass");
    if fs::symlink_metadata(&directory).is_ok_and(|m| m.file_type().is_symlink()) {
        return json!({"status":"unavailable","reason":"symlink","events":[]});
    }
    let mut events = Vec::new();
    let mut found = false;
    let mut unavailable = false;
    for name in ["diagnostics.jsonl.1", "diagnostics.jsonl"] {
        let path = directory.join(name);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                unavailable = true;
                continue;
            }
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            unavailable = true;
            continue;
        }
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let Ok(mut file) = options.open(&path) else {
            unavailable = true;
            continue;
        };
        found = true;
        let offset = metadata.len().saturating_sub(65536);
        if file.seek(SeekFrom::Start(offset)).is_err() {
            unavailable = true;
            continue;
        }
        let mut bytes = Vec::new();
        if file.take(65536).read_to_end(&mut bytes).is_err() {
            unavailable = true;
            continue;
        }
        for (index, line) in bytes.split(|b| *b == b'\n').enumerate() {
            if offset > 0 && index == 0 {
                continue;
            }
            if let Ok(value) = serde_json::from_slice::<Value>(line)
                && let Some(safe) = sanitize_event(&value)
            {
                events.push(safe);
            }
        }
    }
    if events.len() > 64 {
        events.drain(..events.len() - 64);
    }
    json!({"status":if unavailable {"partial"}else if found{"available"}else{"not_recorded"},"events":events,"maxEvents":64,"maxBytesRead":131072})
}
pub fn session(path: &Path) -> Value {
    match crate::requests::reports(path) {
        Ok(rows) => {
            let reports = rows
                .iter()
                .rev()
                .take(16)
                .map(|v| {
                    let mut safe = serde_json::Map::new();
                    for key in [
                        "elapsed_ms",
                        "first_event_ms",
                        "text_bytes",
                        "reasoning_bytes",
                        "tool_calls",
                        "transport_retries",
                        "cost_usd",
                    ] {
                        if v[key].is_number() {
                            safe.insert(key.into(), v[key].clone());
                        }
                    }
                    safe.insert(
                        "failureClass".into(),
                        json!(
                            crate::recovery::FailureClass::classify(
                                v["outcome"].as_str().unwrap_or("")
                            )
                            .label()
                        ),
                    );
                    for key in ["input_tokens", "output_tokens", "cached_input_tokens"] {
                        if let Some(n) = v["usage"][key].as_u64() {
                            safe.insert(key.into(), json!(n));
                        }
                    }
                    Value::Object(safe)
                })
                .collect::<Vec<_>>();
            json!({"status":"available","maxBytesRead":262144,"recentRequests":reports})
        }
        Err(_) => json!({"status":"unavailable"}),
    }
}
pub fn snapshot(cfg: &crate::config::Config, workspace: &Path) -> Value {
    let manifest = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .and_then(|p| {
            p.parent()?
                .parent()?
                .parent()
                .map(|x| x.join("manifest.json"))
        })
        .and_then(|p| bounded_json(&p));
    let components=manifest.as_ref().map(|m|json!({"core":revision(&m["components"]["core"]),"glass":revision(&m["components"]["glass"]),"web":revision(&m["components"]["web"])}));
    let checks = crate::doctor::inspect_metadata(
        cfg,
        &crate::config::default_path(),
        &crate::config::data_dir(),
    );
    let mut value = json!({"schemaVersion":1,"core":{"version":env!("CARGO_PKG_VERSION"),"checks":checks.checks},"installedComponents":components,"host":{"os":std::env::consts::OS,"architecture":std::env::consts::ARCH,"logicalCpus":std::thread::available_parallelism().ok().map(|n|n.get())},"glass":{"journal":journal(workspace)},"collection":{"readOnly":true,"providerProbes":false,"rawLogsIncluded":false}});
    #[cfg(unix)]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        if let Ok(path) = CString::new(workspace.as_os_str().as_bytes()) {
            let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
            // statvfs writes only to this correctly sized output buffer.
            if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } == 0 {
                let stats = unsafe { stats.assume_init() };
                #[allow(clippy::unnecessary_cast)]
                {
                    // statvfs field widths vary across Unix targets.
                    value["host"]["diskAvailableBytes"] =
                        json!((stats.f_bavail as u64).saturating_mul(stats.f_frsize as u64));
                    value["host"]["diskTotalBytes"] =
                        json!((stats.f_blocks as u64).saturating_mul(stats.f_frsize as u64));
                }
            }
        }
    }
    value
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn journal_events_omit_untrusted_payloads() {
        let v = json!({"component":"glass-runtime-host","errorClass":"timeout","timestampUnixMs":42,"backend":"native","summary":"SECRET","toolName":"SECRET","arguments":"SECRET","sessionHash":"a".repeat(64),"correlationHash":"bad","toolCatalog":{"count":12,"entries":["SECRET"]}});
        let out = sanitize_event(&v).unwrap();
        assert!(!out.to_string().contains("SECRET"));
        assert!(out["correlationHash"].is_null());
        assert_eq!(out["errorClass"], "timeout");
        assert!(sanitize_event(&json!({"component":"SECRET","errorClass":"timeout"})).is_none());
    }
    #[test]
    fn revisions_reject_paths_and_credentials() {
        assert!(revision(&json!("/private/key")).is_null());
        assert_eq!(revision(&json!("abc1234")), "abc1234");
    }
}
