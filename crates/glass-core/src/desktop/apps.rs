//! Bounded discovery and launch of installed desktop entries.
//!
//! App launches use GTK's desktop-entry resolver with an exact installed ID;
//! callers never supply a command line or URI.

use serde_json::{Value, json};
#[cfg(test)]
use std::io::Write;
#[cfg(test)]
use std::os::unix::fs::PermissionsExt;
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use super::process;

const MAX_APP_FILES: usize = 2048;
const MAX_VISIBLE_APPS: usize = 256;
const MAX_DESKTOP_FILE_BYTES: u64 = 64 * 1024;

pub(super) struct LaunchContext {
    pub(super) environment: process::Environment,
    pub(super) display: String,
    pub(super) home: PathBuf,
    #[cfg(test)]
    pub(super) session_directory: PathBuf,
    pub(super) xauthority: PathBuf,
    pub(super) runtime_directory: PathBuf,
    pub(super) session_bus_address: String,
    pub(super) atspi_bus_address: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AppEntry {
    id: String,
    name: String,
}

pub(super) fn list(home: &Path) -> Value {
    let entries = discover(home);
    let truncated = entries.len() > MAX_VISIBLE_APPS;
    let applications = entries
        .into_iter()
        .take(MAX_VISIBLE_APPS)
        .map(|entry| json!({"desktopEntryId":entry.id,"name":entry.name}))
        .collect::<Vec<_>>();
    json!({"applications":applications,"truncated":truncated})
}

pub(super) fn launch(context: &LaunchContext, entry_id: &str) -> Result<Value, String> {
    if !valid_entry_id(entry_id) {
        return Err(
            "desktopEntryId must be an installed desktop entry ID from desktop_launch_app list"
                .into(),
        );
    }
    if !discover(&context.home)
        .iter()
        .any(|entry| entry.id == entry_id)
    {
        return Err(
            "desktopEntryId is not an installed visible application in this managed desktop".into(),
        );
    }
    let gtk_launch = Path::new("/usr/bin/gtk-launch");
    let timeout = Path::new("/usr/bin/timeout");
    if !gtk_launch.is_file() || !timeout.is_file() {
        return Err("Managed desktop app launch requires gtk-launch and timeout".into());
    }

    let mut command = Command::new(timeout);
    command
        .env_clear()
        .env("PATH", process::MANAGED_CHILD_PATH)
        .env(
            "XDG_CURRENT_DESKTOP",
            match context.environment {
                process::Environment::Xfce => "XFCE",
                process::Environment::Gnome => "ubuntu:GNOME",
            },
        )
        .env(
            "DESKTOP_SESSION",
            match context.environment {
                process::Environment::Xfce => "xfce",
                process::Environment::Gnome => "ubuntu",
            },
        )
        .env("XDG_SESSION_TYPE", "x11")
        .env(
            "XDG_SESSION_DESKTOP",
            match context.environment {
                process::Environment::Xfce => "xfce",
                process::Environment::Gnome => "ubuntu",
            },
        )
        .env("HOME", &context.home)
        .env("DISPLAY", &context.display)
        .env("XAUTHORITY", &context.xauthority)
        .env("XDG_CONFIG_HOME", context.home.join(".config"))
        .env("XDG_CACHE_HOME", context.home.join(".cache"))
        .env("XDG_DATA_HOME", context.home.join(".local/share"))
        .env("XDG_CONFIG_DIRS", "/etc/xdg")
        .env("XDG_DATA_DIRS", process::MANAGED_XDG_DATA_DIRS)
        .env("XDG_RUNTIME_DIR", &context.runtime_directory)
        .env("DBUS_SESSION_BUS_ADDRESS", &context.session_bus_address)
        .env("AT_SPI_BUS_ADDRESS", &context.atspi_bus_address)
        .env("GTK_A11Y", "atspi")
        .args(["--signal=TERM", "8s"])
        .arg(gtk_launch)
        .arg(entry_id)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(test)]
    let diagnostic_enabled = std::env::var_os("GLASS_DESKTOP_APP_LAUNCH_DIAGNOSTICS").as_deref()
        == Some(std::ffi::OsStr::new("1"));
    #[cfg(test)]
    if diagnostic_enabled {
        let path = context.session_directory.join("app-launch.stderr");
        let mut options = fs::OpenOptions::new();
        options
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        let file = options
            .open(path)
            .map_err(|error| format!("Opening private app-launch diagnostic: {error}"))?;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("Securing private app-launch diagnostic: {error}"))?;
        command.stderr(Stdio::from(file));
    }
    let status = command
        .status()
        .map_err(|error| format!("Starting installed desktop application: {error}"))?;
    if !status.success() {
        #[cfg(test)]
        if diagnostic_enabled {
            return Err(format!(
                "The installed desktop application launcher returned an error or exceeded its 8 second limit (status {status}); bounded launcher stderr: {}",
                read_bounded_launch_stderr(&context.session_directory, &context.home)
            ));
        }
        return Err("The installed desktop application launcher returned an error or exceeded its 8 second limit".into());
    }
    Ok(json!({
        "desktopEntryId": entry_id,
        "launchRequested": true,
        "verified": false,
        "message": "Launch was accepted by the desktop entry service. Call desktop_observe to verify the application window."
    }))
}

#[cfg(test)]
fn read_bounded_launch_stderr(session_directory: &Path, home: &Path) -> String {
    let path = session_directory.join("app-launch.stderr");
    let Ok(file) = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
    else {
        return String::new();
    };
    let mut bytes = Vec::new();
    if file.take(4096).read_to_end(&mut bytes).is_err() {
        return String::new();
    }
    let mut stderr = String::from_utf8_lossy(&bytes).into_owned();
    for (path, placeholder) in [
        (session_directory, "<managed-session>"),
        (home, "<managed-home>"),
    ] {
        if let Some(path) = path.to_str() {
            stderr = stderr.replace(path, placeholder);
        }
    }
    stderr = stderr.replace("unix:path=", "unix:path=<redacted>");
    stderr
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .take(1024)
        .collect()
}

fn valid_entry_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id != "."
        && id != ".."
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn discover(home: &Path) -> Vec<AppEntry> {
    let mut entries = BTreeMap::<String, AppEntry>::new();
    let mut scanned = 0usize;
    for root in application_roots(home) {
        let Ok(root_metadata) = fs::symlink_metadata(&root) else {
            continue;
        };
        if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
            continue;
        }
        let Ok(files) = fs::read_dir(root) else {
            continue;
        };
        for file in files.flatten() {
            if scanned >= MAX_APP_FILES {
                break;
            }
            scanned += 1;
            let path = file.path();
            let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(id) = filename.strip_suffix(".desktop") else {
                continue;
            };
            if !valid_entry_id(id) || entries.contains_key(id) {
                continue;
            }
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.len() > MAX_DESKTOP_FILE_BYTES
            {
                continue;
            }
            let Ok(file) = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(&path)
            else {
                continue;
            };
            let mut contents = String::new();
            if file
                .take(MAX_DESKTOP_FILE_BYTES + 1)
                .read_to_string(&mut contents)
                .is_err()
                || contents.len() as u64 > MAX_DESKTOP_FILE_BYTES
            {
                continue;
            }
            if let Some(name) = parse_desktop_entry(&contents) {
                entries.insert(
                    id.to_owned(),
                    AppEntry {
                        id: id.to_owned(),
                        name,
                    },
                );
            }
        }
        if scanned >= MAX_APP_FILES {
            break;
        }
    }
    entries.into_values().collect()
}

fn application_roots(home: &Path) -> [PathBuf; 4] {
    [
        home.join(".local/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/var/lib/snapd/desktop/applications"),
    ]
}

