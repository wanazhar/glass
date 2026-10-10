//! Global hard denials. Repository configuration cannot relax these rules.
use serde::{Deserialize, Serialize};
#[derive(Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Policy {
    deny_providers: Vec<String>,
    deny_endpoint_hosts: Vec<String>,
    deny_shell: Vec<String>,
}
fn path() -> std::path::PathBuf {
    crate::config::config_dir().join("policy.json")
}
fn load() -> Result<Policy, String> {
    let path = path();
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Policy::default()),
        Err(e) => return Err(e.to_string()),
        Ok(metadata) if !metadata.is_file() => {
            return Err("Global policy must be a regular file; refusing the operation".into());
        }
        _ => {}
    }
    let policy: Policy = serde_json::from_slice(&crate::requests::bounded(&path, 64 * 1024)?)
        .map_err(|_| "Global policy is invalid; refusing the operation")?;
    for patterns in [
        &policy.deny_providers,
        &policy.deny_endpoint_hosts,
        &policy.deny_shell,
    ] {
        if patterns.len() > 64
            || patterns
                .iter()
                .any(|p| p.is_empty() || p.len() > 256 || p.contains('\0'))
        {
            return Err("Global policy has invalid patterns; refusing the operation".into());
        }
    }
    Ok(policy)
}
fn matches(pattern: &str, value: &str) -> bool {
    // Bounded-pattern wildcard matcher; only the most recent star backtracks.
    let p = pattern.as_bytes();
    let v = value.as_bytes();
    let (mut i, mut j, mut star, mut mark) = (0, 0, None, 0);
    while j < v.len() {
        if i < p.len() && (p[i] == b'?' || p[i] == v[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == b'*' {
            star = Some(i);
            i += 1;
            mark = j;
        } else if let Some(s) = star {
            mark += 1;
            j = mark;
            i = s + 1;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == b'*' {
        i += 1;
    }
    i == p.len()
}
fn check(patterns: &[String], value: &str, kind: &str) -> Result<(), String> {
    if let Some(pattern) = patterns.iter().find(|p| matches(p, value)) {
        return Err(format!(
            "Blocked by global {kind} policy: {}. Project settings and session approvals cannot override it.",
            glass_tui::markdown::sanitize(pattern)
        ));
    }
    Ok(())
}
pub fn provider(kind: &str, endpoint: &str) -> Result<(), String> {
    let p = load()?;
    check(&p.deny_providers, kind, "provider")?;
    let host = url::Url::parse(endpoint)
        .map_err(|_| "Invalid endpoint")?
        .host_str()
        .unwrap_or("")
        .to_owned();
    check(&p.deny_endpoint_hosts, &host, "endpoint")
}
pub fn shell(command: &str) -> Result<(), String> {
    check(&load()?.deny_shell, command, "shell")
}
pub fn command(args: &[String]) -> Result<(), String> {
    let json_output = args.iter().any(|a| a == "--json");
    let args: Vec<String> = args
        .iter()
        .filter(|a| a.as_str() != "--json")
        .cloned()
        .collect();
    let expected = match args.first().map(String::as_str) {
        None => 0,
        Some("shell") => 2,
        Some("provider") => 3,
        _ => return Err("gc policy [shell COMMAND | provider KIND ENDPOINT] [--json]".into()),
    };
    if args.len() != expected {
        return Err("Incorrect policy arguments; use gc policy --help".into());
    }
    let p = load()?;
    let value = match args.first().map(String::as_str) {
        None => {
            serde_json::json!({"path":path(),"deny_providers":p.deny_providers,"deny_endpoint_hosts":p.deny_endpoint_hosts,"deny_shell":p.deny_shell,"scope":"Global only. Shell patterns match command text, not shell semantics. Use the kernel sandbox for actual filesystem/network confinement. Existing external processes and third-party sidecars are outside this policy guard."})
        }
        Some("shell") => {
            let command = args.get(1).ok_or("gc policy shell COMMAND")?;
            let result = shell(command);
            serde_json::json!({"decision":if result.is_ok(){"no global denial; normal permissions still apply"}else{"deny"},"reason":result.err()})
        }
        Some("provider") => {
            let kind = args.get(1).ok_or("gc policy provider KIND ENDPOINT")?;
            let endpoint = args.get(2).ok_or("gc policy provider KIND ENDPOINT")?;
            let result = provider(kind, endpoint);
            serde_json::json!({"decision":if result.is_ok(){"no global denial"}else{"deny"},"reason":result.err()})
        }
        _ => {
            return Err(
                "Usage: gc policy [shell COMMAND | provider KIND ENDPOINT] [--json]".into(),
            );
        }
    };
    if json_output {
        crate::emit_json(value);
    } else {
        crate::cli::report("Global policy", &value);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wildcards_are_full_value() {
        assert!(matches("git push*", "git push origin main"));
        assert!(matches("*secret*", "read secret.txt"));
        assert!(!matches("git push", "echo git push"));
        assert!(matches("a?c", "abc"));
        assert!(!matches("*x", ""));
    }
}
