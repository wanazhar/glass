use super::error::{NativeEngineError, NativeWorkerFailureKind};
use std::path::Path;
use std::path::PathBuf;
use tokio::process::{Child, Command};

pub(crate) const MAX_NATIVE_CONTENT_ADDRESS_SPACE_BYTES: u64 = 1024 * 1024 * 1024;

pub(crate) struct NativeContentSandbox {
    #[cfg(windows)]
    job: Option<WindowsJob>,
}

impl NativeContentSandbox {
    pub(crate) fn attach(&mut self, child: &Child) -> Result<(), NativeEngineError> {
        #[cfg(windows)]
        {
            let Some(job) = self.job.as_ref() else {
                return Err(sandbox_error("Windows content job was not created"));
            };
            let Some(process) = child.raw_handle() else {
                return Err(sandbox_error(
                    "Windows content process handle is unavailable",
                ));
            };
            let assigned = unsafe {
                windows_sys::Win32::System::JobObjects::AssignProcessToJobObject(
                    job.handle as _,
                    process,
                )
            };
            if assigned == 0 {
                return Err(sandbox_error(
                    "Windows content process could not join its job",
                ));
            }
        }
        #[cfg(not(windows))]
        let _ = child;
        Ok(())
    }
}

pub(crate) fn prepare_worker_command(
    worker_path: &Path,
    storage_path: Option<&Path>,
    allowed_file_roots: &[PathBuf],
) -> Result<(Command, NativeContentSandbox), NativeEngineError> {
    #[cfg(target_os = "linux")]
    {
        prepare_linux(worker_path, storage_path, allowed_file_roots)
    }
    #[cfg(target_os = "macos")]
    {
        return prepare_macos(worker_path, storage_path, allowed_file_roots);
    }
    #[cfg(windows)]
    {
        let _ = (storage_path, allowed_file_roots);
        return prepare_windows(worker_path);
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = (worker_path, storage_path, allowed_file_roots);
        Err(sandbox_error(
            "native content sandbox is not implemented for this operating system",
        ))
    }
}

