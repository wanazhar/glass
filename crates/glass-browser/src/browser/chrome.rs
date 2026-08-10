//! Chrome process lifecycle management.
//!
//! Handles Chrome/Chromium binary resolution, process launch with
//! configurable flags, managed Chrome for Testing installation,
//! port locking, and health checks.

use fs2::FileExt;
use futures_util::StreamExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
use tracing::{info, warn};

const PORT_LAUNCH_LOCK_TIMEOUT: Duration = Duration::from_secs(20);
const PORT_LAUNCH_LOCK_RETRY: Duration = Duration::from_millis(25);
const CHROME_STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const CHROME_STARTUP_POLL_INTERVAL: Duration = Duration::from_millis(100);
const CHROME_SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
const PINNED_CHROME_VERSION: &str = "150.0.7871.115";
const MAX_CHROME_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_CHROME_EXTRACTED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_CHROME_ARCHIVE_ENTRIES: usize = 20_000;

/// An advisory, cross-process lock for an owned Chrome launch on one CDP port.
///
/// The lock file is intentionally retained after release. The OS-level lock is
/// owned by this file handle and is released automatically if the holder exits,
/// so a stale file from a crash never blocks a later launch.
pub struct PortLaunchLock {
    file: File,
}

impl PortLaunchLock {
    /// Wait for exclusive ownership of the selected CDP port's launch lock.
    pub async fn acquire(port: u16) -> Result<Self, Box<dyn std::error::Error>> {
        let directory = std::env::temp_dir().join("glass").join("launch-locks");
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(format!("cdp-{port}.lock"));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;

        let deadline = Instant::now() + PORT_LAUNCH_LOCK_TIMEOUT;
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => return Ok(Self { file }),
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(format!(
                            "timed out waiting to launch Chrome on CDP port {port}; use --attach for an existing endpoint or choose another --port"
                        )
                        .into());
                    }
                    tokio::time::sleep(PORT_LAUNCH_LOCK_RETRY).await;
                }
                Err(error) => {
                    return Err(format!(
                        "could not lock CDP port {port} for an owned Chrome launch: {error}"
                    )
                    .into());
                }
            }
        }
    }
}

impl Drop for PortLaunchLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

/// A Chrome process started by Glass.
pub struct ChromeProcess {
    pub port: u16,
    pub pid: u32,
    browser_ws_url: String,
    child: Option<Child>,
    stderr_drain: Option<JoinHandle<()>>,
}

impl ChromeProcess {
    pub fn ws_debug_url(&self) -> String {
        format!("http://127.0.0.1:{}/json/version", self.port)
    }

    /// Return the browser WebSocket URL verified as belonging to this process.
    pub fn browser_ws_url(&self) -> &str {
        &self.browser_ws_url
    }

    /// Stop a Chrome process owned by this instance.
    pub async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let shutdown_result = match self.child.take() {
            Some(mut child) => match child.try_wait() {
                Ok(None) => match tokio::time::timeout(CHROME_SHUTDOWN_GRACE, child.wait()).await {
                    Ok(Ok(_)) => Ok(()),
                    Ok(Err(error)) => Err(Box::new(error) as Box<dyn std::error::Error>),
                    Err(_) => match child.kill().await {
                        Ok(()) => {
                            let _ = child.wait().await;
                            Ok(())
                        }
                        Err(error) => Err(Box::new(error) as Box<dyn std::error::Error>),
                    },
                },
                Ok(Some(_)) => Ok(()),
                Err(error) => Err(Box::new(error) as Box<dyn std::error::Error>),
            },
            None => Ok(()),
        };
        if let Some(stderr_drain) = self.stderr_drain.take() {
            let _ = stderr_drain.await;
        }
        shutdown_result
    }
}

impl Drop for ChromeProcess {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.start_kill();
        }
        if let Some(stderr_drain) = self.stderr_drain.take() {
            stderr_drain.abort();
        }
    }
}