fn parse_desktop_entry(contents: &str) -> Option<String> {
    if contents.len() > MAX_DESKTOP_FILE_BYTES as usize {
        return None;
    }
    let mut in_entry = false;
    let mut entry_type = None;
    let mut name = None;
    let mut hidden = false;
    let mut no_display = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some(value) = line.strip_prefix("Type=") {
            entry_type = Some(value);
        } else if let Some(value) = line.strip_prefix("Name=") {
            name.get_or_insert_with(|| {
                value
                    .trim()
                    .chars()
                    .filter(|character| !character.is_control())
                    .take(160)
                    .collect()
            });
        } else if let Some(value) = line.strip_prefix("Hidden=") {
            hidden = value.eq_ignore_ascii_case("true");
        } else if let Some(value) = line.strip_prefix("NoDisplay=") {
            no_display = value.eq_ignore_ascii_case("true");
        }
    }
    if entry_type != Some("Application") || hidden || no_display {
        return None;
    }
    name.filter(|name: &String| !name.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::{ffi::OsStrExt, fs::PermissionsExt, process::CommandExt},
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn managed_browser_process_diagnostics(
        binary: &Path,
        profile: &Path,
        expected_environment: &[(&str, String)],
    ) -> Value {
        let Ok(processes) = fs::read_dir("/proc") else {
            return json!({"running":false,"procReadable":false});
        };
        let binary = binary.as_os_str().as_bytes();
        let profile = profile.as_os_str().as_bytes();
        let mut browser_pid = None;
        for entry in processes.flatten().take(8192) {
            if entry.file_name().to_string_lossy().parse::<u32>().is_err() {
                continue;
            }
            let Ok(command_line) = fs::read(entry.path().join("cmdline")) else {
                continue;
            };
            let contains = |needle: &[u8]| {
                !needle.is_empty()
                    && command_line
                        .windows(needle.len())
                        .any(|part| part == needle)
            };
            if !contains(binary) || !contains(profile) || contains(b"--type=") {
                continue;
            }
            browser_pid = entry.file_name().to_string_lossy().parse::<u32>().ok();
            break;
        }
        let Some(mut current_pid) = browser_pid else {
            return json!({"running":false,"processChain":[]});
        };
        let mut process_chain = Vec::new();
        for _ in 0..6 {
            let directory = Path::new("/proc").join(current_pid.to_string());
            let comm = fs::read_to_string(directory.join("comm"))
                .unwrap_or_default()
                .trim()
                .to_owned();
            let executable = fs::read_link(directory.join("exe")).ok().and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            });
            let status = fs::read_to_string(directory.join("stat")).unwrap_or_default();
            let stat_fields = status
                .rsplit_once(')')
                .map(|(_, fields)| fields.split_whitespace().collect::<Vec<_>>())
                .unwrap_or_default();
            let parent_pid = stat_fields
                .get(1)
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or_default();
            let process_group = stat_fields
                .get(2)
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or_default();
            let environment = fs::read(directory.join("environ")).ok();
            let wait_channel = fs::read_to_string(directory.join("wchan"))
                .ok()
                .map(|value| value.trim().chars().take(80).collect::<String>());
            let (present, matches) = if let Some(environment) = environment.as_deref() {
                let results = expected_environment
                    .iter()
                    .map(|(name, expected)| {
                        let mut prefix = name.as_bytes().to_vec();
                        prefix.push(b'=');
                        let actual = environment
                            .split(|byte| *byte == 0)
                            .find_map(|value| value.strip_prefix(prefix.as_slice()));
                        (
                            name.to_string(),
                            actual.is_some(),
                            actual == Some(expected.as_bytes()),
                        )
                    })
                    .collect::<Vec<_>>();
                let present = results
                    .iter()
                    .map(|(name, present, _)| (name.clone(), Value::Bool(*present)))
                    .collect::<serde_json::Map<_, _>>();
                let matches = results
                    .iter()
                    .map(|(name, _, matches)| (name.clone(), Value::Bool(*matches)))
                    .collect::<serde_json::Map<_, _>>();
                (json!(present), json!(matches))
            } else {
                (Value::Null, Value::Null)
            };
            process_chain.push(json!({
                "pid":current_pid,
                "state":stat_fields.first(),
                "comm":comm,
                "executable":executable,
                "ppid":parent_pid,
                "pgrp":process_group,
                "waitChannel":wait_channel,
                "environmentPresent":present,
                "environmentMatches":matches,
            }));
            if parent_pid <= 1 || parent_pid == current_pid {
                break;
            }
            current_pid = parent_pid;
        }
        json!({"running":true,"processChain":process_chain})
    }

    fn direct_browser_exec_diagnostic(
        desktop: &super::super::Desktop,
        session_id: &str,
        binary: &Path,
        profile: &Path,
        home: &Path,
    ) -> Value {
        if fs::create_dir(profile)
            .and_then(|()| fs::set_permissions(profile, fs::Permissions::from_mode(0o700)))
            .is_err()
        {
            return json!({"spawned":false,"reason":"could not create private profile"});
        }
        let (
            environment,
            display,
            xauthority,
            runtime_directory,
            session_bus_address,
            atspi_bus_address,
        ) = {
            let guard = desktop.session.lock().unwrap();
            let Some(session) = guard.as_ref() else {
                return json!({"spawned":false,"reason":"managed session disappeared"});
            };
            let atspi =
                fs::read_to_string(session.directory.join("atspi-bus.address")).unwrap_or_default();
            (
                session.environment,
                session.display.clone(),
                session.directory.join("Xauthority"),
                session.directory.join("runtime"),
                session.session_bus_address.clone(),
                atspi.trim().to_owned(),
            )
        };
        let (current_desktop, desktop_session) = match environment {
            super::super::process::Environment::Xfce => ("XFCE", "xfce"),
            super::super::process::Environment::Gnome => ("ubuntu:GNOME", "ubuntu"),
        };
        let mut child = match Command::new(binary)
            .env_clear()
            .env("PATH", super::super::process::MANAGED_CHILD_PATH)
            .env("XDG_CURRENT_DESKTOP", current_desktop)
            .env("DESKTOP_SESSION", desktop_session)
            .env("XDG_SESSION_TYPE", "x11")
            .env("XDG_SESSION_DESKTOP", desktop_session)
            .env("HOME", home)
            .env("DISPLAY", &display)
            .env("XAUTHORITY", &xauthority)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_CACHE_HOME", home.join(".cache"))
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("XDG_CONFIG_DIRS", "/etc/xdg")
            .env(
                "XDG_DATA_DIRS",
                super::super::process::MANAGED_XDG_DATA_DIRS,
            )
            .env("XDG_RUNTIME_DIR", &runtime_directory)
            .env("DBUS_SESSION_BUS_ADDRESS", &session_bus_address)
            .env("AT_SPI_BUS_ADDRESS", &atspi_bus_address)
            .env("GTK_A11Y", "atspi")
            .env("ACCESSIBILITY_ENABLED", "1")
            .arg(format!("--user-data-dir={}", profile.display()))
            .args([
                "--no-first-run",
                "--no-default-browser-check",
                "--password-store=basic",
                "--force-renderer-accessibility=complete",
                "--disable-gpu",
                "--enable-logging=stderr",
            ])
            .arg(format!(
                "--log-file={}",
                profile.join("chrome.log").display()
            ))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                return json!({"spawned":false,"reason":error.to_string()});
            }
        };
        let process_group = child.id();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        let mut window_observed = false;
        let mut last_observation = Value::Null;
        while std::time::Instant::now() < deadline {
            if let Ok(snapshot) = desktop.observe_accessibility(Some(session_id), None) {
                let chrome_windows = snapshot["windows"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|window| {
                        window["application"]
                            .as_str()
                            .is_some_and(|name| name.to_ascii_lowercase().contains("chrome"))
                    })
                    .take(8)
                    .cloned()
                    .collect::<Vec<_>>();
                last_observation = json!({
                    "applications":snapshot["applications"],
                    "windows":chrome_windows,
                    "focusedWindow":snapshot["focusedWindow"],
                });
                window_observed = last_observation["windows"]
                    .as_array()
                    .is_some_and(|windows| !windows.is_empty());
                if window_observed {
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        // This comparison is test-owned. The fresh process group ensures the
        // isolated Chromium and all of its children are stopped after the
        // observation, without signaling any other desktop or user process.
        unsafe {
            libc::kill(-(process_group as i32), libc::SIGTERM);
        }
        let kill_deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if child.try_wait().ok().flatten().is_some()
                || std::time::Instant::now() >= kill_deadline
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        unsafe {
            libc::kill(-(process_group as i32), libc::SIGKILL);
        }
        let _ = child.wait();
        json!({
            "spawned":true,
            "windowObserved":window_observed,
            "lastObservation":last_observation,
            "chromeLog":read_bounded_chrome_log(profile),
        })
    }

    fn temp_home() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "glass-desktop-apps-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(".local/share/applications")).unwrap();
        root
    }

    #[test]
    fn desktop_entry_listing_excludes_hidden_and_never_exposes_commands() {
        let home = temp_home();
        let apps = home.join(".local/share/applications");
        fs::write(
            apps.join("org.example.Editor.desktop"),
            "[Desktop Entry]\nType=Application\nName=Editor\nExec=secret-command --flag\n",
        )
        .unwrap();
        fs::write(
            apps.join("org.example.Hidden.desktop"),
            "[Desktop Entry]\nType=Application\nName=Hidden\nNoDisplay=true\n",
        )
        .unwrap();

        let entries = list(&home);
        assert!(
            entries["applications"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| {
                    entry["desktopEntryId"] == "org.example.Editor" && entry["name"] == "Editor"
                })
        );
        assert!(
            !entries["applications"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["name"] == "Hidden")
        );
        let rendered = serde_json::to_string(&entries).unwrap();
        assert!(!rendered.contains("secret-command"));
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn desktop_entry_ids_cannot_encode_paths_or_arguments() {
        for invalid in [
            "",
            ".",
            "..",
            "../editor",
            "org.example/editor",
            "editor;id",
            "editor --help",
        ] {
            assert!(!valid_entry_id(invalid), "accepted {invalid:?}");
        }
        assert!(valid_entry_id("org.example.Editor"));
        assert!(valid_entry_id("xfce4-web-browser"));
    }

    #[test]
    fn managed_desktop_launches_only_a_listed_entry_and_observes_its_window() {
        if std::env::var_os("GLASS_DESKTOP_LAUNCH_APP_E2E").as_deref()
            != Some(std::ffi::OsStr::new("1"))
        {
            return;
        }
        let root = crate::config::data_dir().join(format!(
            "desktop-launch-app-e2e-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let workspace = root.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let mut desktop = super::super::Desktop::new(workspace);
        desktop.root = root.join("desktop");
        let started = desktop
            .start_selected("xfce", "disposable")
            .unwrap_or_else(|error| panic!("could not start isolated XFCE: {error}"));
        let session_id = started["sessionId"].as_str().unwrap().to_owned();

        let outcome = (|| {
            let binary = std::env::var_os("GLASS_DESKTOP_BROWSER_BINARY")
                .map(PathBuf::from)
                .ok_or("set GLASS_DESKTOP_BROWSER_BINARY to the managed browser binary")?
                .canonicalize()
                .map_err(|error| format!("resolving managed browser binary: {error}"))?;
            if !binary.is_file() || binary.to_string_lossy().chars().any(char::is_whitespace) {
                return Err("managed browser path must be a file path without spaces".into());
            }
            let home = desktop
                .session
                .lock()
                .unwrap()
                .as_ref()
                .ok_or("managed desktop session disappeared before app setup")?
                .directory
                .join("home");
            let browser_profile = home.join("chromium-profile");
            fs::create_dir(&browser_profile)
                .and_then(|()| {
                    fs::set_permissions(&browser_profile, fs::Permissions::from_mode(0o700))
                })
                .map_err(|error| format!("creating isolated browser profile: {error}"))?;
            write_test_application_entry(&home, &binary, &browser_profile)
                .map_err(|error| format!("writing private managed browser entry: {error}"))?;
            let listed = desktop
                .launch_app(&json!({"action":"list","sessionId":session_id}))
                .map_err(|error| format!("listing managed desktop apps failed: {error}"))?;
            let apps = listed["applications"]
                .as_array()
                .ok_or("managed app list is malformed")?;
            let browser_id = apps
                .iter()
                .find(|app| app["desktopEntryId"] == "glass-managed-browser")
                .and_then(|app| app["desktopEntryId"].as_str())
                .ok_or("managed browser desktop entry was not listed")?
                .to_owned();
            if desktop
                .launch_app(&json!({"action":"launch","sessionId":"stale","desktopEntryId":browser_id.clone()}))
                .is_ok()
            {
                return Err("stale managed session ID was accepted".into());
            }
            desktop
                .set_lease("take-control")
                .map_err(|error| format!("could not take human lease: {error}"))?;
            if desktop
                .launch_app(&json!({"action":"launch","sessionId":session_id,"desktopEntryId":browser_id.clone()}))
                .is_ok()
            {
                return Err("app launch was accepted while the human held the lease".into());
            }
            desktop
                .set_lease("return-to-agent")
                .map_err(|error| format!("could not return agent lease: {error}"))?;
            let launched = desktop
                .launch_app(
                    &json!({"action":"launch","sessionId":session_id,"desktopEntryId":browser_id}),
                )
                .map_err(|error| format!("listed app launch failed: {error}"))?;
            if launched["verified"] != false || launched["launchRequested"] != true {
                return Err(format!("launch result overstated verification: {launched}"));
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            let mut latest = Value::Null;
            let mut browser_window_seen = false;
            while std::time::Instant::now() < deadline {
                if let Ok(snapshot) = desktop.observe_accessibility(Some(&session_id), None) {
                    let diagnostic_nodes = snapshot["nodes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|node| {
                            matches!(node["role"].as_str(), Some("frame" | "window" | "dialog"))
                                || node["application"].as_str().is_some_and(|application| {
                                    application.to_ascii_lowercase().contains("chrome")
                                })
                        })
                        .take(20)
                        .cloned()
                        .collect::<Vec<_>>();
                    latest = json!({
                        "applications": snapshot["applications"],
                        "focusedWindow": snapshot["focusedWindow"],
                        "diagnosticNodes": diagnostic_nodes
                    });
                    if diagnostic_nodes.iter().any(|node| {
                        matches!(node["role"].as_str(), Some("frame" | "window"))
                            && node["application"].as_str().is_some_and(|application| {
                                application.to_ascii_lowercase().contains("chrome")
                            })
                    }) {
                        browser_window_seen = true;
                        break;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            if !browser_window_seen {
                let screenshot_path = std::env::temp_dir().join(format!(
                    "glass-browser-launch-diagnostic-{}.jpg",
                    std::process::id()
                ));
                let screenshot_saved = desktop
                    .frame_after(0, std::time::Duration::from_secs(2))
                    .ok()
                    .flatten()
                    .and_then(|frame| {
                        fs::write(&screenshot_path, frame.2)
                            .ok()
                            .and_then(|()| {
                                fs::set_permissions(
                                    &screenshot_path,
                                    fs::Permissions::from_mode(0o600),
                                )
                                .ok()
                            })
                            .map(|()| screenshot_path.clone())
                    });
                let expected_environment = {
                    let guard = desktop.session.lock().unwrap();
                    let session = guard.as_ref().unwrap();
                    let atspi_bus = fs::read_to_string(session.directory.join("atspi-bus.address"))
                        .unwrap_or_default();
                    vec![
                        ("DISPLAY", session.display.clone()),
                        (
                            "XAUTHORITY",
                            session.directory.join("Xauthority").display().to_string(),
                        ),
                        (
                            "XDG_RUNTIME_DIR",
                            session.directory.join("runtime").display().to_string(),
                        ),
                        (
                            "DBUS_SESSION_BUS_ADDRESS",
                            session.session_bus_address.clone(),
                        ),
                        ("AT_SPI_BUS_ADDRESS", atspi_bus.trim().to_owned()),
                        ("GTK_A11Y", "atspi".to_owned()),
                    ]
                };
                let browser_process = managed_browser_process_diagnostics(
                    &binary,
                    &browser_profile,
                    &expected_environment,
                );
                let launch_stderr = if std::env::var_os("GLASS_DESKTOP_APP_LAUNCH_DIAGNOSTICS")
                    .as_deref()
                    == Some(std::ffi::OsStr::new("1"))
                {
                    super::read_bounded_launch_stderr(
                        &desktop.session.lock().unwrap().as_ref().unwrap().directory,
                        &home,
                    )
                } else {
                    String::new()
                };
                let direct_exec = direct_browser_exec_diagnostic(
                    &desktop,
                    &session_id,
                    &binary,
                    &home.join("chromium-direct-profile"),
                    &home,
                );
                return Err(format!(
                    "managed browser launch was accepted but no browser window appeared within 30 seconds: {}",
                    json!({"browserProcess":browser_process,"lastObservation":latest,"launchStderr":launch_stderr,"chromeLog":read_bounded_chrome_log(&browser_profile),"screenshot":screenshot_saved,"directExec":direct_exec})
                ));
            }
            let (bus_address, revision) = {
                let guard = desktop.session.lock().unwrap();
                let session = guard.as_ref().unwrap();
                (
                    session.session_bus_address.clone(),
                    session.accessibility_revision,
                )
            };
            let application_root_names =
                super::super::accessibility::observe(&bus_address, &session_id, revision, None)
                    .map(|snapshot| snapshot.application_root_names)
                    .unwrap_or_default();
            latest["applicationRootNames"] = json!(application_root_names);
            if std::env::var_os("GLASS_DESKTOP_PUBLIC_AX_DIAGNOSTIC").as_deref()
                == Some(std::ffi::OsStr::new("1"))
            {
                return run_public_page_ax_diagnostic(&desktop, &session_id);
            }
            let home_page = root.join("browser-home.html");
            let chapter_page = root.join("browser-chapter.html");
            let detail_page = root.join("browser-detail.html");
            fs::write(
                &home_page,
                "<!doctype html><html><head><title>Managed browser home fixture</title></head><body><h1>Home page</h1><p>Visible introduction for the current viewport.</p><div style=\"height:5000px\"></div><a href=\"browser-chapter.html\">Observed chapter link</a></body></html>",
            )
            .map_err(|error| format!("writing isolated browser home fixture: {error}"))?;
            fs::write(
                &chapter_page,
                "<!doctype html><html><head><title>Managed browser chapter fixture</title></head><body><h1>Chapter page</h1><p>Second-level content.</p><a href=\"browser-detail.html\">Observed detail link</a></body></html>",
            )
            .map_err(|error| format!("writing isolated browser chapter fixture: {error}"))?;
            fs::write(
                &detail_page,
                "<!doctype html><html><head><title>Managed browser detail fixture</title></head><body><h1>Detail page</h1><a href=\"#section\">Observed section link</a><h2 id=\"section\">Detail section</h2></body></html>",
            )
            .map_err(|error| format!("writing isolated browser detail fixture: {error}"))?;
            let read_expected_address = |expected: &str| -> Result<String, String> {
                let observation = desktop
                    .observe_accessibility(Some(&session_id), Some("Chrome address"))
                    .map_err(|error| format!("observing browser address: {error}"))?;
                if observation["truncated"] != false || observation["queryAmbiguous"] != false {
                    return Err(format!(
                        "browser address observation is incomplete: {observation}"
                    ));
                }
                unique_browser_address_value(&observation)
                    .filter(|value| value.contains(expected))
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| {
                        format!("browser address was not unique or did not contain {expected:?}: {observation}")
                    })
            };
            let fixture_url = url::Url::from_file_path(&home_page)
                .map_err(|_| "could not form local browser fixture URL")?
                .to_string();
            desktop
                .input(
                    super::super::Lease::Agent,
                    &json!({"action":"key-chord","keys":["Control","L"]}),
                )
                .map_err(|error| format!("focusing Chrome's address bar: {}", error.message()))?;
            desktop
                .input(
                    super::super::Lease::Agent,
                    &json!({"action":"text","text":fixture_url}),
                )
                .map_err(|error| format!("entering the local fixture URL: {}", error.message()))?;
            desktop
                .input(
                    super::super::Lease::Agent,
                    &json!({"action":"key-tap","key":"Enter"}),
                )
                .map_err(|error| {
                    format!(
                        "opening the observed local browser URL: {}",
                        error.message()
                    )
                })?;
            let home_address_deadline =
                std::time::Instant::now() + std::time::Duration::from_secs(15);
            let home_address = loop {
                match read_expected_address("browser-home.html") {
                    Ok(address) => break address,
                    Err(error) if std::time::Instant::now() < home_address_deadline => {
                        let _ = error;
                        std::thread::sleep(std::time::Duration::from_millis(150));
                    }
                    Err(error) => return Err(error),
                }
            };
            let page_deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            let mut page_diagnostics = Value::Null;
            let mut home_link = None;
            let initial_search = desktop
                .observe_accessibility(Some(&session_id), Some("Chrome Observed chapter link link"))
                .map_err(|error| format!("searching for the below-fold home link: {error}"))?;
            let initial_matches = initial_search["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|node| node["role"] == "link" && node["name"] == "Observed chapter link")
                .collect::<Vec<_>>();
            if initial_matches.len() != 1
                || initial_search["truncated"] != false
                || initial_search["queryAmbiguous"] != false
            {
                return Err(format!(
                    "below-fold link was not a unique complete search result: {initial_search}"
                ));
            }
            let candidate = initial_matches[0];
            if candidate["onscreen"] != false
                || !candidate["targetId"].is_null()
                || !candidate["pointerTarget"].is_null()
            {
                return Err(format!(
                    "below-fold link received an action token before scrolling: {candidate}"
                ));
            }
            while std::time::Instant::now() < page_deadline {
                if let Ok(observation) = desktop.observe_accessibility(
                    Some(&session_id),
                    Some("Chrome Observed chapter link link"),
                ) {
                    let nodes = observation["nodes"].as_array().cloned().unwrap_or_default();
                    page_diagnostics = json!({
                        "focusedWindow": observation["focusedWindow"],
                        "nodes": nodes.iter().take(8).cloned().collect::<Vec<_>>(),
                        "nodesVisited": observation["nodesVisited"],
                        "truncated": observation["truncated"],
                        "queryAmbiguous": observation["queryAmbiguous"]
                    });
                    let matches = nodes
                        .iter()
                        .filter(|node| {
                            node["role"] == "link"
                                && node["name"] == "Observed chapter link"
                                && node["targetId"].is_string()
                                && node["onscreen"] == true
                                && node["pointerTarget"].is_object()
                        })
                        .collect::<Vec<_>>();
                    if matches.len() == 1
                        && observation["queryAmbiguous"] == false
                        && observation["truncated"] == false
                    {
                        home_link = Some((observation, matches[0].clone()));
                        break;
                    }
                }
                desktop
                    .input(
                        super::super::Lease::Agent,
                        &json!({"action":"scroll","deltaY":800.0,"x":0.5,"y":0.5}),
                    )
                    .map_err(|error| {
                        format!(
                            "scrolling to the observed below-fold link: {}",
                            error.message()
                        )
                    })?;
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            let Some((home_observation, chapter_link)) = home_link else {
                return Err(format!(
                    "browser home link was not uniquely actionable: {page_diagnostics}; launch observation: {latest}"
                ));
            };
            let click_observed_link =
                |observation: &Value, link: &Value| -> Result<Value, String> {
                    match desktop.act_accessibility(&json!({
                        "sessionId": session_id,
                        "revision": observation["revision"],
                        "targetId": link["targetId"],
                        "action": "click",
                    })) {
                        Ok(result) => Ok(result),
                        Err(error)
                            if error.contains("no recognized accessibility click action")
                                || error.contains("no accessibility action") =>
                        {
                            let pointer = &link["pointerTarget"];
                            let x = pointer["x"].as_f64().ok_or_else(|| {
                                "fresh observed link has no pointer x coordinate".to_owned()
                            })?;
                            let y = pointer["y"].as_f64().ok_or_else(|| {
                                "fresh observed link has no pointer y coordinate".to_owned()
                            })?;
                            desktop
                            .input(
                                super::super::Lease::Agent,
                                &json!({"action":"pointer-click","button":"left","x":x,"y":y}),
                            )
                            .map(|_| json!({"verified":false,"action":"fresh pointerTarget click"}))
                            .map_err(|input_error| {
                                format!(
                                    "fresh pointerTarget click failed after {error}: {}",
                                    input_error.message()
                                )
                            })
                        }
                        Err(error) => Err(error),
                    }
                };
            let clicked_chapter = click_observed_link(&home_observation, &chapter_link);
            let (home_observation, clicked_chapter) = match clicked_chapter {
                Ok(clicked) => (home_observation, clicked),
                Err(error)
                    if error.contains("stale")
                        || error.contains("identity changed")
                        || error.contains("window changed") =>
                {
                    let refreshed = desktop
                        .observe_accessibility(
                            Some(&session_id),
                            Some("Chrome Observed chapter link link"),
                        )
                        .map_err(|refresh_error| {
                            format!("stale-link recovery observation failed: {refresh_error}")
                        })?;
                    let refreshed_links = refreshed["nodes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|node| {
                            node["role"] == "link"
                                && node["name"] == "Observed chapter link"
                                && node["targetId"].is_string()
                        })
                        .collect::<Vec<_>>();
                    if refreshed["truncated"] != false
                        || refreshed["queryAmbiguous"] != false
                        || refreshed_links.len() != 1
                    {
                        return Err(format!(
                            "stale-link recovery did not produce one complete target: {refreshed}"
                        ));
                    }
                    let refreshed_link = refreshed_links[0].clone();
                    let clicked = click_observed_link(&refreshed, &refreshed_link).map_err(
                        |retry_error| {
                            format!("clicking freshly observed chapter link failed: {retry_error}")
                        },
                    )?;
                    (refreshed, clicked)
                }
                Err(error) => return Err(format!("clicking observed chapter link: {error}")),
            };
            if clicked_chapter["verified"] != false {
                return Err(format!(
                    "semantic click overstated navigation verification: {clicked_chapter}"
                ));
            }
            let chapter_address_deadline =
                std::time::Instant::now() + std::time::Duration::from_secs(15);
            let chapter_address = loop {
                match read_expected_address("browser-chapter.html") {
                    Ok(address) => break address,
                    Err(error) if std::time::Instant::now() < chapter_address_deadline => {
                        let _ = error;
                        std::thread::sleep(std::time::Duration::from_millis(150));
                    }
                    Err(error) => return Err(error),
                }
            };
            let chapter_deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            let mut chapter_observation = Value::Null;
            let mut detail_link = None;
            while std::time::Instant::now() < chapter_deadline {
                if let Ok(observation) = desktop.observe_accessibility(
                    Some(&session_id),
                    Some("Chrome Observed detail link link"),
                ) {
                    let nodes = observation["nodes"].as_array().cloned().unwrap_or_default();
                    chapter_observation = json!({
                        "focusedWindow": observation["focusedWindow"],
                        "nodes": nodes.iter().take(8).cloned().collect::<Vec<_>>(),
                        "nodesVisited": observation["nodesVisited"],
                        "truncated": observation["truncated"],
                        "queryAmbiguous": observation["queryAmbiguous"]
                    });
                    let matches = nodes
                        .iter()
                        .filter(|node| {
                            node["role"] == "link"
                                && node["name"] == "Observed detail link"
                                && node["targetId"].is_string()
                        })
                        .collect::<Vec<_>>();
                    if matches.len() == 1
                        && observation["queryAmbiguous"] == false
                        && observation["truncated"] == false
                        && observation["focusedWindow"]["name"]
                            .as_str()
                            .is_some_and(|name| name.contains("Managed browser chapter fixture"))
                    {
                        detail_link = Some((observation, matches[0].clone()));
                        break;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            let Some((_chapter_observation, detail_link)) = detail_link else {
                return Err(format!(
                    "browser chapter link was not uniquely actionable: {chapter_observation}"
                ));
            };
            let detail_pointer = &detail_link["pointerTarget"];
            let pointer_x = detail_pointer["x"]
                .as_f64()
                .ok_or("fresh observed detail link has no pointer x coordinate")?;
            let pointer_y = detail_pointer["y"]
                .as_f64()
                .ok_or("fresh observed detail link has no pointer y coordinate")?;
            desktop
                .input(
                    super::super::Lease::Agent,
                    &json!({"action":"pointer-click","button":"middle","x":pointer_x,"y":pointer_y}),
                )
                .map_err(|error| {
                    format!(
                        "opening fresh observed detail link in a new tab by middle click: {}",
                        error.message()
                    )
                })?;
            let all_tabs = desktop
                .observe_accessibility(Some(&session_id), Some("Chrome tab"))
                .map_err(|error| format!("observing both browser tabs failed: {error}"))?;
            let tab_names = all_tabs["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|node| node["role"] == "page tab")
                .filter_map(|node| node["name"].as_str().map(ToOwned::to_owned))
                .collect::<Vec<_>>();
            if !tab_names
                .iter()
                .any(|name| name == "Managed browser chapter fixture")
                || !tab_names
                    .iter()
                    .any(|name| name == "Managed browser detail fixture")
            {
                return Err(format!(
                    "both distinct browser tabs were not observed: {all_tabs}"
                ));
            }
            let detail_tab = desktop
                .observe_accessibility(
                    Some(&session_id),
                    Some("Chrome Managed browser detail fixture tab"),
                )
                .map_err(|error| format!("uniquely observing detail tab failed: {error}"))?;
            let detail_tab_target = detail_tab["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|node| {
                    node["role"] == "page tab"
                        && node["name"] == "Managed browser detail fixture"
                        && node["targetId"].is_string()
                })
                .cloned()
                .ok_or_else(|| {
                    format!("detail tab was not a unique actionable result: {detail_tab}")
                })?;
            if detail_tab["truncated"] != false || detail_tab["queryAmbiguous"] != false {
                return Err(format!(
                    "detail tab search was incomplete or ambiguous: {detail_tab}"
                ));
            }
            let tab_pointer = &detail_tab_target["pointerTarget"];
            let tab_x = tab_pointer["x"]
                .as_f64()
                .ok_or("fresh detail tab has no pointer x coordinate")?;
            let tab_y = tab_pointer["y"]
                .as_f64()
                .ok_or("fresh detail tab has no pointer y coordinate")?;
            desktop
                .input(
                    super::super::Lease::Agent,
                    &json!({"action":"pointer-click","button":"left","x":tab_x,"y":tab_y}),
                )
                .map_err(|error| {
                    format!(
                        "selecting the observed detail tab failed: {}",
                        error.message()
                    )
                })?;
            let detail_address_deadline =
                std::time::Instant::now() + std::time::Duration::from_secs(15);
            let detail_address = loop {
                match read_expected_address("browser-detail.html") {
                    Ok(address) => break address,
                    Err(error) if std::time::Instant::now() < detail_address_deadline => {
                        let _ = error;
                        std::thread::sleep(std::time::Duration::from_millis(150));
                    }
                    Err(error) => return Err(error),
                }
            };
            let detail_deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            let mut detail_observation = Value::Null;
            while std::time::Instant::now() < detail_deadline {
                if let Ok(observation) = desktop.observe_accessibility(
                    Some(&session_id),
                    Some("Chrome Observed section link link"),
                ) {
                    detail_observation = observation;
                    if detail_observation["focusedWindow"]["name"]
                        .as_str()
                        .is_some_and(|name| name.contains("Managed browser detail fixture"))
                    {
                        break;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            if !detail_observation["focusedWindow"]["name"]
                .as_str()
                .is_some_and(|name| name.contains("Managed browser detail fixture"))
            {
                let tab_observation = all_tabs;
                let screenshot = std::env::temp_dir()
                    .join(format!("glass-browser-tabs-e2e-{}.jpg", std::process::id()));
                if let Ok(Some((_, _, jpeg))) =
                    desktop.frame_after(0, std::time::Duration::from_secs(1))
                {
                    let mut options = fs::OpenOptions::new();
                    options
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .mode(0o600)
                        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
                    if let Ok(mut file) = options.open(&screenshot) {
                        let _ = file.write_all(&jpeg);
                    }
                }
                return Err(format!(
                    "second tab did not reach the detail page: {detail_observation}; tab observation: {tab_observation}; screenshot: {}",
                    screenshot.display()
                ));
            }
            let section_link = detail_observation["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|node| {
                    node["role"] == "link"
                        && node["name"] == "Observed section link"
                        && node["targetId"].is_string()
                })
                .cloned()
                .ok_or_else(|| {
                    format!("detail section link was not actionable: {detail_observation}")
                })?;
            click_observed_link(&detail_observation, &section_link)
                .map_err(|error| format!("clicking observed section link: {error}"))?;
            let section_deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
            let mut section_address = String::new();
            let mut section_observed = false;
            let mut section_snapshot = Value::Null;
            while std::time::Instant::now() < section_deadline {
                section_snapshot = desktop
                    .observe_accessibility(Some(&session_id), Some("Chrome Detail section heading"))
                    .unwrap_or(Value::Null);
                let address_snapshot = desktop
                    .observe_accessibility(Some(&session_id), Some("Chrome address"))
                    .ok();
                let current_address = address_snapshot
                    .as_ref()
                    .and_then(unique_browser_address_value);
                if current_address
                    .is_some_and(|address| address.contains("browser-detail.html#section"))
                    && section_snapshot["truncated"] == false
                    && section_snapshot["queryAmbiguous"] == false
                    && section_snapshot["nodes"].as_array().is_some_and(|nodes| {
                        nodes.iter().any(|node| {
                            node["role"] == "heading"
                                && node["name"] == "Detail section"
                                && node["states"].as_array().is_some_and(|states| {
                                    states.iter().any(|state| state == "visible")
                                        && states.iter().any(|state| state == "showing")
                                })
                        })
                    })
                {
                    section_address = current_address.unwrap().to_owned();
                    section_observed = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            if !section_observed {
                return Err(format!(
                    "observed section link did not show the expected fragment and heading: {}",
                    json!({
                        "focusedWindow":section_snapshot["focusedWindow"],
                        "nodesVisited":section_snapshot["nodesVisited"],
                        "truncated":section_snapshot["truncated"],
                        "queryAmbiguous":section_snapshot["queryAmbiguous"],
                        "sectionNodes":section_snapshot["nodes"],
                    })
                ));
            }
            desktop
                .input(
                    super::super::Lease::Agent,
                    &json!({"action":"key-chord","keys":["Alt","ArrowLeft"]}),
                )
                .map_err(|error| format!("browser Back input failed: {}", error.message()))?;
            let back_deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
            let mut back_verified = false;
            let mut back_observation = Value::Null;
            while std::time::Instant::now() < back_deadline {
                let observation = desktop
                    .observe_accessibility(Some(&session_id), Some("Chrome address"))
                    .unwrap_or(Value::Null);
                let address = unique_browser_address_value(&observation);
                let title_matches = observation["focusedWindow"]["name"]
                    .as_str()
                    .is_some_and(|name| name.contains("Managed browser detail fixture"));
                if title_matches
                    && address.is_some_and(|address| {
                        address.contains("browser-detail.html") && !address.contains("#section")
                    })
                {
                    back_observation = observation;
                    back_verified = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            if !back_verified {
                let diagnostic = desktop
                    .observe_accessibility(Some(&session_id), Some("Chrome address"))
                    .ok();
                let address = diagnostic
                    .as_ref()
                    .and_then(|snapshot| snapshot["nodes"].as_array())
                    .into_iter()
                    .flatten()
                    .find_map(|node| node["value"].as_str());
                return Err(format!(
                    "browser Back did not restore the detail page: {}",
                    json!({
                        "focusedWindow":diagnostic.as_ref().map(|snapshot| &snapshot["focusedWindow"]),
                        "windows":diagnostic.as_ref().map(|snapshot| &snapshot["windows"]),
                        "address":address,
                        "backObservation":back_observation,
                        "nodesVisited":diagnostic.as_ref().map(|snapshot| &snapshot["nodesVisited"]),
                        "truncated":diagnostic.as_ref().map(|snapshot| &snapshot["truncated"]),
                    })
                ));
            }
            let back_address = read_expected_address("browser-detail.html")?;
            desktop
                .input(
                    super::super::Lease::Agent,
                    &json!({"action":"key-chord","keys":["Alt","ArrowRight"]}),
                )
                .map_err(|error| format!("browser Forward input failed: {}", error.message()))?;
            let forward_deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
            let mut forward_verified = false;
            let mut forward_observation = Value::Null;
            let mut forward_section = Value::Null;
            while std::time::Instant::now() < forward_deadline {
                let observation = desktop
                    .observe_accessibility(Some(&session_id), Some("Chrome address"))
                    .unwrap_or(Value::Null);
                let address = unique_browser_address_value(&observation);
                let title_matches = observation["focusedWindow"]["name"]
                    .as_str()
                    .is_some_and(|name| name.contains("Managed browser detail fixture"));
                let section = desktop
                    .observe_accessibility(Some(&session_id), Some("Chrome Detail section heading"))
                    .unwrap_or(Value::Null);
                let heading_matches = section["truncated"] == false
                    && section["queryAmbiguous"] == false
                    && section["nodes"].as_array().is_some_and(|nodes| {
                        nodes.iter().any(|node| {
                            node["role"] == "heading"
                                && node["name"] == "Detail section"
                                && node["states"].as_array().is_some_and(|states| {
                                    states.iter().any(|state| state == "visible")
                                        && states.iter().any(|state| state == "showing")
                                })
                        })
                    });
                if title_matches
                    && address
                        .is_some_and(|address| address.contains("browser-detail.html#section"))
                    && heading_matches
                {
                    forward_observation = observation;
                    forward_section = section;
                    forward_verified = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            if !forward_verified {
                let diagnostic = desktop
                    .observe_accessibility(Some(&session_id), Some("Chrome address"))
                    .ok();
                let address = diagnostic
                    .as_ref()
                    .and_then(|snapshot| snapshot["nodes"].as_array())
                    .into_iter()
                    .flatten()
                    .find_map(|node| node["value"].as_str());
                return Err(format!(
                    "browser Forward did not restore the section state: {}",
                    json!({
                        "focusedWindow":diagnostic.as_ref().map(|snapshot| &snapshot["focusedWindow"]),
                        "windows":diagnostic.as_ref().map(|snapshot| &snapshot["windows"]),
                        "address":address,
                        "forwardObservation":forward_observation,
                        "sectionNodes":forward_section["nodes"],
                        "nodesVisited":diagnostic.as_ref().map(|snapshot| &snapshot["nodesVisited"]),
                        "truncated":diagnostic.as_ref().map(|snapshot| &snapshot["truncated"]),
                    })
                ));
            }
            let forward_address = read_expected_address("#section")?;
            let original_tab = desktop
                .observe_accessibility(
                    Some(&session_id),
                    Some("Chrome Managed browser chapter fixture tab"),
                )
                .map_err(|error| format!("uniquely observing original tab failed: {error}"))?;
            let original_tab_target = original_tab["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|node| {
                    node["role"] == "page tab"
                        && node["name"] == "Managed browser chapter fixture"
                        && node["targetId"].is_string()
                })
                .cloned()
                .ok_or_else(|| {
                    format!("original chapter tab was not actionable: {original_tab}")
                })?;
            if original_tab["truncated"] != false || original_tab["queryAmbiguous"] != false {
                return Err(format!(
                    "original tab search was incomplete or ambiguous: {original_tab}"
                ));
            }
            let original_pointer = &original_tab_target["pointerTarget"];
            let original_x = original_pointer["x"]
                .as_f64()
                .ok_or("fresh original tab has no pointer x coordinate")?;
            let original_y = original_pointer["y"]
                .as_f64()
                .ok_or("fresh original tab has no pointer y coordinate")?;
            desktop
                .input(
                    super::super::Lease::Agent,
                    &json!({"action":"pointer-click","button":"left","x":original_x,"y":original_y}),
                )
                .map_err(|error| format!("selecting the observed original tab failed: {}", error.message()))?;
            let original_tab_deadline =
                std::time::Instant::now() + std::time::Duration::from_secs(8);
            let mut original_tab_preserved = false;
            while std::time::Instant::now() < original_tab_deadline {
                if let Ok(observation) = desktop.observe_accessibility(Some(&session_id), None)
                    && observation["focusedWindow"]["name"]
                        .as_str()
                        .is_some_and(|name| name.contains("Managed browser chapter fixture"))
                {
                    original_tab_preserved = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            if !original_tab_preserved {
                return Err("switching tabs did not preserve the chapter page in its tab".into());
            }
            let original_tab_address = read_expected_address("browser-chapter.html")?;
            Ok(json!({
                "desktopEntryId": browser_id,
                "browserWindowObserved": true,
                "homeToChapterObservedLink": true,
                "chapterToDetailObservedLinkOpenedNewTab": true,
                "sectionObservedLinkClicked": true,
                "backAndForwardVerifiedByTitle": true,
                "originalTabPreserved": true,
                "deepNavigationVerified": true,
                "firstPageTitle": home_observation["focusedWindow"]["name"],
                "secondPageTitle": detail_observation["focusedWindow"]["name"],
                "tabNames": tab_names,
                "addresses": {
                    "home": home_address,
                    "chapter": chapter_address,
                    "detail": detail_address,
                    "section": section_address,
                    "back": back_address,
                    "forward": forward_address,
                    "originalTabAfterSwitch": original_tab_address
                },
                "nodesVisited": detail_observation["nodesVisited"],
            }))
        })();
        let stopped = desktop.stop();
        let _ = fs::remove_dir_all(&root);
        stopped.unwrap_or_else(|error| panic!("isolated desktop cleanup failed: {error}"));
        let report =
            outcome.unwrap_or_else(|error| panic!("managed desktop launch E2E failed: {error}"));
        if std::env::var_os("GLASS_DESKTOP_APP_LAUNCH_DIAGNOSTICS").as_deref()
            == Some(std::ffi::OsStr::new("1"))
        {
            eprintln!("managed app launch acceptance: {report}");
        }
        assert_eq!(report["browserWindowObserved"], true);
        if std::env::var_os("GLASS_DESKTOP_PUBLIC_AX_DIAGNOSTIC").as_deref()
            != Some(std::ffi::OsStr::new("1"))
        {
            assert_eq!(
                report["deepNavigationVerified"], true,
                "navigation was not verified: {report}"
            );
            assert_eq!(
                report["originalTabPreserved"], true,
                "original tab was not preserved: {report}"
            );
        }
    }

    fn run_public_page_ax_diagnostic(
        desktop: &super::super::Desktop,
        session_id: &str,
    ) -> Result<Value, String> {
        desktop
            .input(
                super::super::Lease::Agent,
                &json!({"action":"key-chord","keys":["Control","L"]}),
            )
            .map_err(|error| format!("focusing the browser address: {}", error.message()))?;
        std::thread::sleep(std::time::Duration::from_millis(200));
        desktop
            .input(
                super::super::Lease::Agent,
                &json!({"action":"text","text":"https://doc.rust-lang.org/book/"}),
            )
            .map_err(|error| format!("entering the public URL: {}", error.message()))?;
        std::thread::sleep(std::time::Duration::from_millis(300));
        let address_observation = desktop
            .observe_accessibility(Some(session_id), Some("Chrome address"))
            .map_err(|error| format!("checking the browser address field: {error}"))?;
        let address_value = address_observation["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|node| node["value"].as_str().map(ToOwned::to_owned));
        if !address_value
            .as_deref()
            .is_some_and(|value| value.contains("doc.rust-lang.org/book/"))
        {
            return Err(format!(
                "the managed browser address field did not contain the requested public URL: {}",
                json!({"snapshot":address_observation,"addressValue":address_value})
            ));
        }
        if address_observation["focusedWindow"]["windowId"]
            .as_str()
            .is_none()
        {
            return Err("initial browser observation did not expose a visible windowId".into());
        }
        desktop
            .input(
                super::super::Lease::Agent,
                &json!({"action":"key-tap","key":"Enter"}),
            )
            .map_err(|error| format!("opening the public URL: {}", error.message()))?;
        std::thread::sleep(std::time::Duration::from_millis(300));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut browser_title = String::new();
        while std::time::Instant::now() < deadline {
            if let Ok(snapshot) = desktop.observe_accessibility(Some(session_id), Some("Chrome"))
                && let Some(title) = snapshot["focusedWindow"]["name"].as_str()
            {
                browser_title = title.to_owned();
                if title
                    .to_ascii_lowercase()
                    .contains("the rust programming language")
                {
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let mut browser_window_id = select_rust_book_window(
            desktop,
            session_id,
            "Chrome The Rust Programming Language",
            "rust programming language",
        )?;
        let toggle_query = "Chrome Toggle Table of Contents label";
        let toggle_snapshot = desktop
            .observe_accessibility_scoped(
                Some(session_id),
                Some(toggle_query),
                Some(&browser_window_id),
            )
            .map_err(|error| format!("finding the visible table-of-contents toggle: {error}"))?;
        let toggle = toggle_snapshot["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|node| {
                node["pointerFallback"] == true
                    && node["onscreen"] == true
                    && node["targetId"].is_null()
                    && node["pointerTarget"].is_object()
            })
            .cloned()
            .ok_or_else(|| {
                format!(
                    "the Rust Book table-of-contents toggle did not yield a unique fresh screen target: {}",
                    json!({
                        "windowId":browser_window_id,
                        "focusedWindow":toggle_snapshot["focusedWindow"],
                        "windows":toggle_snapshot["windows"],
                        "truncated":toggle_snapshot["truncated"],
                        "queryAmbiguous":toggle_snapshot["queryAmbiguous"],
                        "ownerWindowScoped":toggle_snapshot["_testOwnerWindowScoped"],
                        "nodes":toggle_snapshot["nodes"]
                    })
                )
            })?;
        if toggle_snapshot["truncated"] == true || toggle_snapshot["queryAmbiguous"] == true {
            return Err("the table-of-contents toggle search was incomplete or ambiguous".into());
        }
        let pointer = &toggle["pointerTarget"];
        desktop
            .input(
                super::super::Lease::Agent,
                &json!({
                    "action":"pointer-click",
                    "button":"left",
                    "x":pointer["x"],
                    "y":pointer["y"]
                }),
            )
            .map_err(|error| {
                format!(
                    "clicking the observed table-of-contents toggle: {}",
                    error.message()
                )
            })?;
        std::thread::sleep(std::time::Duration::from_millis(250));
        let ownership_query = "Chrome Understanding Ownership link";
        let ownership = desktop
            .observe_accessibility_scoped(
                Some(session_id),
                Some(ownership_query),
                Some(&browser_window_id),
            )
            .map_err(|error| format!("searching the opened table of contents: {error}"))?;
        let ownership_matches = ownership["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|node| {
                node["role"] == "link"
                    && node["name"]
                        .as_str()
                        .is_some_and(|name| name.trim() == "Understanding Ownership")
            })
            .map(|node| {
                json!({
                    "name":node["name"],
                    "owningWindow":node["owningWindow"],
                    "onscreen":node["onscreen"],
                    "hasTarget":node["targetId"].is_string(),
                    "hasPointer":node["pointerTarget"].is_object(),
                })
            })
            .collect::<Vec<_>>();
        let ownership_link = ownership["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|node| {
                node["role"] == "link"
                    && node["name"]
                        .as_str()
                        .is_some_and(|name| name.trim() == "Understanding Ownership")
                    && node["onscreen"] == true
                    && node["targetId"].is_string()
            })
            .cloned();
        let mut ownership_navigation = Value::Null;
        if ownership["truncated"] == false
            && ownership["queryAmbiguous"] == false
            && let Some(link) = ownership_link.as_ref()
        {
            let semantic_result = desktop.act_accessibility(&json!({
                "sessionId":session_id,
                "revision":ownership["revision"],
                "targetId":link["targetId"],
                "action":"click"
            }));
            match semantic_result {
                Ok(result) => ownership_navigation = result,
                Err(error) if error.contains("no recognized accessibility click action") => {
                    let fresh = desktop
                        .observe_accessibility_scoped(
                            Some(session_id),
                            Some(ownership_query),
                            Some(&browser_window_id),
                        )
                        .map_err(|error| {
                            format!("refreshing the unsupported Ownership link: {error}")
                        })?;
                    let fresh_link = fresh["nodes"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .find(|node| {
                                node["role"] == "link"
                                    && node["name"].as_str().is_some_and(|name| {
                                        name.trim() == "Understanding Ownership"
                                    })
                                    && node["onscreen"] == true
                                    && node["pointerTarget"].is_object()
                            })
                            .cloned()
                            .ok_or_else(|| {
                                format!(
                                    "fresh Ownership link search did not return a unique visible pointer target: {}",
                                    json!({
                                        "truncated":fresh["truncated"],
                                        "queryAmbiguous":fresh["queryAmbiguous"],
                                        "nodes":fresh["nodes"]
                                    })
                                )
                            })?;
                    if fresh["truncated"] == true || fresh["queryAmbiguous"] == true {
                        return Err(
                            "fresh Ownership link search was incomplete or ambiguous".into()
                        );
                    }
                    let pointer = &fresh_link["pointerTarget"];
                    desktop
                        .input(
                            super::super::Lease::Agent,
                            &json!({
                                "action":"pointer-click",
                                "button":"left",
                                "x":pointer["x"],
                                "y":pointer["y"]
                            }),
                        )
                        .map_err(|error| {
                            format!(
                                "clicking the freshly observed Ownership link: {}",
                                error.message()
                            )
                        })?;
                    ownership_navigation = json!({
                        "semanticClickRejected":error,
                        "fallback":"fresh scoped pointerTarget",
                        "observedLink":fresh_link,
                        "revision":fresh["revision"]
                    });
                }
                Err(error) => {
                    return Err(format!(
                        "following the freshly observed Ownership link: {error}"
                    ));
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
            if let Ok(address_snapshot) =
                desktop.observe_accessibility(Some(session_id), Some("Chrome address"))
            {
                ownership_navigation["addressObservation"] = address_snapshot;
            }
        }
        if ownership["truncated"] != false
            || ownership["queryAmbiguous"] != false
            || ownership_matches.len() != 1
            || ownership["_testOwnerWindowScoped"] != true
        {
            let scoped_nodes = ownership["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .take(24)
                .map(|node| {
                    json!({
                        "role":node["role"],
                        "name":node["name"],
                        "application":node["application"],
                        "owningWindow":node["owningWindow"],
                        "onscreen":node["onscreen"],
                        "targetIdPresent":node["targetId"].is_string(),
                    })
                })
                .collect::<Vec<_>>();
            // If an explicitly scoped traversal misses a known page link,
            // compare one unscoped title-filtered observation. This is only
            // diagnostic: no token from it is used for an action.
            let title_comparison = if ownership_matches.is_empty() {
                desktop
                    .observe_accessibility(
                        Some(session_id),
                        Some("Chrome Understanding Ownership link"),
                    )
                    .ok()
                    .map(|snapshot| {
                        let nodes = snapshot["nodes"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .take(24)
                            .map(|node| {
                                json!({
                                    "role":node["role"],
                                    "name":node["name"],
                                    "application":node["application"],
                                    "owningWindow":node["owningWindow"],
                                    "onscreen":node["onscreen"],
                                    "targetIdPresent":node["targetId"].is_string(),
                                })
                            })
                            .collect::<Vec<_>>();
                        json!({
                            "focusedWindow":snapshot["focusedWindow"],
                            "windows":snapshot["windows"],
                            "nodesVisited":snapshot["nodesVisited"],
                            "truncated":snapshot["truncated"],
                            "queryAmbiguous":snapshot["queryAmbiguous"],
                            "visitedSamples":snapshot["_testVisitedSamples"],
                            "prunedBranches":snapshot["_testPrunedBranches"],
                            "roleCounts":snapshot["_testRoleCounts"],
                            "nodes":nodes,
                        })
                    })
            } else {
                None
            };
            return Err(format!(
                "the owner-scoped Ownership link search was not complete and unique: {}",
                json!({
                    "truncated":ownership["truncated"],
                    "queryAmbiguous":ownership["queryAmbiguous"],
                    "nodesVisited":ownership["nodesVisited"],
                    "ownerWindowScoped":ownership["_testOwnerWindowScoped"],
                    "visitedSamples":ownership["_testVisitedSamples"],
                    "prunedBranches":ownership["_testPrunedBranches"],
                    "roleCounts":ownership["_testRoleCounts"],
                    "focusedWindow":ownership["focusedWindow"],
                    "windows":ownership["windows"],
                    "matches":ownership_matches,
                    "nodes":scoped_nodes,
                    "titleComparison":title_comparison,
                })
            ));
        }
        browser_window_id = select_rust_book_window(
            desktop,
            session_id,
            "Chrome Understanding Ownership The Rust Programming Language",
            "understanding ownership",
        )?;
        let address_value = ownership_navigation["addressObservation"]["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|node| node["value"].as_str());
        if !address_value
            .is_some_and(|address| address.contains("/book/ch04-00-understanding-ownership.html"))
        {
            return Err(format!(
                "the public Ownership link did not reach its chapter URL: {}",
                json!({"address":address_value,"navigation":ownership_navigation})
            ));
        }
        let ownership_toc = "Chrome Understanding Ownership Toggle Table of Contents label";
        let _ = pointer_click_fresh_target(
            desktop,
            session_id,
            &browser_window_id,
            ownership_toc,
            "Toggle Table of Contents",
            true,
        )?;
        let references_query = "Chrome References and Borrowing link";
        let references_click = match click_fresh_link(
            desktop,
            session_id,
            &browser_window_id,
            references_query,
            "References and Borrowing",
        ) {
            Ok(result) => result,
            Err(direct_error) => {
                let fallback = desktop
                    .observe_accessibility(
                        Some(session_id),
                        Some("Chrome References and Borrowing link"),
                    )
                    .unwrap_or_else(|error| json!({"error":error}));
                return Err(format!(
                    "direct-window References search failed: {direct_error}; title-scoped comparison: {}",
                    json!({"focusedWindow":fallback["focusedWindow"],"windows":fallback["windows"],"nodesVisited":fallback["nodesVisited"],"truncated":fallback["truncated"],"queryAmbiguous":fallback["queryAmbiguous"],"visitedSamples":fallback["_testVisitedSamples"],"prunedBranches":fallback["_testPrunedBranches"],"nodes":fallback["nodes"]})
                ));
            }
        };
        std::thread::sleep(std::time::Duration::from_millis(300));
        browser_window_id = select_rust_book_window(
            desktop,
            session_id,
            "Chrome References Borrowing The Rust Programming Language",
            "references and borrowing",
        )?;
        let references_address = read_browser_address(desktop, session_id, &browser_window_id)?;
        if !references_address.contains("/book/ch04-02-references-and-borrowing.html") {
            return Err(format!(
                "the observed References and Borrowing link did not reach its chapter: {references_address}"
            ));
        }
        let references_toc = "Chrome Toggle Table of Contents label";
        let _ = pointer_click_fresh_target(
            desktop,
            session_id,
            &browser_window_id,
            references_toc,
            "Toggle Table of Contents",
            true,
        )?;
        let slice_query = "Chrome The Slice Type link";
        let current_view_query = "References Borrowing heading";
        let current_view = desktop
            .observe_accessibility_scoped(
                Some(session_id),
                Some(current_view_query),
                Some(&browser_window_id),
            )
            .map_err(|error| format!("checking the current References page view: {error}"))?;
        let current_view_marker = current_view["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|node| {
                node["role"] == "heading"
                    && node["name"]
                        .as_str()
                        .is_some_and(|name| name.trim() == "References and Borrowing")
            })
            .ok_or_else(|| {
                format!("current References heading was not observed: {current_view}")
            })?;
        if current_view["truncated"] != false
            || current_view["queryAmbiguous"] != false
            || !current_view_marker["states"]
                .as_array()
                .is_some_and(|states| states.iter().any(|state| state == "visible"))
            || !current_view_marker["states"]
                .as_array()
                .is_some_and(|states| states.iter().any(|state| state == "showing"))
        {
            return Err(format!(
                "current References heading is not a complete visible anchor: {}",
                json!({"truncated":current_view["truncated"],"queryAmbiguous":current_view["queryAmbiguous"],"marker":current_view_marker})
            ));
        }
        let slice_snapshot = desktop
            .observe_accessibility_scoped(
                Some(session_id),
                Some(slice_query),
                Some(&browser_window_id),
            )
            .map_err(|error| format!("searching the References TOC for Slice Type: {error}"))?;
        let slice_matches = slice_snapshot["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|node| {
                node["role"] == "link"
                    && node["name"]
                        .as_str()
                        .is_some_and(|name| name.trim() == "The Slice Type")
            })
            .map(|node| {
                json!({
                    "name":node["name"],
                    "onscreen":node["onscreen"],
                    "hasTarget":node["targetId"].is_string(),
                    "hasPointer":node["pointerTarget"].is_object(),
                })
            })
            .collect::<Vec<_>>();
        if slice_snapshot["truncated"] != false
            || slice_snapshot["queryAmbiguous"] != false
            || slice_matches.len() != 1
        {
            return Err(format!(
                "owner-scoped Slice Type query did not find one complete match: {}",
                json!({
                    "truncated":slice_snapshot["truncated"],
                    "queryAmbiguous":slice_snapshot["queryAmbiguous"],
                    "nodesVisited":slice_snapshot["nodesVisited"],
                    "ownerWindowScoped":slice_snapshot["_testOwnerWindowScoped"],
                    "matches":slice_matches
                })
            ));
        }
        let initial_slice = slice_snapshot["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|node| {
                node["role"] == "link"
                    && node["name"]
                        .as_str()
                        .is_some_and(|name| name.trim() == "The Slice Type")
            })
            .ok_or("complete Slice Type query lost its descriptive match")?;
        if initial_slice["onscreen"] != false
            || !initial_slice["targetId"].is_null()
            || !initial_slice["pointerTarget"].is_null()
        {
            return Err(format!(
                "initial offscreen Slice Type result must remain tokenless: {initial_slice}"
            ));
        }
        let slice_scroll = scroll_until_query_link_is_onscreen(
            desktop,
            session_id,
            &browser_window_id,
            slice_query,
            "The Slice Type",
        )?;
        let after_scroll = slice_scroll
            .last()
            .ok_or("scroll-to-target returned no fresh query result")?;
        if after_scroll["onscreen"] != true
            || after_scroll["hasTarget"] != true
            || after_scroll["hasPointer"] != true
        {
            return Err(format!(
                "fresh post-scroll Slice Type result lacks onscreen action tokens: {after_scroll}"
            ));
        }
        let slice_click = click_fresh_link(
            desktop,
            session_id,
            &browser_window_id,
            slice_query,
            "The Slice Type",
        )?;
        std::thread::sleep(std::time::Duration::from_millis(300));
        browser_window_id = select_rust_book_window(
            desktop,
            session_id,
            "Chrome The Slice Type The Rust Programming Language",
            "the slice type",
        )?;
        let slice_address = read_browser_address(desktop, session_id, &browser_window_id)?;
        if !slice_address.contains("/book/ch04-03-slices.html") {
            return Err(format!(
                "the observed Slice Type link did not reach its chapter: {slice_address}"
            ));
        }
        let slice_heading_snapshot = desktop
            .observe_accessibility_scoped(
                Some(session_id),
                Some("Chrome The Slice Type heading"),
                Some(&browser_window_id),
            )
            .map_err(|error| format!("observing the Slice Type page heading: {error}"))?;
        let slice_heading = slice_heading_snapshot["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|node| {
                node["role"] == "heading"
                    && node["name"]
                        .as_str()
                        .is_some_and(|name| name.trim() == "The Slice Type")
            })
            .ok_or_else(|| {
                format!("Slice Type URL did not expose its page heading: {slice_heading_snapshot}")
            })?;
        if slice_heading_snapshot["truncated"] != false
            || slice_heading_snapshot["queryAmbiguous"] != false
            || !slice_heading["states"]
                .as_array()
                .is_some_and(|states| states.iter().any(|state| state == "visible"))
            || !slice_heading["states"]
                .as_array()
                .is_some_and(|states| states.iter().any(|state| state == "showing"))
        {
            return Err(format!(
                "Slice Type page heading was not complete and visible: {}",
                json!({
                    "url":slice_address,
                    "focusedWindow":slice_heading_snapshot["focusedWindow"],
                    "windowId":browser_window_id,
                    "heading":slice_heading,
                    "truncated":slice_heading_snapshot["truncated"],
                    "queryAmbiguous":slice_heading_snapshot["queryAmbiguous"],
                })
            ));
        }
        let screenshot_path = std::env::temp_dir().join(format!(
            "glass-public-rust-book-ax-{}.jpg",
            std::process::id()
        ));
        let screenshot = desktop
            .frame_after(0, std::time::Duration::from_secs(2))
            .map_err(|error| format!("capturing isolated diagnostic frame: {error}"))?
            .ok_or("isolated diagnostic frame was unavailable")?;
        fs::write(&screenshot_path, screenshot.2)
            .map_err(|error| format!("saving isolated diagnostic frame: {error}"))?;
        fs::set_permissions(&screenshot_path, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("protecting isolated diagnostic frame: {error}"))?;
        let report = json!({
            "diagnosticOnly":true,
            "browserTitle":browser_title,
            "browserWindowId":browser_window_id,
            "toggleQuery":toggle_query,
            "toggle":toggle,
            "ownershipQuery":ownership_query,
            "ownershipSearch":{"truncated":ownership["truncated"],"queryAmbiguous":ownership["queryAmbiguous"],"nodesVisited":ownership["nodesVisited"],"ownerWindowScoped":ownership["_testOwnerWindowScoped"],"matches":ownership_matches},
            "ownershipNavigation":{"fallback":ownership_navigation["fallback"],"semanticClickRejected":ownership_navigation["semanticClickRejected"],"address":address_value},
            "referencesNavigation":{"address":references_address,"result":references_click},
            "sliceSearch":{"query":slice_query,"nodesVisited":slice_snapshot["nodesVisited"],"truncated":slice_snapshot["truncated"],"queryAmbiguous":slice_snapshot["queryAmbiguous"],"ownerWindowScoped":slice_snapshot["_testOwnerWindowScoped"],"matches":slice_matches},
            "sliceNavigation":{"address":slice_address,"scrolls":slice_scroll,"result":slice_click},
            "slicePageContent":{"windowId":browser_window_id,"heading":slice_heading["name"],"visible":true},
            "screenshot":screenshot_path,
        });
        eprintln!("GLASS_PUBLIC_AX_DIAGNOSTIC {report}");
        Ok(json!({"browserWindowObserved":true,"diagnosticOnly":true}))
    }

    fn pointer_click_fresh_target(
        desktop: &super::super::Desktop,
        session_id: &str,
        window_id: &str,
        query: &str,
        expected_name: &str,
        require_pointer_fallback: bool,
    ) -> Result<Value, String> {
        let snapshot = desktop
            .observe_accessibility_scoped(Some(session_id), Some(query), Some(window_id))
            .map_err(|error| format!("observing `{query}` before pointer click: {error}"))?;
        if snapshot["truncated"] != false || snapshot["queryAmbiguous"] != false {
            return Err(format!(
                "pointer query `{query}` was incomplete or ambiguous: {}",
                json!({
                    "truncated":snapshot["truncated"],
                    "queryAmbiguous":snapshot["queryAmbiguous"],
                    "nodesVisited":snapshot["nodesVisited"],
                    "roleCounts":snapshot["_testRoleCounts"],
                    "nodes":snapshot["nodes"]
                })
            ));
        }
        let target = snapshot["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|node| {
                node["name"]
                    .as_str()
                    .is_some_and(|name| name.trim() == expected_name)
                    && node["onscreen"] == true
                    && node["pointerTarget"].is_object()
                    && (!require_pointer_fallback || node["pointerFallback"] == true)
            })
            .cloned()
            .ok_or_else(|| {
                format!(
                    "fresh pointer query `{query}` did not produce the expected onscreen target: {}",
                    json!({"nodesVisited":snapshot["nodesVisited"],"nodes":snapshot["nodes"]})
                )
            })?;
        let pointer = &target["pointerTarget"];
        desktop
            .input(
                super::super::Lease::Agent,
                &json!({
                    "action":"pointer-click",
                    "button":"left",
                    "x":pointer["x"],
                    "y":pointer["y"]
                }),
            )
            .map_err(|error| {
                format!(
                    "clicking fresh `{expected_name}` pointer: {}",
                    error.message()
                )
            })?;
        Ok(
            json!({"name":target["name"],"pointerFallback":target["pointerFallback"],"revision":snapshot["revision"]}),
        )
    }

    fn click_fresh_link(
        desktop: &super::super::Desktop,
        session_id: &str,
        window_id: &str,
        query: &str,
        expected_name: &str,
    ) -> Result<Value, String> {
        let snapshot = desktop
            .observe_accessibility_scoped(Some(session_id), Some(query), Some(window_id))
            .map_err(|error| format!("observing `{query}` before link click: {error}"))?;
        if snapshot["truncated"] != false || snapshot["queryAmbiguous"] != false {
            return Err(format!(
                "link query `{query}` was incomplete or ambiguous: {}",
                json!({
                    "truncated":snapshot["truncated"],
                    "queryAmbiguous":snapshot["queryAmbiguous"],
                    "nodesVisited":snapshot["nodesVisited"],
                    "nodes":snapshot["nodes"]
                })
            ));
        }
        let target = snapshot["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|node| {
                node["role"] == "link"
                    && node["name"]
                        .as_str()
                        .is_some_and(|name| name.trim() == expected_name)
                    && node["onscreen"] == true
            })
            .cloned()
            .ok_or_else(|| {
                format!(
                    "link query `{query}` did not find the exact onscreen `{expected_name}` link: {}",
                    json!({
                        "windowId":window_id,
                        "nodesVisited":snapshot["nodesVisited"],
                        "truncated":snapshot["truncated"],
                        "queryAmbiguous":snapshot["queryAmbiguous"],
                        "focusedWindow":snapshot["focusedWindow"],
                        "windows":snapshot["windows"],
                        "roleCounts":snapshot["_testRoleCounts"],
                        "visitedSamples":snapshot["_testVisitedSamples"],
                        "prunedBranches":snapshot["_testPrunedBranches"],
                        "nodes":snapshot["nodes"].as_array().into_iter().flatten().take(24).collect::<Vec<_>>(),
                    })
                )
            })?;
        if let Some(target_id) = target["targetId"].as_str() {
            match desktop.act_accessibility(&json!({
                "sessionId":session_id,
                "revision":snapshot["revision"],
                "targetId":target_id,
                "action":"click"
            })) {
                Ok(result) => return Ok(json!({"method":"desktop_act","result":result})),
                Err(error) if error.contains("no recognized accessibility click action") => {}
                Err(error) => {
                    return Err(format!("desktop_act failed for `{expected_name}`: {error}"));
                }
            }
        }
        let fresh = desktop
            .observe_accessibility_scoped(Some(session_id), Some(query), Some(window_id))
            .map_err(|error| format!("reobserving `{query}` for pointer fallback: {error}"))?;
        if fresh["truncated"] != false || fresh["queryAmbiguous"] != false {
            return Err(format!(
                "fresh link query `{query}` was incomplete or ambiguous: {}",
                json!({"truncated":fresh["truncated"],"queryAmbiguous":fresh["queryAmbiguous"]})
            ));
        }
        let fresh_target = fresh["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|node| {
                node["role"] == "link"
                    && node["name"]
                        .as_str()
                        .is_some_and(|name| name.trim() == expected_name)
                    && node["onscreen"] == true
                    && node["pointerTarget"].is_object()
            })
            .cloned()
            .ok_or_else(|| {
                format!(
                    "fresh link query `{query}` has no onscreen pointer target: {}",
                    json!({"nodesVisited":fresh["nodesVisited"],"nodes":fresh["nodes"]})
                )
            })?;
        let pointer = &fresh_target["pointerTarget"];
        desktop
            .input(
                super::super::Lease::Agent,
                &json!({
                    "action":"pointer-click",
                    "button":"left",
                    "x":pointer["x"],
                    "y":pointer["y"]
                }),
            )
            .map_err(|error| {
                format!(
                    "clicking fresh `{expected_name}` link pointer: {}",
                    error.message()
                )
            })?;
        Ok(json!({
            "method":"fresh_pointerTarget",
            "semanticClickRejected":"no recognized accessibility click action",
            "revision":fresh["revision"],
            "name":fresh_target["name"]
        }))
    }

    fn scroll_until_query_link_is_onscreen(
        desktop: &super::super::Desktop,
        session_id: &str,
        window_id: &str,
        query: &str,
        expected_name: &str,
    ) -> Result<Vec<Value>, String> {
        let mut observations = Vec::new();
        for _ in 0..6 {
            let snapshot = desktop
                .observe_accessibility_scoped(Some(session_id), Some(query), Some(window_id))
                .map_err(|error| format!("observing `{query}` while scrolling: {error}"))?;
            if snapshot["truncated"] != false || snapshot["queryAmbiguous"] != false {
                return Err(format!(
                    "scroll query `{query}` was incomplete or ambiguous: {}",
                    json!({"nodesVisited":snapshot["nodesVisited"],"truncated":snapshot["truncated"],"queryAmbiguous":snapshot["queryAmbiguous"]})
                ));
            }
            let target = snapshot["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|node| {
                    node["role"] == "link"
                        && node["name"]
                            .as_str()
                            .is_some_and(|name| name.trim() == expected_name)
                })
                .ok_or_else(|| format!("scroll query `{query}` lost `{expected_name}`"))?;
            observations.push(json!({
                "onscreen":target["onscreen"],
                "hasTarget":target["targetId"].is_string(),
                "hasPointer":target["pointerTarget"].is_object(),
                "nodesVisited":snapshot["nodesVisited"]
            }));
            if target["onscreen"] == true {
                return Ok(observations);
            }
            if target["targetId"].is_string() || target["pointerTarget"].is_object() {
                return Err("offscreen query result unexpectedly carried an action token".into());
            }
            // Slice Type is the next TOC sibling just below the visible
            // References row. Move down one wheel increment and refresh after
            // every step so the candidate cannot be skipped past the viewport.
            desktop
                .input(
                    super::super::Lease::Agent,
                    &json!({"action":"scroll","x":0.15,"y":0.8,"deltaY":40}),
                )
                .map_err(|error| {
                    format!("scrolling toward `{expected_name}`: {}", error.message())
                })?;
            std::thread::sleep(std::time::Duration::from_millis(120));
        }
        let diagnostic_frame = desktop
            .frame_after(0, std::time::Duration::from_millis(500))
            .ok()
            .flatten()
            .and_then(|(_, _, bytes)| save_scroll_diagnostic_frame(&bytes).ok());
        Err(format!(
            "`{expected_name}` remained offscreen after bounded scrolls: {} (private frame: {})",
            json!(observations),
            diagnostic_frame.as_deref().unwrap_or("unavailable")
        ))
    }

    fn save_scroll_diagnostic_frame(bytes: &[u8]) -> Result<String, std::io::Error> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "glass-scroll-diagnostic-{}-{nonce}.jpg",
            std::process::id()
        ));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(path.display().to_string())
    }

    fn read_browser_address(
        desktop: &super::super::Desktop,
        session_id: &str,
        window_id: &str,
    ) -> Result<String, String> {
        let snapshot = desktop
            .observe_accessibility_scoped(Some(session_id), Some("Chrome address"), Some(window_id))
            .map_err(|error| format!("reading Chrome address: {error}"))?;
        if snapshot["truncated"] != false || snapshot["queryAmbiguous"] != false {
            return Err(format!(
                "Chrome address observation was incomplete or ambiguous: {snapshot}"
            ));
        }
        let values = snapshot["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|node| node["value"].as_str().map(ToOwned::to_owned))
            .collect::<Vec<_>>();
        match values.as_slice() {
            [address] => Ok(address.clone()),
            _ => Err(format!(
                "Chrome address observation was not unique: {snapshot}"
            )),
        }
    }

    fn select_rust_book_window(
        desktop: &super::super::Desktop,
        session_id: &str,
        query: &str,
        expected_title: &str,
    ) -> Result<String, String> {
        let snapshot = desktop
            .observe_accessibility(Some(session_id), Some(query))
            .map_err(|error| format!("selecting visible Rust Book window: {error}"))?;
        snapshot["windows"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|window| {
                window["application"]
                    .as_str()
                    .is_some_and(|name| name.to_ascii_lowercase().contains("chrome"))
                    && window["name"]
                        .as_str()
                        .is_some_and(|name| name.to_ascii_lowercase().contains(expected_title))
            })
            .and_then(|window| window["windowId"].as_str())
            .map(ToOwned::to_owned)
            .ok_or_else(|| format!("visible Rust Book window had no selector: {snapshot}"))
    }

    fn unique_browser_address_value(snapshot: &Value) -> Option<&str> {
        if snapshot["truncated"] != false || snapshot["queryAmbiguous"] != false {
            return None;
        }
        let mut values = snapshot["nodes"]
            .as_array()?
            .iter()
            .filter_map(|node| node["value"].as_str());
        let address = values.next()?;
        values.next().is_none().then_some(address)
    }

    fn write_test_application_entry(
        home: &Path,
        binary: &Path,
        profile: &Path,
    ) -> std::io::Result<()> {
        let applications = home.join(".local/share/applications");
        fs::create_dir_all(&applications)?;
        fs::set_permissions(&applications, fs::Permissions::from_mode(0o700))?;
        let binary = binary.display().to_string();
        let profile = profile.display().to_string();
        if binary
            .chars()
            .any(|character| !(character.is_ascii_alphanumeric() || "/._-".contains(character)))
            || profile
                .chars()
                .any(|character| !(character.is_ascii_alphanumeric() || "/._-".contains(character)))
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "test path contains unsupported desktop-entry characters",
            ));
        }
        let path = applications.join("glass-managed-browser.desktop");
        let mut options = fs::OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        let mut file = options.open(path)?;
        writeln!(
            file,
            "[Desktop Entry]\nType=Application\nName=Managed Chromium (Glass acceptance)\nExec=/usr/bin/env ACCESSIBILITY_ENABLED=1 {binary} --user-data-dir={profile} --no-first-run --no-default-browser-check --password-store=basic --force-renderer-accessibility=complete --disable-gpu --enable-logging=stderr --log-file={profile}/chrome.log\nStartupWMClass=chrome\nTerminal=false\nNoDisplay=false\n"
        )
    }

    fn read_bounded_chrome_log(profile: &Path) -> String {
        let path = profile.join("chrome.log");
        let Ok(file) = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&path)
        else {
            return String::new();
        };
        let mut contents = String::new();
        if file.take(4097).read_to_string(&mut contents).is_err() {
            return String::new();
        }
        contents = contents.replace(&profile.display().to_string(), "<private-profile>");
        contents = contents.replace("unix:path=", "unix:path=<redacted>");
        contents
            .chars()
            .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
            .take(4096)
            .collect()
    }
}
