//! Read-only local diagnostics shared by CLI JSON and the doctor modal.
//!
//! This report deliberately contains labels and states only. It never includes
//! configured URLs, paths, credential values, connection names, or raw errors.

use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Check {
    pub area: &'static str,
    pub status: &'static str,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Report {
    pub schema: u32,
    pub checks: Vec<Check>,
}

impl Report {
    pub fn json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{\"schema\":1,\"checks\":[]}".into())
    }

    pub fn text(&self) -> String {
        self.checks
            .iter()
            .map(|c| format!("{}: {} — {}", c.area, c.status, c.detail))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Inspect already-loaded config and local files. Does not repair, create, or
/// modify anything and does not contact providers or extension processes.
pub fn inspect(cfg: &crate::config::Config, config_path: &Path, session_dir: &Path) -> Report {
    let mut checks = Vec::new();
    let tty = std::io::IsTerminal::is_terminal(&std::io::stdout());
    let term = std::env::var("TERM")
        .ok()
        .filter(|v| !v.is_empty() && v != "dumb");
    checks.push(Check {
        area: "terminal",
        status: if tty && term.is_some() {
            "ok"
        } else {
            "notice"
        },
        detail: if tty && term.is_some() {
            "interactive terminal detected".into()
        } else {
            "terminal capabilities may be limited".into()
        },
    });

    checks.push(Check {
        area: "configuration",
        status: if config_path.is_file() {
            "ok"
        } else {
            "notice"
        },
        detail: if config_path.is_file() {
            "configuration loaded".into()
        } else {
            "defaults active; config file not present".into()
        },
    });
    let env_credential_present = cfg
        .api_key_env
        .as_deref()
        .is_some_and(|name| std::env::var(name).is_ok_and(|v| !v.trim().is_empty()));
    let auth_status = if cfg.api_key_env.is_some() {
        if env_credential_present {
            "ok"
        } else {
            "notice"
        }
    } else if cfg.connection.is_some() {
        "notice"
    } else {
        "ok"
    };
    checks.push(Check {
        area: "authentication",
        status: auth_status,
        detail: if cfg.api_key_env.is_none() && cfg.connection.is_some() {
            "saved account reference configured; offline validity not checked".into()
        } else if cfg.api_key_env.is_none() {
            "no API key environment variable or account reference configured".into()
        } else if env_credential_present {
            "credential source appears available".into()
        } else {
            "configured credential environment variable is empty or missing".into()
        },
    });

    let policy = match cfg.sandbox {
        crate::sandbox::Policy::Off => "off",
        crate::sandbox::Policy::Workspace => "workspace",
        crate::sandbox::Policy::Strict => "strict",
    };
    checks.push(Check {
        area: "sandbox",
        status: if policy == "off" { "notice" } else { "ok" },
        detail: format!("shell policy: {policy}; extension confinement is not probed"),
    });

    let missing = cfg
        .extensions
        .iter()
        .filter(|item| !executable_available(item))
        .count();
    checks.push(Check {
        area: "extensions",
        status: if missing == 0 { "ok" } else { "notice" },
        detail: if cfg.extensions.is_empty() {
            "none configured".into()
        } else if missing == 0 {
            format!("{} configured reference(s) available", cfg.extensions.len())
        } else {
            format!(
                "{} configured reference(s); {missing} unavailable",
                cfg.extensions.len()
            )
        },
    });

    let (status, detail) = session_integrity(session_dir);
    checks.push(Check {
        area: "sessions",
        status,
        detail,
    });
    Report { schema: 1, checks }
}

fn executable_available(reference: &str) -> bool {
    let path = Path::new(reference);
    if path.components().count() > 1 || path.is_absolute() {
        return is_executable(path);
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .any(|dir| is_executable(&dir.join(reference)))
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn session_integrity(dir: &Path) -> (&'static str, String) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return ("ok", "session directory not created yet".into());
        }
        Err(_) => return ("error", "session directory cannot be read".into()),
    };
    let mut files = 0usize;
    let mut malformed = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "jsonl") {
            files += 1;
            match std::fs::File::open(&path) {
                Ok(file) => {
                    use std::io::BufRead;
                    for line in std::io::BufReader::new(file).split(b'\n') {
                        match line {
                            Ok(line) if line.iter().all(u8::is_ascii_whitespace) => {}
                            Ok(line)
                                if serde_json::from_slice::<serde_json::Value>(&line).is_ok() => {}
                            _ => malformed += 1,
                        }
                    }
                }
                Err(_) => malformed += 1,
            }
        } else if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().ends_with(".live.json"))
        {
            if std::fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                .is_none()
            {
                malformed += 1;
            }
        }
    }
    if malformed == 0 {
        (
            "ok",
            format!("{files} session log(s) checked; no malformed records"),
        )
    } else {
        (
            "notice",
            format!(
                "{files} session log(s) checked; {malformed} malformed record or sidecar(s) found; resume may require recovery"
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config, sandbox};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "gc-doctor-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn reports_local_integrity_without_echoing_secrets_or_paths() {
        let root = scratch();
        let sessions = root.join("sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(sessions.join("s.jsonl"), b"{\"type\":\"user\"}\nnot-json\n").unwrap();
        std::fs::write(sessions.join("s.live.json"), b"incomplete sidecar").unwrap();
        let mut cfg = config::Config::default();
        cfg.base_url = "https://secret.example/path?token=do-not-print".into();
        cfg.api_key_env = Some("GC_DOCTOR_SECRET_ENV_NAME".into());
        cfg.sandbox = sandbox::Policy::Off;
        let report = inspect(&cfg, &root.join("config.toml"), &sessions);
        let output = report.json();
        assert!(output.contains("2 malformed record"));
        assert!(!output.contains("secret.example"));
        assert!(!output.contains("do-not-print"));
        assert!(!output.contains("GC_DOCTOR_SECRET_ENV_NAME"));
        assert!(!output.contains(root.to_str().unwrap()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn absent_data_is_healthy_and_read_only() {
        let root = scratch();
        let cfg = config::Config::default();
        let r = inspect(&cfg, &root.join("config.toml"), &root.join("missing"));
        assert_eq!(r.checks.last().unwrap().status, "ok");
        assert!(!root.exists());
    }

    #[cfg(unix)]
    #[test]
    fn extension_reference_requires_executable_permission() {
        use std::os::unix::fs::PermissionsExt;
        let root = scratch();
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("extension");
        std::fs::write(&target, "fixture").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(!is_executable(&target));
        assert!(!executable_available(target.to_str().unwrap()));
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(is_executable(&target));
        assert!(executable_available(target.to_str().unwrap()));
        assert!(
            executable_available("sh"),
            "PATH lookup should require a runnable file"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
