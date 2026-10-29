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
        os::unix::fs::PermissionsExt,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);

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
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            let mut latest = Value::Null;
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
                        break;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
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
                "<!doctype html><html><head><title>Managed browser detail fixture</title></head><body><h1>Detail page</h1><a href=\"#section\">Observed section link</a><h2 id=\"section\">Detail section</h2><script>addEventListener('hashchange',()=>document.title=location.hash?'Managed browser section fixture':'Managed browser detail fixture')</script></body></html>",
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
                observation["nodes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|node| node["value"].as_str())
                    .find(|value| value.contains(expected))
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| {
                        format!("browser address did not contain {expected:?}: {observation}")
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
            let section_title_deadline =
                std::time::Instant::now() + std::time::Duration::from_secs(8);
            let mut section_title_observed = false;
            while std::time::Instant::now() < section_title_deadline {
                if let Ok(observation) = desktop.observe_accessibility(Some(&session_id), None)
                    && observation["focusedWindow"]["name"]
                        .as_str()
                        .is_some_and(|name| name.contains("Managed browser section fixture"))
                {
                    section_title_observed = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            if !section_title_observed {
                return Err("observed section link did not update the browser page title".into());
            }
            let section_address = read_expected_address("#section")?;
            desktop
                .input(
                    super::super::Lease::Agent,
                    &json!({"action":"key-chord","keys":["Alt","ArrowLeft"]}),
                )
                .map_err(|error| format!("browser Back input failed: {}", error.message()))?;
            let back_deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
            let mut back_verified = false;
            while std::time::Instant::now() < back_deadline {
                if let Ok(observation) = desktop.observe_accessibility(Some(&session_id), None)
                    && observation["focusedWindow"]["name"]
                        .as_str()
                        .is_some_and(|name| name.contains("Managed browser detail fixture"))
                {
                    back_verified = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            if !back_verified {
                return Err("browser Back did not restore the detail page".into());
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
            while std::time::Instant::now() < forward_deadline {
                if let Ok(observation) = desktop.observe_accessibility(Some(&session_id), None)
                    && observation["focusedWindow"]["name"]
                        .as_str()
                        .is_some_and(|name| name.contains("Managed browser section fixture"))
                {
                    forward_verified = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            if !forward_verified {
                return Err("browser Forward did not restore the section state".into());
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
        assert_eq!(
            report["deepNavigationVerified"], true,
            "navigation was not verified: {report}"
        );
        assert_eq!(
            report["originalTabPreserved"], true,
            "original tab was not preserved: {report}"
        );
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
            "[Desktop Entry]\nType=Application\nName=Managed Chromium (Glass acceptance)\nExec=/usr/bin/env ACCESSIBILITY_ENABLED=1 {binary} --user-data-dir={profile} --no-first-run --no-default-browser-check --password-store=basic --force-renderer-accessibility=complete\nStartupWMClass=chrome\nTerminal=false\nNoDisplay=false\n"
        )
    }
}
