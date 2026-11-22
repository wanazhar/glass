//! Typed `browser_*` tools for standalone Core.
//!
//! When `glass-browser` is installed and supports the agent tool profile,
//! its curated catalog is offered to the model as first-class tools next to
//! the `desktop_*` family, instead of leaving the model to discover the CLI
//! through `bash`. One `glass-browser --mcp --tool-profile agent` process per
//! workspace keeps the page, tabs and refs alive across calls.

use base64::Engine;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::ffi::CString;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const CALL_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_BODY_BYTES: usize = 60_000;

type Schema = (String, String, String);

static CATALOG: OnceLock<Vec<Schema>> = OnceLock::new();
static CLIENTS: OnceLock<Mutex<HashMap<PathBuf, Client>>> = OnceLock::new();

pub fn is_tool(name: &str) -> bool {
    name.len() > "browser_".len()
        && name.starts_with("browser_")
        && schemas().iter().any(|(tool, _, _)| tool == name)
}

/// The agent catalog advertised by the installed `glass-browser`, or nothing
/// when it is missing, disabled with `GC_BROWSER_TOOLS=0`, or too old.
pub fn schemas() -> Vec<Schema> {
    CATALOG
        .get_or_init(|| {
            if std::env::var("GC_BROWSER_TOOLS").is_ok_and(|value| value == "0") {
                return Vec::new();
            }
            probe().unwrap_or_default()
        })
        .clone()
}

fn binary() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    if let Some(path) = std::env::var_os("GC_BROWSER_BIN") {
        let path = PathBuf::from(path);
        if path.components().count() > 1 || path.is_absolute() || path.starts_with(".") {
            return Some(cwd.join(path));
        }
        return std::env::split_paths(&std::env::var_os("PATH")?)
            .map(|dir| cwd.join(dir).join(&path))
            .find(|path| path.is_file());
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| cwd.join(dir).join("glass-browser"))
        .find(|path| path.is_file())
}

fn command(workspace: Option<&Path>) -> Option<Command> {
    Some(command_for_binary(binary()?, workspace))
}

fn command_for_binary(binary: PathBuf, workspace: Option<&Path>) -> Command {
    let runtime = runtime(&binary);
    let mut command = Command::new(binary);
    if let Some(runtime) = &runtime {
        command.args(["--browser-runtime", runtime]);
    }
    if runtime.as_deref() == Some("chromium")
        && let Some(port) = free_port()
    {
        command.args(["--port", &port.to_string()]);
    }
    command
        .args(["--mcp", "--tool-profile", "agent", "--incognito"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(workspace) = workspace {
        command.current_dir(workspace);
    }
    command
}

/// `GC_BROWSER_RUNTIME` wins. Otherwise Linux hosts whose bubblewrap cannot
/// lock user namespaces (Ubuntu 22.04 ships 0.6) use Chromium, because the
/// native runtime would refuse network pages there. Local pages still work
/// without bubblewrap, so do not select an unavailable Chromium runtime.
fn runtime(binary: &Path) -> Option<String> {
    match std::env::var("GC_BROWSER_RUNTIME") {
        Ok(runtime) if !runtime.is_empty() => Some(runtime),
        _ => {
            if native_sandbox_available() {
                return None;
            }
            let report = doctor_report(binary, true).or_else(|| doctor_report(binary, false));
            fallback_runtime(false, report.as_ref())
        }
    }
}

fn doctor_report(binary: &Path, discovery_only: bool) -> Option<Value> {
    let mut command = Command::new(binary);
    command.args(["doctor", "--json"]);
    if discovery_only {
        command.arg("--browser-discovery-only");
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.take(1024 * 1024).read_to_end(&mut bytes);
        let _ = sender.send(result.ok().map(|_| bytes));
    });
    let expires = std::time::Instant::now() + PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let bytes = receiver
                    .recv_timeout(expires.saturating_duration_since(std::time::Instant::now()))
                    .ok()??;
                return serde_json::from_slice(&bytes).ok();
            }
            Ok(None) if std::time::Instant::now() < expires => {
                std::thread::sleep(Duration::from_millis(10))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

fn fallback_runtime(native_available: bool, report: Option<&Value>) -> Option<String> {
    (!native_available && report.is_some_and(|report| report["browser"]["chromeAvailable"] == true))
        .then(|| "chromium".to_owned())
}

#[cfg(target_os = "linux")]
fn native_sandbox_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let Some(bwrap) = std::env::var_os("PATH").and_then(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join("bwrap"))
                .find(|candidate| candidate.is_file())
        }) else {
            return false;
        };
        Command::new(bwrap)
            .arg("--help")
            .stdin(Stdio::null())
            .output()
            .is_ok_and(|output| {
                bwrap_help_supports_native(&String::from_utf8_lossy(
                    &[output.stdout, output.stderr].concat(),
                ))
            })
    })
}