/// Detect Chrome/Chromium installation on the system.
pub fn detect_chrome() -> Option<PathBuf> {
    let candidates = [
        "google-chrome",
        "google-chrome-stable",
        "chromium-browser",
        "chromium",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium-browser",
        "/usr/bin/chromium",
        "/snap/bin/chromium",
    ];

    for candidate in candidates {
        let path = Path::new(candidate);
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        if !candidate.contains('/')
            && let Ok(output) = std::process::Command::new("which").arg(candidate).output()
            && output.status.success()
        {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
    }

    None
}

/// Return the directory used by `install-chromium` for Chrome for Testing.
pub fn chromium_install_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("glass")
        .join("chromium")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManagedChromePlatform {
    LinuxX64,
    MacX64,
    MacArm64,
    WindowsX64,
    WindowsX86,
}

impl ManagedChromePlatform {
    fn current() -> Option<Self> {
        Self::for_target(std::env::consts::OS, std::env::consts::ARCH)
    }

    fn for_target(os: &str, architecture: &str) -> Option<Self> {
        match (os, architecture) {
            ("linux", "x86_64") => Some(Self::LinuxX64),
            ("macos", "x86_64") => Some(Self::MacX64),
            ("macos", "aarch64") => Some(Self::MacArm64),
            ("windows", "x86_64") => Some(Self::WindowsX64),
            ("windows", "x86") => Some(Self::WindowsX86),
            _ => None,
        }
    }

    fn download_platform(self) -> &'static str {
        match self {
            Self::LinuxX64 => "linux64",
            Self::MacX64 => "mac-x64",
            Self::MacArm64 => "mac-arm64",
            Self::WindowsX64 => "win64",
            Self::WindowsX86 => "win32",
        }
    }

    fn executable_relative_path(self) -> &'static str {
        match self {
            Self::LinuxX64 => "chrome-linux64/chrome",
            Self::MacX64 => {
                "chrome-mac-x64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"
            }
            Self::MacArm64 => {
                "chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"
            }
            Self::WindowsX64 => "chrome-win64/chrome.exe",
            Self::WindowsX86 => "chrome-win32/chrome.exe",
        }
    }

    fn pinned_archive(self) -> Option<(&'static str, u64)> {
        match self {
            Self::LinuxX64 => Some((
                "1be2db033133c5e2dd1a4e8664bf67b19a61bcf6ed28d2b00f433b3f0b4f9585",
                187_332_810,
            )),
            Self::MacX64 => Some((
                "1ede6b48b674a0ac0189f77debbc9bca21881d129eefa30252060117df705cf7",
                191_275_197,
            )),
            Self::MacArm64 => Some((
                "c418e4a29046bb62e340dfd9479b329bdf53ddd14a127b1362444a1e1b734a48",
                180_511_523,
            )),
            Self::WindowsX64 => Some((
                "90e0d112cb2f2743a7fd723f030c15b6421940be61dcbda7758563f821dd81da",
                194_764_577,
            )),
            Self::WindowsX86 => Some((
                "7df8d60ff06fb9a6bf7728091fd9b816e9b172ebce665de3a36cdd924e958628",
                168_049_061,
            )),
        }
    }
}

fn managed_chrome_path_in(install_dir: &Path, platform: ManagedChromePlatform) -> Option<PathBuf> {
    let records = std::fs::read_dir(install_dir.join("current")).ok()?;
    let version_name = records
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .max_by_key(|entry| entry.file_name())
        .and_then(|entry| std::fs::read_to_string(entry.path()).ok())?;
    let version_name = version_name.trim();
    if !version_name.starts_with(&format!("{PINNED_CHROME_VERSION}-")) {
        return None;
    }
    let root = install_dir.join("versions").join(version_name);
    let marker = std::fs::read_to_string(root.join(".complete")).ok()?;
    if marker.trim() != PINNED_CHROME_VERSION {
        return None;
    }
    let path = root.join(platform.executable_relative_path());
    path.is_file().then_some(path)
}

/// Find the Chrome for Testing executable installed by `glass install-chromium`.
pub fn managed_chrome_path() -> Option<PathBuf> {
    let platform = ManagedChromePlatform::current()?;
    managed_chrome_path_in(&chromium_install_dir(), platform)
}

/// Resolve an explicitly configured browser, then managed Chrome, then a
/// system Chrome/Chromium installation.
pub fn resolve_chrome_path(explicit: Option<PathBuf>) -> Option<PathBuf> {
    explicit.or_else(managed_chrome_path).or_else(detect_chrome)
}

#[cfg(test)]
fn choose_chrome_path(
    explicit: Option<PathBuf>,
    managed: Option<PathBuf>,
    detected: Option<PathBuf>,
) -> Option<PathBuf> {
    explicit.or(managed).or(detected)
}

