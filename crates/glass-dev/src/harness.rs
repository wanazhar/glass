//! Discovery and safe handoff metadata for installed coding harnesses.
//!
//! Glass owns the workspace, trust boundary, and resident Pi agent. Other
//! coding harnesses remain their own interactive programs, so the TUI only
//! discovers fixed executable names and hands the terminal to a selected
//! program. It never interpolates user-provided commands or emulates another
//! harness's protocol.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

/// Fixed metadata for a supported external coding harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HarnessSpec {
    /// Stable machine-readable identifier.
    pub id: &'static str,
    /// Human-readable display label.
    pub label: &'static str,
    /// Executable basename searched on `PATH`.
    pub binary: &'static str,
    /// Short description shown in discovery summaries.
    pub description: &'static str,
}

/// PATH discovery result for one harness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessStatus {
    /// Harness metadata from [`SPECS`].
    pub spec: HarnessSpec,
    /// Resolved executable path, when found and executable.
    pub path: Option<PathBuf>,
}

/// A validated harness launch target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedHarness {
    /// Harness metadata.
    pub spec: HarnessSpec,
    /// Executable path selected from `PATH`.
    pub path: PathBuf,
}

/// Fixed launch catalog. Discovery is PATH-only; no network or version probe
/// runs during TUI refreshes.
pub const SPECS: &[HarnessSpec] = &[
    HarnessSpec {
        id: "amp",
        label: "Amp",
        binary: "amp",
        description: "threads, review mode, and supervised orb services",
    },
    HarnessSpec {
        id: "aider",
        label: "Aider",
        binary: "aider",
        description: "repo map, checkpoints, and lint/test loops",
    },
    HarnessSpec {
        id: "claude",
        label: "Claude Code",
        binary: "claude",
        description: "permission modes, hooks, and subagents",
    },
    HarnessSpec {
        id: "codex",
        label: "Codex CLI",
        binary: "codex",
        description: "sandboxed local work and resumable tasks",
    },
    HarnessSpec {
        id: "gemini",
        label: "Gemini CLI",
        binary: "gemini",
        description: "extensions, MCP, and interactive sessions",
    },
    HarnessSpec {
        id: "goose",
        label: "Goose",
        binary: "goose",
        description: "recipes, extensions, and session workflows",
    },
    HarnessSpec {
        id: "kiro",
        label: "Kiro CLI",
        binary: "kiro-cli",
        description: "spec-driven tasks, steering, and hooks",
    },
    HarnessSpec {
        id: "opencode",
        label: "OpenCode",
        binary: "opencode",
        description: "local TUI/server sessions and GitHub workflows",
    },
    HarnessSpec {
        id: "pi",
        label: "Pi",
        binary: "pi",
        description: "interactive sessions, extensions, and provider login",
    },
    HarnessSpec {
        id: "qwen",
        label: "Qwen Code",
        binary: "qwen",
        description: "MCP, extensions, and resumable coding sessions",
    },
    HarnessSpec {
        id: "cursor",
        label: "Cursor Agent",
        binary: "cursor-agent",
        description: "background agents, rules, and worktree workflows",
    },
    HarnessSpec {
        id: "windsurf",
        label: "Windsurf",
        binary: "windsurf",
        description: "interactive coding sessions and remote workflows",
    },
];

/// Return the fixed supported-harness catalog.
pub fn specs() -> &'static [HarnessSpec] {
    SPECS
}

/// Discover supported harnesses using the process `PATH`.
pub fn discover() -> Vec<HarnessStatus> {
    discover_from_path(std::env::var_os("PATH").as_deref())
}

fn discover_from_path(path: Option<&OsStr>) -> Vec<HarnessStatus> {
    SPECS
        .iter()
        .copied()
        .map(|spec| HarnessStatus {
            spec,
            path: executable_on_path(spec.binary, path),
        })
        .collect()
}

/// Format a compact availability summary without probing versions or network.
pub fn summary() -> String {
    let statuses = discover();
    summary_for(&statuses)
}

fn summary_for(statuses: &[HarnessStatus]) -> String {
    statuses
        .iter()
        .map(|status| {
            let marker = if status.path.is_some() { "●" } else { "○" };
            format!(
                "{marker} {:<9} {} · {}",
                status.spec.id, status.spec.label, status.spec.description
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Resolve a case-insensitive id, label, or binary and require it on `PATH`.
pub fn resolve(name: &str) -> Result<ResolvedHarness, String> {
    let spec = SPECS
        .iter()
        .copied()
        .find(|spec| {
            spec.id.eq_ignore_ascii_case(name)
                || spec.binary.eq_ignore_ascii_case(name)
                || spec.label.eq_ignore_ascii_case(name)
        })
        .ok_or_else(|| {
            format!(
                "unknown harness `{name}`; choose one of {}",
                SPECS
                    .iter()
                    .map(|spec| spec.id)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
    let path = executable_on_path(spec.binary, std::env::var_os("PATH").as_deref())
        .ok_or_else(|| format!("{} is not installed or not on PATH", spec.label))?;
    Ok(ResolvedHarness { spec, path })
}

/// Launch a resolved harness in the requested project directory.
pub fn launch_resolved(resolved: &ResolvedHarness, root: &Path) -> Result<ExitStatus, String> {
    Command::new(&resolved.path)
        .current_dir(root)
        .status()
        .map_err(|error| format!("{} failed to start: {error}", resolved.spec.label))
}

fn executable_on_path(binary: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    let path = path?;
    for directory in std::env::split_paths(path) {
        let candidate = directory.join(binary);
        if is_executable(&candidate) {
            return Some(candidate);
        }
        #[cfg(windows)]
        for extension in [".exe", ".cmd", ".bat"] {
            let candidate = directory.join(format!("{binary}{extension}"));
            if is_executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn is_executable(path: &std::path::Path) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn catalog_covers_requested_harness_families() {
        for name in [
            "amp", "aider", "claude", "codex", "gemini", "goose", "kiro", "opencode", "pi", "qwen",
            "cursor", "windsurf",
        ] {
            assert!(SPECS.iter().any(|spec| spec.id == name), "missing {name}");
        }
    }

    #[test]
    fn resolves_case_insensitive_ids_and_binary_aliases() {
        assert_eq!(
            SPECS
                .iter()
                .find(|spec| spec.id == "kiro")
                .map(|spec| spec.binary),
            Some("kiro-cli")
        );
        assert_eq!(
            SPECS
                .iter()
                .copied()
                .find(|spec| {
                    spec.id.eq_ignore_ascii_case("OpenCode")
                        || spec.binary.eq_ignore_ascii_case("OpenCode")
                })
                .map(|spec| spec.id),
            Some("opencode")
        );
    }

    #[test]
    fn discovery_requires_an_executable_file_on_path() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("glass-harness-{suffix}"));
        fs::create_dir_all(&root).expect("create fixture");
        let pi = root.join("pi");
        fs::write(&pi, "#!/bin/sh\n").expect("write fixture");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&pi, fs::Permissions::from_mode(0o755))
                .expect("make fixture executable");
        }
        let statuses = discover_from_path(Some(root.as_os_str()));
        assert!(
            statuses
                .iter()
                .find(|status| status.spec.id == "pi")
                .and_then(|status| status.path.as_ref())
                .is_some()
        );
        assert!(
            statuses
                .iter()
                .find(|status| status.spec.id == "amp")
                .is_some_and(|status| status.path.is_none())
        );
        let _ = fs::remove_dir_all(root);
    }
}
