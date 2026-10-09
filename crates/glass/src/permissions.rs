//! Security review helpers for the shared-key-reader UI.
//!
//! The dispatcher owns persistence and restart handling. This module returns
//! explicit user choices and never changes runtime confinement behind its back.

use crate::{config, input::KeyReader, modal, sandbox};
use std::sync::{Mutex, OnceLock};

static SESSION_APPROVALS: OnceLock<Mutex<sandbox::SessionApprovals>> = OnceLock::new();

fn approval_store() -> &'static Mutex<sandbox::SessionApprovals> {
    SESSION_APPROVALS.get_or_init(|| Mutex::new(sandbox::SessionApprovals::default()))
}

/// Store one explicit exact-command exception in process memory only.
pub fn approve_session_command(command: &str) -> Result<(), String> {
    let mut approvals = approval_store()
        .lock()
        .map_err(|_| "session approval store is unavailable".to_string())?;
    approvals.approve_exact(command);
    Ok(())
}

/// Build the foundation's exact-match approval object for one child command.
pub fn approvals_for_command(command: &str) -> Result<sandbox::SessionApprovals, String> {
    let approved = approval_store()
        .lock()
        .map_err(|_| "session approval store is unavailable".to_string())?
        .permits_exact(command);
    let mut approvals = sandbox::SessionApprovals::default();
    if approved {
        approvals.approve_exact(command);
    }
    Ok(approvals)
}

pub fn clear_session_approvals() -> Result<(), String> {
    let mut approvals = approval_store()
        .lock()
        .map_err(|_| "session approval store is unavailable".to_string())?;
    *approvals = sandbox::SessionApprovals::default();
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewAction {
    SelectProfile(sandbox::PermissionProfile),
    ApproveExactCommand(String),
    Close,
}

/// Persist only the opt-in profile field while preserving other TOML text.
pub fn persist_profile(
    path: &std::path::Path,
    profile: sandbox::PermissionProfile,
) -> Result<(), String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return Err("cannot read config.toml; profile was not changed".into()),
    };
    if !text.is_empty() && toml::from_str::<toml::Value>(&text).is_err() {
        return Err("config.toml is invalid; profile was not changed".into());
    }
    let value = match profile {
        sandbox::PermissionProfile::Unrestricted => "unrestricted",
        sandbox::PermissionProfile::WorkspaceEdit => "workspace-edit",
        sandbox::PermissionProfile::ReadOnly => "read-only",
    };
    let mut found = false;
    let mut in_root = true;
    let mut output = String::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            in_root = false;
        }
        if in_root
            && let Some((key, _)) = trimmed.split_once('=')
            && key.trim() == "permission_profile"
        {
            let indent = &line[..line.len() - trimmed.len()];
            let comment = line.find('#').map(|index| &line[index..]).unwrap_or("");
            output.push_str(&format!("{indent}permission_profile = \"{value}\""));
            if !comment.is_empty() {
                output.push(' ');
                output.push_str(comment);
            }
            output.push('\n');
            found = true;
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    if !found {
        // TOML keys after a table header belong to that table. Insert a new
        // root key before the first section instead of appending at EOF.
        if let Some(insert_at) = output
            .starts_with('[')
            .then_some(0)
            .or_else(|| output.find("\n[").map(|section| section + 1))
        {
            output.insert_str(
                insert_at,
                &format!("permission_profile = \"{value}\"\n\n"),
            );
        } else {
            if !output.is_empty() && !output.ends_with("\n\n") {
                output.push('\n');
            }
            output.push_str(&format!("permission_profile = \"{value}\"\n"));
        }
    }
    crate::auth::secure_write(path, output.as_bytes())
        .map_err(|_| String::from("cannot save config.toml; profile was not changed"))?;
    Ok(())
}