/// Atomically install the Chrome for Testing version pinned by this release.
pub async fn download_chromium(update: bool) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let install_dir = chromium_install_dir();
    let platform = ManagedChromePlatform::current()
        .ok_or("Chrome for Testing is unavailable for this platform")?;
    let (expected_sha256, expected_size) = platform
        .pinned_archive()
        .ok_or("Chrome for Testing publishes no managed build for this target")?;

    std::fs::create_dir_all(&install_dir)?;
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(install_dir.join("install.lock"))?;
    lock_file.lock_exclusive()?;
    cleanup_install_staging(&install_dir)?;
    if !update && let Some(path) = managed_chrome_path_in(&install_dir, platform) {
        prune_managed_chrome(&install_dir)?;
        info!(path = %path.display(), "Chromium already installed");
        return Ok(path);
    }
    let nonce = format!(
        "{}-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
        std::process::id()
    );
    let staging = install_dir.join(format!(".staging-{nonce}"));
    std::fs::create_dir(&staging)?;
    let guard = InstallStagingGuard(staging.clone());
    let archive_path = staging.join("chrome.zip.part");
    let download_url = format!(
        "https://storage.googleapis.com/chrome-for-testing-public/{PINNED_CHROME_VERSION}/{}/chrome-{}.zip",
        platform.download_platform(),
        platform.download_platform()
    );
    info!(
        version = PINNED_CHROME_VERSION,
        platform = platform.download_platform(),
        "downloading pinned Chrome for Testing"
    );
    let response = reqwest::Client::new()
        .get(&download_url)
        .timeout(Duration::from_secs(120))
        .send()
        .await?
        .error_for_status()?;
    if response.content_length() != Some(expected_size) || expected_size > MAX_CHROME_ARCHIVE_BYTES
    {
        return Err("pinned Chrome archive length did not match the release manifest".into());
    }
    let mut archive = tokio::fs::File::create(&archive_path).await?;
    let mut stream = response.bytes_stream();
    let mut hasher = Sha256::new();
    let mut downloaded = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        downloaded = downloaded
            .checked_add(chunk.len() as u64)
            .ok_or("Chrome archive length overflowed")?;
        if downloaded > expected_size || downloaded > MAX_CHROME_ARCHIVE_BYTES {
            return Err("Chrome archive exceeded its pinned byte budget".into());
        }
        hasher.update(&chunk);
        archive.write_all(&chunk).await?;
    }
    archive.flush().await?;
    archive.sync_all().await?;
    if downloaded != expected_size {
        return Err("Chrome archive was truncated".into());
    }
    let actual_sha256 = format!("{:x}", hasher.finalize());
    if actual_sha256 != expected_sha256 {
        return Err("Chrome archive integrity digest did not match the release manifest".into());
    }
    // Windows denies removal while this writer still owns the archive handle.
    drop(archive);
    let extract_root = staging.join("payload");
    std::fs::create_dir(&extract_root)?;
    let archive_for_extract = archive_path.clone();
    let root_for_extract = extract_root.clone();
    tokio::task::spawn_blocking(move || {
        extract_chrome_zip(&archive_for_extract, &root_for_extract)
    })
    .await?
    .map_err(|error| format!("Chrome archive extraction failed: {error}"))?;
    tokio::fs::remove_file(&archive_path).await?;
    let path = managed_chrome_path_in_payload(&extract_root, platform)
        .ok_or("Chrome archive did not contain a browser executable")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = tokio::fs::metadata(&path).await?.permissions();
        permissions.set_mode(0o755);
        tokio::fs::set_permissions(&path, permissions).await?;
    }
    #[cfg(not(unix))]
    let _ = path;
    let mut marker = File::create(extract_root.join(".complete"))?;
    use std::io::Write as _;
    marker.write_all(format!("{PINNED_CHROME_VERSION}\n").as_bytes())?;
    marker.sync_all()?;
    sync_directory(&extract_root)?;
    let versions = install_dir.join("versions");
    std::fs::create_dir_all(&versions)?;
    let final_dir = versions.join(format!("{PINNED_CHROME_VERSION}-{nonce}"));
    std::fs::rename(&extract_root, &final_dir)?;
    sync_directory(&versions)?;
    let current = install_dir.join("current");
    std::fs::create_dir_all(&current)?;
    let generation = next_current_generation(&current)?;
    let pending_record = staging.join("current-record");
    let mut record = File::create(&pending_record)?;
    record.write_all(final_dir.file_name().unwrap().to_string_lossy().as_bytes())?;
    record.sync_all()?;
    std::fs::rename(&pending_record, current.join(format!("{generation:020}")))?;
    sync_directory(&current)?;
    drop(guard);
    prune_managed_chrome(&install_dir)?;
    let path = managed_chrome_path_in(&install_dir, platform)
        .ok_or("installed Chrome executable was not discoverable")?;
    info!(path = %path.display(), version = PINNED_CHROME_VERSION, "Chrome for Testing installed atomically");
    Ok(path)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(windows)]
fn sync_directory(_path: &Path) -> std::io::Result<()> {
    // Windows does not permit opening directories with ordinary std file
    // handles. File and marker contents are synced; publication still uses an
    // atomic same-volume rename, while directory metadata durability is left
    // to the filesystem.
    Ok(())
}

fn next_current_generation(current: &Path) -> std::io::Result<u64> {
    let greatest = std::fs::read_dir(current)?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_string_lossy().parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    greatest
        .checked_add(1)
        .ok_or_else(|| std::io::Error::other("managed Chrome generation overflowed"))
}

fn prune_managed_chrome(install_dir: &Path) -> std::io::Result<()> {
    let current = install_dir.join("current");
    let mut records = std::fs::read_dir(&current)?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    records.sort_by_key(|entry| entry.file_name());
    let retained = records
        .iter()
        .rev()
        .take(2)
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .map(|name| name.trim().to_owned())
        .collect::<std::collections::HashSet<_>>();
    for entry in records.into_iter().rev().skip(2) {
        std::fs::remove_file(entry.path())?;
    }
    for entry in std::fs::read_dir(install_dir.join("versions"))? {
        let entry = entry?;
        if !retained.contains(&entry.file_name().to_string_lossy().into_owned()) {
            make_removable(&entry.path())?;
            std::fs::remove_dir_all(entry.path())?;
        }
    }
    sync_directory(&current)?;
    sync_directory(&install_dir.join("versions"))?;
    Ok(())
}

struct InstallStagingGuard(PathBuf);

