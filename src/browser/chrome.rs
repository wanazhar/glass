use fs2::FileExt;
use serde::Deserialize;
use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
use tracing::{info, warn};

const PORT_LAUNCH_LOCK_TIMEOUT: Duration = Duration::from_secs(20);
const PORT_LAUNCH_LOCK_RETRY: Duration = Duration::from_millis(25);
const CHROME_STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const CHROME_STARTUP_POLL_INTERVAL: Duration = Duration::from_millis(100);

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
    child: Option<Child>,
    stderr_drain: Option<JoinHandle<()>>,
}

impl ChromeProcess {
    pub fn ws_debug_url(&self) -> String {
        format!("http://127.0.0.1:{}/json/version", self.port)
    }

    /// Stop a Chrome process owned by this instance.
    pub async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let shutdown_result = match self.child.take() {
            Some(mut child) => match child.try_wait() {
                Ok(None) => match child.kill().await {
                    Ok(()) => {
                        let _ = child.wait().await;
                        Ok(())
                    }
                    Err(error) => Err(Box::new(error) as Box<dyn std::error::Error>),
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
    LinuxArm64,
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
            ("linux", "aarch64") => Some(Self::LinuxArm64),
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
            Self::LinuxArm64 => "linux-arm64",
            Self::MacX64 => "mac-x64",
            Self::MacArm64 => "mac-arm64",
            Self::WindowsX64 => "win64",
            Self::WindowsX86 => "win32",
        }
    }

    fn executable_relative_path(self) -> &'static str {
        match self {
            Self::LinuxX64 => "chrome-linux64/chrome",
            Self::LinuxArm64 => "chrome-linux-arm64/chrome",
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
}

fn managed_chrome_path_in(install_dir: &Path, platform: ManagedChromePlatform) -> Option<PathBuf> {
    let path = install_dir.join(platform.executable_relative_path());
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

/// Download the latest stable Chrome for Testing build for this platform.
///
/// This remains a convenience fallback. Production deployments should prefer
/// a system-managed Chrome/Chromium installation so the browser is updated
/// independently from the Glass binary. Extraction uses the platform's
/// `unzip` command to keep the Glass executable small.
pub async fn download_chromium() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let install_dir = chromium_install_dir();
    let platform = ManagedChromePlatform::current()
        .ok_or("Chrome for Testing is unavailable for this platform")?;

    std::fs::create_dir_all(&install_dir)?;
    if let Some(path) = managed_chrome_path_in(&install_dir, platform) {
        info!(path = %path.display(), "Chromium already installed");
        return Ok(path);
    }
    let metadata_url = "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json";
    let metadata: serde_json::Value = reqwest::Client::new()
        .get(metadata_url)
        .timeout(Duration::from_secs(15))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let download_url = metadata["channels"]["Stable"]["downloads"]["chrome"]
        .as_array()
        .and_then(|downloads| {
            downloads
                .iter()
                .find(|download| download["platform"] == platform.download_platform())
        })
        .and_then(|download| download["url"].as_str())
        .ok_or_else(|| {
            format!(
                "no stable Chrome for Testing download for {}",
                platform.download_platform()
            )
        })?;

    info!(
        platform = platform.download_platform(),
        "downloading Chrome for Testing"
    );
    let archive_path = install_dir.join("chrome-for-testing.zip");
    let bytes = reqwest::Client::new()
        .get(download_url)
        .timeout(Duration::from_secs(120))
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    tokio::fs::write(&archive_path, &bytes).await?;

    let status = Command::new("unzip")
        .args(["-q", "-o"])
        .arg(&archive_path)
        .arg("-d")
        .arg(&install_dir)
        .status()
        .await
        .map_err(|error| format!("failed to run unzip: {error}"))?;
    tokio::fs::remove_file(&archive_path).await.ok();
    if !status.success() {
        return Err("failed to extract Chrome for Testing archive; install unzip and retry".into());
    }

    let path = managed_chrome_path_in(&install_dir, platform)
        .ok_or("Chrome archive did not contain a browser executable")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = tokio::fs::metadata(&path).await?.permissions();
        permissions.set_mode(0o755);
        tokio::fs::set_permissions(&path, permissions).await?;
    }
    info!(path = %path.display(), "Chrome for Testing installed");
    Ok(path)
}

/// Launch Chrome with remote debugging enabled in headless mode.
pub async fn launch_chrome(
    chrome_path: &Path,
    port: u16,
    profile_dir: Option<&Path>,
) -> Result<ChromeProcess, Box<dyn std::error::Error>> {
    launch_chrome_with_options(chrome_path, port, profile_dir, false, false).await
}

/// Launch Chrome with remote debugging enabled.
pub async fn launch_chrome_with_options(
    chrome_path: &Path,
    port: u16,
    profile_dir: Option<&Path>,
    headed: bool,
    incognito: bool,
) -> Result<ChromeProcess, Box<dyn std::error::Error>> {
    let args = chrome_arguments(port, profile_dir, headed, incognito);

    info!(binary = %chrome_path.display(), arguments = %args.join(" "), "launching Chrome");
    let mut child = Command::new(chrome_path)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
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

    // Chrome can continue to write diagnostic output after startup. Drain it
    // rather than retaining an unread pipe that could eventually block the
    // child, but do not retain any diagnostic text in Glass memory.
    let mut stderr = stderr.into_inner();
    let stderr_drain = tokio::spawn(async move {
        let mut sink = tokio::io::sink();
        let _ = tokio::io::copy(&mut stderr, &mut sink).await;
    });

    Ok(ChromeProcess {
        port,
        pid,
        child: Some(child),
        stderr_drain: Some(stderr_drain),
    })
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
        "--disable-features=Translate,BackForwardCache".to_string(),
        "--window-size=1280,720".to_string(),
    ];