/// Review one command exception. The exact bytes typed are returned for the
/// caller to place in the in-memory session approval set after this consent.
pub fn approve_exact_command(keys: &KeyReader) -> Option<String> {
    let command = modal::field(
        keys,
        "Approve one exact command",
        "The approval applies only to this exact command text and this session.",
        "",
        false,
    )?;
    if command.trim().is_empty() || command.len() > 16_384 || command.contains('\0') {
        modal::info(
            keys,
            "Command not approved",
            "Enter a non-empty command under 16 KiB.",
        );
        return None;
    }
    let options = [
        "Approve this exact command for this session".into(),
        "Cancel".into(),
    ];
    (modal::choose(keys, "Review command exception", &command, &options) == Some(0))
        .then_some(command)
}

fn capabilities_summary(label: &str, spec: &str, workspace: &std::path::Path) -> String {
    match sandbox::inspect_extension_capabilities(spec, workspace) {
        Ok(capabilities) => format!(
            "{label}\nFilesystem reads: {}\nFilesystem writes: {}\nTCP destination ports: {}\nSubprocesses: {}\n\nNetwork policy is port-only, not hostname filtering. Subprocess limits depend on the active OS confinement support.",
            show_paths(&capabilities.filesystem.read),
            show_paths(&capabilities.filesystem.write),
            if capabilities.network.is_empty() {
                "none".into()
            } else {
                capabilities
                    .network
                    .iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            },
            if capabilities.subprocess {
                "declared"
            } else {
                "not declared"
            },
        ),
        Err(error) => format!(
            "{label}\nCapability declaration unavailable or invalid:\n{error}\n\nStrict isolation refuses missing declarations. Inspect the sidecar before enabling this extension."
        ),
    }
}

