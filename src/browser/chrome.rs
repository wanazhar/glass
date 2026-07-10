use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};
use tracing::{info, warn};

/// A Chrome process started by Glass.
pub struct ChromeProcess {
    pub port: u16,
    pub pid: u32,
    child: Option<Child>,
}

impl ChromeProcess {
    pub fn ws_debug_url(&self) -> String {
        format!("http://127.0.0.1:{}/json/version", self.port)
    }

    /// Stop a Chrome process owned by this instance.
    pub async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(mut child) = self.child.take()
            && child.try_wait()?.is_none()
        {
            child.kill().await?;
            let _ = child.wait().await;
        }
        Ok(())
    }
}

impl Drop for ChromeProcess {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.start_kill();
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

/// Download the latest stable Chrome for Testing build for this platform.
///
/// This remains a convenience fallback. Production deployments should prefer
/// a system-managed Chrome/Chromium installation so the browser is updated
/// independently from the Glass binary. Extraction uses the platform's
/// `unzip` command to keep the Glass executable small.
pub async fn download_chromium() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let install_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("glass")
        .join("chromium");

    std::fs::create_dir_all(&install_dir)?;
    let executable_name = if cfg!(windows) {
        "chrome.exe"
    } else {
        "chrome"
    };
    if let Some(path) = find_file_named(&install_dir, executable_name)? {
        info!(path = %path.display(), "Chromium already installed");
        return Ok(path);
    }

    let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux64",
        ("linux", "aarch64") => "linux-arm64",
        ("macos", "x86_64") => "mac-x64",
        ("macos", "aarch64") => "mac-arm64",
        ("windows", "x86_64") => "win64",
        ("windows", "x86") => "win32",
        _ => return Err("Chrome for Testing is unavailable for this platform".into()),
    };
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
                .find(|download| download["platform"] == platform)
        })
        .and_then(|download| download["url"].as_str())
        .ok_or_else(|| format!("no stable Chrome for Testing download for {platform}"))?;

    info!(%platform, "downloading Chrome for Testing");
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

    let path = find_file_named(&install_dir, executable_name)?
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

fn find_file_named(root: &Path, name: &str) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    if !root.is_dir() {
        return Ok(None);
    }
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.file_name().and_then(|file| file.to_str()) == Some(name) && path.is_file() {
            return Ok(Some(path));
        }
        if path.is_dir()
            && let Some(found) = find_file_named(&path, name)?
        {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// Launch Chrome with remote debugging enabled in headless mode.
pub async fn launch_chrome(
    chrome_path: &Path,
    port: u16,
    profile_dir: Option<&Path>,
) -> Result<ChromeProcess, Box<dyn std::error::Error>> {
    launch_chrome_with_options(chrome_path, port, profile_dir, false).await
}

/// Launch Chrome with remote debugging enabled.
pub async fn launch_chrome_with_options(
    chrome_path: &Path,
    port: u16,
    profile_dir: Option<&Path>,
    headed: bool,
) -> Result<ChromeProcess, Box<dyn std::error::Error>> {
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
    args.push("about:blank".to_string());

    info!(binary = %chrome_path.display(), arguments = %args.join(" "), "launching Chrome");
    let mut child = Command::new(chrome_path)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let pid = child.id().unwrap_or(0);

    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if check_chrome_health(port).await {
            return Ok(ChromeProcess {
                port,
                pid,
                child: Some(child),
            });
        }
        if let Some(status) = child.try_wait()? {
            return Err(format!("Chrome exited during startup with status {status}").into());
        }
        if Instant::now() >= deadline {
            let _ = child.kill().await;
            return Err(format!("Chrome did not become ready on port {port}").into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Check if Chrome is running and responsive on the given port.
pub async fn check_chrome_health(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{port}/json/version");
    match reqwest::Client::new()
        .get(url)
        .timeout(Duration::from_millis(500))
        .send()
        .await
    {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
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

/// Get the WebSocket debugger URL for the first page target.
pub async fn get_ws_url(port: u16) -> Result<String, Box<dyn std::error::Error>> {
    let url = format!("http://127.0.0.1:{port}/json");
    let response = reqwest::Client::new()
        .get(url)
        .timeout(Duration::from_secs(2))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(format!("Chrome target listing failed with {}", response.status()).into());
    }
    let targets: Vec<serde_json::Value> = response.json().await?;

    targets
        .iter()
        .find(|target| target["type"] == "page")
        .and_then(|target| target["webSocketDebuggerUrl"].as_str())
        .map(String::from)
        .ok_or_else(|| "No page target with a WebSocket debugger URL was found".into())
}