#[cfg(not(target_os = "linux"))]
fn native_sandbox_available() -> bool {
    true
}

#[cfg(any(target_os = "linux", test))]
fn bwrap_help_supports_native(help: &str) -> bool {
    help.contains("--disable-userns") && help.contains("--assert-userns-disabled")
}

/// A per-process CDP port, so a Chrome the user already runs on 9222 never
/// blocks the agent's browser.
fn free_port() -> Option<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").ok()?;
    Some(listener.local_addr().ok()?.port())
}

fn probe() -> Option<Vec<Schema>> {
    let mut client = Client::spawn(None, PROBE_TIMEOUT)?;
    let tools = client
        .request("tools/list", json!({}), PROBE_TIMEOUT)
        .ok()?;
    let schemas = tools["tools"]
        .as_array()?
        .iter()
        .filter_map(|tool| {
            let name = tool["name"].as_str()?;
            name.starts_with("browser_").then(|| {
                (
                    name.to_owned(),
                    tool["description"].as_str().unwrap_or_default().to_owned(),
                    tool["inputSchema"].to_string(),
                )
            })
        })
        .collect::<Vec<_>>();
    (!schemas.is_empty()).then_some(schemas)
}

struct Client {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    next_id: u64,
}

impl Client {
    fn spawn(workspace: Option<&Path>, timeout: Duration) -> Option<Self> {
        let mut child = command(workspace)?.spawn().ok()?;
        let stdin = child.stdin.take()?;
        let stdout = child.stdout.take()?;
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            stdin: Some(stdin),
            lines,
            next_id: 0,
        };
        client
            .request(
                "initialize",
                json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"glass-core","version":env!("CARGO_PKG_VERSION")}}),
                timeout,
            )
            .ok()?;
        client
            .send(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .ok()?;
        Some(client)
    }

    fn send(&mut self, message: &Value) -> std::io::Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::BrokenPipe))?;
        writeln!(stdin, "{message}")?;
        stdin.flush()
    }

    fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .map_err(|error| format!("glass-browser is not accepting requests: {error}"))?;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            let line = match self.lines.recv_timeout(remaining) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => {
                    return Err(format!(
                        "glass-browser did not answer within {} s",
                        timeout.as_secs()
                    ));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("glass-browser exited".into());
                }
            };
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if message["id"] != id {
                continue;
            }
            if let Some(error) = message.get("error") {
                return Err(error["message"]
                    .as_str()
                    .unwrap_or("glass-browser request failed")
                    .to_owned());
            }
            return Ok(message["result"].clone());
        }
    }
}

