//! Optional OS-level confinement for shell commands.
//!
//! glass-core ships no permission system on purpose: `bash` runs what the
//! model asks for, and the README says to run it somewhere you are willing to
//! lose. That is the right default for a single-user tool, but it leaves
//! unattended runs and prompt-injection scenarios unbounded. This module is
//! the opt-in middle ground: kernel-enforced confinement that needs no
//! prompts, ported from Codex's sandbox modes (research/harness-features-2026.md).
//!
//! Three policies:
//!
//! - `off` (default): exactly the old behavior.
//! - `workspace`: writes only under the workspace, the temp dir, the glass
//!   data directory, the user's cache/toolchain dirs (`~/.cache`, `~/.cargo`),
//!   and `/dev/null`. Reads are unrestricted.
//! - `strict`: the write-jail plus a read/execute allowlist (system dirs, the
//!   toolchain, the workspace). Anything outside it fails — deliberately
//!   breakage-tolerant, documented as such.
//!
//! Linux uses Landlock (kernel 5.13+; raw syscalls, no dependency beyond the
//! `libc` the crate already uses). macOS wraps commands with `sandbox-exec`.
//!
//! With confinement on, two more boundaries apply: outbound TCP is limited to
//! an allowlist of ports (Landlock ABI 4, Linux 6.7+; per-port is all the
//! kernel offers — no host filtering, no localhost-only), and credential-ish
//! environment variables are removed from shell children so the model cannot
//! read the harness's own API keys out of `env`. A platform that cannot
//! deliver the requested policy warns once; `strict` refuses to run commands
//! at all rather than silently downgrading.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use crate::config;

/// What shell commands may touch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Policy {
    /// No confinement. The shipped default.
    #[default]
    Off,
    /// Writes only under the workspace, temp, caches, and the glass data dir.
    Workspace,
    /// Write-jail plus a read/execute allowlist.
    Strict,
}

impl Policy {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "none" => Some(Policy::Off),
            "workspace" => Some(Policy::Workspace),
            "strict" => Some(Policy::Strict),
            _ => None,
        }
    }
}

/// Model shell permission profile. It is separate from extension confinement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermissionProfile {
    #[default]
    Unrestricted,
    WorkspaceEdit,
    ReadOnly,
}

impl PermissionProfile {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "unrestricted" => Some(Self::Unrestricted),
            "workspace-edit" | "workspace_edit" => Some(Self::WorkspaceEdit),
            "read-only" | "read_only" => Some(Self::ReadOnly),
            _ => None,
        }
    }
}

/// Exact command approvals held only for the lifetime of one session.
#[derive(Debug, Default)]
#[allow(dead_code)] // host approval integration hook, intentionally not implicitly granted
pub struct SessionApprovals(std::collections::BTreeSet<String>);

#[allow(dead_code)] // exposed for the host approval UI to consume in the next integration step
impl SessionApprovals {
    pub fn approve_exact(&mut self, command: &str) {
        self.0.insert(command.to_owned());
    }
    pub fn permits_exact(&self, command: &str) -> bool {
        self.0.contains(command)
    }

    /// Check an exception request. A caller may show an interactive prompt
    /// only for `NeedsInteractiveApproval`; headless callers must surface the
    /// refusal and must not launch the command with a wider policy.
    pub fn decision(&self, command: &str, headless: bool) -> ApprovalDecision {
        if self.permits_exact(command) {
            ApprovalDecision::Approved
        } else if headless {
            ApprovalDecision::RefusedHeadless
        } else {
            ApprovalDecision::NeedsInteractiveApproval
        }
    }
}