#[cfg(target_os = "linux")]
fn prepare_linux(
    worker_path: &Path,
    storage_path: Option<&Path>,
    allowed_file_roots: &[PathBuf],
) -> Result<(Command, NativeContentSandbox), NativeEngineError> {
    let Some(bwrap) = find_executable("bwrap") else {
        return Err(sandbox_error(
            "Linux native content requires bubblewrap; refusing to run the worker unsandboxed",
        ));
    };

    let sandbox_worker = Path::new("/glass-native-content-worker");
    let mut command = Command::new(bwrap);
    command
        .args([
            "--die-with-parent",
            "--new-session",
            "--unshare-user",
            "--unshare-pid",
            "--unshare-uts",
            "--unshare-ipc",
            "--disable-userns",
            "--assert-userns-disabled",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--share-net",
            "--dir",
            "/etc",
            "--dir",
            "/etc/ssl",
            "--dir",
            "/glass-native-content-worker-parent",
        ])
        .arg("--ro-bind")
        .arg(worker_path)
        .arg(sandbox_worker)
        .args([
            "--ro-bind",
            "/usr",
            "/usr",
            "--ro-bind",
            "/bin",
            "/bin",
            "--ro-bind",
            "/lib",
            "/lib",
        ]);
    if Path::new("/lib64").is_dir() {
        command.args(["--ro-bind", "/lib64", "/lib64"]);
    }
    for path in [
        "/etc/hosts",
        "/etc/resolv.conf",
        "/etc/nsswitch.conf",
        "/etc/services",
        "/etc/localtime",
    ] {
        if Path::new(path).is_file() {
            command.args(["--ro-bind", path, path]);
        }
    }
    if Path::new("/etc/ssl/certs").is_dir() {
        command.args(["--ro-bind", "/etc/ssl/certs", "/etc/ssl/certs"]);
    }
    if let Some(storage_path) = storage_path {
        command.arg("--bind").arg(storage_path).arg(storage_path);
        let storage_lock_path = storage_path.with_extension("lock");
        if storage_lock_path.is_file() {
            command
                .arg("--bind")
                .arg(&storage_lock_path)
                .arg(&storage_lock_path);
        }
    }
    for root in allowed_file_roots {
        command.arg("--ro-bind").arg(root).arg(root);
    }
    command
        .arg("--")
        .arg(sandbox_worker)
        .arg("--native-content-worker");
    unsafe {
        command.pre_exec(|| {
            apply_linux_address_space_limit()?;
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok((command, NativeContentSandbox {}))
}

#[cfg(target_os = "macos")]
fn prepare_macos(
    worker_path: &Path,
    storage_path: Option<&Path>,
    allowed_file_roots: &[PathBuf],
) -> Result<(Command, NativeContentSandbox), NativeEngineError> {
    let seatbelt = Path::new("/usr/bin/sandbox-exec");
    if !seatbelt.is_file() {
        return Err(sandbox_error(
            "macOS native content requires /usr/bin/sandbox-exec; refusing to run the worker unsandboxed",
        ));
    }
    let worker = quote_profile_path(worker_path);
    let storage_rules = storage_path
        .and_then(Path::parent)
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| {
            let parent = quote_profile_path(parent);
            format!(
                " (allow file-read* (subpath \"{parent}\")) (allow file-write* (subpath \"{parent}\"))"
            )
        })
        .unwrap_or_default();
    let file_root_rules = allowed_file_roots
        .iter()
        .map(|root| {
            let root = quote_profile_path(root);
            format!(" (allow file-read* (subpath \"{root}\"))")
        })
        .collect::<String>();
    let profile = format!(
        "(version 1) (deny default) (allow process-exec (literal \"{worker}\")) (allow file-read* (subpath \"/System\") (subpath \"/usr\") (subpath \"/Library\") (literal \"{worker}\")) (allow network-outbound) (allow sysctl-read) (allow mach-lookup){storage_rules}{file_root_rules}"
    );
    let mut command = Command::new(seatbelt);
    command
        .arg("-p")
        .arg(profile)
        .arg(worker_path)
        .arg("--native-content-worker");
    Ok((command, NativeContentSandbox {}))
}

#[cfg(windows)]
fn prepare_windows(
    worker_path: &Path,
) -> Result<(Command, NativeContentSandbox), NativeEngineError> {
    let job = WindowsJob::new()?;
    let mut command = Command::new(worker_path);
    command.arg("--native-content-worker");
    Ok((command, NativeContentSandbox { job: Some(job) }))
}

#[cfg(windows)]
struct WindowsJob {
    handle: usize,
}

#[cfg(windows)]
impl WindowsJob {
    fn new() -> Result<Self, NativeEngineError> {
        use std::mem::size_of;
        use std::ptr::null_mut;
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectExtendedLimitInformation, SetInformationJobObject,
        };
        let handle = unsafe { CreateJobObjectW(null_mut(), std::ptr::null()) };
        if handle.is_null() {
            return Err(sandbox_error("Windows content job could not be created"));
        }
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY;
        limits.BasicLimitInformation.ActiveProcessLimit = 1;
        limits.ProcessMemoryLimit = MAX_NATIVE_CONTENT_ADDRESS_SPACE_BYTES as usize;
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            unsafe {
                CloseHandle(handle);
            }
            return Err(sandbox_error(
                "Windows content job limits could not be applied",
            ));
        }
        Ok(Self {
            handle: handle as usize,
        })
    }
}

#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle as _);
        }
    }
}

fn sandbox_error(reason: impl Into<String>) -> NativeEngineError {
    NativeEngineError::worker_failure(
        "content sandbox",
        NativeWorkerFailureKind::SandboxUnavailable,
        reason,
    )
}