impl Drop for Client {
    /// Closing stdin lets glass-browser shut its Chrome down; killing it
    /// outright would orphan the browser process.
    fn drop(&mut self) {
        drop(self.stdin.take());
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if !matches!(self.child.try_wait(), Ok(None)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn execute(call: &glass_mod_api::ToolCall, workspace: &Path) -> crate::tools::Outcome {
    let arguments = match serde_json::from_str::<Value>(&call.arguments) {
        Ok(value) if value.is_object() => value,
        Ok(Value::Null) => json!({}),
        Ok(_) => return crate::tools::Outcome::err("arguments must be a JSON object"),
        Err(error) => {
            return crate::tools::Outcome::err(format!("invalid JSON arguments: {error}"));
        }
    };
    let workspace = match workspace.canonicalize() {
        Ok(workspace) => workspace,
        Err(error) => {
            return crate::tools::Outcome::err(format!(
                "browser workspace is unavailable: {error}"
            ));
        }
    };
    let mut guard = CLIENTS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let reusable = guard
        .get_mut(&workspace)
        .is_some_and(|client| matches!(client.child.try_wait(), Ok(None)));
    if !reusable {
        guard.remove(&workspace);
        match Client::spawn(Some(&workspace), PROBE_TIMEOUT) {
            Some(client) => {
                guard.insert(workspace.clone(), client);
            }
            None => {
                return crate::tools::Outcome::err(
                    "glass-browser could not start. Check `glass-browser --help` with bash; on Linux hosts without a working native sandbox set GC_BROWSER_RUNTIME=chromium.",
                );
            }
        }
    }
    let client = guard.get_mut(&workspace).expect("client was just ensured");
    let result = client.request(
        "tools/call",
        json!({"name": call.name, "arguments": arguments}),
        CALL_TIMEOUT,
    );
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            guard.remove(&workspace);
            return crate::tools::Outcome::err(format!(
                "{error}. The browser session was reset; call browser_navigate again."
            ));
        }
    };
    drop(guard);
    let body = render(&result, &workspace);
    if result["isError"] == true {
        crate::tools::Outcome::err(body)
    } else {
        crate::tools::Outcome::ok(body)
    }
}

pub(crate) fn release_workspace(workspace: &Path) {
    let Ok(workspace) = workspace.canonicalize() else {
        return;
    };
    let client = CLIENTS.get().and_then(|clients| {
        clients
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .remove(&workspace)
    });
    drop(client);
}

fn render(result: &Value, workspace: &Path) -> String {
    let mut parts = Vec::new();
    for item in result["content"].as_array().into_iter().flatten() {
        match item["type"].as_str() {
            Some("text") => parts.push(item["text"].as_str().unwrap_or_default().to_owned()),
            Some("image") => parts.push(
                save_image(item, workspace)
                    .unwrap_or_else(|error| format!("Screenshot could not be saved: {error}")),
            ),
            _ => {}
        }
    }
    let mut body = parts.join("\n");
    if body.len() > MAX_BODY_BYTES {
        let mut end = MAX_BODY_BYTES;
        while !body.is_char_boundary(end) {
            end -= 1;
        }
        body.truncate(end);
        body.push_str("\n[truncated; use browser_snapshot with a region, or browser_get_text]");
    }
    body
}

fn save_image(item: &Value, workspace: &Path) -> Result<String, String> {
    let data = item["data"].as_str().ok_or("no image data")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|error| error.to_string())?;
    let extension = match item["mimeType"].as_str() {
        Some("image/jpeg") => "jpg",
        Some("image/webp") => "webp",
        _ => "png",
    };
    let root = workspace
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let mut directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&root)
        .map_err(|error| error.to_string())?;
    for component in [".gc-artifacts", "browser"] {
        directory = screenshot_subdirectory(&directory, component).map_err(|error| {
            format!("browser screenshot directory escapes or is unavailable: {error}")
        })?;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let filename = format!("browser-{stamp}-{}.{extension}", std::process::id());
    screenshot_file(&directory, &filename)
        .and_then(|mut file| file.write_all(&bytes))
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "Screenshot saved to `.gc-artifacts/browser/{filename}`. Use `view_image` with that path to see it; pixel positions are CSS pixels for browser_click_at."
    ))
}

fn screenshot_subdirectory(parent: &File, name: &str) -> std::io::Result<File> {
    let name = CString::new(name)?;
    // SAFETY: name is NUL-terminated and parent remains open for both calls.
    let created = unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) };
    if created < 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: openat returns an owned descriptor; no symlink is followed.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        // SAFETY: the successful openat descriptor is uniquely owned here.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

