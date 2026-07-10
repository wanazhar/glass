use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{info, warn};


/// Chrome process handle.
pub struct ChromeProcess {
    pub port: u16,
    pub pid: u32,
}

impl ChromeProcess {
    pub fn ws_debug_url(&self) -> String {
        format!("http://127.0.0.1:{}/json/version", self.port)
    }
}

/// Detect Chrome/Chromium installation on the system.
pub fn detect_chrome() -> Option<PathBuf> {
    let candidates = [
        // Linux
        "google-chrome",
        "google-chrome-stable",
        "chromium-browser",
        "chromium",
        // macOS
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        // Common Linux paths
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium-browser",
        "/usr/bin/chromium",
        "/snap/bin/chromium",
    ];

    for candidate in &candidates {
        if std::path::Path::new(candidate).exists() {
            return Some(PathBuf::from(candidate));
        }
    }

    // Try which
    if let Ok(output) = std::process::Command::new("which").arg("google-chrome").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
    }
    if let Ok(output) = std::process::Command::new("which").arg("chromium").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
    }

    None
}

/// Download Chromium using the Chromium snapshot service.
pub async fn download_chromium() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let install_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("glass")
        .join("chromium");

    std::fs::create_dir_all(&install_dir)?;

    let chrome_path = install_dir.join("chrome");
    if chrome_path.exists() {
        info!("Chromium already installed at {}", chrome_path.display());
        return Ok(chrome_path);
    }

    info!("Downloading Chromium to {}...", install_dir.display());

    // Use chromium snapshots for the current platform
    let (platform, ext, archive_ext) = match std::env::consts::OS {
        "linux" => ("Linux", "", ".tar.xz"),
        "macos" => ("Mac", "", ".zip"),
        "windows" => ("Win", ".exe", ".zip"),
        _ => return Err("Unsupported platform".into()),
    };

    let url = format!(
        "https://storage.googleapis.com/chromium-browser-snapshots/{}/{}/chrome-{}{}",
        if std::env::consts::ARCH == "aarch64" { "Arm64" } else { "Linux_x64" },
        "LAST_CHANGE",
        platform,
        ext,
    );

    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await?;

    if !resp.status().is_success() {
        return Err(format!("Failed to download Chromium: HTTP {}", resp.status()).into());
    }

    let bytes = resp.bytes().await?;

    if archive_ext == ".tar.xz" {
        // Write archive, then extract
        let archive_path = install_dir.join("chrome.tar.xz");
        tokio::fs::write(&archive_path, &bytes).await?;

        let status = tokio::process::Command::new("tar")
            .args(["xf", archive_path.to_str().unwrap(), "-C", install_dir.to_str().unwrap()])
            .status()
            .await?;

        if !status.success() {
            return Err("Failed to extract Chromium archive".into());
        }

        tokio::fs::remove_file(&archive_path).await.ok();
    } else {
        tokio::fs::write(&chrome_path, &bytes).await?;
    }

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(&chrome_path).await?.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(&chrome_path, perms).await?;
    }

    info!("Chromium installed at {}", chrome_path.display());
    Ok(chrome_path)
}

/// Launch Chrome with remote debugging enabled.
pub async fn launch_chrome(
    chrome_path: &std::path::Path,
    port: u16,
    profile_dir: Option<&std::path::Path>,
) -> Result<ChromeProcess, Box<dyn std::error::Error>> {
    let mut args = vec![
        format!("--remote-debugging-port={port}"),
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
        "--disable-background-networking".to_string(),
        "--disable-sync".to_string(),
        "--disable-translate".to_string(),
        "--disable-extensions".to_string(),
        "--disable-default-apps".to_string(),
        "--disable-gpu".to_string(),
        "--window-size=1280,720".to_string(),
    ];

    if let Some(profile) = profile_dir {
        args.push(format!("--user-data-dir={}", profile.display()));
    }

    args.push("about:blank".to_string());

    info!("Launching Chrome: {} {}", chrome_path.display(), args.join(" "));

    let child = Command::new(chrome_path)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let pid = child.id().unwrap_or(0);

    // Wait a moment for Chrome to start
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    Ok(ChromeProcess { port, pid })
}

/// Check if Chrome is running and responsive on the given port.
pub async fn check_chrome_health(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{}/json/version", port);
    match reqwest::get(&url).await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Kill Chrome processes on the given port.
pub async fn kill_chrome(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    // Try to find and kill processes using the debugging port
    let output = tokio::process::Command::new("lsof")
        .args(["-ti", &format!(":{port}")])
        .output()
        .await?;

    if output.status.success() {
        let pids = String::from_utf8_lossy(&output.stdout);
        for pid in pids.lines() {
            if let Ok(pid_num) = pid.trim().parse::<u32>() {
                warn!("Killing Chrome process {pid_num}");
                unsafe {
                    libc::kill(pid_num as i32, libc::SIGTERM);
                }
            }
        }
    }

    Ok(())
}

/// Get the WebSocket debugger URL for the first available target.
pub async fn get_ws_url(port: u16) -> Result<String, Box<dyn std::error::Error>> {
    let url = format!("http://127.0.0.1:{}/json", port);
    let resp = reqwest::get(&url).await?;
    let targets: Vec<serde_json::Value> = resp.json().await?;

    targets
        .iter()
        .find(|t| t["type"] == "page")
        .and_then(|t| t["webSocketDebuggerUrl"].as_str())
        .map(String::from)
        .ok_or_else(|| "No page target found".into())
}