#[cfg(target_os = "linux")]
fn apply_linux_address_space_limit() -> std::io::Result<()> {
    let mut inherited = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    if unsafe { libc::getrlimit(libc::RLIMIT_AS, &mut inherited) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let ceiling = MAX_NATIVE_CONTENT_ADDRESS_SPACE_BYTES as libc::rlim_t;
    let limit = libc::rlimit {
        rlim_cur: inherited.rlim_cur.min(ceiling),
        rlim_max: inherited.rlim_max.min(ceiling),
    };
    if unsafe { libc::setrlimit(libc::RLIMIT_AS, &limit) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn find_executable(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::unix::process::CommandExt;
    use std::process::Command as StdCommand;

    fn sandbox_command(disable_userns: bool, executable: &Path, arguments: &[&str]) -> StdCommand {
        let bwrap = find_executable("bwrap").expect("Linux tests require Bubblewrap");
        let mut command = StdCommand::new(bwrap);
        command.args([
            "--unshare-user",
            "--unshare-pid",
            "--proc",
            "/proc",
            "--ro-bind",
            "/usr",
            "/usr",
        ]);
        command.args(["--ro-bind", "/bin", "/bin", "--ro-bind", "/lib", "/lib"]);
        if Path::new("/lib64").is_dir() {
            command.args(["--ro-bind", "/lib64", "/lib64"]);
        }
        if disable_userns {
            command.args(["--disable-userns", "--assert-userns-disabled"]);
        }
        command.arg("--").arg(executable).args(arguments);
        command
    }

    #[test]
    fn linux_content_sandbox_blocks_nested_user_namespaces() {
        let unshare = Path::new("/usr/bin/unshare");
        assert!(unshare.is_file(), "Linux tests require /usr/bin/unshare");

        let true_program = Path::new("/usr/bin/true");
        assert!(true_program.is_file(), "Linux tests require /usr/bin/true");

        let mut restricted_control = sandbox_command(true, true_program, &[]);
        let restricted_control = restricted_control
            .output()
            .expect("Bubblewrap deny-and-assert options should start");
        assert!(
            restricted_control.status.success(),
            "the hardened sandbox control must launch: {}",
            String::from_utf8_lossy(&restricted_control.stderr)
        );

        let mut unrestricted = sandbox_command(false, unshare, &["--user", "true"]);
        let unrestricted = unrestricted
            .output()
            .expect("baseline nested-userns check should start");
        assert!(
            unrestricted.status.success(),
            "the control must create a nested user namespace: {}",
            String::from_utf8_lossy(&unrestricted.stderr)
        );

        let mut restricted = sandbox_command(true, unshare, &["--user", "true"]);
        let restricted = restricted
            .output()
            .expect("restricted nested-userns check should start");
        assert!(
            !restricted.status.success(),
            "Bubblewrap must deny nested user namespace creation"
        );

        let (worker_command, _) =
            prepare_linux(Path::new("/usr/bin/true"), None, &[]).expect("sandbox args");
        let worker_args = worker_command
            .as_std()
            .get_args()
            .filter_map(|argument| argument.to_str())
            .collect::<Vec<_>>();
        assert!(worker_args.contains(&"--disable-userns"));
        assert!(worker_args.contains(&"--assert-userns-disabled"));
    }

    #[test]
    fn linux_content_worker_inherits_a_bounded_address_space() {
        let limit_kib = address_space_limit_kib(None);
        assert!(
            limit_kib > 0 && limit_kib <= MAX_NATIVE_CONTENT_ADDRESS_SPACE_BYTES / 1024,
            "address-space limit exceeded the Glass worker ceiling: {limit_kib} KiB"
        );

        const STRICTER_PARENT_LIMIT_BYTES: u64 = 64 * 1024 * 1024;
        assert_eq!(
            address_space_limit_kib(Some(STRICTER_PARENT_LIMIT_BYTES)),
            STRICTER_PARENT_LIMIT_BYTES / 1024,
            "the worker must retain a stricter inherited limit"
        );
    }

    fn address_space_limit_kib(parent_limit: Option<u64>) -> u64 {
        let mut command = StdCommand::new("/bin/sh");
        command.args(["-c", "ulimit -v"]);
        unsafe {
            command.pre_exec(move || {
                if let Some(bytes) = parent_limit {
                    let limit = libc::rlimit {
                        rlim_cur: bytes as libc::rlim_t,
                        rlim_max: bytes as libc::rlim_t,
                    };
                    if libc::setrlimit(libc::RLIMIT_AS, &limit) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                apply_linux_address_space_limit()
            });
        }
        let output = command.output().expect("limited shell should start");
        assert!(
            output.status.success(),
            "limited shell failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u64>()
            .expect("ulimit should report a finite KiB limit")
    }
}

#[cfg(target_os = "macos")]
fn quote_profile_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}
