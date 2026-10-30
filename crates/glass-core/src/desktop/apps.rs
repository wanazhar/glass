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

    fn private_busctl(address: &str, args: &[&str]) -> Option<std::process::Output> {
        Command::new("/usr/bin/timeout")
            .args(["--kill-after=0.25s", "2s", "/usr/bin/busctl"])
            .arg(format!("--address={address}"))
            .args(args)
            .output()
            .ok()
    }

    struct PrivateBusName {
        name: String,
        pid: Option<u32>,
    }

    fn private_bus_names(address: &str) -> (Option<Vec<PrivateBusName>>, bool) {
        let Some(output) = private_busctl(address, &["list", "--no-pager", "--no-legend"]) else {
            return (None, false);
        };
        if !output.status.success() {
            return (None, matches!(output.status.code(), Some(124 | 137)));
        }
        (
            Some(
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .filter_map(|line| {
                        let mut fields = line.split_whitespace();
                        let name = fields.next()?.to_owned();
                        let pid = fields.next().and_then(|value| value.parse::<u32>().ok());
                        Some(PrivateBusName { name, pid })
                    })
                    .collect(),
            ),
            false,
        )
    }

    fn private_desktop_bus_diagnostics(
        session_bus: &str,
        atspi_bus: &str,
        browser_process_ids: &[u32],
    ) -> Value {
        let (session_names, session_bus_probe_timed_out) = private_bus_names(session_bus);
        let (atspi_names, atspi_bus_probe_timed_out) = private_bus_names(atspi_bus);
        let atspi_address_output = private_busctl(
            session_bus,
            &[
                "call",
                "org.a11y.Bus",
                "/org/a11y/bus",
                "org.a11y.Bus",
                "GetAddress",
            ],
        );
        let atspi_address_probe_timed_out = atspi_address_output
            .as_ref()
            .is_some_and(|output| matches!(output.status.code(), Some(124 | 137)));
        let advertised_atspi_address_matches = atspi_address_output
            .filter(|output| output.status.success())
            .and_then(|output| {
                String::from_utf8(output.stdout).ok().and_then(|output| {
                    output
                        .split_once('"')
                        .and_then(|(_, rest)| rest.split_once('"'))
                        .map(|(address, _)| address == atspi_bus)
                })
            });
        json!({
            "sessionBusReachable": session_names.is_some(),
            "sessionBusProbeTimedOut": session_bus_probe_timed_out,
            "sessionBusNameCount": session_names.as_ref().map(Vec::len),
            "a11yBusOwned": session_names.as_ref().is_some_and(|names| names.iter().any(|entry| entry.name == "org.a11y.Bus")),
            "portalDesktopOwned": session_names.as_ref().is_some_and(|names| names.iter().any(|entry| entry.name == "org.freedesktop.portal.Desktop")),
            "hostPortalRegistryOwned": session_names.as_ref().is_some_and(|names| names.iter().any(|entry| entry.name == "org.freedesktop.host.portal.Registry")),
            "atspiBusReachable": atspi_names.is_some(),
            "atspiBusProbeTimedOut": atspi_bus_probe_timed_out,
            "atspiBusNameCount": atspi_names.as_ref().map(Vec::len),
            "atspiRegistryOwned": atspi_names.as_ref().is_some_and(|names| names.iter().any(|entry| entry.name == "org.a11y.atspi.Registry")),
            "atspiUniquePeerCount": atspi_names.as_ref().map(|names| names.iter().filter(|entry| entry.name.starts_with(':')).count()),
            "browserSessionBusPeerCount": session_names.as_ref().map(|names| matching_bus_peer_count(names, browser_process_ids)),
            "browserAtspiBusPeerCount": atspi_names.as_ref().map(|names| matching_bus_peer_count(names, browser_process_ids)),
            "advertisedAtspiAddressMatches": advertised_atspi_address_matches,
            "atspiAddressProbeTimedOut": atspi_address_probe_timed_out,
        })
    }

    fn browser_startup_log_classes(log: &str) -> Value {
        json!({
            "missingOAuthClient": log.contains("no OAuth client ID and client secret"),
            "screensaverServiceUnavailable": log.contains("ScreenSaver services could be used"),
            "hostPortalRegistryUnavailable": log.contains("Failed to register with org.freedesktop.host.portal.Registry"),
            "gcmDeprecatedEndpoint": log.contains("Registration response error message: DEPRECATED_ENDPOINT"),
            "sandboxDiagnosticPresent": log.to_ascii_lowercase().contains("sandbox"),
        })
    }

    fn matching_bus_peer_count(names: &[PrivateBusName], process_ids: &[u32]) -> usize {
        names
            .iter()
            .filter(|entry| {
                entry.name.starts_with(':')
                    && entry.pid.is_some_and(|pid| process_ids.contains(&pid))
            })
            .count()
    }

    fn matching_browser_process_ids(binary: &Path, profile: &Path) -> Vec<u32> {
        let Ok(processes) = fs::read_dir("/proc") else {
            return Vec::new();
        };
        let binary = binary.as_os_str().as_bytes();
        let profile = profile.as_os_str().as_bytes();
        processes
            .flatten()
            .take(8192)
            .filter_map(|entry| {
                let pid = entry.file_name().to_string_lossy().parse::<u32>().ok()?;
                let command_line = fs::read(entry.path().join("cmdline")).ok()?;
                let contains = |needle: &[u8]| {
                    command_line
                        .windows(needle.len())
                        .any(|window| window == needle)
                };
                (contains(binary) && contains(profile)).then_some(pid)
            })
            .collect()
    }

    fn stop_matching_browser_processes(binary: &Path, profile: &Path) {
        let signal_matching = |signal| {
            for pid in matching_browser_process_ids(binary, profile) {
                unsafe {
                    libc::kill(pid as libc::pid_t, signal);
                }
            }
        };
        signal_matching(libc::SIGTERM);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline
            && !matching_browser_process_ids(binary, profile).is_empty()
        {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        signal_matching(libc::SIGKILL);
    }

    fn managed_browser_ax_diagnostics(
        desktop: &super::super::Desktop,
        binary: &Path,
        profile: &Path,
    ) -> Value {
        let (session_bus_address, atspi_bus_address, expected_environment) = {
            let guard = desktop.session.lock().unwrap();
            let session = guard.as_ref().unwrap();
            let atspi_bus =
                fs::read_to_string(session.directory.join("atspi-bus.address")).unwrap_or_default();
            (
                session.session_bus_address.clone(),
                atspi_bus.trim().to_owned(),
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
                    ("ACCESSIBILITY_ENABLED", "1".to_owned()),
                    ("XDG_CURRENT_DESKTOP", "XFCE".to_owned()),
                    ("DESKTOP_SESSION", "xfce".to_owned()),
                    ("XDG_SESSION_TYPE", "x11".to_owned()),
                ],
            )
        };
        let browser_process_ids = matching_browser_process_ids(binary, profile);
        json!({
            "privateBus": private_desktop_bus_diagnostics(&session_bus_address, &atspi_bus_address, &browser_process_ids),
            "browserProcess": managed_browser_process_diagnostics(binary, profile, &expected_environment),
            "chromeLogClasses": browser_startup_log_classes(&read_bounded_chrome_log(profile)),
        })
    }

    fn managed_browser_process_diagnostics(
        binary: &Path,
        profile: &Path,
        expected_environment: &[(&str, String)],
    ) -> Value {
        let Ok(processes) = fs::read_dir("/proc") else {
            return json!({"processCount":0,"procReadable":false,"processes":[]});
        };
        let binary = binary.as_os_str().as_bytes();
        let profile = profile.as_os_str().as_bytes();
        let mut browser_processes = Vec::new();
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
            let pid = entry.file_name().to_string_lossy().parse::<u32>().ok();
            let environment = pid.and_then(|pid| {
                fs::read(Path::new("/proc").join(pid.to_string()).join("environ")).ok()
            });
            let (environment_present, environment_matches) =
                if let Some(environment) = environment.as_deref() {
                    let results = expected_environment
                        .iter()
                        .map(|(name, expected)| {
                            let mut prefix = name.as_bytes().to_vec();
                            prefix.push(b'=');
                            let actual = environment
                                .split(|byte| *byte == 0)
                                .find_map(|value| value.strip_prefix(prefix.as_slice()));
                            (name, actual.is_some(), actual == Some(expected.as_bytes()))
                        })
                        .collect::<Vec<_>>();
                    let present = results
                        .iter()
                        .map(|(name, present, _)| ((**name).to_owned(), Value::Bool(*present)))
                        .collect::<serde_json::Map<_, _>>();
                    let matches = results
                        .iter()
                        .map(|(name, _, matches)| ((**name).to_owned(), Value::Bool(*matches)))
                        .collect::<serde_json::Map<_, _>>();
                    (Some(json!(present)), Some(json!(matches)))
                } else {
                    (None, None)
                };
            let force_accessibility = contains(b"--force-renderer-accessibility=complete");
            let accessibility_disabled = contains(b"--disable-renderer-accessibility");
            let no_sandbox = contains(b"--no-sandbox");
            let setuid_sandbox_disabled = contains(b"--disable-setuid-sandbox");
            let seccomp_sandbox_disabled = contains(b"--disable-seccomp-filter-sandbox");
            browser_processes.push(json!({
                "executableIsChrome": fs::read_link(entry.path().join("exe")).ok().is_some_and(|path| path.file_name().is_some_and(|name| name == "chrome")),
                "forceRendererAccessibilityComplete": force_accessibility,
                "rendererAccessibilityDisabled": accessibility_disabled,
                "noSandbox": no_sandbox,
                "setuidSandboxDisabled": setuid_sandbox_disabled,
                "seccompSandboxDisabled": seccomp_sandbox_disabled,
                "environmentReadable": environment.is_some(),
                "environmentPresent": environment_present,
                "environmentMatches": environment_matches,
            }));
        }
        json!({"processCount":browser_processes.len(),"processes":browser_processes})
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
            return json!({"spawned":false,"profileCreated":false});
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
                return json!({"spawned":false,"managedSessionAvailable":false});
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
            Err(_) => {
                return json!({"spawned":false,"spawnSucceeded":false});
            }
        };
        let process_group = child.id();
        let expected_environment = vec![
            ("XDG_CURRENT_DESKTOP", current_desktop.to_owned()),
            ("DESKTOP_SESSION", desktop_session.to_owned()),
            ("XDG_SESSION_TYPE", "x11".to_owned()),
            ("XDG_SESSION_DESKTOP", desktop_session.to_owned()),
            ("HOME", home.display().to_string()),
            ("DISPLAY", display.clone()),
            ("XAUTHORITY", xauthority.display().to_string()),
            (
                "XDG_CONFIG_HOME",
                home.join(".config").display().to_string(),
            ),
            ("XDG_CACHE_HOME", home.join(".cache").display().to_string()),
            (
                "XDG_DATA_HOME",
                home.join(".local/share").display().to_string(),
            ),
            ("XDG_CONFIG_DIRS", "/etc/xdg".to_owned()),
            (
                "XDG_DATA_DIRS",
                super::super::process::MANAGED_XDG_DATA_DIRS.to_owned(),
            ),
            ("XDG_RUNTIME_DIR", runtime_directory.display().to_string()),
            ("DBUS_SESSION_BUS_ADDRESS", session_bus_address.clone()),
            ("AT_SPI_BUS_ADDRESS", atspi_bus_address.clone()),
            ("GTK_A11Y", "atspi".to_owned()),
            ("ACCESSIBILITY_ENABLED", "1".to_owned()),
        ];
        let process_diagnostics =
            managed_browser_process_diagnostics(binary, profile, &expected_environment);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        let mut window_observed = false;
        let mut last_observation = json!({
            "applicationRootCount": null,
            "browserApplicationRootCount": null,
            "browserWindowCount": null,
            "nodesVisited": null,
            "focusedWindowIsBrowser": null,
        });
        while std::time::Instant::now() < deadline {
            if let Ok(snapshot) = desktop.observe_accessibility(Some(session_id), None) {
                let browser_window_count = snapshot["windows"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|window| {
                        window["application"]
                            .as_str()
                            .is_some_and(super::super::is_browser_application)
                    })
                    .count();
                let browser_application_root_count = snapshot["applications"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|application| {
                        application
                            .as_str()
                            .is_some_and(super::super::is_browser_application)
                    })
                    .count();
                last_observation = json!({
                    "applicationRootCount":snapshot["applications"].as_array().map(Vec::len),
                    "browserApplicationRootCount":browser_application_root_count,
                    "browserWindowCount":browser_window_count,
                    "nodesVisited":snapshot["nodesVisited"],
                    "focusedWindowIsBrowser":snapshot["focusedWindow"]["application"].as_str().is_some_and(super::super::is_browser_application),
                    "truncated":snapshot["truncated"],
                    "queryAmbiguous":snapshot["queryAmbiguous"],
                });
                window_observed = browser_window_count > 0;
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
            "process":process_diagnostics,
            "chromeLogClasses":browser_startup_log_classes(&read_bounded_chrome_log(profile)),
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
    fn browser_address_reading_uses_one_editable_address_field_and_exact_url() {
        let home = "file:///tmp/glass-fixture/browser-home.html";
        let snapshot = json!({
            "truncated":false,
            "queryAmbiguous":false,
            "nodes":[
                {"role":"static","name":"Address hint","value":"not an address"},
                {"role":"entry","name":"Address and search bar","editable":true,"value":home}
            ]
        });
        assert_eq!(unique_browser_address_value(&snapshot), Some(home));
        assert!(address_matches_expected(home, home));
        assert!(!address_matches_expected(
            "Search Google or enter address",
            home
        ));
        assert!(!address_matches_expected(
            "file:///tmp/glass-fixture/browser-home.html?unexpected=1",
            home
        ));
        assert!(!address_matches_expected("chrome://newtab/", home));

        let duplicate_fields = json!({
            "truncated":false,
            "queryAmbiguous":false,
            "nodes":[
                {"role":"entry","name":"Address bar","editable":true,"value":home},
                {"role":"combo box","name":"Address search","editable":true,"value":home}
            ]
        });
        assert_eq!(unique_browser_address_value(&duplicate_fields), None);

        let incomplete = json!({
            "truncated":true,
            "queryAmbiguous":false,
            "nodes":[{"role":"entry","name":"Address bar","editable":true,"value":home}]
        });
        assert_eq!(unique_browser_address_value(&incomplete), None);
    }

    #[test]
    fn managed_browser_key_input_loads_owned_fixture_with_debugger_route_check() {
        if std::env::var_os("GLASS_DESKTOP_APP_ADDRESS_CONTROL_E2E").as_deref()
            != Some(std::ffi::OsStr::new("1"))
        {
            return;
        }
        let root = crate::config::data_dir().join(format!(
            "desktop-address-control-e2e-{}-{}",
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
            .unwrap_or_else(|_| panic!("could not start isolated XFCE"));
        let session_id = started["sessionId"].as_str().unwrap().to_owned();

        let mut debugger_port = None;
        let mut owned_browser_profile = None;
        let outcome = (|| -> Result<Value, &'static str> {
            let binary = std::env::var_os("GLASS_DESKTOP_BROWSER_BINARY")
                .map(PathBuf::from)
                .ok_or("browser_binary_missing")?
                .canonicalize()
                .map_err(|_| "browser_binary_missing")?;
            if !binary.is_file() || binary.to_string_lossy().chars().any(char::is_whitespace) {
                return Err("browser_binary_invalid");
            }
            let session_home = desktop
                .session
                .lock()
                .unwrap()
                .as_ref()
                .ok_or("desktop_session_missing")?
                .directory
                .join("home");
            let profile = session_home.join("chromium-address-control-profile");
            fs::create_dir(&profile).map_err(|_| "profile_create_failed")?;
            fs::set_permissions(&profile, fs::Permissions::from_mode(0o700))
                .map_err(|_| "profile_create_failed")?;
            owned_browser_profile = Some((binary.clone(), profile.clone()));

            let fixture = root.join("address-control.html");
            fs::write(
                &fixture,
                "<!doctype html><title>Address control fixture</title><h1>Local navigation completed</h1>",
            )
            .map_err(|_| "fixture_write_failed")?;
            let expected_url = url::Url::from_file_path(&fixture)
                .map_err(|_| "fixture_url_invalid")?
                .to_string();
            let direct_file_accessibility =
                std::env::var_os("GLASS_DESKTOP_APP_DIRECT_FILE_ACCESSIBILITY_E2E").as_deref()
                    == Some(std::ffi::OsStr::new("1"));
            write_test_application_entry(
                &session_home,
                &binary,
                &profile,
                true,
                direct_file_accessibility.then_some(expected_url.as_str()),
            )
            .map_err(|_| "desktop_entry_write_failed")?;
            let listed = desktop
                .launch_app(&json!({"action":"list","sessionId":session_id}))
                .map_err(|_| "app_list_failed")?;
            let browser_id = listed["applications"]
                .as_array()
                .and_then(|apps| {
                    apps.iter()
                        .find(|app| app["desktopEntryId"] == "glass-managed-browser")
                })
                .and_then(|app| app["desktopEntryId"].as_str())
                .ok_or("browser_entry_not_listed")?;
            desktop
                .launch_app(
                    &json!({"action":"launch","sessionId":session_id,"desktopEntryId":browser_id}),
                )
                .map_err(|_| "browser_launch_failed")?;

            let debugger_deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            while std::time::Instant::now() < debugger_deadline {
                debugger_port = fs::read_to_string(profile.join("DevToolsActivePort"))
                    .ok()
                    .and_then(|contents| contents.lines().next()?.parse::<u16>().ok());
                if debugger_port.is_some() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let debugger_port = debugger_port.ok_or("debugger_endpoint_not_ready")?;
            let mut initial_route = Err("endpoint_unavailable");
            let route_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while std::time::Instant::now() < route_deadline {
                initial_route = devtools_current_route_category(debugger_port, &expected_url);
                if initial_route.is_ok() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let mut direct_fixture_route_observed =
                initial_route.as_ref().ok().copied() == Some("fixture");
            let mut direct_route_probe_error = initial_route.as_ref().err().copied();
            if direct_file_accessibility {
                let navigation_deadline =
                    std::time::Instant::now() + std::time::Duration::from_secs(12);
                while !direct_fixture_route_observed
                    && std::time::Instant::now() < navigation_deadline
                {
                    match devtools_current_route_category(debugger_port, &expected_url) {
                        Ok(route) => {
                            direct_fixture_route_observed = route == "fixture";
                            direct_route_probe_error = None;
                        }
                        Err(error) => direct_route_probe_error = Some(error),
                    }
                    if !direct_fixture_route_observed {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            }
            let accessibility_snapshot =
                desktop.observe_accessibility(Some(&session_id), None).ok();
            let browser_ax_window_observed = accessibility_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot["windows"].as_array())
                .is_some_and(|windows| {
                    windows.iter().any(|window| {
                        window["application"]
                            .as_str()
                            .is_some_and(super::super::is_browser_application)
                    })
                });
            let focused_window_is_browser = accessibility_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot["focusedWindow"]["application"].as_str())
                .is_some_and(super::super::is_browser_application);
            let accessibility_snapshot_complete = accessibility_snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot["truncated"] == false);
            let accessibility_snapshot_unambiguous = accessibility_snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot["queryAmbiguous"] == false);

            if direct_file_accessibility {
                let screenshot_captured = if browser_ax_window_observed
                    && focused_window_is_browser
                    && accessibility_snapshot_complete
                    && accessibility_snapshot_unambiguous
                {
                    false
                } else {
                    desktop
                        .frame_after(0, std::time::Duration::from_secs(2))
                        .ok()
                        .flatten()
                        .map(|frame| frame.0)
                        .and_then(|watermark| {
                            desktop
                                .frame_after(watermark, std::time::Duration::from_secs(2))
                                .ok()
                                .flatten()
                        })
                        .and_then(|frame| {
                            std::env::var_os("GLASS_DESKTOP_APP_ADDRESS_CONTROL_SCREENSHOT_PATH")
                                .map(PathBuf::from)
                                .map(|path| path.with_file_name("direct-file-ax.jpg"))
                                .map(|path| {
                                    write_private_diagnostic_screenshot(&path, &frame.2).is_ok()
                                })
                        })
                        .unwrap_or(false)
                };
                return Ok(json!({
                    "stage":"direct_file_accessibility",
                    "fixtureRouteObserved":direct_fixture_route_observed,
                    "routeProbeError":direct_route_probe_error,
                    "browserAXWindowObserved":browser_ax_window_observed,
                    "focusedWindowIsBrowser":focused_window_is_browser,
                    "accessibilitySnapshotComplete":accessibility_snapshot_complete,
                    "accessibilitySnapshotUnambiguous":accessibility_snapshot_unambiguous,
                    "screenshotCaptured":screenshot_captured,
                    "fixtureUrlLoaded":direct_fixture_route_observed,
                }));
            }

            if !browser_ax_window_observed
                || !focused_window_is_browser
                || !accessibility_snapshot_complete
                || !accessibility_snapshot_unambiguous
            {
                let focus_gate_screenshot_captured = desktop
                    .frame_after(0, std::time::Duration::from_secs(2))
                    .ok()
                    .flatten()
                    .map(|frame| frame.0)
                    .and_then(|watermark| {
                        desktop
                            .frame_after(watermark, std::time::Duration::from_secs(2))
                            .ok()
                            .flatten()
                    })
                    .and_then(|frame| {
                        std::env::var_os("GLASS_DESKTOP_APP_ADDRESS_CONTROL_SCREENSHOT_PATH")
                            .map(PathBuf::from)
                            .map(|path| path.with_file_name("focus-gate.jpg"))
                            .map(|path| {
                                write_private_diagnostic_screenshot(&path, &frame.2).is_ok()
                            })
                    })
                    .unwrap_or(false);
                return Ok(json!({
                    "stage":"chromium_focus_unverified",
                    "browserAXWindowObserved":browser_ax_window_observed,
                    "focusedWindowIsBrowser":focused_window_is_browser,
                    "accessibilitySnapshotComplete":accessibility_snapshot_complete,
                    "accessibilitySnapshotUnambiguous":accessibility_snapshot_unambiguous,
                    "focusGateScreenshotCaptured":focus_gate_screenshot_captured,
                    "debuggerPortPublished":true,
                    "debuggerRouteObserved":initial_route.is_ok(),
                    "initialRoute":initial_route.as_ref().ok().copied(),
                    "initialProbeError":initial_route.as_ref().err().copied(),
                    "fixtureUrlLoaded":false,
                }));
            }

            let frame_watermark = desktop
                .frame_after(0, std::time::Duration::from_secs(3))
                .ok()
                .flatten()
                .map(|frame| frame.0)
                .ok_or("pre_input_frame_watermark_unavailable")?;
            desktop
                .input(
                    super::super::Lease::Agent,
                    &json!({"action":"key-chord","keys":["Control","L"]}),
                )
                .map_err(|_| "control_l_input_failed")?;
            desktop
                .input(
                    super::super::Lease::Agent,
                    &json!({"action":"text","text":expected_url}),
                )
                .map_err(|_| "address_text_input_failed")?;
            std::thread::sleep(std::time::Duration::from_millis(150));
            let frame = desktop
                .frame_after(frame_watermark, std::time::Duration::from_secs(3))
                .ok()
                .flatten()
                .ok_or("pre_enter_frame_unavailable")?;
            let screenshot_path =
                std::env::var_os("GLASS_DESKTOP_APP_ADDRESS_CONTROL_SCREENSHOT_PATH")
                    .map(PathBuf::from)
                    .ok_or("private_screenshot_path_missing")?;
            write_private_diagnostic_screenshot(&screenshot_path, &frame.2)
                .map_err(|_| "private_screenshot_write_failed")?;
            let typed_address_snapshot = desktop
                .observe_accessibility(Some(&session_id), Some("Chromium address"))
                .ok();
            if let Some(snapshot) = typed_address_snapshot.as_ref() {
                append_private_address_evidence(
                    &screenshot_path.with_file_name("address-control.jsonl"),
                    "typed-address-before-enter",
                    &expected_url,
                    snapshot,
                )
                .map_err(|_| "private_address_evidence_write_failed")?;
            }
            let typed_address_field_focused = typed_address_snapshot
                .as_ref()
                .is_some_and(browser_address_field_is_focused);
            let typed_url_matches_expected = typed_address_snapshot
                .as_ref()
                .and_then(unique_browser_address_value)
                .is_some_and(|actual| address_matches_expected(actual, &expected_url));
            if !typed_address_field_focused || !typed_url_matches_expected {
                return Ok(json!({
                    "stage":"typed_address_unverified",
                    "browserAXWindowObserved":browser_ax_window_observed,
                    "focusedWindowIsBrowser":focused_window_is_browser,
                    "addressSnapshotComplete":typed_address_snapshot.as_ref().is_some_and(|snapshot| snapshot["truncated"] == false),
                    "addressSnapshotUnambiguous":typed_address_snapshot.as_ref().is_some_and(|snapshot| snapshot["queryAmbiguous"] == false),
                    "typedAddressFieldFocused":typed_address_field_focused,
                    "typedUrlMatchesExpected":typed_url_matches_expected,
                    "fixtureUrlLoaded":false,
                }));
            }
            desktop
                .input(
                    super::super::Lease::Agent,
                    &json!({"action":"key-tap","key":"Enter"}),
                )
                .map_err(|_| "enter_input_failed")?;
            let after_enter_frame = desktop
                .frame_after(frame.0, std::time::Duration::from_secs(3))
                .ok()
                .flatten()
                .ok_or("post_enter_frame_unavailable")?;
            let screenshot_path =
                std::env::var_os("GLASS_DESKTOP_APP_ADDRESS_CONTROL_SCREENSHOT_PATH")
                    .map(PathBuf::from)
                    .ok_or("private_screenshot_path_missing")?;
            let after_enter_path = screenshot_path.with_file_name("after-enter.jpg");
            write_private_diagnostic_screenshot(&after_enter_path, &after_enter_frame.2)
                .map_err(|_| "post_enter_screenshot_write_failed")?;

            let navigation_deadline =
                std::time::Instant::now() + std::time::Duration::from_secs(12);
            let mut post_enter_route = initial_route.as_ref().ok().copied();
            let mut post_enter_probe_error = initial_route.as_ref().err().copied();
            while std::time::Instant::now() < navigation_deadline {
                match devtools_current_route_category(debugger_port, &expected_url) {
                    Ok(route) => {
                        post_enter_route = Some(route);
                        post_enter_probe_error = None;
                    }
                    Err(error) => {
                        post_enter_probe_error = Some(error);
                    }
                }
                if post_enter_route == Some("fixture") {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Ok(json!({
                "stage":"complete",
                "browserAXWindowObserved":browser_ax_window_observed,
                "focusedWindowIsBrowser":focused_window_is_browser,
                "accessibilitySnapshotComplete":accessibility_snapshot_complete,
                "accessibilitySnapshotUnambiguous":accessibility_snapshot_unambiguous,
                "typedAddressFieldFocused":typed_address_field_focused,
                "typedUrlMatchesExpected":typed_url_matches_expected,
                "debuggerPortPublished":true,
                "debuggerRouteObserved":initial_route.is_ok(),
                "initialRoute":initial_route.as_ref().ok().copied(),
                "initialProbeError":initial_route.as_ref().err().copied(),
                "postEnterRoute":post_enter_route,
                "postEnterProbeError":post_enter_probe_error,
                "fixtureUrlLoaded":post_enter_route == Some("fixture"),
            }))
        })();
        let stopped = desktop.stop();
        if let Some((binary, profile)) = owned_browser_profile.as_ref() {
            stop_matching_browser_processes(binary, profile);
        }
        let endpoint_closed_after_stop = debugger_port.is_none_or(|port| {
            let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while std::time::Instant::now() < deadline {
                if std::net::TcpStream::connect_timeout(
                    &address,
                    std::time::Duration::from_millis(250),
                )
                .is_err()
                {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            false
        });
        let _ = fs::remove_dir_all(&root);
        stopped.unwrap_or_else(|_| panic!("isolated desktop cleanup failed"));
        let report = outcome.unwrap_or_else(|stage| {
            panic!("managed browser address-control diagnostic stopped at {stage}")
        });
        let mut report = report;
        report["endpointClosedAfterStop"] = json!(endpoint_closed_after_stop);
        eprintln!("managed browser address-control diagnostic: {report}");
        assert_eq!(
            report["fixtureUrlLoaded"], true,
            "local fixture URL was not loaded"
        );
        if std::env::var_os("GLASS_DESKTOP_APP_DIRECT_FILE_ACCESSIBILITY_E2E").as_deref()
            == Some(std::ffi::OsStr::new("1"))
        {
            assert_eq!(
                report["browserAXWindowObserved"], true,
                "direct local fixture load did not expose a Chromium AX window"
            );
            assert_eq!(
                report["focusedWindowIsBrowser"], true,
                "direct local fixture load did not focus the Chromium AX window"
            );
            assert_eq!(
                report["accessibilitySnapshotComplete"], true,
                "direct local fixture AX snapshot was incomplete"
            );
            assert_eq!(
                report["accessibilitySnapshotUnambiguous"], true,
                "direct local fixture AX snapshot was ambiguous"
            );
        }
        assert_eq!(
            report["endpointClosedAfterStop"], true,
            "test-owned Chromium endpoint remained active after cleanup"
        );
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
            .unwrap_or_else(|error| {
                let capture_frame_unavailable =
                    error.starts_with("Desktop capture did not produce a fresh frame");
                panic!(
                    "could not start isolated XFCE; captureFrameUnavailable={capture_frame_unavailable}"
                )
            });
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
            write_test_application_entry(&home, &binary, &browser_profile, false, None)
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
            let launched = match desktop.launch_app(&json!({
                "action":"launch",
                "sessionId":session_id,
                "desktopEntryId":browser_id
            })) {
                Ok(launched) => launched,
                Err(error) => {
                    if std::env::var_os("GLASS_DESKTOP_APP_LAUNCH_DIAGNOSTICS").as_deref()
                        == Some(std::ffi::OsStr::new("1"))
                    {
                        eprintln!(
                            "managed browser launch AX diagnostics: {}",
                            managed_browser_ax_diagnostics(&desktop, &binary, &browser_profile)
                        );
                    }
                    let accessibility_observation_timed_out =
                        error.contains("Accessibility observation exceeded");
                    return Err(format!(
                        "listed app launch failed; accessibilityObservationTimedOut={accessibility_observation_timed_out}"
                    ));
                }
            };
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
                                    super::super::is_browser_application(application)
                                })
                        })
                        .take(20)
                        .cloned()
                        .collect::<Vec<_>>();
                    let browser_ax_node_count = diagnostic_nodes
                        .iter()
                        .filter(|node| {
                            node["application"]
                                .as_str()
                                .is_some_and(super::super::is_browser_application)
                        })
                        .count();
                    let browser_window_node_count = diagnostic_nodes
                        .iter()
                        .filter(|node| {
                            matches!(node["role"].as_str(), Some("frame" | "window"))
                                && node["application"]
                                    .as_str()
                                    .is_some_and(super::super::is_browser_application)
                        })
                        .count();
                    let browser_application_root_count = snapshot["applications"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|application| {
                            application
                                .as_str()
                                .is_some_and(super::super::is_browser_application)
                        })
                        .count();
                    latest = json!({
                        "applicationRootCount": snapshot["applications"].as_array().map(Vec::len),
                        "browserApplicationRootCount": browser_application_root_count,
                        "browserAXNodeCount": browser_ax_node_count,
                        "browserWindowNodeCount": browser_window_node_count,
                        "focusedWindowIsBrowser": snapshot["focusedWindow"]["application"].as_str().is_some_and(super::super::is_browser_application),
                        "nodesVisited": snapshot["nodesVisited"],
                        "truncated": snapshot["truncated"],
                        "queryAmbiguous": snapshot["queryAmbiguous"],
                    });
                    if browser_window_node_count > 0 {
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
                let (session_bus_address, atspi_bus_address) = {
                    let guard = desktop.session.lock().unwrap();
                    let session = guard.as_ref().unwrap();
                    let atspi_bus = fs::read_to_string(session.directory.join("atspi-bus.address"))
                        .unwrap_or_default();
                    (
                        session.session_bus_address.clone(),
                        atspi_bus.trim().to_owned(),
                    )
                };
                let browser_process = managed_browser_process_diagnostics(
                    &binary,
                    &browser_profile,
                    &expected_environment,
                );
                let browser_process_ids = matching_browser_process_ids(&binary, &browser_profile);
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
                let bus_diagnostics = private_desktop_bus_diagnostics(
                    &session_bus_address,
                    &atspi_bus_address,
                    &browser_process_ids,
                );
                return Err(format!(
                    "managed browser launch was accepted but no Chromium accessibility window appeared within 30 seconds: {}",
                    json!({
                        "managedBrowserProcess":browser_process,
                        "directExec":direct_exec,
                        "privateBus":bus_diagnostics,
                        "lastObservation":latest,
                        "launchStderrClasses":browser_startup_log_classes(&launch_stderr),
                        "chromeLogClasses":browser_startup_log_classes(&read_bounded_chrome_log(&browser_profile)),
                        "screenshotCaptured":screenshot_saved.is_some(),
                    })
                ));
            }
            if std::env::var_os("GLASS_DESKTOP_APP_LAUNCH_DIAGNOSTICS").as_deref()
                == Some(std::ffi::OsStr::new("1"))
            {
                eprintln!(
                    "managed browser AX diagnostics: {}",
                    json!({
                        "diagnostic": managed_browser_ax_diagnostics(&desktop, &binary, &browser_profile),
                        "lastObservation": latest,
                    })
                );
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
            let home_fixture_url = url::Url::from_file_path(&home_page)
                .map_err(|_| "could not form local browser fixture URL")?
                .to_string();
            let chapter_fixture_url = url::Url::from_file_path(&chapter_page)
                .map_err(|_| "could not form local browser fixture URL")?
                .to_string();
            let detail_fixture_url = url::Url::from_file_path(&detail_page)
                .map_err(|_| "could not form local browser fixture URL")?
                .to_string();
            let mut section_fixture = url::Url::parse(&detail_fixture_url)
                .map_err(|_| "could not form local browser fixture URL")?;
            section_fixture.set_fragment(Some("section"));
            let known_fixture_routes = [
                ("home", home_fixture_url.as_str()),
                ("chapter", chapter_fixture_url.as_str()),
                ("detail", detail_fixture_url.as_str()),
                ("section", section_fixture.as_str()),
            ];
            let address_evidence_path =
                std::env::var_os("GLASS_DESKTOP_APP_ADDRESS_EVIDENCE_PATH").map(PathBuf::from);
            let capture_address_evidence =
                |route: &str, expected: &str, snapshot: &Value| -> Result<(), String> {
                    if let Some(path) = address_evidence_path.as_deref() {
                        append_private_address_evidence(path, route, expected, snapshot)
                            .map_err(|_| "writing private address evidence failed".to_owned())?;
                    }
                    Ok(())
                };
            let read_expected_address = |route: &str,
                                         expected: &str,
                                         expected_window_title: &str|
             -> Result<String, String> {
                let observation = desktop
                    .observe_accessibility(Some(&session_id), Some("Chromium address"))
                    .map_err(|error| format!("observing {route} browser address: {error}"))?;
                capture_address_evidence(route, expected, &observation)?;
                let focused_title_matches = observation["focusedWindow"]["name"]
                    .as_str()
                    .is_some_and(|name| name.contains(expected_window_title));
                if observation["truncated"] != false
                    || observation["queryAmbiguous"] != false
                    || !observation["focusedWindow"]["application"]
                        .as_str()
                        .is_some_and(super::super::is_browser_application)
                {
                    return Err(format!(
                        "{route} browser address observation is incomplete: truncated={} queryAmbiguous={} focusedBrowser={} focusedTitleMatches={focused_title_matches}",
                        observation["truncated"],
                        observation["queryAmbiguous"],
                        observation["focusedWindow"]["application"]
                            .as_str()
                            .is_some_and(super::super::is_browser_application),
                    ));
                }
                let address = unique_browser_address_value(&observation);
                if let Some(address) =
                    address.filter(|value| address_matches_expected(value, expected))
                    && focused_title_matches
                {
                    Ok(address.to_owned())
                } else {
                    let observed_route = address
                        .and_then(|address| {
                            known_fixture_routes
                                .iter()
                                .find(|(_, known_url)| address == *known_url)
                                .map(|(route, _)| *route)
                        })
                        .unwrap_or("other");
                    Err(format!(
                        "{route} browser address did not match the expected fixture path: focusedBrowser=true focusedTitleMatches={focused_title_matches} truncated=false queryAmbiguous=false uniqueAddressObserved={} expectedPathMatched=false observedFixtureRoute={observed_route}",
                        address.is_some(),
                    ))
                }
            };
            desktop
                .input(
                    super::super::Lease::Agent,
                    &json!({"action":"key-chord","keys":["Control","L"]}),
                )
                .map_err(|error| format!("focusing Chrome's address bar: {}", error.message()))?;
            let address_after_focus = desktop
                .observe_accessibility(Some(&session_id), Some("Chromium address"))
                .map_err(|error| format!("observing the focused address field: {error}"))?;
            capture_address_evidence(
                "omnibox-after-control-l",
                &home_fixture_url,
                &address_after_focus,
            )?;
            if !browser_address_field_is_focused(&address_after_focus) {
                return Err(format!(
                    "Control+L did not focus one editable address field: focusedBrowser={} complete={} queryAmbiguous={}",
                    address_after_focus["focusedWindow"]["application"]
                        .as_str()
                        .is_some_and(super::super::is_browser_application),
                    address_after_focus["truncated"] == false,
                    address_after_focus["queryAmbiguous"],
                ));
            }
            desktop
                .input(
                    super::super::Lease::Agent,
                    &json!({"action":"text","text":home_fixture_url}),
                )
                .map_err(|error| format!("entering the local fixture URL: {}", error.message()))?;
            let address_after_typing = desktop
                .observe_accessibility(Some(&session_id), Some("Chromium address"))
                .map_err(|error| format!("observing the typed address field: {error}"))?;
            capture_address_evidence(
                "omnibox-after-text-before-enter",
                &home_fixture_url,
                &address_after_typing,
            )?;
            if !browser_address_field_is_focused(&address_after_typing)
                || !unique_browser_address_value(&address_after_typing)
                    .is_some_and(|value| address_matches_expected(value, &home_fixture_url))
            {
                return Err(format!(
                    "typed fixture URL was not present in the focused address field before Enter: focusedBrowser={} complete={} queryAmbiguous={} addressFieldFocused={} exactExpectedValueMatch={}",
                    address_after_typing["focusedWindow"]["application"]
                        .as_str()
                        .is_some_and(super::super::is_browser_application),
                    address_after_typing["truncated"] == false,
                    address_after_typing["queryAmbiguous"],
                    browser_address_field_is_focused(&address_after_typing),
                    unique_browser_address_value(&address_after_typing)
                        .is_some_and(|value| address_matches_expected(value, &home_fixture_url)),
                ));
            }
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
                match read_expected_address(
                    "home",
                    &home_fixture_url,
                    "Managed browser home fixture",
                ) {
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
                .observe_accessibility(
                    Some(&session_id),
                    Some("Chromium Observed chapter link link"),
                )
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
                    Some("Chromium Observed chapter link link"),
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
                            Some("Chromium Observed chapter link link"),
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
                match read_expected_address(
                    "chapter",
                    &chapter_fixture_url,
                    "Managed browser chapter fixture",
                ) {
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
                    Some("Chromium Observed detail link link"),
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
            let middle_click_deadline =
                std::time::Instant::now() + std::time::Duration::from_secs(5);
            let chapter_after_middle_click = loop {
                match read_expected_address(
                    "middle-click",
                    &chapter_fixture_url,
                    "Managed browser chapter fixture",
                ) {
                    Ok(address) => break address,
                    Err(error) if std::time::Instant::now() < middle_click_deadline => {
                        let _ = error;
                        std::thread::sleep(std::time::Duration::from_millis(150));
                    }
                    Err(error) => {
                        return Err(format!(
                            "middle-click did not preserve the chapter tab before switching: {error}"
                        ));
                    }
                }
            };
            desktop
                .input(
                    super::super::Lease::Agent,
                    &json!({"action":"key-chord","keys":["Control","Tab"]}),
                )
                .map_err(|error| {
                    format!(
                        "switching to the opened detail tab failed: {}",
                        error.message()
                    )
                })?;
            let detail_address_deadline =
                std::time::Instant::now() + std::time::Duration::from_secs(15);
            let detail_address = loop {
                match read_expected_address(
                    "detail",
                    &detail_fixture_url,
                    "Managed browser detail fixture",
                ) {
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
                    Some("Chromium Observed section link link"),
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
                let screenshot = std::env::temp_dir()
                    .join(format!("glass-browser-tabs-e2e-{}.jpg", std::process::id()));
                let screenshot_captured = if let Ok(Some((_, _, jpeg))) =
                    desktop.frame_after(0, std::time::Duration::from_secs(1))
                {
                    let mut options = fs::OpenOptions::new();
                    options
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .mode(0o600)
                        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
                    options
                        .open(&screenshot)
                        .and_then(|mut file| file.write_all(&jpeg))
                        .is_ok()
                } else {
                    false
                };
                return Err(format!(
                    "second tab did not reach the detail page: focusedBrowser={} truncated={} queryAmbiguous={} screenshotCaptured={screenshot_captured}",
                    detail_observation["focusedWindow"]["application"]
                        .as_str()
                        .is_some_and(super::super::is_browser_application),
                    detail_observation["truncated"],
                    detail_observation["queryAmbiguous"],
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
                    .observe_accessibility(
                        Some(&session_id),
                        Some("Chromium Detail section heading"),
                    )
                    .unwrap_or(Value::Null);
                let address_snapshot = desktop
                    .observe_accessibility(Some(&session_id), Some("Chromium address"))
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
                    .observe_accessibility(Some(&session_id), Some("Chromium address"))
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
                    .observe_accessibility(Some(&session_id), Some("Chromium address"))
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
                        "Chromium address":address,
                        "backObservation":back_observation,
                        "nodesVisited":diagnostic.as_ref().map(|snapshot| &snapshot["nodesVisited"]),
                        "truncated":diagnostic.as_ref().map(|snapshot| &snapshot["truncated"]),
                    })
                ));
            }
            let back_address = read_expected_address(
                "back",
                &detail_fixture_url,
                "Managed browser detail fixture",
            )?;
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
                    .observe_accessibility(Some(&session_id), Some("Chromium address"))
                    .unwrap_or(Value::Null);
                let address = unique_browser_address_value(&observation);
                let title_matches = observation["focusedWindow"]["name"]
                    .as_str()
                    .is_some_and(|name| name.contains("Managed browser detail fixture"));
                let section = desktop
                    .observe_accessibility(
                        Some(&session_id),
                        Some("Chromium Detail section heading"),
                    )
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
                    .observe_accessibility(Some(&session_id), Some("Chromium address"))
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
                        "Chromium address":address,
                        "forwardObservation":forward_observation,
                        "sectionNodes":forward_section["nodes"],
                        "nodesVisited":diagnostic.as_ref().map(|snapshot| &snapshot["nodesVisited"]),
                        "truncated":diagnostic.as_ref().map(|snapshot| &snapshot["truncated"]),
                    })
                ));
            }
            let forward_address = read_expected_address(
                "forward",
                section_fixture.as_str(),
                "Managed browser detail fixture",
            )?;
            desktop
                .input(
                    super::super::Lease::Agent,
                    &json!({"action":"key-chord","keys":["Control","Shift","Tab"]}),
                )
                .map_err(|error| {
                    format!(
                        "switching back to the chapter tab failed: {}",
                        error.message()
                    )
                })?;
            let original_tab_address = read_expected_address(
                "original",
                &chapter_fixture_url,
                "Managed browser chapter fixture",
            )?;
            Ok(json!({
                "desktopEntryId": browser_id,
                "browserWindowObserved": true,
                "homeToChapterObservedLink": true,
                "chapterToDetailObservedLinkOpenedNewTab": true,
                "chapterPreservedAfterMiddleClick": chapter_after_middle_click.contains("browser-chapter.html"),
                "sectionObservedLinkClicked": true,
                "backAndForwardVerifiedByUrl": true,
                "originalTabPreserved": true,
                "twoTabNavigationVerified": true,
                "deepNavigationVerified": true,
                "firstPageTitle": home_observation["focusedWindow"]["name"],
                "secondPageTitle": detail_observation["focusedWindow"]["name"],
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
            .observe_accessibility(Some(session_id), Some("Chromium address"))
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
            "Chromium The Rust Programming Language",
            "rust programming language",
        )?;
        let toggle_query = "Chromium Toggle Table of Contents label";
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
        let ownership_query = "Chromium Understanding Ownership link";
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
                desktop.observe_accessibility(Some(session_id), Some("Chromium address"))
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
                        Some("Chromium Understanding Ownership link"),
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
            "Understanding Ownership The Rust Programming Language",
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
                json!({"Chromium address":address_value,"navigation":ownership_navigation})
            ));
        }
        let ownership_toc = "Chromium Understanding Ownership Toggle Table of Contents label";
        let _ = pointer_click_fresh_target(
            desktop,
            session_id,
            &browser_window_id,
            ownership_toc,
            "Toggle Table of Contents",
            true,
        )?;
        let references_query = "Chromium References and Borrowing link";
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
                        Some("Chromium References and Borrowing link"),
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
            "Chromium References Borrowing The Rust Programming Language",
            "references and borrowing",
        )?;
        let references_address = read_browser_address(desktop, session_id, &browser_window_id)?;
        if !references_address.contains("/book/ch04-02-references-and-borrowing.html") {
            return Err(format!(
                "the observed References and Borrowing link did not reach its chapter: {references_address}"
            ));
        }
        let references_toc = "Chromium Toggle Table of Contents label";
        let _ = pointer_click_fresh_target(
            desktop,
            session_id,
            &browser_window_id,
            references_toc,
            "Toggle Table of Contents",
            true,
        )?;
        let slice_query = "Chromium The Slice Type link";
        let current_view_query = "Chromium References Borrowing heading";
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
            "Chromium The Slice Type The Rust Programming Language",
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
                Some("Chromium The Slice Type heading"),
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
            "ownershipNavigation":{"fallback":ownership_navigation["fallback"],"semanticClickRejected":ownership_navigation["semanticClickRejected"],"Chromium address":address_value},
            "referencesNavigation":{"Chromium address":references_address,"result":references_click},
            "sliceSearch":{"query":slice_query,"nodesVisited":slice_snapshot["nodesVisited"],"truncated":slice_snapshot["truncated"],"queryAmbiguous":slice_snapshot["queryAmbiguous"],"ownerWindowScoped":slice_snapshot["_testOwnerWindowScoped"],"matches":slice_matches},
            "sliceNavigation":{"Chromium address":slice_address,"scrolls":slice_scroll,"result":slice_click},
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
            .observe_accessibility_scoped(
                Some(session_id),
                Some("Chromium address"),
                Some(window_id),
            )
            .map_err(|error| format!("reading Chrome address: {error}"))?;
        if snapshot["truncated"] != false || snapshot["queryAmbiguous"] != false {
            return Err(format!(
                "address observation was incomplete or ambiguous: {snapshot}"
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
            _ => Err(format!("address observation was not unique: {snapshot}")),
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
                    .is_some_and(super::super::is_browser_application)
                    && window["name"]
                        .as_str()
                        .is_some_and(|name| name.to_ascii_lowercase().contains(expected_title))
            })
            .and_then(|window| window["windowId"].as_str())
            .map(ToOwned::to_owned)
            .ok_or_else(|| format!("visible Rust Book window had no selector: {snapshot}"))
    }

    fn unique_browser_address_value(snapshot: &Value) -> Option<&str> {
        unique_browser_address_field(snapshot).and_then(|field| field["value"].as_str())
    }

    fn unique_browser_address_field(snapshot: &Value) -> Option<&Value> {
        if snapshot["truncated"] != false || snapshot["queryAmbiguous"] != false {
            return None;
        }
        let mut address_fields = snapshot["nodes"].as_array()?.iter().filter(|node| {
            let role = node["role"].as_str().unwrap_or_default();
            let name = node["name"].as_str().unwrap_or_default();
            matches!(role, "entry" | "text entry" | "text box" | "combo box")
                && node["editable"] == true
                && name.to_ascii_lowercase().contains("address")
        });
        let field = address_fields.next()?;
        if address_fields.next().is_some() {
            return None;
        }
        Some(field)
    }

    fn browser_address_field_is_focused(snapshot: &Value) -> bool {
        unique_browser_address_field(snapshot)
            .and_then(|field| field["states"].as_array())
            .is_some_and(|states| states.iter().any(|state| state == "focused"))
    }

    fn private_address_evidence_record(route: &str, expected: &str, snapshot: &Value) -> Value {
        let field = unique_browser_address_field(snapshot);
        let address = field.and_then(|field| field["value"].as_str());
        let address_file_exists = address
            .and_then(|value| url::Url::parse(value).ok())
            .filter(|value| value.scheme() == "file")
            .and_then(|value| value.to_file_path().ok())
            .map(|path| path.is_file());
        let role = field
            .and_then(|field| field["role"].as_str())
            .map(|role| match role {
                "entry" => "entry",
                "text entry" | "text box" => "text_entry",
                "combo box" => "combo_box",
                _ => "other",
            })
            .unwrap_or("none");
        let title = snapshot["focusedWindow"]["name"]
            .as_str()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let focused_window_route = if title.contains("managed browser home fixture") {
            "home"
        } else if title.contains("managed browser chapter fixture") {
            "chapter"
        } else if title.contains("managed browser detail fixture") {
            "detail"
        } else {
            "other"
        };
        json!({
            "route": route,
            "focusedWindowRoute": focused_window_route,
            "focusedBrowser": snapshot["focusedWindow"]["application"]
                .as_str()
                .is_some_and(super::super::is_browser_application),
            "complete": snapshot["truncated"] == false,
            "queryAmbiguous": snapshot["queryAmbiguous"],
            "addressFieldCount": snapshot["nodes"].as_array().map(|nodes| {
                nodes.iter().filter(|node| {
                    let role = node["role"].as_str().unwrap_or_default();
                    let name = node["name"].as_str().unwrap_or_default();
                    matches!(role, "entry" | "text entry" | "text box" | "combo box")
                        && node["editable"] == true
                        && name.to_ascii_lowercase().contains("address")
                }).count()
            }),
            "addressFieldRole": role,
            "addressFieldEditable": field.is_some_and(|field| field["editable"] == true),
            "addressFieldFocused": browser_address_field_is_focused(snapshot),
            "addressValue": address,
            "addressFileExists": address_file_exists,
            "expectedAddressMatches": address.is_some_and(|actual| address_matches_expected(actual, expected)),
        })
    }

    fn append_private_address_evidence(
        path: &Path,
        route: &str,
        expected: &str,
        snapshot: &Value,
    ) -> std::io::Result<()> {
        let record = private_address_evidence_record(route, expected, snapshot);
        let bytes = serde_json::to_vec(&record).map_err(std::io::Error::other)?;
        let mut options = fs::OpenOptions::new();
        options
            .write(true)
            .create(true)
            .append(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        let mut file = options.open(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")
    }

    fn address_matches_expected(actual: &str, expected: &str) -> bool {
        url::Url::parse(actual)
            .ok()
            .zip(url::Url::parse(expected).ok())
            .is_some_and(|(actual, expected)| actual == expected)
    }

    fn write_test_application_entry(
        home: &Path,
        binary: &Path,
        profile: &Path,
        remote_debugging: bool,
        initial_url: Option<&str>,
    ) -> std::io::Result<()> {
        let applications = home.join(".local/share/applications");
        fs::create_dir_all(&applications)?;
        fs::set_permissions(&applications, fs::Permissions::from_mode(0o700))?;
        let binary = binary.display().to_string();
        let profile = profile.display().to_string();
        let remote_debugging = if remote_debugging {
            " --remote-debugging-address=127.0.0.1 --remote-debugging-port=0 --disable-background-networking --host-resolver-rules=\"MAP * ~NOTFOUND\""
        } else {
            ""
        };
        let initial_page = initial_url.map(|url| format!(" {url}")).unwrap_or_else(|| {
            if remote_debugging.is_empty() {
                String::new()
            } else {
                " about:blank".to_owned()
            }
        });
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
            "[Desktop Entry]\nType=Application\nName=Managed Chromium (Glass acceptance)\nExec=/usr/bin/env ACCESSIBILITY_ENABLED=1 {binary} --user-data-dir={profile} --no-first-run --no-default-browser-check --password-store=basic --force-renderer-accessibility=complete{remote_debugging} --disable-gpu --enable-logging=stderr --log-file={profile}/chrome.log{initial_page}\nStartupWMClass=chrome\nTerminal=false\nNoDisplay=false\n"
        )
    }

    fn write_private_diagnostic_screenshot(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        let parent = path.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
        let parent_mode = fs::metadata(parent)?.permissions().mode() & 0o777;
        if parent_mode & 0o077 != 0 {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        let mut options = fs::OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        let mut file = options.open(path)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(bytes)
    }

    fn devtools_current_route_category(
        port: u16,
        expected_url: &str,
    ) -> Result<&'static str, &'static str> {
        use std::{
            io::{Read, Write},
            net::{SocketAddr, TcpStream},
            time::Duration,
        };

        let address = SocketAddr::from(([127, 0, 0, 1], port));
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(500))
            .map_err(|_| "endpoint_unreachable")?;
        stream
            .set_read_timeout(Some(Duration::from_millis(500)))
            .map_err(|_| "endpoint_setup_failed")?;
        stream
            .set_write_timeout(Some(Duration::from_secs(1)))
            .map_err(|_| "endpoint_setup_failed")?;
        stream
            .write_all(
                b"GET /json/list HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n",
            )
            .map_err(|_| "http_request_failed")?;
        let mut response = Vec::new();
        let mut chunk = [0_u8; 4096];
        let read_deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < read_deadline && response.len() < 1_048_576 {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(size) => response.extend_from_slice(&chunk[..size]),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    break;
                }
                Err(_) => return Err("http_read_failed"),
            }
            if let Some(header_end) = response.windows(4).position(|window| window == b"\r\n\r\n") {
                let headers = std::str::from_utf8(&response[..header_end])
                    .map_err(|_| "http_headers_invalid")?;
                if let Some(content_length) = headers.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                }) && response.len() >= header_end + 4 + content_length
                {
                    break;
                }
                if response[header_end + 4..].ends_with(b"0\r\n\r\n") {
                    break;
                }
            }
        }
        let header_end = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .ok_or("http_headers_missing")?;
        let headers =
            std::str::from_utf8(&response[..header_end]).map_err(|_| "http_headers_invalid")?;
        if !headers
            .lines()
            .next()
            .ok_or("http_status_missing")?
            .contains(" 200 ")
        {
            return Err("http_status_unexpected");
        }
        let mut body = response[header_end + 4..].to_vec();
        if headers.lines().any(|line| {
            line.to_ascii_lowercase()
                .starts_with("transfer-encoding: chunked")
        }) {
            let mut decoded = Vec::new();
            let mut offset = 0;
            loop {
                let line_end = body[offset..]
                    .windows(2)
                    .position(|window| window == b"\r\n")
                    .ok_or("http_chunk_incomplete")?
                    + offset;
                let size = usize::from_str_radix(
                    std::str::from_utf8(&body[offset..line_end])
                        .map_err(|_| "http_chunk_invalid")?
                        .split(';')
                        .next()
                        .ok_or("http_chunk_invalid")?,
                    16,
                )
                .map_err(|_| "http_chunk_invalid")?;
                offset = line_end + 2;
                if size == 0 {
                    break;
                }
                let chunk_end = offset.checked_add(size).ok_or("http_chunk_invalid")?;
                decoded
                    .extend_from_slice(body.get(offset..chunk_end).ok_or("http_chunk_incomplete")?);
                offset = chunk_end.checked_add(2).ok_or("http_chunk_invalid")?;
                if body.get(chunk_end..offset).ok_or("http_chunk_incomplete")? != b"\r\n" {
                    return Err("http_chunk_invalid");
                }
            }
            body = decoded;
        }
        let targets = serde_json::from_slice::<Vec<Value>>(&body).map_err(|_| "json_invalid")?;
        let expected = url::Url::parse(expected_url).map_err(|_| "expected_url_invalid")?;
        let mut page_count = 0;
        let mut first_route = None;
        let mut fixture_count = 0;
        for target in targets.iter().filter(|target| target["type"] == "page") {
            page_count += 1;
            let actual = url::Url::parse(target["url"].as_str().ok_or("target_url_missing")?)
                .map_err(|_| "target_url_invalid")?;
            let route = if actual == expected {
                fixture_count += 1;
                "fixture"
            } else if actual.as_str() == "about:blank" {
                "blank"
            } else if actual.as_str() == "chrome://newtab/" {
                "new_tab"
            } else if actual.scheme() == "file" {
                "other_file"
            } else if actual.scheme() == "chrome-error" {
                "chrome_error"
            } else if matches!(actual.scheme(), "http" | "https") {
                "web_url"
            } else if actual.scheme() == "chrome" {
                "chrome_url"
            } else {
                "other_url"
            };
            first_route.get_or_insert(route);
        }
        if page_count == 0 {
            return Err("page_target_missing");
        }
        Ok(if fixture_count == 1 {
            "fixture"
        } else if fixture_count > 1 {
            "multiple_fixture_pages"
        } else if page_count > 1 {
            "multiple_pages_without_fixture"
        } else {
            first_route.unwrap_or("other_url")
        })
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
