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
//! Network is not restricted in this version. A platform that cannot deliver
//! the requested policy warns once; `strict` refuses to run commands at all
//! rather than silently downgrading.

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

struct State {
    policy: Policy,
    /// Linux: prepared Landlock ruleset, applied in each child before exec.
    #[cfg(target_os = "linux")]
    jail: Option<std::os::fd::RawFd>,
    /// macOS: the SBPL profile handed to sandbox-exec.
    #[cfg(target_os = "macos")]
    profile: Option<String>,
    /// Set when `strict` was requested and the platform cannot deliver it:
    /// commands must be refused rather than run bare.
    refusal: Option<String>,
}

static STATE: OnceLock<State> = OnceLock::new();

impl State {
    fn off() -> Self {
        Self {
            policy: Policy::Off,
            #[cfg(target_os = "linux")]
            jail: None,
            #[cfg(target_os = "macos")]
            profile: None,
            refusal: None,
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
    let state = if cfg.sandbox == Policy::Off {
        State::off()
    } else {
        build_state(cfg, cfg.sandbox, &mut warnings)
    };
    let _ = STATE.set(state);
    warnings
}

#[cfg(target_os = "linux")]
fn build_state(
    cfg: &config::Config,
    policy: Policy,
    warnings: &mut Vec<String>,
) -> State {
    let strict = policy == Policy::Strict;
    match landlock::build(&writable_paths(&cfg.workspace), &readable_paths(&cfg.workspace), strict)
    {
        Ok(fd) => State {
            policy,
            jail: Some(fd),
            refusal: None,
        },
        Err(e) => {
            if strict {
                warnings.push(format!(
                    "sandbox = strict could not be set up ({e}); shell commands will be refused"
                ));
                State {
                    policy,
                    jail: None,
                    refusal: Some("the Landlock ruleset could not be created".into()),
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
fn build_state(
    cfg: &config::Config,
    policy: Policy,
    warnings: &mut Vec<String>,
) -> State {
    if !std::path::Path::new("/usr/bin/sandbox-exec").exists() {
        if policy == Policy::Strict {
            warnings.push(
                "sandbox = strict needs /usr/bin/sandbox-exec; shell commands will be refused"
                    .into(),
            );
            return State {
                policy,
                profile: None,
                refusal: Some("sandbox-exec is not available".into()),
            };
        }
        warnings.push("sandbox unavailable (no sandbox-exec); commands run unsandboxed".into());
        return State::off();
    }
    let profile = mac::profile(
        &cfg.workspace,
        &writable_paths(&cfg.workspace),
        &readable_paths(&cfg.workspace),
        policy == Policy::Strict,
    );
    State {
        policy,
        profile: Some(profile),
        refusal: None,
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn build_state(
    _cfg: &config::Config,
    policy: Policy,
    warnings: &mut Vec<String>,
) -> State {
    if policy == Policy::Strict {
        warnings
            .push("sandbox = strict is not supported on this platform; commands will be refused".into());
        State {
            policy,
            refusal: Some("unsupported platform".into()),
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

/// The effective policy, for diagnostics.
pub fn policy() -> Policy {
    STATE.get().map(|s| s.policy).unwrap_or(Policy::Off)
}

#[cfg(target_os = "linux")]
mod landlock {
    //! Landlock via raw syscalls: enough of ABI 1–3 for a filesystem jail.

    use super::*;
    use std::ffi::CString;
    use std::os::fd::RawFd;
    use std::os::unix::ffi::OsStrExt;

    // Landlock uses the same syscall numbers on x86_64 and aarch64.
    const SYS_CREATE_RULESET: libc::c_long = 444;
    const SYS_ADD_RULE: libc::c_long = 445;
    const SYS_RESTRICT_SELF: libc::c_long = 446;
    const CREATE_VERSION: u32 = 1 << 0;
    const RULE_PATH_BENEATH: u32 = 1;

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
    /// execs only under `writable` and `readable`.
    pub fn build(writable: &[PathBuf], readable: &[PathBuf], strict: bool) -> Result<RawFd, String> {
        let abi = abi().ok_or("Landlock is not available on this kernel")?;
        let writes = write_rights(abi);
        let mut handled = writes;
        if strict {
            handled |= READ_RIGHTS | EXECUTE;
        }
        let fd = unsafe {
            libc::syscall(
                SYS_CREATE_RULESET,
                &handled as *const u64,
                std::mem::size_of::<u64>(),
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
        let extra = if strict { READ_RIGHTS | EXECUTE } else { 0 };
        let mut failure = None;
        for path in writable {
            if let Err(e) = add(fd, path, writes | extra) {
                failure = Some(e);
                break;
            }
        }
        if failure.is_none() {
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
        match failure {
            None => Ok(fd),
            Some(e) => {
                unsafe { libc::close(fd) };
                Err(e)
            }
        }
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
            )
            .expect("ruleset");
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
            let fd = build(std::slice::from_ref(&ws), &readable, true).expect("ruleset");
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
            build(&[ws, ghost], &[], false).expect("a missing dir must be skipped");
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
    ) -> String {
        let mut p = String::from("(version 1)\n(allow default)\n(deny file-write*)\n");
        p.push_str(&format!(
            "(allow file-write* (subpath \"{}\"))\n",
            workspace.display()
        ));
        for dir in writable {
            p.push_str(&format!(
                "(allow file-write* (subpath \"{}\"))\n",
                dir.display()
            ));
        }
        p.push_str("(allow file-write* (literal \"/dev/null\"))\n");
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