impl Drop for InstallStagingGuard {
    fn drop(&mut self) {
        let _ = make_removable(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cleanup_install_staging(install_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for entry in std::fs::read_dir(install_dir)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with(".staging-") {
            let _ = make_removable(&entry.path());
            std::fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}

fn managed_chrome_path_in_payload(root: &Path, platform: ManagedChromePlatform) -> Option<PathBuf> {
    let path = root.join(platform.executable_relative_path());
    path.is_file().then_some(path)
}

fn extract_chrome_zip(
    archive: &Path,
    destination: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let file = File::open(archive)?;
    let mut archive = zip::ZipArchive::new(file)?;
    if archive.len() > MAX_CHROME_ARCHIVE_ENTRIES {
        return Err("Chrome archive contained too many entries".into());
    }
    let mut extracted = 0_u64;
    #[cfg(unix)]
    let mut symlinks: Vec<(PathBuf, PathBuf)> = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let relative = entry
            .enclosed_name()
            .ok_or("Chrome archive contained an unsafe path")?;
        let is_symlink = entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000);
        extracted = extracted
            .checked_add(entry.size())
            .ok_or("Chrome extracted size overflowed")?;
        if extracted > MAX_CHROME_EXTRACTED_BYTES {
            return Err("Chrome archive exceeded the extracted byte budget".into());
        }
        let output = destination.join(&relative);
        if is_symlink {
            #[cfg(not(unix))]
            return Err("Chrome archive contained a symbolic link on a non-Unix target".into());
            #[cfg(unix)]
            {
                use std::io::Read as _;
                if entry.size() > 4096 {
                    return Err("Chrome archive symlink target was too long".into());
                }
                let mut target = String::new();
                entry.read_to_string(&mut target)?;
                let target = PathBuf::from(target);
                if !safe_relative_symlink(&relative, &target) {
                    return Err("Chrome archive symlink escaped the extraction root".into());
                }
                symlinks.push((output, target));
                continue;
            }
        }
        if entry.is_dir() {
            std::fs::create_dir_all(&output)?;
            continue;
        }
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = File::create(&output)?;
        std::io::copy(&mut entry, &mut file)?;
        file.sync_all()?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&output, std::fs::Permissions::from_mode(mode & 0o777))?;
        }
        #[cfg(windows)]
        {
            let metadata = std::fs::metadata(&output)?;
            if metadata.permissions().readonly() {
                let mut perms = metadata.permissions();
                perms.set_readonly(false);
                std::fs::set_permissions(&output, perms)?;
            }
        }
    }
    #[cfg(unix)]
    for (link, target) in symlinks {
        if let Some(parent) = link.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::os::unix::fs::symlink(target, link)?;
    }
    sync_directory_tree(destination)?;
    Ok(())
}

fn sync_directory_tree(root: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            sync_directory_tree(&entry.path())?;
        }
    }
    sync_directory(root)
}

