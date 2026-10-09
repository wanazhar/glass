//! Security review helpers for the shared-key-reader UI.
//!
//! The dispatcher owns persistence and restart handling. This module returns
//! explicit user choices and never changes runtime confinement behind its back.

use crate::{config, input::KeyReader, modal, sandbox};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewAction {
    SelectProfile(sandbox::PermissionProfile),
    ApproveExactCommand(String),
    Close,
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
    let profile = sandbox::permission_profile_label();
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
        "Configured profile: {profile}\n{limits}\n\nA profile change must be saved to config.toml and takes effect after restarting the session. Approved exact commands are in-memory and session scoped."
    )
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
                    "This selection must be saved and takes effect after restart.",
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