    if !headed {
        args.push("--headless=new".to_string());
        args.push("--hide-scrollbars".to_string());
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
}

/// Get the WebSocket debugger URL for an explicitly selected page target.
///
/// A single page target is selected automatically. Multiple page targets are
/// never silently adopted: callers must provide the matching Chrome target ID.
pub async fn get_ws_url(
    port: u16,
    target_id: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let url = format!("http://127.0.0.1:{port}/json");
    let response = reqwest::Client::new()
        .get(url)
        .timeout(Duration::from_secs(2))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(format!("Chrome target listing failed with {}", response.status()).into());
    }
    let targets: Vec<PageTarget> = response.json().await?;
    select_page_target(&targets, target_id)
        .map(|target| {
            target
                .websocket_debugger_url
                .clone()
                .expect("filtered above")
        })
        .map_err(Into::into)
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
        }
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
            ("linux", "aarch64", "chrome-linux-arm64/chrome"),
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
            let executable = root.join(expected_relative_path);
            std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
            std::fs::write(&executable, "managed chrome").unwrap();

            assert_eq!(platform.executable_relative_path(), expected_relative_path);
            assert_eq!(managed_chrome_path_in(&root, platform), Some(executable));

            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn managed_chrome_platform_rejects_unsupported_targets() {
        assert_eq!(ManagedChromePlatform::for_target("freebsd", "x86_64"), None);
        assert_eq!(
            ManagedChromePlatform::for_target("windows", "aarch64"),
            None
        );
    }

    #[test]
    fn chrome_arguments_add_incognito_and_disposable_profile_dir() {
        let profile = Path::new("/tmp/glass-incognito");
        let args = chrome_arguments(9222, Some(profile), false, true);

        assert!(args.contains(&"--incognito".to_string()));
        assert!(args.contains(&"--user-data-dir=/tmp/glass-incognito".to_string()));
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

        let error = launch_chrome_with_options(&script, port, Some(&root), false, false)
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