/// On Windows, recursively clear the read-only attribute from all files so
/// that `remove_dir_all` can delete them. On Unix this is a no-op because
/// directory write permission is sufficient for deletion.
#[cfg(windows)]
fn make_removable(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            make_removable(&entry.path())?;
        }
    }
    let metadata = std::fs::metadata(path)?;
    if metadata.permissions().readonly() {
        let mut perms = metadata.permissions();
        perms.set_readonly(false);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn make_removable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn safe_relative_symlink(link: &Path, target: &Path) -> bool {
    if target.is_absolute() {
        return false;
    }
    let mut depth = link
        .parent()
        .map_or(0, |parent| parent.components().count());
    for component in target.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(_) => depth += 1,
            Component::ParentDir if depth > 0 => depth -= 1,
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    true
}

/// Launch Chrome with remote debugging enabled in headless mode.
pub async fn launch_chrome(
    chrome_path: &Path,
    port: u16,
    profile_dir: Option<&Path>,
) -> Result<ChromeProcess, Box<dyn std::error::Error>> {
    launch_chrome_with_viewport(chrome_path, port, profile_dir, false, false, None, None).await
}

/// Launch Chrome with remote debugging enabled and an optional initial window size.
pub async fn launch_chrome_with_viewport(
    chrome_path: &Path,
    port: u16,
    profile_dir: Option<&Path>,
    headed: bool,
    incognito: bool,
    host_resolver_rules: Option<&str>,
    viewport: Option<(i64, i64)>,
) -> Result<ChromeProcess, Box<dyn std::error::Error>> {
    let args = chrome_arguments(port, profile_dir, headed, incognito, viewport);
    info!(binary = %chrome_path.display(), arguments = %args.join(" "), "launching Chrome");
    let mut command = Command::new(chrome_path);
    command
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(rules) = host_resolver_rules {
        command.arg(format!("--host-resolver-rules={rules}"));
    }
    let mut child = command.spawn()?;
    let pid = child.id().unwrap_or(0);
    let stderr = child
        .stderr
        .take()
        .ok_or("Chrome startup did not provide a stderr stream")?;
    let mut stderr = BufReader::new(stderr).lines();
    let child_debugger_url = match wait_for_child_debugger_url(&mut child, &mut stderr, port).await
    {
        Ok(url) => url,
        Err(error) => {
            stop_startup_child(&mut child).await;
            return Err(error);
        }
    };
    if let Err(error) =
        wait_for_owned_debugger_endpoint(&mut child, port, &child_debugger_url).await
    {
        stop_startup_child(&mut child).await;
        return Err(error);
    }
    let mut stderr = stderr.into_inner();
    let stderr_drain = tokio::spawn(async move {
        let mut sink = tokio::io::sink();
        let _ = tokio::io::copy(&mut stderr, &mut sink).await;
    });
    Ok(ChromeProcess {
        port,
        pid,
        browser_ws_url: child_debugger_url,
        child: Some(child),
        stderr_drain: Some(stderr_drain),
    })
}

/// Legacy launcher preserving the historical default viewport.
pub async fn launch_chrome_with_options(
    chrome_path: &Path,
    port: u16,
    profile_dir: Option<&Path>,
    headed: bool,
    incognito: bool,
    host_resolver_rules: Option<&str>,
) -> Result<ChromeProcess, Box<dyn std::error::Error>> {
    launch_chrome_with_viewport(
        chrome_path,
        port,
        profile_dir,
        headed,
        incognito,
        host_resolver_rules,
        None,
    )
    .await
}

/// Wait for Chrome's own DevTools listener announcement.
///
/// A health check alone is insufficient: another process could have claimed
/// the port after the caller checked it. Chrome emits a unique browser
/// WebSocket URL on stderr only after its own DevTools listener starts; that
/// URL is subsequently compared against `/json/version` before the endpoint
/// is accepted.
async fn wait_for_child_debugger_url(
    child: &mut Child,
    stderr: &mut tokio::io::Lines<BufReader<tokio::process::ChildStderr>>,
    port: u16,
) -> Result<String, Box<dyn std::error::Error>> {
    let deadline = tokio::time::Instant::now() + CHROME_STARTUP_TIMEOUT;
    loop {
        tokio::select! {
            line = stderr.next_line() => match line? {
                Some(line) => {
                    if let Some(debugger_url) = child_debugger_url_from_stderr(&line, port) {
                        return Ok(debugger_url);
                    }
                }
                None => {
                    if let Some(status) = child.try_wait()? {
                        return Err(format!("Chrome exited during startup with status {status}").into());
                    }
                    return Err(format!("Chrome closed stderr before reporting its DevTools listener on port {port}").into());
                }
            },
            _ = tokio::time::sleep(CHROME_STARTUP_POLL_INTERVAL) => {
                if let Some(status) = child.try_wait()? {
                    return Err(format!("Chrome exited during startup with status {status}").into());
                }
                if tokio::time::Instant::now() >= deadline {
                    return Err(format!("Chrome did not report its DevTools listener on port {port}").into());
                }
            }
        }
    }
}

fn child_debugger_url_from_stderr(line: &str, port: u16) -> Option<String> {
    let marker = "DevTools listening on ";
    let offset = line.find(marker)? + marker.len();
    let debugger_url = line[offset..].trim();
    let port_and_path = format!(":{port}/devtools/browser/");
    (debugger_url.starts_with("ws://") && debugger_url.contains(&port_and_path))
        .then(|| debugger_url.to_string())
}

async fn wait_for_owned_debugger_endpoint(
    child: &mut Child,
    port: u16,
    child_debugger_url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = tokio::time::Instant::now() + CHROME_STARTUP_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait()? {
            return Err(format!("Chrome exited during startup with status {status}").into());
        }
        if let Some(debugger_url) = debugger_url_at(port).await {
            if debugger_url == child_debugger_url {
                return Ok(());
            }
            return Err(format!(
                "CDP port {port} is serving a different Chrome endpoint; use --attach to connect to it or choose another --port"
            )
            .into());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("Chrome did not become ready on port {port}").into());
        }
        tokio::time::sleep(CHROME_STARTUP_POLL_INTERVAL).await;
    }
}

async fn stop_startup_child(child: &mut Child) {
    let _ = child.kill().await;
    let _ = child.wait().await;
}

fn chrome_arguments(
    port: u16,
    profile_dir: Option<&Path>,
    headed: bool,
    incognito: bool,
    viewport: Option<(i64, i64)>,
) -> Vec<String> {
    let mut args = vec![
        format!("--remote-debugging-port={port}"),
        "--remote-debugging-address=127.0.0.1".to_string(),
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
        "--disable-background-networking".to_string(),
        "--disable-sync".to_string(),
        "--disable-translate".to_string(),
        "--disable-extensions".to_string(),
        "--disable-default-apps".to_string(),
        "--disable-session-crashed-bubble".to_string(),
        "--disable-restore-session-state".to_string(),
        "--disable-features=Translate,BackForwardCache".to_string(),
        format!(
            "--window-size={},{}",
            viewport.map_or(1280, |(width, _)| width),
            viewport.map_or(720, |(_, height)| height)
        ),
    ];
    if !headed {
        args.push("--headless=new".to_string());
        args.push("--hide-scrollbars".to_string());
    }
    if chrome_sandbox_disabled(std::env::var_os("GLASS_DISABLE_CHROME_SANDBOX").as_deref()) {
        args.push("--no-sandbox".to_string());
    }
    if let Some(profile) = profile_dir {
        args.push(format!("--user-data-dir={}", profile.display()));
    }
    if incognito {
        args.push("--incognito".to_string());
    }
    args.push("about:blank".to_string());
    args
}