fn show_paths(paths: &[std::path::PathBuf]) -> String {
    if paths.is_empty() {
        "none".into()
    } else {
        paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn profile_text(cfg: &config::Config) -> String {
    let configured = permission_profile_name(cfg.permission_profile);
    let effective = sandbox::permission_profile_label();
    let limits = match cfg.permission_profile {
        sandbox::PermissionProfile::Unrestricted => {
            "Commands retain the user's normal OS permissions. This is the legacy default."
        }
        sandbox::PermissionProfile::WorkspaceEdit => {
            "Writes are confined to the workspace and temporary directories where supported."
        }
        sandbox::PermissionProfile::ReadOnly => {
            "Writes are denied by the OS where supported; unsupported requested confinement refuses execution."
        }
    };
    format!(
        "Configured for next start: {configured}\nEffective in this process: {effective}\nNext-start behavior: {limits}\n\nA profile change is saved to config.toml and takes effect after restarting glass-core. Approved exact commands are in-memory and scoped to this conversation."
    )
}

fn permission_profile_name(profile: sandbox::PermissionProfile) -> &'static str {
    match profile {
        sandbox::PermissionProfile::Unrestricted => "unrestricted",
        sandbox::PermissionProfile::WorkspaceEdit => "workspace-edit",
        sandbox::PermissionProfile::ReadOnly => "read-only",
    }
}

/// Shared security-review modal. It uses the caller's KeyReader and returns
/// choices to the caller; it does not persist config or alter the sandbox.
pub fn review(keys: &KeyReader, cfg: &config::Config, extension_specs: &[String]) -> ReviewAction {
    loop {
        let entries = [
            "Review permission profile".to_string(),
            "Select permission profile for next session".to_string(),
            "Approve exact command for this session".to_string(),
            "Inspect extension capability declarations".to_string(),
            "Close".to_string(),
        ];
        match modal::choose(
            keys,
            "Security and permissions",
            &profile_text(cfg),
            &entries,
        ) {
            Some(0) => modal::info(keys, "Permission profile", &profile_text(cfg)),
            Some(1) => {
                let choices = [
                    "Unrestricted · legacy default".into(),
                    "Workspace edit · workspace and temp writes".into(),
                    "Read only · deny writes where supported".into(),
                ];
                if let Some(i) = modal::choose(
                    keys,
                    "Permission profile for next session",
                    "This selection is saved for the next glass-core start. The active permission policy cannot change until restart.",
                    &choices,
                ) {
                    let profile = match i {
                        0 => sandbox::PermissionProfile::Unrestricted,
                        1 => sandbox::PermissionProfile::WorkspaceEdit,
                        _ => sandbox::PermissionProfile::ReadOnly,
                    };
                    return ReviewAction::SelectProfile(profile);
                }
            }
            Some(2) => {
                if sandbox::permission_profile_label() == "unrestricted"
                    && sandbox::policy() == sandbox::Policy::Off
                {
                    modal::info(
                        keys,
                        "No active command sandbox",
                        "This session is unrestricted, so an exact-command exception would not change command access. Select a confined profile in config.toml and restart first.",
                    );
                    continue;
                }
                if let Some(command) = approve_exact_command(keys) {
                    return ReviewAction::ApproveExactCommand(command);
                }
            }
            Some(3) => {
                if extension_specs.is_empty() {
                    modal::info(keys, "Extension capabilities", "No extensions are enabled.");
                } else {
                    let rows: Vec<String> = extension_specs
                        .iter()
                        .enumerate()
                        .map(|(index, spec)| {
                            let status =
                                sandbox::inspect_extension_capabilities(spec, &cfg.workspace)
                                    .map(|_| "declared")
                                    .unwrap_or("missing or invalid");
                            format!("Extension {} · {status}", index + 1)
                        })
                        .collect();
                    if let Some(i) = modal::choose(
                        keys,
                        "Extension capabilities",
                        "Review declared needs before enabling.",
                        &rows,
                    ) {
                        let label = format!("Extension {}", i + 1);
                        modal::info(
                            keys,
                            "Capability declaration",
                            &capabilities_summary(&label, &extension_specs[i], &cfg.workspace),
                        );
                    }
                }
            }
            _ => return ReviewAction::Close,
        }
    }
}

/// Show declared capabilities and get an explicit go-ahead before spawning an
/// extension from the extension manager.
pub fn confirm_extension_enable(
    keys: &KeyReader,
    spec: &str,
    workspace: &std::path::Path,
) -> bool {
    let summary = capabilities_summary("Extension capability review", spec, workspace);
    modal::info(keys, "Review extension capabilities", &summary);
    modal::choose(
        keys,
        "Enable extension",
        "The capability declaration is inspected before startup. Strict mode refuses a missing or invalid declaration.",
        &["Continue to enable review".into(), "Cancel".into()],
    ) == Some(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "glass-profile-{}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn profile_persistence_inserts_a_root_key_before_tables_and_preserves_config() {
        let path = temp_path();
        std::fs::write(&path, "provider = \"openai\"\n\n[connection]\nname = \"local\"\n")
            .unwrap();
        persist_profile(&path, sandbox::PermissionProfile::ReadOnly).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let parsed: toml::Value = toml::from_str(&text).unwrap();
        assert_eq!(parsed["permission_profile"].as_str(), Some("read-only"));
        assert_eq!(parsed["connection"]["name"].as_str(), Some("local"));
        persist_profile(&path, sandbox::PermissionProfile::WorkspaceEdit).unwrap();
        let parsed: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed["permission_profile"].as_str(), Some("workspace-edit"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn exact_command_approval_is_session_scoped_and_byte_exact() {
        clear_session_approvals().unwrap();
        assert!(!approvals_for_command("cargo test").unwrap().permits_exact("cargo test"));
        approve_session_command("cargo test").unwrap();
        assert!(approvals_for_command("cargo test").unwrap().permits_exact("cargo test"));
        assert!(!approvals_for_command("cargo test ").unwrap().permits_exact("cargo test "));
        clear_session_approvals().unwrap();
        assert!(!approvals_for_command("cargo test").unwrap().permits_exact("cargo test"));
    }
}
