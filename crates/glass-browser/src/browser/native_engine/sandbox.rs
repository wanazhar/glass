use super::error::{NativeEngineError, NativeWorkerFailureKind};
use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::PathBuf;
use tokio::process::{Child, Command};

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
) -> Result<(Command, NativeContentSandbox), NativeEngineError> {
    #[cfg(target_os = "linux")]
    {
        return prepare_linux(worker_path, storage_path);
    }
    #[cfg(target_os = "macos")]
    {
        return prepare_macos(worker_path, storage_path);
    }
    #[cfg(windows)]
    {
        let _ = storage_path;
        return prepare_windows(worker_path);
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = (worker_path, storage_path);
        Err(sandbox_error(
            "native content sandbox is not implemented for this operating system",
        ))
    }
}

#[cfg(target_os = "linux")]
fn prepare_linux(
    worker_path: &Path,
    storage_path: Option<&Path>,
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
    }
    command
        .arg("--")
        .arg(sandbox_worker)
        .arg("--native-content-worker");
    unsafe {
        command.pre_exec(|| {
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
    let profile = format!(
        "(version 1) (deny default) (allow process-exec (literal \"{worker}\")) (allow file-read* (subpath \"/System\") (subpath \"/usr\") (subpath \"/Library\") (literal \"{worker}\")) (allow network-outbound) (allow sysctl-read) (allow mach-lookup){storage_rules}"
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
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };
        let handle = unsafe { CreateJobObjectW(null_mut(), std::ptr::null()) };
        if handle.is_null() {
            return Err(sandbox_error("Windows content job could not be created"));
        }
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
        limits.BasicLimitInformation.ActiveProcessLimit = 1;
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
fn find_executable(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(target_os = "macos")]
fn quote_profile_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}