fn screenshot_file(directory: &File, name: &str) -> std::io::Result<File> {
    let name = CString::new(name)?;
    // SAFETY: name is NUL-terminated; directory stays open during openat.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        // SAFETY: the successful openat descriptor is uniquely owned here.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_browser_override_uses_path_not_workspace() {
        const MARKER: &str = "GLASS_BROWSER_PATH_TEST";
        if let Some(root) = std::env::var_os(MARKER) {
            let root = PathBuf::from(root);
            assert_eq!(binary(), Some(root.join("trusted/browser-command")));
            return;
        }
        let root = std::env::temp_dir().join(format!("gc-browser-path-{}", std::process::id()));
        std::fs::create_dir_all(root.join("trusted")).unwrap();
        std::fs::create_dir_all(root.join("workspace")).unwrap();
        for directory in ["trusted", "workspace"] {
            std::fs::write(root.join(directory).join("browser-command"), "fixture").unwrap();
        }
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "browser_tools::tests::bare_browser_override_uses_path_not_workspace",
            ])
            .env(MARKER, &root)
            .env("GC_BROWSER_BIN", "browser-command")
            .env("PATH", root.join("trusted"))
            .current_dir(root.join("workspace"))
            .status()
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert!(status.success());
    }

    #[test]
    fn workspace_clients_survive_switches_and_isolate_transport_failures() {
        use std::os::unix::fs::PermissionsExt;
        const MARKER: &str = "GLASS_WORKSPACE_BROWSER_CHILD";
        if std::env::var_os(MARKER).is_none() {
            let root =
                std::env::temp_dir().join(format!("gc-browser-workspaces-{}", std::process::id()));
            std::fs::create_dir_all(root.join("parent")).unwrap();
            std::fs::create_dir_all(root.join("teammate")).unwrap();
            let script = root.join("fake-browser");
            std::fs::write(
                &script,
                r#"#!/usr/bin/env python3
import json, sys
assert '--incognito' in sys.argv
count = 0
for line in sys.stdin:
    request = json.loads(line)
    if 'id' not in request:
        continue
    response = {'jsonrpc': '2.0', 'id': request['id']}
    if request['method'] == 'tools/call':
        count += 1
        if request['params']['arguments'].get('fail'):
            response['error'] = {'code': -1, 'message': 'mock transport failure'}
        else:
            response['result'] = {'content': [{'type': 'text', 'text': str(count)}]}
    else:
        response['result'] = {}
    print(json.dumps(response), flush=True)
"#,
            )
            .unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
            let status = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "browser_tools::tests::workspace_clients_survive_switches_and_isolate_transport_failures", "--nocapture"])
                .current_dir(&root)
                .env(MARKER, "1")
                .env("GC_BROWSER_BIN", "./fake-browser")
                .status().unwrap();
            std::fs::remove_dir_all(root).unwrap();
            assert!(status.success());
            return;
        }
        let call = |workspace: &str, arguments: &str| {
            execute(
                &glass_mod_api::ToolCall {
                    id: "test".into(),
                    name: "browser_snapshot".into(),
                    arguments: arguments.into(),
                },
                Path::new(workspace),
            )
        };
        assert_eq!(call("parent", "{}").body, "1");
        assert_eq!(call("teammate", "{}").body, "1");
        assert_eq!(call("parent", "{}").body, "2");
        assert!(call("missing", "{}").body.starts_with("error:"));
        assert!(
            call("parent", r#"{"fail":true}"#)
                .body
                .starts_with("error:")
        );
        assert_eq!(call("teammate", "{}").body, "2");
        assert_eq!(call("parent", "{}").body, "1");
        release_workspace(Path::new("teammate"));
        assert_eq!(call("teammate", "{}").body, "1");
        assert_eq!(call("parent", "{}").body, "2");
    }

    #[test]
    fn screenshot_descriptors_survive_directory_symlink_substitution() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("gc-browser-dirfd-{}", std::process::id()));
        std::fs::create_dir_all(root.join("outside")).unwrap();
        let parent = File::open(&root).unwrap();
        let directory = screenshot_subdirectory(&parent, "browser").unwrap();
        std::fs::rename(root.join("browser"), root.join("original")).unwrap();
        std::os::unix::fs::symlink(root.join("outside"), root.join("browser")).unwrap();
        let mut image = screenshot_file(&directory, "image.png").unwrap();
        image.write_all(b"private").unwrap();
        assert_eq!(
            image.metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::read(root.join("original/image.png")).unwrap(),
            b"private"
        );
        assert_eq!(std::fs::read_dir(root.join("outside")).unwrap().count(), 0);
        assert!(screenshot_file(&directory, "image.png").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn browser_screenshot_artifacts_reject_symlink_escapes_and_do_not_clobber_siblings() {
        let root = std::env::temp_dir().join(format!("gc-browser-confine-{}", std::process::id()));
        let workspace = root.join("workspace");
        let outside = root.join("outside");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::create_dir(&outside).unwrap();
        let image = json!({"type":"image","mimeType":"image/png","data":"aW1hZ2U="});
        let artifacts = workspace.join(".gc-artifacts");
        std::os::unix::fs::symlink(&outside, &artifacts).unwrap();
        assert!(
            save_image(&image, &workspace)
                .unwrap_err()
                .contains("escapes")
        );
        assert!(!outside.join("browser").exists());
        std::fs::remove_file(&artifacts).unwrap();
        std::fs::create_dir(&artifacts).unwrap();
        std::os::unix::fs::symlink(&outside, artifacts.join("browser")).unwrap();
        assert!(
            save_image(&image, &workspace)
                .unwrap_err()
                .contains("escapes")
        );
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
        std::fs::remove_file(artifacts.join("browser")).unwrap();
        assert!(save_image(&json!({"data":"%%%"}), &workspace).is_err());
        assert!(save_image(&json!({}), &workspace).is_err());
        let first = save_image(&image, &workspace).unwrap();
        let second = save_image(&image, &workspace).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            std::fs::read_dir(artifacts.join("browser"))
                .unwrap()
                .count(),
            2
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn old_bubblewrap_selects_the_chromium_runtime() {
        assert_eq!(fallback_runtime(false, None), None);
        assert_eq!(
            fallback_runtime(false, Some(&json!({"browser":{"chromeAvailable":false}}))),
            None
        );
        assert_eq!(
            fallback_runtime(false, Some(&json!({"browser":{"chromeAvailable":true}}))),
            Some("chromium".to_owned())
        );
        assert_eq!(
            fallback_runtime(true, Some(&json!({"browser":{"chromeAvailable":true}}))),
            None
        );
        assert!(!bwrap_help_supports_native(
            "usage: bwrap [OPTIONS...] [--] COMMAND [ARGS...]\n    --unshare-user"
        ));
        assert!(bwrap_help_supports_native(
            "    --disable-userns\n    --assert-userns-disabled"
        ));
    }

    #[test]
    fn mixed_version_discovery_and_native_ports() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("gc-browser-version-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let binary = root.join("browser");
        std::fs::write(&binary, "#!/bin/sh\ncase \"$*\" in *--browser-discovery-only*) exit 2;; esac\nprintf '%s' '{\"browser\":{\"chromeAvailable\":true}}'\n").unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(doctor_report(&binary, true).is_none());
        assert_eq!(
            fallback_runtime(false, doctor_report(&binary, false).as_ref()),
            Some("chromium".into())
        );
        let marker = "GLASS_TEST_BROWSER_PORT";
        if let Ok(runtime) = std::env::var(marker) {
            let command = command_for_binary(binary, None);
            let args: Vec<_> = command
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect();
            assert_eq!(
                args.iter().any(|arg| arg == "--port"),
                runtime == "chromium"
            );
            assert!(args.iter().any(|arg| arg == "--incognito"));
            return;
        }
        for runtime in ["native", "chromium"] {
            assert!(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "browser_tools::tests::mixed_version_discovery_and_native_ports"
                    ])
                    .env(marker, runtime)
                    .env("GC_BROWSER_RUNTIME", runtime)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn render_saves_images_and_keeps_text_order() {
        let workspace =
            std::env::temp_dir().join(format!("gc-browser-tools-{}", std::process::id()));
        std::fs::create_dir_all(&workspace).unwrap();
        let png = base64::engine::general_purpose::STANDARD.encode(b"\x89PNG\r\n\x1a\n");
        let body = render(
            &json!({"content":[{"type":"text","text":"{\"width\":800}"},{"type":"image","data":png,"mimeType":"image/png"}]}),
            &workspace,
        );
        let mut lines = body.lines();
        assert_eq!(lines.next(), Some("{\"width\":800}"));
        let saved = lines.next().unwrap();
        assert!(saved.contains(".gc-artifacts/browser/browser-"), "{saved}");
        assert!(saved.contains("view_image"), "{saved}");
        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[test]
    fn render_bounds_large_text() {
        let body = render(
            &json!({"content":[{"type":"text","text":"é".repeat(MAX_BODY_BYTES)}]}),
            Path::new("."),
        );
        assert!(body.len() < MAX_BODY_BYTES + 100);
        assert!(body.ends_with("browser_get_text]"));
    }

    /// Opt-in: `GLASS_E2E=1 GC_BROWSER_BIN=target/debug/glass-browser
    /// GC_BROWSER_RUNTIME=chromium cargo test -p glass-core browser_tools`.
    #[test]
    fn typed_browser_tools_drive_a_real_page() {
        if std::env::var("GLASS_E2E").as_deref() != Ok("1")
            || std::env::var_os("GC_BROWSER_BIN").is_none()
        {
            return;
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            const PAGE: &str = "<!doctype html><title>Core bridge</title><main><h1>Orders</h1><label>Email <input id=email></label><button onclick=\"document.querySelector('h1').textContent='Saved '+document.querySelector('#email').value\">Save</button></main>";
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let mut buffer = [0u8; 2048];
                let _ = std::io::Read::read(&mut stream, &mut buffer);
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{PAGE}",
                    PAGE.len()
                );
            }
        });
        let names = schemas()
            .into_iter()
            .map(|(name, _, _)| name)
            .collect::<Vec<_>>();
        assert_eq!(names.len(), 19, "{names:?}");
        assert!(is_tool("browser_navigate") && !is_tool("browser_unknown"));
        let workspace = std::env::temp_dir().join(format!("gc-browser-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&workspace).unwrap();
        let call = |name: &str, arguments: Value| {
            execute(
                &glass_mod_api::ToolCall {
                    id: name.into(),
                    name: name.into(),
                    arguments: arguments.to_string(),
                },
                &workspace,
            )
        };
        let outcome = call(
            "browser_navigate",
            json!({"url": format!("http://{address}/")}),
        );
        assert!(outcome.exit_code == Some(0), "{}", outcome.body);
        let snapshot = call("browser_snapshot", json!({}));
        assert!(snapshot.body.contains("Orders"), "{}", snapshot.body);
        let typed = call(
            "browser_type",
            json!({"target":"name=Email","text":"ada@example.com"}),
        );
        assert!(typed.exit_code == Some(0), "{}", typed.body);
        let clicked = call("browser_click", json!({"target":"role=button;name=Save"}));
        assert!(clicked.exit_code == Some(0), "{}", clicked.body);
        let waited = call("browser_wait_for", json!({"text":"Saved ada@example.com"}));
        assert!(waited.exit_code == Some(0), "{}", waited.body);
        let shot = call("browser_screenshot", json!({}));
        assert!(
            shot.body.contains(".gc-artifacts/browser/"),
            "{}",
            shot.body
        );
        assert!(
            std::fs::read_dir(workspace.join(".gc-artifacts/browser"))
                .unwrap()
                .count()
                >= 1
        );
        let missing = call(
            "browser_click",
            json!({"target":"role=button;name=Delete everything"}),
        );
        assert!(missing.exit_code != Some(0));
        assert!(missing.body.contains("Next:"), "{}", missing.body);
        let _ = std::fs::remove_dir_all(&workspace);
    }
}