/// Apply the configured jail or a previously approved exact-command
/// exception to this one child. Approval bypasses only this child's jail; it
/// never mutates the process-wide profile or approves another command.
#[allow(dead_code)] // consumed by the host's per-command approval integration
pub fn harden_command(
    cmd: &mut Command,
    command: &str,
    approvals: &SessionApprovals,
    exception_requested: bool,
    headless: bool,
) -> Result<(), String> {
    if !exception_requested {
        harden(cmd);
        return Ok(());
    }
    match approvals.decision(command, headless) {
        ApprovalDecision::Approved => Ok(()),
        ApprovalDecision::NeedsInteractiveApproval => Err(
            "this exact command needs an interactive permission decision before execution".into(),
        ),
        ApprovalDecision::RefusedHeadless => Err(
            "headless mode cannot approve this exact-command exception; retry in an interactive session".into(),
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ApprovalDecision {
    Approved,
    NeedsInteractiveApproval,
    RefusedHeadless,
}

struct State {
    policy: Policy,
    permission_profile: PermissionProfile,
    extension_strict: bool,
    /// Linux: prepared Landlock ruleset, applied in each child before exec.
    #[cfg(target_os = "linux")]
    jail: Option<std::os::fd::RawFd>,
    /// macOS: the SBPL profile handed to sandbox-exec.
    #[cfg(target_os = "macos")]
    profile: Option<String>,
    /// Set when `strict` was requested and the platform cannot deliver it:
    /// commands must be refused rather than run bare.
    refusal: Option<String>,
    /// Remove credential-ish variables from shell children.
    scrub_env: bool,
}

static STATE: OnceLock<State> = OnceLock::new();

impl State {
    fn off() -> Self {
        Self {
            policy: Policy::Off,
            permission_profile: PermissionProfile::Unrestricted,
            extension_strict: false,
            #[cfg(target_os = "linux")]
            jail: None,
            #[cfg(target_os = "macos")]
            profile: None,
            refusal: None,
            scrub_env: false,
        }
    }
}

/// Paths a confined command may write beneath.
fn writable_paths(workspace: &std::path::Path) -> Vec<PathBuf> {
    let mut out = vec![
        workspace.to_path_buf(),
        std::env::temp_dir(),
        PathBuf::from("/var/tmp"),
        config::data_dir(),
        config::config_dir(),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        // Build tools write caches here; jailing these away would make the
        // sandbox useless for the work it exists to make safe.
        out.push(home.join(".cache"));
        out.push(home.join(".cargo"));
    }
    out
}

fn workspace_edit_paths(workspace: &std::path::Path) -> Vec<PathBuf> {
    vec![
        workspace.to_path_buf(),
        std::env::temp_dir(),
        PathBuf::from("/var/tmp"),
    ]
}

/// Paths a `strict` command may read and execute beneath.
fn readable_paths(workspace: &std::path::Path) -> Vec<PathBuf> {
    let mut out = vec![
        workspace.to_path_buf(),
        PathBuf::from("/usr"),
        PathBuf::from("/bin"),
        PathBuf::from("/sbin"),
        PathBuf::from("/lib"),
        PathBuf::from("/lib64"),
        PathBuf::from("/etc"),
        PathBuf::from("/opt"),
        PathBuf::from("/run"),
        PathBuf::from("/dev"),
        PathBuf::from("/proc"),
        PathBuf::from("/sys"),
        std::env::temp_dir(),
        PathBuf::from("/var/tmp"),
        config::data_dir(),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        out.push(home.join(".cargo"));
        out.push(home.join(".rustup"));
        out.push(home.join(".cache"));
    }
    out
}

/// Initialise the process-wide sandbox state. Returns warnings to print.
pub fn init(cfg: &config::Config) -> Vec<String> {
    let mut warnings = Vec::new();
    let effective = match cfg.permission_profile {
        PermissionProfile::ReadOnly => Policy::Strict,
        PermissionProfile::WorkspaceEdit if cfg.sandbox == Policy::Off => Policy::Workspace,
        _ => cfg.sandbox,
    };
    let scrub = match cfg.sandbox_env.as_str() {
        "inherit" => false,
        "scrub" => true,
        // Default follows the sandbox: confinement on means the model should
        // not be able to read the harness's own credentials out of `env`.
        _ => effective != Policy::Off,
    };
    let state = if effective == Policy::Off {
        State {
            permission_profile: cfg.permission_profile,
            scrub_env: scrub,
            ..State::off()
        }
    } else {
        let mut state = build_state(cfg, effective, &mut warnings);
        state.permission_profile = cfg.permission_profile;
        state.scrub_env = scrub;
        state
    };
    let mut state = state;
    state.extension_strict = cfg.extension_isolation == "strict";
    let _ = STATE.set(state);
    warnings
}

#[cfg(target_os = "linux")]
fn build_state(cfg: &config::Config, policy: Policy, warnings: &mut Vec<String>) -> State {
    let strict = policy == Policy::Strict;
    let ports = cfg.sandbox_ports.as_slice();
    let writable = if cfg.permission_profile == PermissionProfile::ReadOnly {
        Vec::new()
    } else if cfg.permission_profile == PermissionProfile::WorkspaceEdit {
        workspace_edit_paths(&cfg.workspace)
    } else {
        writable_paths(&cfg.workspace)
    };
    match landlock::build(
        &writable,
        &readable_paths(&cfg.workspace),
        strict,
        Some(ports),
    ) {
        Ok((fd, notes)) => {
            warnings.extend(notes);
            State {
                policy,
                permission_profile: PermissionProfile::Unrestricted,
                jail: Some(fd),
                refusal: None,
                scrub_env: false,
                extension_strict: false,
            }
        }
        Err(e) => {
            if strict || cfg.permission_profile != PermissionProfile::Unrestricted {
                warnings.push(format!(
                    "sandbox = strict could not be set up ({e}); shell commands will be refused"
                ));
                State {
                    policy,
                    permission_profile: PermissionProfile::Unrestricted,
                    jail: None,
                    refusal: Some("the Landlock ruleset could not be created".into()),
                    scrub_env: false,
                    extension_strict: false,
                }
            } else {
                warnings.push(format!(
                    "sandbox = workspace unavailable ({e}); commands run unsandboxed"
                ));
                State::off()
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn build_state(cfg: &config::Config, policy: Policy, warnings: &mut Vec<String>) -> State {
    if !std::path::Path::new("/usr/bin/sandbox-exec").exists() {
        if policy == Policy::Strict || cfg.permission_profile != PermissionProfile::Unrestricted {
            warnings.push(
                "sandbox = strict needs /usr/bin/sandbox-exec; shell commands will be refused"
                    .into(),
            );
            return State {
                policy,
                permission_profile: PermissionProfile::Unrestricted,
                profile: None,
                refusal: Some("sandbox-exec is not available".into()),
                scrub_env: false,
                extension_strict: false,
            };
        }
        warnings.push("sandbox unavailable (no sandbox-exec); commands run unsandboxed".into());
        return State::off();
    }
    let profile = mac::profile(
        &cfg.workspace,
        &if cfg.permission_profile == PermissionProfile::ReadOnly {
            Vec::new()
        } else if cfg.permission_profile == PermissionProfile::WorkspaceEdit {
            workspace_edit_paths(&cfg.workspace)
        } else {
            writable_paths(&cfg.workspace)
        },
        &readable_paths(&cfg.workspace),
        policy == Policy::Strict,
        cfg.permission_profile == PermissionProfile::ReadOnly,
    );
    warnings.push(
        "sandbox: outbound network is not restricted on macOS in this version; \
         credential environment scrubbing still applies"
            .into(),
    );
    State {
        policy,
        permission_profile: PermissionProfile::Unrestricted,
        profile: Some(profile),
        refusal: None,
        scrub_env: false,
        extension_strict: false,
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn build_state(_cfg: &config::Config, policy: Policy, warnings: &mut Vec<String>) -> State {
    if policy == Policy::Strict || _cfg.permission_profile != PermissionProfile::Unrestricted {
        warnings.push(
            "sandbox = strict is not supported on this platform; commands will be refused".into(),
        );
        State {
            policy,
            permission_profile: PermissionProfile::Unrestricted,
            refusal: Some("unsupported platform".into()),
            scrub_env: false,
            extension_strict: false,
        }
    } else {
        warnings.push("sandbox is not supported on this platform; running unsandboxed".into());
        State::off()
    }
}

/// Build the shell command for one line, wrapped if the platform demands it.
pub fn shell(command: &str) -> Command {
    #[cfg(target_os = "macos")]
    if let Some(state) = STATE.get()
        && state.policy != Policy::Off
        && let Some(profile) = &state.profile
    {
        let mut cmd = Command::new("/usr/bin/sandbox-exec");
        cmd.arg("-p")
            .arg(profile)
            .arg("/bin/sh")
            .arg("-c")
            .arg(command);
        return cmd;
    }
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c").arg(command);
    cmd
}

/// Apply the kernel-side confinement to a child before it execs.
pub fn harden(cmd: &mut Command) {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        let Some(fd) = STATE.get().and_then(|s| s.jail) else {
            return;
        };
        // Runs after fork, before exec: syscalls only, no allocation on the
        // happy path, so it is safe in a pre-exec context.
        unsafe {
            cmd.pre_exec(move || landlock::enter(fd));
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = cmd;
}

/// Why commands must be refused, when `strict` could not be delivered.
pub fn refusal() -> Option<&'static str> {
    STATE.get().and_then(|s| s.refusal.as_deref())
}

/// Variable names that hand a shell child ambient credentials.
///
/// Reading the harness's own API key out of `env` is the cheapest possible
/// exfiltration path for a prompt-injected command, and shared build hosts
/// leak CI tokens the same way. The list errs toward stripping: a build that
/// needs a secret can be given one explicitly in the command.
pub(crate) fn is_sensitive(name: &str) -> bool {
    const EXACT: &[&str] = &[
        "SSH_AUTH_SOCK",
        "GITHUB_TOKEN",
        "GH_TOKEN",
        "NPM_TOKEN",
        "NODE_AUTH_TOKEN",
        "CARGO_REGISTRY_TOKEN",
        "DOCKER_AUTH_CONFIG",
        "NETRC",
    ];
    let upper = name.to_ascii_uppercase();
    if EXACT.contains(&upper.as_str()) {
        return true;
    }
    if upper.starts_with("AWS_")
        || upper.starts_with("GH_")
        || upper.starts_with("GITHUB_")
        || upper.starts_with("DOCKER_")
    {
        return true;
    }
    [
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "PASSWD",
        "API_KEY",
        "APIKEY",
        "ACCESS_KEY",
        "PRIVATE_KEY",
        "CREDENTIAL",
    ]
    .iter()
    .any(|needle| upper.contains(needle))
}

/// Remove sensitive variables from a shell child when scrubbing is on.
pub fn scrub_env(cmd: &mut Command) {
    if !STATE.get().is_some_and(|s| s.scrub_env) {
        return;
    }
    for (key, _) in std::env::vars_os() {
        if is_sensitive(&key.to_string_lossy()) {
            cmd.env_remove(key);
        }
    }
}

/// Remove credential-like variables unconditionally, for extension children.
/// Their grandchildren inherit this scrubbed environment.
pub fn scrub_sensitive_env(cmd: &mut Command) {
    let safe = std::env::vars_os()
        .filter(|(key, _)| !is_sensitive(&key.to_string_lossy()))
        .collect::<Vec<_>>();
    cmd.env_clear().envs(safe);
}

/// The effective policy, for diagnostics.
pub fn policy() -> Policy {
    STATE.get().map(|s| s.policy).unwrap_or(Policy::Off)
}

/// Whether strict extension declarations must be inspected before launch.
pub fn extension_isolation_strict() -> bool {
    STATE.get().is_some_and(|state| state.extension_strict)
}

/// Permission profile label for status and doctor surfaces.
#[allow(dead_code)]
pub fn permission_profile_label() -> &'static str {
    match STATE
        .get()
        .map(|state| state.permission_profile)
        .unwrap_or_default()
    {
        PermissionProfile::Unrestricted => "unrestricted",
        PermissionProfile::WorkspaceEdit => "workspace-edit",
        PermissionProfile::ReadOnly => "read-only",
    }
}

/// Extension confinement label for status and doctor surfaces.
#[allow(dead_code)]
pub fn extension_isolation_label() -> &'static str {
    if extension_isolation_strict() {
        "strict"
    } else {
        "off"
    }
}

/// A value-free summary suitable for diagnostic display.
#[allow(dead_code)]
pub fn security_summary() -> String {
    let refusal = refusal()
        .map(|reason| format!("; refused: {reason}"))
        .unwrap_or_default();
    format!(
        "permission_profile={}; extension_isolation={}{}",
        permission_profile_label(),
        extension_isolation_label(),
        refusal
    )
}

/// Filesystem and network access declared before an extension process starts.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionCapabilities {
    pub filesystem: ExtensionFilesystem,
    pub network: Vec<u16>,
    pub subprocess: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionFilesystem {
    pub read: Vec<PathBuf>,
    pub write: Vec<PathBuf>,
}

/// Read `<script-or-binary>.capabilities.json` before startup. Strict mode
/// refuses missing, invalid, or unresolvable declarations.
#[allow(dead_code)] // state-aware launch-readiness helper for the host integration
pub fn extension_capabilities(
    spec: &str,
    workspace: &std::path::Path,
) -> Result<Option<ExtensionCapabilities>, String> {
    if !extension_isolation_strict() {
        return Ok(None);
    }
    inspect_extension_capabilities(spec, workspace).map(Some)
}

/// Inspect and validate the pre-execution capability sidecar without launching
/// the extension. Status/enable flows can use this even while isolation is off.
#[allow(dead_code)] // host extension review integration hook
pub fn inspect_extension_capabilities(
    spec: &str,
    workspace: &std::path::Path,
) -> Result<ExtensionCapabilities, String> {
    #[cfg(target_os = "linux")]
    {
        let roots = landlock::PathRoots::new(workspace)?;
        let (caps, _sidecar, _entrypoint) = load_extension_capabilities(spec, &roots)?;
        let (_writes, _reads) = landlock::pin_capability_paths(&caps, &roots)?;
        Ok(caps)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err("safe capability path inspection requires Linux openat2".into())
    }
}

#[cfg(target_os = "linux")]
fn load_extension_capabilities(
    spec: &str,
    roots: &landlock::PathRoots,
) -> Result<(ExtensionCapabilities, PathBuf, std::fs::File), String> {
    let parts = crate::ext::host::arguments(spec)?;
    let target = parts.last().ok_or("extension command is empty")?;
    let raw = PathBuf::from(target);
    let relative_with_path = !raw.is_absolute() && (target.contains('/') || target.contains('\\'));
    // A trailing argument is interpreted from the extension's configured
    // current directory (the workspace), even when it has no slash. Only a
    // bare one-token command is resolved through PATH.
    let path_is_executable = parts.len() == 1;
    let candidates = if raw.is_absolute() || relative_with_path || !path_is_executable {
        vec![raw.clone()]
    } else {
        std::env::var_os("PATH")
            .into_iter()
            .flat_map(|paths| {
                std::env::split_paths(&paths)
                    .map(|dir| dir.join(&raw))
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    #[cfg(target_os = "linux")]
    {
        let mut last_error = None;
        for candidate in candidates {
            match roots.open(&candidate, libc::O_PATH | libc::O_CLOEXEC) {
                Ok(entrypoint) => {
                    if entrypoint
                        .metadata()
                        .map_err(|error| error.to_string())?
                        .file_type()
                        .is_symlink()
                    {
                        last_error =
                            Some(format!("entrypoint {} is a symlink", candidate.display()));
                        continue;
                    }
                    let mut sidecar_name = candidate.as_os_str().to_os_string();
                    sidecar_name.push(".capabilities.json");
                    let sidecar = PathBuf::from(sidecar_name);
                    let descriptor = roots.open(&sidecar, libc::O_RDONLY | libc::O_CLOEXEC)
                        .map_err(|error| format!("strict extension isolation requires capability declaration {} as a regular no-symlink file: {error}", sidecar.display()))?;
                    let caps = parse_pinned_sidecar(descriptor, &sidecar)?;
                    return Ok((caps, sidecar, entrypoint));
                }
                Err(error) => last_error = Some(error),
            }
        }
        Err(format!(
            "strict extension isolation cannot safely resolve entrypoint `{target}` or its sidecar: {}",
            last_error.unwrap_or_else(|| "no candidate path".into())
        ))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = candidates;
        Err("strict extension capability inspection requires Linux openat2".into())
    }
}

#[cfg(target_os = "linux")]
fn parse_pinned_sidecar(
    mut descriptor: std::fs::File,
    sidecar: &std::path::Path,
) -> Result<ExtensionCapabilities, String> {
    let metadata = descriptor.metadata().map_err(|error| {
        format!(
            "cannot inspect extension capability declaration {}: {error}",
            sidecar.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "extension capability declaration {} is not a regular file",
            sidecar.display()
        ));
    }
    if metadata.len() > 32 * 1024 {
        return Err(format!(
            "extension capability declaration {} exceeds 32 KiB",
            sidecar.display()
        ));
    }
    use std::io::Read;
    use std::os::unix::fs::MetadataExt;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    descriptor
        .by_ref()
        .take(32 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            format!(
                "cannot read pinned extension capability declaration {}: {error}",
                sidecar.display()
            )
        })?;
    if bytes.len() > 32 * 1024 {
        return Err(format!(
            "extension capability declaration {} exceeds 32 KiB",
            sidecar.display()
        ));
    }
    let after = descriptor
        .metadata()
        .map_err(|error| format!("cannot recheck capability declaration: {error}"))?;
    if bytes.len() as u64 != metadata.len()
        || metadata.dev() != after.dev()
        || metadata.ino() != after.ino()
        || metadata.len() != after.len()
        || metadata.mtime() != after.mtime()
        || metadata.mtime_nsec() != after.mtime_nsec()
        || metadata.ctime() != after.ctime()
        || metadata.ctime_nsec() != after.ctime_nsec()
    {
        return Err(format!(
            "extension capability declaration {} changed while it was read",
            sidecar.display()
        ));
    }
    serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "invalid extension capability declaration {}: {error}",
            sidecar.display()
        )
    })
}

/// Apply a per-extension jail in the child process. MCP subprocesses inherit
/// Landlock restrictions. Landlock cannot deny subprocess creation itself.
#[cfg(target_os = "linux")]
pub fn harden_extension(
    cmd: &mut Command,
    spec: &str,
    workspace: &std::path::Path,
) -> Result<Vec<String>, String> {
    use std::os::unix::process::CommandExt;
    if !extension_isolation_strict() {
        return Ok(Vec::new());
    }
    let roots = landlock::PathRoots::new(workspace)?;
    let (caps, _sidecar, entrypoint) = load_extension_capabilities(spec, &roots)?;
    if landlock::abi().is_none_or(|version| version < 4) {
        return Err("strict extension network capabilities require Landlock ABI 4 (Linux 6.7+); refusing downgrade".into());
    }
    let (fd, notes) = landlock::build_extension(&caps, &roots, entrypoint)?;
    use std::os::fd::{AsRawFd, FromRawFd};
    let fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) };
    unsafe {
        cmd.pre_exec(move || landlock::enter(fd.as_raw_fd()));
    }
    let process_note = if caps.subprocess {
        "extension subprocesses are allowed and inherit the same Landlock jail"
    } else {
        "Landlock cannot deny subprocess creation; any child inherits the same jail"
    };
    let mut notes = notes;
    notes.push(format!(
        "declared capabilities: read {:?}; write {:?}; TCP destination ports {:?}",
        caps.filesystem.read, caps.filesystem.write, caps.network
    ));
    notes.push("Landlock network rules constrain destination ports only; they do not filter destination hosts or local bind addresses".into());
    notes.push(process_note.into());
    let _ = workspace;
    Ok(notes)
}

#[cfg(not(target_os = "linux"))]
pub fn harden_extension(
    _cmd: &mut Command,
    spec: &str,
    workspace: &std::path::Path,
) -> Result<Vec<String>, String> {
    if extension_capabilities(spec, workspace)?.is_some() {
        return Err(
            "strict extension isolation requires Linux Landlock; refusing downgrade".into(),
        );
    }
    Ok(Vec::new())
}

#[cfg(target_os = "linux")]
mod landlock {
    //! Landlock via raw syscalls: enough of ABI 1–3 for a filesystem jail.

    use super::*;
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, RawFd};
    use std::os::unix::ffi::OsStrExt;

    // Landlock uses the same syscall numbers on x86_64 and aarch64.
    const SYS_CREATE_RULESET: libc::c_long = 444;
    const SYS_ADD_RULE: libc::c_long = 445;
    const SYS_RESTRICT_SELF: libc::c_long = 446;
    const SYS_OPENAT2: libc::c_long = 437;
    const CREATE_VERSION: u32 = 1 << 0;
    const RULE_PATH_BENEATH: u32 = 1;
    const RESOLVE_NO_MAGICLINKS: u64 = 0x02;
    const RESOLVE_NO_SYMLINKS: u64 = 0x04;
    const RESOLVE_BENEATH: u64 = 0x08;

    const EXECUTE: u64 = 1 << 0;
    const WRITE_FILE: u64 = 1 << 1;
    const READ_FILE: u64 = 1 << 2;
    const READ_DIR: u64 = 1 << 3;
    const REMOVE_FILE: u64 = 1 << 4;
    const REMOVE_DIR: u64 = 1 << 5;
    const MAKE_CHAR: u64 = 1 << 6;
    const MAKE_DIR: u64 = 1 << 7;
    const MAKE_REG: u64 = 1 << 8;
    const MAKE_SOCK: u64 = 1 << 9;
    const MAKE_FIFO: u64 = 1 << 10;
    const MAKE_BLOCK: u64 = 1 << 11;
    const MAKE_SYM: u64 = 1 << 12;
    const REFER: u64 = 1 << 13;
    const TRUNCATE: u64 = 1 << 14;

    // Network rights live in a separate `handled_access_net` field of the
    // ruleset attribute (ABI 4, Linux 6.7+). The bit values collide with the
    // filesystem rights numerically; the kernel disambiguates by rule type.
    const NET_CONNECT_TCP: u64 = 1 << 1;
    const RULE_NET_PORT: u32 = 2;

    const READ_RIGHTS: u64 = READ_FILE | READ_DIR;

    /// ABI version, if Landlock exists on this kernel.
    pub fn abi() -> Option<u32> {
        let v = unsafe {
            libc::syscall(
                SYS_CREATE_RULESET,
                std::ptr::null::<u8>(),
                0usize,
                CREATE_VERSION,
            )
        };
        (v >= 1).then_some(v as u32)
    }

    /// Stable roots shared by sidecar reading, capability resolution and
    /// Landlock rule installation. Relative paths always remain beneath the
    /// same pinned workspace inode, even if its pathname is renamed.
    pub struct PathRoots {
        filesystem: std::fs::File,
        workspace: std::fs::File,
    }

    impl PathRoots {
        pub fn new(workspace: &std::path::Path) -> Result<Self, String> {
            use std::os::fd::FromRawFd;
            let filesystem = unsafe {
                libc::open(
                    c"/".as_ptr(),
                    libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC,
                )
            };
            if filesystem < 0 {
                return Err(format!(
                    "cannot pin filesystem root: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let filesystem = unsafe { std::fs::File::from_raw_fd(filesystem) };
            let workspace_relative = workspace.strip_prefix("/").map_err(|_| {
                format!(
                    "workspace {} must be absolute under strict extension isolation",
                    workspace.display()
                )
            })?;
            if workspace_relative.as_os_str().is_empty() {
                return Err(
                    "strict extension isolation cannot use filesystem root as workspace".into(),
                );
            }
            let workspace = openat2_file(
                filesystem.as_raw_fd(),
                workspace_relative,
                libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
            .map_err(|error| {
                format!(
                    "cannot safely pin workspace {}: {error}",
                    workspace.display()
                )
            })?;
            Ok(Self {
                filesystem,
                workspace,
            })
        }

        pub fn open(
            &self,
            path: &std::path::Path,
            flags: libc::c_int,
        ) -> Result<std::fs::File, String> {
            let (root, relative) = if path.is_absolute() {
                (
                    self.filesystem.as_raw_fd(),
                    path.strip_prefix("/")
                        .map_err(|_| "invalid absolute path")?,
                )
            } else {
                (self.workspace.as_raw_fd(), path)
            };
            if relative.as_os_str().is_empty() || relative == std::path::Path::new(".") {
                if !path.is_absolute() {
                    return self
                        .workspace
                        .try_clone()
                        .map_err(|error| error.to_string());
                }
                return Err("strict extension capabilities cannot name filesystem root".into());
            }
            openat2_file(root, relative, flags | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .map_err(|error| format!("openat2 refused {}: {error}", path.display()))
        }

        fn capability_relative(&self, path: &std::path::Path) -> Result<PathBuf, String> {
            if path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
            {
                return Err(format!(
                    "invalid workspace capability path {}",
                    path.display()
                ));
            }
            if path.as_os_str().is_empty() {
                return Ok(PathBuf::from("."));
            }
            Ok(path.to_path_buf())
        }
    }

    fn openat2_file(
        root: RawFd,
        path: &std::path::Path,
        flags: libc::c_int,
    ) -> Result<std::fs::File, String> {
        use std::os::fd::FromRawFd;
        use std::os::unix::ffi::OsStrExt;
        if path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(format!("path {} contains `..`", path.display()));
        }
        let c = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| format!("path {} contains NUL", path.display()))?;
        let how = OpenHow {
            flags: flags as u64,
            mode: 0,
            resolve: RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS,
        };
        let fd = unsafe {
            libc::syscall(
                SYS_OPENAT2,
                root,
                c.as_ptr(),
                &how,
                std::mem::size_of::<OpenHow>(),
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(unsafe { std::fs::File::from_raw_fd(fd as i32) })
    }

    #[repr(C)]
    struct OpenHow {
        flags: u64,
        mode: u64,
        resolve: u64,
    }

    pub fn pin_capability_paths(
        caps: &ExtensionCapabilities,
        roots: &PathRoots,
    ) -> Result<(Vec<std::fs::File>, Vec<std::fs::File>), String> {
        let mut writes = Vec::new();
        let mut reads = Vec::new();
        for path in &caps.filesystem.write {
            writes.push(pin_declared_path(roots, path)?);
        }
        for path in &caps.filesystem.read {
            reads.push(pin_declared_path(roots, path)?);
        }
        for path in [
            "/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc", "/opt", "/run", "/dev", "/proc",
            "/sys",
        ] {
            let path = PathBuf::from(path);
            let path = match path.canonicalize() {
                Ok(path) => path,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(format!(
                        "cannot resolve runtime path {}: {error}",
                        path.display()
                    ));
                }
            };
            reads.push(roots.open(&path, libc::O_PATH)?);
        }
        Ok((writes, reads))
    }

    fn pin_declared_path(
        roots: &PathRoots,
        path: &std::path::Path,
    ) -> Result<std::fs::File, String> {
        let file = if path.is_absolute() {
            roots.open(path, libc::O_PATH)?
        } else {
            let relative = roots.capability_relative(path)?;
            roots.open(&relative, libc::O_PATH)?
        };
        if file
            .metadata()
            .map_err(|error| error.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err(format!(
                "declared capability {} escapes the workspace through a symlink",
                path.display()
            ));
        }
        Ok(file)
    }

    pub fn build_extension(
        caps: &ExtensionCapabilities,
        roots: &PathRoots,
        entrypoint: std::fs::File,
    ) -> Result<(RawFd, Vec<String>), String> {
        let abi = abi().ok_or("Landlock is not available on this kernel")?;
        if abi < 4 {
            return Err("strict extension isolation requires Landlock ABI 4 (Linux 6.7+) for network default-deny; refusing downgrade".into());
        }
        let (writes, mut reads) = pin_capability_paths(caps, roots)?;
        reads.push(entrypoint);
        build_extension_pinned(caps, abi, writes, reads)
    }

    pub(super) fn build_extension_pinned(
        caps: &ExtensionCapabilities,
        abi: u32,
        writes: Vec<std::fs::File>,
        reads: Vec<std::fs::File>,
    ) -> Result<(RawFd, Vec<String>), String> {
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::MetadataExt;
        let mut grants = std::collections::BTreeMap::<(u64, u64), (RawFd, u64)>::new();
        for file in &reads {
            let meta = file.metadata().map_err(|e| e.to_string())?;
            let read_access = if meta.is_dir() {
                READ_RIGHTS
            } else {
                READ_FILE
            };
            grants
                .entry((meta.dev(), meta.ino()))
                .and_modify(|grant| grant.1 |= read_access | EXECUTE)
                .or_insert((file.as_raw_fd(), read_access | EXECUTE));
        }
        for file in &writes {
            let meta = file.metadata().map_err(|e| e.to_string())?;
            let read_access = if meta.is_dir() {
                READ_RIGHTS
            } else {
                READ_FILE
            };
            grants
                .entry((meta.dev(), meta.ino()))
                .and_modify(|grant| grant.1 |= write_rights(abi) | read_access | EXECUTE)
                .or_insert((file.as_raw_fd(), write_rights(abi) | read_access | EXECUTE));
        }
        let fs_handled = write_rights(abi) | READ_RIGHTS | EXECUTE;
        let net_handled = NET_CONNECT_TCP;
        #[repr(C)]
        struct RulesetAttr {
            fs: u64,
            net: u64,
        }
        let attr = RulesetAttr {
            fs: fs_handled,
            net: net_handled,
        };
        let fd = unsafe {
            libc::syscall(
                SYS_CREATE_RULESET,
                &attr as *const RulesetAttr,
                std::mem::size_of::<RulesetAttr>(),
                0u32,
            )
        };
        if fd < 0 {
            return Err(format!(
                "landlock_create_ruleset: {}",
                std::io::Error::last_os_error()
            ));
        }
        let fd = fd as RawFd;
        for (path_fd, rights) in grants.values() {
            if let Err(error) = add_fd(fd, *path_fd, *rights) {
                unsafe { libc::close(fd) };
                return Err(error);
            }
        }
        for port in &caps.network {
            if let Err(error) = add_port(fd, *port, NET_CONNECT_TCP) {
                unsafe { libc::close(fd) };
                return Err(error);
            }
        }
        let _ = (writes, reads); // keep pinned objects alive until every rule is installed
        Ok((fd, Vec::new()))
    }

    /// Write-ish rights this ABI knows about.
    ///
    /// REFER arrived in ABI 2 and TRUNCATE in ABI 3; handling a right the
    /// kernel does not know would make ruleset creation fail outright.
    fn write_rights(abi: u32) -> u64 {
        let mut r = WRITE_FILE
            | REMOVE_FILE
            | REMOVE_DIR
            | MAKE_CHAR
            | MAKE_DIR
            | MAKE_REG
            | MAKE_SOCK
            | MAKE_FIFO
            | MAKE_BLOCK
            | MAKE_SYM;
        if abi >= 2 {
            r |= REFER;
        }
        if abi >= 3 {
            r |= TRUNCATE;
        }
        r
    }

    /// Create a ruleset fd: writes under `writable`; when `strict`, reads and
    /// execs only under `writable` and `readable`; and, when `ports` is given
    /// and the kernel is ABI 4+, outbound TCP limited to those ports.
    ///
    /// Returns the fd plus notes about anything requested but unavailable
    /// (network rules on an older kernel), which the caller surfaces as
    /// warnings rather than silence.
    pub fn build(
        writable: &[PathBuf],
        readable: &[PathBuf],
        strict: bool,
        ports: Option<&[u16]>,
    ) -> Result<(RawFd, Vec<String>), String> {
        let mut notes = Vec::new();
        let abi = abi().ok_or("Landlock is not available on this kernel")?;
        let writes = write_rights(abi);
        let mut handled = writes;
        if strict {
            handled |= READ_RIGHTS | EXECUTE;
        }
        let mut handled_net: u64 = 0;
        if let Some(ports) = ports {
            if abi >= 4 {
                handled_net = NET_CONNECT_TCP;
                let _ = ports;
            } else {
                notes.push(
                    "sandbox: outbound network is unrestricted (network rules need \
                     Landlock ABI 4 / Linux 6.7+)"
                        .into(),
                );
            }
        }
        #[repr(C)]
        struct RulesetAttr {
            fs: u64,
            net: u64,
        }
        let attr = RulesetAttr {
            fs: handled,
            net: handled_net,
        };
        // Older kernels reject an attribute larger than they know, so the
        // size argument must stop at the fields actually in use.
        let (ptr, size): (*const std::ffi::c_void, usize) = if handled_net != 0 {
            (
                (&attr as *const RulesetAttr).cast(),
                std::mem::size_of::<RulesetAttr>(),
            )
        } else {
            ((&attr.fs as *const u64).cast(), std::mem::size_of::<u64>())
        };
        let fd = unsafe { libc::syscall(SYS_CREATE_RULESET, ptr, size, 0u32) };
        if fd < 0 {
            return Err(format!(
                "landlock_create_ruleset: {}",
                std::io::Error::last_os_error()
            ));
        }
        let fd = fd as RawFd;
        let extra = if strict { READ_RIGHTS | EXECUTE } else { 0 };
        let mut failure = None;
        for path in writable {
            if let Err(e) = add(fd, path, writes | extra) {
                failure = Some(e);
                break;
            }
        }
        if failure.is_none() && !writable.is_empty() {
            // /dev/null is a file: only file rights are valid there, and a
            // rule may only grant rights the ruleset handles — hence READ_FILE
            // only under strict, where reads are handled.
            let null = WRITE_FILE | if strict { READ_FILE } else { 0 };
            if let Err(e) = add(fd, std::path::Path::new("/dev/null"), null) {
                failure = Some(e);
            }
        }
        if failure.is_none() && strict {
            for path in readable {
                if let Err(e) = add(fd, path, READ_RIGHTS | EXECUTE) {
                    failure = Some(e);
                    break;
                }
            }
        }
        if failure.is_none() && handled_net != 0 {
            // No rule for a port means it is denied: the handled set is the
            // boundary and the rules are the exceptions to it. Binds are left
            // unrestricted so local test servers keep working.
            for port in ports.unwrap_or(&[]) {
                if let Err(e) = add_port(fd, *port, NET_CONNECT_TCP) {
                    failure = Some(e);
                    break;
                }
            }
        }
        match failure {
            None => Ok((fd, notes)),
            Some(e) => {
                unsafe { libc::close(fd) };
                Err(e)
            }
        }
    }

    fn add_port(fd: RawFd, port: u16, rights: u64) -> Result<(), String> {
        #[repr(C)]
        struct NetPort {
            allowed: u64,
            port: u64,
        }
        let rule = NetPort {
            allowed: rights,
            port: port as u64,
        };
        let rc = unsafe {
            libc::syscall(
                SYS_ADD_RULE,
                fd,
                RULE_NET_PORT,
                &rule as *const NetPort,
                0u32,
            )
        };
        if rc < 0 {
            return Err(format!(
                "landlock_add_rule net port {port}: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    fn add(fd: RawFd, path: &std::path::Path, rights: u64) -> Result<(), String> {
        let Ok(c) = CString::new(path.as_os_str().as_bytes()) else {
            return Ok(()); // a path we cannot express simply stays unallowed
        };
        let pfd = unsafe { libc::open(c.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
        if pfd < 0 {
            // A missing allowlist entry (no ~/.cargo, no /lib64) is fine.
            return Ok(());
        }
        #[repr(C)]
        struct PathBeneath {
            allowed: u64,
            parent: RawFd,
        }
        let rule = PathBeneath {
            allowed: rights,
            parent: pfd,
        };
        let rc = unsafe {
            libc::syscall(
                SYS_ADD_RULE,
                fd,
                RULE_PATH_BENEATH,
                &rule as *const PathBeneath,
                0u32,
            )
        };
        let error = std::io::Error::last_os_error();
        unsafe { libc::close(pfd) };
        if rc < 0 {
            return Err(format!("landlock_add_rule {}: {error}", path.display()));
        }
        Ok(())
    }

    fn add_fd(fd: RawFd, path_fd: RawFd, rights: u64) -> Result<(), String> {
        #[repr(C)]
        struct PathBeneath {
            allowed: u64,
            parent: RawFd,
        }
        let rule = PathBeneath {
            allowed: rights,
            parent: path_fd,
        };
        let rc = unsafe {
            libc::syscall(
                SYS_ADD_RULE,
                fd,
                RULE_PATH_BENEATH,
                &rule as *const PathBeneath,
                0u32,
            )
        };
        if rc < 0 {
            return Err(format!(
                "landlock_add_rule pinned fd {path_fd}: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    /// Restrict the calling (child) process. Call after fork, before exec.
    pub fn enter(fd: RawFd) -> std::io::Result<()> {
        unsafe {
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::syscall(SYS_RESTRICT_SELF, fd as libc::c_long, 0u32) != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::process::Command;

        fn scratch(tag: &str) -> PathBuf {
            static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let d = std::env::temp_dir().join(format!("gc-jail-{}-{tag}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            d
        }

        #[test]
        fn a_workspace_jail_allows_inside_writes_and_unrestricted_reads() {
            if abi().is_none() {
                return; // kernel without Landlock: nothing to test
            }
            let dir = scratch("ws");
            let ws = dir.join("ws");
            std::fs::create_dir_all(&ws).unwrap();
            std::fs::write(dir.join("secret.txt"), "SECRET-READABLE").unwrap();
            let fd = build(
                std::slice::from_ref(&ws),
                std::slice::from_ref(&ws),
                false,
                None,
            )
            .expect("ruleset")
            .0;
            let mut cmd = Command::new("/bin/sh");
            cmd.arg("-c").arg(format!(
                "echo ok > '{ws}/in.txt'; echo no > '{dir}/out.txt' 2>/dev/null || echo WRITE-DENIED; cat '{dir}/secret.txt'",
                ws = ws.display(),
                dir = dir.display()
            ));
            unsafe {
                use std::os::unix::process::CommandExt;
                cmd.pre_exec(move || enter(fd));
            }
            let out = cmd.output().unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(ws.join("in.txt").exists(), "inside write must succeed");
            assert!(
                !dir.join("out.txt").exists(),
                "outside write must be denied"
            );
            assert!(stdout.contains("WRITE-DENIED"), "{stdout}");
            assert!(
                stdout.contains("SECRET-READABLE"),
                "workspace policy does not restrict reads: {stdout}"
            );
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn a_strict_jail_also_restricts_reads() {
            if abi().is_none() {
                return;
            }
            let dir = scratch("strict");
            let ws = dir.join("ws");
            std::fs::create_dir_all(&ws).unwrap();
            std::fs::write(ws.join("inside.txt"), "INSIDE").unwrap();
            std::fs::write(dir.join("secret.txt"), "SECRET").unwrap();
            // A strict ruleset must be able to exec /bin/sh at all: system
            // dirs are readable in the production allowlist for the same
            // reason.
            let readable: Vec<PathBuf> = [
                ws.clone(),
                PathBuf::from("/usr"),
                PathBuf::from("/bin"),
                PathBuf::from("/lib"),
                PathBuf::from("/lib64"),
                PathBuf::from("/etc"),
            ]
            .to_vec();
            let fd = build(std::slice::from_ref(&ws), &readable, true, None)
                .expect("ruleset")
                .0;
            let mut cmd = Command::new("/bin/sh");
            cmd.arg("-c").arg(format!(
                "cat '{ws}/inside.txt'; cat '{dir}/secret.txt' 2>/dev/null || echo READ-DENIED",
                ws = ws.display(),
                dir = dir.display()
            ));
            unsafe {
                use std::os::unix::process::CommandExt;
                cmd.pre_exec(move || enter(fd));
            }
            let out = cmd.output().unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(stdout.contains("INSIDE"), "{stdout}");
            assert!(stdout.contains("READ-DENIED"), "{stdout}");
            assert!(!stdout.contains("SECRET"), "{stdout}");
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn a_missing_allowlist_entry_is_not_fatal() {
            if abi().is_none() {
                return;
            }
            let dir = scratch("missing");
            let ws = dir.join("ws");
            std::fs::create_dir_all(&ws).unwrap();
            let ghost = dir.join("does-not-exist");
            build(&[ws, ghost], &[], false, None).expect("a missing dir must be skipped");
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn network_egress_is_limited_to_the_allowed_ports() {
            if abi().unwrap_or(0) < 4 {
                return; // per-port network rules arrived in ABI 4
            }
            if Command::new("python3").arg("--version").output().is_err() {
                return;
            }
            let dir = scratch("net");
            let allowed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let denied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let (fd, notes) = build(
                std::slice::from_ref(&dir),
                std::slice::from_ref(&dir),
                false,
                Some(&[allowed.local_addr().unwrap().port()]),
            )
            .expect("ruleset");
            assert!(notes.is_empty(), "{notes:?}");
            let script = format!(
                "import socket\n\
                 for name, port in (('allowed', {a}), ('denied', {d})):\n\
                 \x20   try:\n\
                 \x20       socket.create_connection(('127.0.0.1', port), timeout=3).close()\n\
                 \x20       print(name, 'CONNECTED')\n\
                 \x20   except OSError:\n\
                 \x20       print(name, 'DENIED')\n",
                a = allowed.local_addr().unwrap().port(),
                d = denied.local_addr().unwrap().port()
            );
            let mut cmd = Command::new("python3");
            cmd.arg("-c").arg(script);
            unsafe {
                use std::os::unix::process::CommandExt;
                cmd.pre_exec(move || enter(fd));
            }
            let out = cmd.output().unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(stdout.contains("allowed CONNECTED"), "{stdout}");
            assert!(stdout.contains("denied DENIED"), "{stdout}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    //! SBPL profile for sandbox-exec. macOS is a courtesy port: the profile
    //! grants `workspace` semantics (writes under the allowlist, reads as
    //! usual) and, for `strict`, narrows reads to the same allowlist.

    use super::*;

    pub fn profile(
        workspace: &std::path::Path,
        writable: &[PathBuf],
        readable: &[PathBuf],
        strict: bool,
        read_only: bool,
    ) -> String {
        let mut p = String::from("(version 1)\n(allow default)\n(deny file-write*)\n");
        if !read_only {
            p.push_str(&format!(
                "(allow file-write* (subpath \"{}\"))\n",
                workspace.display()
            ));
        }
        for dir in writable {
            p.push_str(&format!(
                "(allow file-write* (subpath \"{}\"))\n",
                dir.display()
            ));
        }
        if !read_only {
            p.push_str("(allow file-write* (literal \"/dev/null\"))\n");
        }
        if strict {
            p.push_str("(deny file-read*)\n");
            p.push_str(&format!(
                "(allow file-read* (subpath \"{}\"))\n",
                workspace.display()
            ));
            for dir in readable {
                p.push_str(&format!(
                    "(allow file-read* (subpath \"{}\"))\n",
                    dir.display()
                ));
            }
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_names_are_recognised_by_shape_and_by_list() {
        for sensitive in [
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "AWS_SECRET_ACCESS_KEY",
            "GITHUB_TOKEN",
            "GH_TOKEN",
            "NPM_TOKEN",
            "MY_SERVICE_PASSWORD",
            "SSH_AUTH_SOCK",
            "DOCKER_AUTH_CONFIG",
            "some_credential",
        ] {
            assert!(is_sensitive(sensitive), "{sensitive}");
        }
        for safe in ["PATH", "HOME", "CARGO_HOME", "RUSTUP_HOME", "EDITOR", "PWD"] {
            assert!(!is_sensitive(safe), "{safe}");
        }
    }

    #[test]
    fn permission_profiles_keep_unrestricted_as_default_and_parse_opt_ins() {
        assert_eq!(
            PermissionProfile::default(),
            PermissionProfile::Unrestricted
        );
        assert_eq!(
            PermissionProfile::parse("workspace-edit"),
            Some(PermissionProfile::WorkspaceEdit)
        );
        assert_eq!(
            PermissionProfile::parse("read-only"),
            Some(PermissionProfile::ReadOnly)
        );
        assert_eq!(PermissionProfile::parse("surprise"), None);
    }

    #[test]
    fn command_approval_is_exact_and_session_scoped() {
        let mut approvals = SessionApprovals::default();
        approvals.approve_exact("printf ok > /tmp/x");
        assert!(approvals.permits_exact("printf ok > /tmp/x"));
        assert_eq!(
            approvals.decision("printf ok > /tmp/x", true),
            ApprovalDecision::Approved
        );
        assert!(!approvals.permits_exact("printf other > /tmp/x"));
        assert_eq!(
            approvals.decision("printf other > /tmp/x", true),
            ApprovalDecision::RefusedHeadless
        );
        assert_eq!(
            approvals.decision("printf other > /tmp/x", false),
            ApprovalDecision::NeedsInteractiveApproval
        );
        assert!(!SessionApprovals::default().permits_exact("printf ok > /tmp/x"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn read_only_landlock_denies_workspace_and_outside_writes() {
        if landlock::abi().is_none() {
            return;
        }
        use std::os::unix::process::CommandExt;
        let dir = std::env::temp_dir().join(format!("gc-readonly-{}", std::process::id()));
        let workspace = dir.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let inside = workspace.join("no.txt");
        let outside = dir.join("outside.txt");
        let readable = vec![
            workspace.clone(),
            PathBuf::from("/usr"),
            PathBuf::from("/bin"),
            PathBuf::from("/lib"),
            PathBuf::from("/lib64"),
            PathBuf::from("/etc"),
        ];
        let (fd, _) = landlock::build(&[], &readable, true, Some(&[])).unwrap();
        let mut child = Command::new("/bin/sh");
        child.arg("-c").arg(format!(
            "printf no > '{}'; printf no > '{}'",
            inside.display(),
            outside.display()
        ));
        unsafe {
            child.pre_exec(move || landlock::enter(fd));
        }
        let _ = child.status().unwrap();
        assert!(!inside.exists(), "read-only must deny workspace writes");
        assert!(!outside.exists(), "read-only must deny outside writes");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn strict_extension_requires_a_complete_sidecar_and_rejects_symlink_sidecars() {
        let dir = std::env::temp_dir().join(format!("gc-ext-caps-{}", std::process::id()));
        let ws = dir.join("workspace");
        std::fs::create_dir_all(&ws).unwrap();
        let script = ws.join("ext.py");
        std::fs::write(&script, "pass\n").unwrap();
        let spec = format!("python3 {}", script.display());
        assert!(
            inspect_extension_capabilities(&spec, &ws).is_err(),
            "missing declaration refuses"
        );
        let sidecar = PathBuf::from(format!("{}.capabilities.json", script.display()));
        std::fs::write(
            &sidecar,
            r#"{"filesystem":{"read":[],"write":[]},"network":[],"subprocess":false}"#,
        )
        .unwrap();
        assert!(inspect_extension_capabilities(&spec, &ws).is_ok());
        let outside = dir.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let link = ws.join("escape");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        std::fs::write(
            &sidecar,
            r#"{"filesystem":{"read":["escape"],"write":[]},"network":[],"subprocess":false}"#,
        )
        .unwrap();
        #[cfg(unix)]
        assert!(
            inspect_extension_capabilities(&spec, &ws)
                .unwrap_err()
                .contains("escapes the workspace")
        );
        std::fs::write(
            &sidecar,
            r#"{"filesystem":{"read":[],"write":[]},"network":[],"subprocess":false}"#,
        )
        .unwrap();
        let real = dir.join("real.json");
        std::fs::rename(&sidecar, &real).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &sidecar).unwrap();
        #[cfg(unix)]
        assert!(
            inspect_extension_capabilities(&spec, &ws)
                .unwrap_err()
                .contains("openat2 refused")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn pinned_sidecar_fd_cannot_be_redirected_after_open() {
        let dir = std::env::temp_dir().join(format!("gc-sidecar-race-{}", std::process::id()));
        let ws = dir.join("workspace");
        std::fs::create_dir_all(&ws).unwrap();
        let sidecar = ws.join("extension.py.capabilities.json");
        let safe = r#"{"filesystem":{"read":[],"write":[]},"network":[],"subprocess":false}"#;
        let outside = dir.join("outside.json");
        std::fs::write(&sidecar, safe).unwrap();
        std::fs::write(&outside, "not valid capabilities").unwrap();
        let roots = landlock::PathRoots::new(&ws).unwrap();
        let descriptor = roots.open(&sidecar, libc::O_RDONLY).unwrap();
        std::fs::rename(&sidecar, ws.join("pinned.json")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &sidecar).unwrap();
        let parsed = parse_pinned_sidecar(descriptor, &sidecar).unwrap();
        assert!(parsed.filesystem.read.is_empty());
        assert!(roots.open(&sidecar, libc::O_RDONLY).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn relative_extension_entrypoint_uses_workspace_cwd_not_host_cwd() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let name = format!("gc-security-cwd-{suffix}.py");
        let host_script = std::env::current_dir().unwrap().join(&name);
        let host_sidecar = PathBuf::from(format!("{}.capabilities.json", host_script.display()));
        let dir = std::env::temp_dir().join(format!("gc-workspace-cwd-{suffix}"));
        let workspace = dir.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join(&name), "pass\n").unwrap();
        std::fs::write(
            format!("{}.capabilities.json", workspace.join(&name).display()),
            r#"{"filesystem":{"read":[],"write":[]},"network":[12345],"subprocess":false}"#,
        )
        .unwrap();
        std::fs::write(&host_script, "pass\n").unwrap();
        std::fs::write(
            &host_sidecar,
            r#"{"filesystem":{"read":[],"write":[]},"network":[23456],"subprocess":false}"#,
        )
        .unwrap();

        let inspected = inspect_extension_capabilities(&format!("python3 {name}"), &workspace);
        let _ = std::fs::remove_file(host_sidecar);
        let _ = std::fs::remove_file(host_script);
        let _ = std::fs::remove_dir_all(dir);
        assert_eq!(inspected.unwrap().network, vec![12345]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn pinned_capability_fd_survives_path_replacement_before_rule_install() {
        if landlock::abi().is_none_or(|abi| abi < 4) {
            return;
        }
        let dir = std::env::temp_dir().join(format!("gc-cap-race-{}", std::process::id()));
        let ws = dir.join("workspace");
        let allowed = ws.join("allowed");
        let outside = dir.join("outside");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let roots = landlock::PathRoots::new(&ws).unwrap();
        let caps = ExtensionCapabilities {
            filesystem: ExtensionFilesystem {
                read: vec![],
                write: vec![PathBuf::from("allowed")],
            },
            network: vec![],
            subprocess: false,
        };
        let (writes, reads) = landlock::pin_capability_paths(&caps, &roots).unwrap();
        std::fs::rename(&allowed, ws.join("pinned")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &allowed).unwrap();
        let (fd, _) =
            landlock::build_extension_pinned(&caps, landlock::abi().unwrap(), writes, reads)
                .unwrap();
        let descriptor_flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        assert!(
            descriptor_flags >= 0 && descriptor_flags & libc::FD_CLOEXEC != 0,
            "kernel ruleset descriptors must not leak through exec"
        );
        use std::os::unix::process::CommandExt;
        let mut child = Command::new("/bin/sh");
        child.arg("-c").arg(format!(
            "printf ok > '{}/ok'; printf no > '{}/escape' 2>/dev/null || true",
            ws.join("pinned").display(),
            outside.display()
        ));
        unsafe {
            child.pre_exec(move || landlock::enter(fd));
        }
        let status = child.status().unwrap();
        assert!(status.success());
        assert!(
            ws.join("pinned/ok").exists(),
            "pinned directory remains writable"
        );
        assert!(
            !outside.join("escape").exists(),
            "replacement symlink target remains denied"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn declared_extension_jail_denies_undeclared_writes_in_child_processes() {
        if landlock::abi().is_none() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("gc-ext-jail-{}", std::process::id()));
        let workspace = dir.join("workspace");
        let allowed = workspace.join("allowed");
        let denied = workspace.join("denied");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&denied).unwrap();
        let script = workspace.join("ext.py");
        std::fs::write(&script, "pass\n").unwrap();
        let sidecar = PathBuf::from(format!("{}.capabilities.json", script.display()));
        std::fs::write(&sidecar, format!(
            "{{\"filesystem\":{{\"read\":[\"{}\"],\"write\":[\"{}\"]}},\"network\":[],\"subprocess\":true}}",
            workspace.display(), allowed.display()
        )).unwrap();
        let mut test_state = State::off();
        test_state.extension_strict = true;
        let shell_jail = landlock::build(
            std::slice::from_ref(&allowed),
            &[
                PathBuf::from("/usr"),
                PathBuf::from("/bin"),
                PathBuf::from("/lib"),
                PathBuf::from("/lib64"),
                PathBuf::from("/etc"),
                workspace.clone(),
            ],
            true,
            Some(&[]),
        )
        .unwrap()
        .0;
        test_state.policy = Policy::Workspace;
        test_state.permission_profile = PermissionProfile::WorkspaceEdit;
        test_state.jail = Some(shell_jail);
        let _ = STATE.set(test_state);
        let missing_script = workspace.join("missing-capability.py");
        std::fs::write(&missing_script, "pass\n").unwrap();
        let mut blocked_extension = Command::new("/bin/sh");
        blocked_extension.arg("-c").arg(format!(
            "touch '{}'",
            denied.join("preflight-ran").display()
        ));
        let missing_error = harden_extension(
            &mut blocked_extension,
            &format!("python3 {}", missing_script.display()),
            &workspace,
        )
        .unwrap_err();
        assert!(missing_error.contains("requires capability declaration"));
        assert!(
            !denied.join("preflight-ran").exists(),
            "missing strict declaration refuses before spawn"
        );
        let spec = format!("python3 {}", script.display());
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg(format!(
            "printf ok > '{}/parent'; /bin/sh -c \"printf ok > '{}/child'; printf no > '{}/denied' 2>/dev/null\"",
            allowed.display(), allowed.display(), denied.display()
        ));
        let notes = harden_extension(&mut command, &spec, &workspace).unwrap();
        assert!(
            notes
                .iter()
                .any(|line| line.contains("inherit the same Landlock jail"))
        );
        let result = command.output().unwrap();
        assert!(
            allowed.join("parent").exists(),
            "declared write path must be allowed"
        );
        assert!(
            allowed.join("child").exists(),
            "grandchild must inherit the allowed path"
        );
        assert!(
            !denied.join("denied").exists(),
            "undeclared path must be denied in a grandchild"
        );
        assert!(String::from_utf8_lossy(&result.stderr).contains("Permission denied"));

        // The production background-job runner uses the shared shell hardener
        // too; verify its child inherits the same deny boundary.
        let jobs_dir = dir.join("job-logs");
        let mut jobs = crate::jobs::Jobs::default();
        jobs.start(
            &format!(
                "printf ok > '{}/background'; printf no > '{}/background' 2>/dev/null",
                allowed.display(),
                denied.display()
            ),
            &workspace,
            &jobs_dir,
        )
        .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while jobs.list().contains("running") {
            assert!(
                std::time::Instant::now() < deadline,
                "background permission probe must finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            allowed.join("background").exists(),
            "background write inside declared workspace succeeds"
        );
        assert!(
            !denied.join("background").exists(),
            "background write outside declared root is denied"
        );

        let approved_path = dir.join("exact-approved.txt");
        let exact = format!("printf ok > '{}'", approved_path.display());
        let mut exception_command = Command::new("/bin/sh");
        exception_command.arg("-c").arg(&exact);
        let approvals = SessionApprovals::default();
        assert!(
            harden_command(&mut exception_command, &exact, &approvals, true, true)
                .unwrap_err()
                .contains("headless mode cannot approve")
        );
        assert!(
            harden_command(
                &mut Command::new("/bin/true"),
                &exact,
                &approvals,
                true,
                false
            )
            .unwrap_err()
            .contains("interactive permission decision")
        );
        let mut approvals = approvals;
        approvals.approve_exact(&exact);
        let mut exception_command = Command::new("/bin/sh");
        exception_command.arg("-c").arg(&exact);
        harden_command(&mut exception_command, &exact, &approvals, true, true).unwrap();
        assert!(exception_command.status().unwrap().success());
        assert!(
            approved_path.exists(),
            "approved exception applies to this child"
        );

        let next_path = dir.join("next-command.txt");
        let next = format!("printf no > '{}'", next_path.display());
        let mut next_command = Command::new("/bin/sh");
        next_command.arg("-c").arg(&next);
        harden_command(&mut next_command, &next, &approvals, false, true).unwrap();
        let _ = next_command.status().unwrap();
        assert!(
            !next_path.exists(),
            "approval does not widen the next child"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn extension_scrub_removes_explicitly_set_secret_environment() {
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg("test -z \"${GLASS_TEST_API_KEY:-}\"");
        command.env("GLASS_TEST_API_KEY", "never-inherited");
        scrub_sensitive_env(&mut command);
        assert!(command.status().unwrap().success());
    }
}