fn chrome_sandbox_disabled(value: Option<&std::ffi::OsStr>) -> bool {
    value == Some(std::ffi::OsStr::new("1"))
}

/// Check if Chrome is running and responsive on the given port.
pub async fn check_chrome_health(port: u16) -> bool {
    debugger_url_at(port).await.is_some()
}

async fn debugger_url_at(port: u16) -> Option<String> {
    #[derive(Deserialize)]
    struct BrowserVersion {
        #[serde(rename = "webSocketDebuggerUrl")]
        websocket_debugger_url: Option<String>,
    }

    let url = format!("http://127.0.0.1:{port}/json/version");
    let response = reqwest::Client::new()
        .get(url)
        .timeout(Duration::from_millis(500))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    response
        .json::<BrowserVersion>()
        .await
        .ok()?
        .websocket_debugger_url
}

/// Return whether any process is currently listening on the local CDP port.
///
/// This is intentionally broader than the Chrome health check: an owned Glass
/// session must not launch onto a port already controlled by another process.
pub async fn is_port_occupied(port: u16) -> bool {
    matches!(
        tokio::time::timeout(
            Duration::from_millis(250),
            TcpStream::connect(("127.0.0.1", port)),
        )
        .await,
        Ok(Ok(_))
    )
}

/// Kill processes listening on a debugging port.
pub async fn kill_chrome(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("lsof")
        .args(["-ti", &format!(":{port}")])
        .output()
        .await?;

    if output.status.success() {
        for pid in String::from_utf8_lossy(&output.stdout).lines() {
            if let Ok(pid_num) = pid.trim().parse::<u32>() {
                warn!(pid = pid_num, "killing Chrome process");
                #[cfg(unix)]
                unsafe {
                    libc::kill(pid_num as i32, libc::SIGTERM);
                }
            }
        }
    }
    Ok(())
}

/// A page target advertised by Chrome's HTTP DevTools endpoint.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct PageTarget {
    id: String,
    #[serde(rename = "type")]
    target_type: String,
    #[serde(rename = "webSocketDebuggerUrl")]
    websocket_debugger_url: Option<String>,
    #[serde(default)]
    url: String,
}

#[derive(Deserialize)]
struct BrowserVersionEndpoint {
    #[serde(rename = "webSocketDebuggerUrl")]
    websocket_debugger_url: String,
}

/// Fetch the browser-level WebSocket debugger URL from Chrome's `/json/version`
/// endpoint on the given port. Returns the `webSocketDebuggerUrl` field.
pub async fn get_browser_ws_url(port: u16) -> Result<String, Box<dyn std::error::Error>> {
    let response = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/json/version"))
        .timeout(Duration::from_secs(2))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(format!("Chrome version endpoint failed with {}", response.status()).into());
    }
    Ok(response
        .json::<BrowserVersionEndpoint>()
        .await?
        .websocket_debugger_url)
}

/// Get the WebSocket debugger URL for an explicitly selected page target.
///
/// A single page target is selected automatically. Multiple page targets are
/// never silently adopted: callers must provide the matching Chrome target ID.
pub async fn get_ws_url(
    port: u16,
    target_id: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let targets = page_targets_at(port).await?;
    select_page_target(&targets, target_id)
        .map(|target| {
            target
                .websocket_debugger_url
                .clone()
                .expect("filtered above")
        })
        .map_err(Into::into)
}

/// Get a page target for a browser owned by Glass. A persistent profile can
/// restore its last page while Chrome also opens Glass's startup page. Owned
/// sessions may adopt the first page target in that case; attached sessions
/// retain strict selection.
pub async fn get_owned_ws_url(
    port: u16,
    target_id: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let targets = page_targets_at(port).await?;
    let target = select_owned_page_target(&targets, target_id)?;
    Ok(target
        .websocket_debugger_url
        .clone()
        .expect("filtered above"))
}

fn select_owned_page_target<'a>(
    targets: &'a [PageTarget],
    target_id: Option<&str>,
) -> Result<&'a PageTarget, String> {
    match select_page_target(targets, target_id) {
        Ok(target) => Ok(target),
        Err(error) if target_id.is_none() && error.starts_with("multiple page targets") => {
            let candidates = targets
                .iter()
                .filter(|target| {
                    target.target_type == "page"
                        && target.websocket_debugger_url.is_some()
                        && target.url != "about:blank"
                })
                .collect::<Vec<_>>();
            match candidates.as_slice() {
                [target] => Ok(target),
                [] => targets
                    .iter()
                    .find(|target| {
                        target.target_type == "page" && target.websocket_debugger_url.is_some()
                    })
                    .ok_or(error),
                _ => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

async fn page_targets_at(port: u16) -> Result<Vec<PageTarget>, Box<dyn std::error::Error>> {
    let url = format!("http://127.0.0.1:{port}/json");
    let response = reqwest::Client::new()
        .get(url)
        .timeout(Duration::from_secs(2))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(format!("Chrome target listing failed with {}", response.status()).into());
    }
    Ok(response.json().await?)
}

fn select_page_target<'a>(
    targets: &'a [PageTarget],
    target_id: Option<&str>,
) -> Result<&'a PageTarget, String> {
    let pages: Vec<&PageTarget> = targets
        .iter()
        .filter(|target| target.target_type == "page" && target.websocket_debugger_url.is_some())
        .collect();

    if let Some(target_id) = target_id {
        return pages
            .into_iter()
            .find(|target| target.id == target_id)
            .ok_or_else(|| format!("page target '{target_id}' was not found"));
    }

    match pages.as_slice() {
        [] => Err("No page target with a WebSocket debugger URL was found".to_string()),
        [target] => Ok(target),
        pages => {
            let ids = pages
                .iter()
                .map(|target| target.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "multiple page targets are available ({ids}); pass --target-id <id> to choose one"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::net::TcpListener;

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn test_directory() -> PathBuf {
        let id = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("glass-chrome-test-{}-{id}", std::process::id()))
    }

    async fn unused_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    }

    fn page_target(id: &str) -> PageTarget {
        PageTarget {
            id: id.to_string(),
            target_type: "page".to_string(),
            websocket_debugger_url: Some(format!("ws://example.test/{id}")),
            url: "about:blank".to_string(),
        }
    }

    #[test]
    fn owned_target_selection_adopts_one_restored_nonblank_page() {
        let mut restored = page_target("restored");
        restored.url = "http://127.0.0.1:1234/".to_string();
        let targets = vec![page_target("startup"), restored];

        assert_eq!(
            select_owned_page_target(&targets, None).unwrap().id,
            "restored"
        );
    }

    #[test]
    fn chrome_path_prefers_explicit_then_managed_then_system() {
        let explicit = PathBuf::from("/explicit/chrome");
        let managed = PathBuf::from("/managed/chrome");
        let system = PathBuf::from("/system/chrome");

        assert_eq!(
            choose_chrome_path(
                Some(explicit.clone()),
                Some(managed.clone()),
                Some(system.clone())
            ),
            Some(explicit)
        );
        assert_eq!(
            choose_chrome_path(None, Some(managed.clone()), Some(system.clone())),
            Some(managed)
        );
        assert_eq!(
            choose_chrome_path(None, None, Some(system.clone())),
            Some(system)
        );
    }

    #[test]
    fn managed_chrome_discovery_uses_each_supported_platform_layout() {
        let layouts = [
            ("linux", "x86_64", "chrome-linux64/chrome"),
            (
                "macos",
                "x86_64",
                "chrome-mac-x64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
            ),
            (
                "macos",
                "aarch64",
                "chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
            ),
            ("windows", "x86_64", "chrome-win64/chrome.exe"),
            ("windows", "x86", "chrome-win32/chrome.exe"),
        ];

        for (os, architecture, expected_relative_path) in layouts {
            let root = test_directory();
            let platform = ManagedChromePlatform::for_target(os, architecture).unwrap();
            let version_name = format!("{PINNED_CHROME_VERSION}-test");
            let version_root = root.join("versions").join(&version_name);
            let executable = version_root.join(expected_relative_path);
            std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
            std::fs::write(&executable, "managed chrome").unwrap();
            std::fs::write(
                version_root.join(".complete"),
                format!("{PINNED_CHROME_VERSION}\n"),
            )
            .unwrap();
            std::fs::create_dir_all(root.join("current")).unwrap();
            std::fs::write(root.join("current/1"), version_name).unwrap();

            assert_eq!(platform.executable_relative_path(), expected_relative_path);
            assert_eq!(managed_chrome_path_in(&root, platform), Some(executable));

            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn managed_chrome_platform_rejects_unsupported_targets() {
        assert_eq!(ManagedChromePlatform::for_target("freebsd", "x86_64"), None);
        assert_eq!(ManagedChromePlatform::for_target("linux", "aarch64"), None);
        assert_eq!(
            ManagedChromePlatform::for_target("windows", "aarch64"),
            None
        );
    }

    #[test]
    fn managed_discovery_ignores_incomplete_installations() {
        let root = test_directory();
        let executable = root.join("versions/150-incomplete/chrome-linux64/chrome");
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::write(&executable, "partial").unwrap();
        assert_eq!(
            managed_chrome_path_in(&root, ManagedChromePlatform::LinuxX64),
            None
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn zip_extraction_rejects_parent_traversal() {
        use std::io::Write;
        let root = test_directory();
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("malicious.zip");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("../escape", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"escape").unwrap();
        writer.finish().unwrap();
        let output = root.join("output");
        std::fs::create_dir(&output).unwrap();
        assert!(extract_chrome_zip(&archive_path, &output).is_err());
        assert!(!root.join("escape").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn symlink_targets_must_remain_inside_the_archive_root() {
        assert!(safe_relative_symlink(
            Path::new("App/Framework/Versions/Current"),
            Path::new("150.0.0")
        ));
        assert!(safe_relative_symlink(
            Path::new("App/Framework/Resources"),
            Path::new("Versions/Current/Resources")
        ));
        assert!(!safe_relative_symlink(
            Path::new("App/link"),
            Path::new("../../escape")
        ));
        assert!(!safe_relative_symlink(
            Path::new("App/link"),
            Path::new("/tmp/escape")
        ));
    }

    #[test]
    fn chrome_arguments_add_incognito_and_disposable_profile_dir() {
        let profile = Path::new("/tmp/glass-incognito");
        let args = chrome_arguments(9222, Some(profile), false, true, None);

        assert!(args.contains(&"--incognito".to_string()));
        assert!(args.contains(&"--user-data-dir=/tmp/glass-incognito".to_string()));
        assert!(args.contains(&"--disable-restore-session-state".to_string()));
    }

    #[test]
    fn chrome_sandbox_requires_exact_explicit_opt_out() {
        use std::ffi::OsStr;

        assert!(!chrome_sandbox_disabled(None));
        assert!(!chrome_sandbox_disabled(Some(OsStr::new("0"))));
        assert!(!chrome_sandbox_disabled(Some(OsStr::new("true"))));
        assert!(chrome_sandbox_disabled(Some(OsStr::new("1"))));
    }

    #[test]
    fn child_debugger_announcement_must_match_the_requested_port() {
        assert_eq!(
            child_debugger_url_from_stderr(
                "DevTools listening on ws://127.0.0.1:9222/devtools/browser/child",
                9222,
            ),
            Some("ws://127.0.0.1:9222/devtools/browser/child".to_string())
        );
        assert!(
            child_debugger_url_from_stderr(
                "DevTools listening on ws://127.0.0.1:9223/devtools/browser/child",
                9222,
            )
            .is_none()
        );
    }

    #[tokio::test]
    async fn port_launch_lock_serializes_competing_owned_starts() {
        let port = unused_port().await;
        let first = PortLaunchLock::acquire(port).await.unwrap();
        let second = tokio::spawn(async move { PortLaunchLock::acquire(port).await.unwrap() });

        tokio::time::sleep(Duration::from_millis(75)).await;
        assert!(!second.is_finished());

        drop(first);
        drop(second.await.unwrap());
    }

    #[test]
    fn port_launch_lock_is_exclusive_across_processes() {
        const HELPER_PORT: &str = "GLASS_PORT_LOCK_HELPER_PORT";
        const HELPER_READY: &str = "GLASS_PORT_LOCK_HELPER_READY";
        const HELPER_RELEASE: &str = "GLASS_PORT_LOCK_HELPER_RELEASE";

        if let Ok(port) = std::env::var(HELPER_PORT) {
            let port = port.parse().unwrap();
            let ready = PathBuf::from(std::env::var(HELPER_READY).unwrap());
            let release = PathBuf::from(std::env::var(HELPER_RELEASE).unwrap());
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .enable_time()
                .build()
                .unwrap();
            let _lock = runtime.block_on(PortLaunchLock::acquire(port)).unwrap();
            std::fs::write(ready, "ready").unwrap();

            let deadline = Instant::now() + Duration::from_secs(10);
            while !release.exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            return;
        }

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .unwrap();
        let port = runtime.block_on(unused_port());
        let root = test_directory();
        std::fs::create_dir_all(&root).unwrap();
        let ready = root.join("ready");
        let release = root.join("release");
        let test_name = "browser::chrome::tests::port_launch_lock_is_exclusive_across_processes";
        let mut helper = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test_name, "--nocapture"])
            .env(HELPER_PORT, port.to_string())
            .env(HELPER_READY, &ready)
            .env(HELPER_RELEASE, &release)
            .spawn()
            .unwrap();

        let ready_deadline = Instant::now() + Duration::from_secs(5);
        while !ready.exists() && Instant::now() < ready_deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(ready.exists(), "lock helper did not become ready");

        let blocked = runtime.block_on(async {
            tokio::time::timeout(Duration::from_millis(150), PortLaunchLock::acquire(port)).await
        });
        assert!(blocked.is_err(), "another process must hold the port lock");

        std::fs::write(&release, "release").unwrap();
        assert!(helper.wait().unwrap().success());
        drop(runtime.block_on(PortLaunchLock::acquire(port)).unwrap());
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn launcher_rejects_a_foreign_healthy_endpoint_after_child_announcement() {
        use std::os::unix::fs::PermissionsExt;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let foreign_url = format!("ws://127.0.0.1:{port}/devtools/browser/foreign");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 1_024];
            let _ = stream.read(&mut request).await.unwrap();
            let body = serde_json::json!({
                "webSocketDebuggerUrl": foreign_url,
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });

        let root = test_directory();
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("fake-chrome.sh");
        let child_url = format!("ws://127.0.0.1:{port}/devtools/browser/child");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' 'DevTools listening on {child_url}' >&2\nsleep 30\n"
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).unwrap();

        let error = launch_chrome_with_options(&script, port, Some(&root), false, false, None)
            .await
            .err()
            .expect("a foreign endpoint must not be adopted");
        assert!(error.to_string().contains("different Chrome endpoint"));

        server.await.unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn target_selection_requires_an_explicit_id_when_ambiguous() {
        let targets = vec![page_target("one"), page_target("two")];

        let error = select_page_target(&targets, None).unwrap_err();
        assert!(error.contains("multiple page targets"));
        assert_eq!(select_page_target(&targets, Some("two")).unwrap().id, "two");
        assert!(select_page_target(&targets, Some("missing")).is_err());
    }
}
