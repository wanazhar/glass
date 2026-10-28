//! A Connect-owned graphical session shared by the browser and GC tools.
//!
//! The session is deliberately isolated from the host login desktop. It is
//! started only by an explicit Connect request and remains alive when a viewer
//! disconnects.

use base64::Engine;
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::{FileTypeExt, OpenOptionsExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    process::Child,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

mod capture;
mod input;
mod process;
use capture::Frames;

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 800;
const MAX_FRAME: usize = 4 * 1024 * 1024;
const FRAME_STALE_AFTER: Duration = Duration::from_millis(1800);
const LEASE_SOCKET: &str = "desktop-agent.sock";
const AGENT_MESSAGE_MAX: u64 = 6 * 1024 * 1024;
static STOPPING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn arm_shutdown() {
    unsafe extern "C" fn signal_shutdown(_: std::ffi::c_int) {
        STOPPING.store(true, std::sync::atomic::Ordering::Release);
    }
    unsafe {
        libc::signal(
            libc::SIGTERM,
            signal_shutdown as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGINT,
            signal_shutdown as *const () as libc::sighandler_t,
        );
    }
}

pub fn stopping() -> bool {
    STOPPING.load(std::sync::atomic::Ordering::Acquire)
}

pub fn service_running() -> bool {
    UnixStream::connect(crate::config::data_dir().join(LEASE_SOCKET)).is_ok()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lease {
    Agent,
    Human,
}

impl Lease {
    fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Human => "human",
        }
    }
}

struct ManagedSession {
    id: String,
    display: String,
    auth_cookie: [u8; 16],
    directory: PathBuf,
    xvfb: Child,
    desktop: Child,
    capture: Child,
    frames: Arc<Mutex<Frames>>,
    lease: Lease,
    held_buttons: HashSet<u8>,
    held_keys: HashSet<String>,
    scroll_x_remainder: f64,
    scroll_y_remainder: f64,
}

impl Drop for ManagedSession {
    fn drop(&mut self) {
        capture::stop_child(&mut self.capture);
        capture::stop_child(&mut self.desktop);
        capture::stop_child(&mut self.xvfb);
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// Owns at most one isolated desktop session and serializes lease changes with
/// every input event. Connect and the local GC tool bridge share this object.
pub struct Desktop {
    workspace: PathBuf,
    root: PathBuf,
    session: Mutex<Option<ManagedSession>>,
}

impl Desktop {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            root: crate::config::data_dir().join("desktop"),
            session: Mutex::new(None),
        }
    }

    pub fn status(&self) -> Value {
        let session = self.session.lock().unwrap();
        let Some(session) = session.as_ref() else {
            return json!({
                "available": false,
                "sessionId": null,
                "mode": null,
                "displayId": null,
                "workspaceRoot": self.workspace,
                "lease": "agent",
                "width": null,
                "height": null,
                "frameSeq": null,
                "frameAgeMs": null,
                "error": null
            });
        };
        let frames = session.frames.lock().unwrap();
        let age = frames
            .current
            .as_ref()
            .map(|frame| frame.captured_at.elapsed());
        let fresh = age.is_some_and(|age| age <= FRAME_STALE_AFTER);
        json!({
            "available": fresh,
            "sessionId": session.id,
            "mode": "managedVirtualSession",
            "displayId": session.display,
            "workspaceRoot": self.workspace,
            "lease": session.lease.as_str(),
            "width": WIDTH,
            "height": HEIGHT,
            "frameSeq": frames.current.as_ref().map(|frame| frame.seq),
            "frameAgeMs": age.map(|age| age.as_millis()),
            "error": if fresh { None } else { frames.error.as_deref().or(Some("Desktop frame source is stale")) }
        })
    }

    pub fn start(&self) -> Result<Value, String> {
        if stopping() {
            return Err("Connect desktop service is stopping".into());
        }
        let mut guard = self
            .session
            .lock()
            .map_err(|_| "Desktop state unavailable")?;
        if guard.is_some() {
            return Err("A managed desktop session is already running".into());
        }
        let session = self.start_session()?;
        let id = session.id.clone();
        let display = session.display.clone();
        *guard = Some(session);
        Ok(
            json!({"started":true,"sessionId":id,"mode":"managedVirtualSession","displayId":display,"width":WIDTH,"height":HEIGHT}),
        )
    }

    fn start_session(&self) -> Result<ManagedSession, String> {
        process::start_session(&self.workspace, &self.root)
    }

    pub fn stop(&self) -> Result<Value, String> {
        let mut guard = self
            .session
            .lock()
            .map_err(|_| "Desktop state unavailable")?;
        if guard.take().is_some() {
            Ok(json!({"stopped":true}))
        } else {
            Err("No managed desktop session is running".into())
        }
    }

    pub fn set_lease(&self, action: &str) -> Result<Value, String> {
        let mut guard = self
            .session
            .lock()
            .map_err(|_| "Desktop state unavailable")?;
        let session = guard
            .as_mut()
            .ok_or("No managed desktop session is running")?;
        match action {
            "take-control" => {
                input::release_held(
                    &session.display,
                    &session.auth_cookie,
                    &session.held_buttons,
                    &session.held_keys,
                )
                .map_err(|error| error.message().to_owned())?;
                session.held_buttons.clear();
                session.held_keys.clear();
                session.scroll_x_remainder = 0.0;
                session.scroll_y_remainder = 0.0;
                session.lease = Lease::Human;
            }
            "return-to-agent" => {
                input::release_held(
                    &session.display,
                    &session.auth_cookie,
                    &session.held_buttons,
                    &session.held_keys,
                )
                .map_err(|error| error.message().to_owned())?;
                session.held_buttons.clear();
                session.held_keys.clear();
                session.scroll_x_remainder = 0.0;
                session.scroll_y_remainder = 0.0;
                session.lease = Lease::Agent;
            }
            _ => return Err("action must be take-control or return-to-agent".into()),
        }
        Ok(json!({"lease":session.lease.as_str()}))
    }

    pub fn input(&self, role: Lease, value: &Value) -> Result<Value, InputError> {
        let mut guard = self.session.lock().map_err(|_| InputError::Unavailable)?;
        let session = guard.as_mut().ok_or(InputError::NoSession)?;
        require_lease(role, session.lease)?;
        let action = value
            .get("action")
            .and_then(Value::as_str)
            .ok_or(InputError::Invalid("Missing action"))?;
        if action == "scroll" {
            input::coordinates(value)?;
            let (delta_x, next_x) = scroll_ticks(
                value.get("deltaX").and_then(Value::as_f64).unwrap_or(0.0),
                session.scroll_x_remainder,
            )?;
            let (delta_y, next_y) = scroll_ticks(
                value.get("deltaY").and_then(Value::as_f64).unwrap_or(0.0),
                session.scroll_y_remainder,
            )?;
            if delta_x == 0 && delta_y == 0 {
                session.scroll_x_remainder = next_x;
                session.scroll_y_remainder = next_y;
                return Ok(json!({"sent":true,"lease":session.lease.as_str()}));
            }
            let normalized = json!({"action":"scroll","x":value["x"],"y":value["y"],"deltaX":delta_x,"deltaY":delta_y});
            input::inject(&session.display, &session.auth_cookie, action, &normalized)?;
            session.scroll_x_remainder = next_x;
            session.scroll_y_remainder = next_y;
        } else {
            input::inject(&session.display, &session.auth_cookie, action, value)?;
        }
        match action {
            "pointer-button" => {
                let button = input::button_id(value["button"].as_str().unwrap_or_default())
                    .ok_or(InputError::Invalid("button must be left, middle, or right"))?;
                if value["pressed"] == true {
                    session.held_buttons.insert(button);
                } else {
                    session.held_buttons.remove(&button);
                }
            }
            "key" => {
                let key = value["key"]
                    .as_str()
                    .ok_or(InputError::Invalid("key must be a string"))?;
                if value["pressed"] == true {
                    session.held_keys.insert(key.to_owned());
                } else {
                    session.held_keys.remove(key);
                }
            }
            _ => {}
        }
        Ok(json!({"sent":true,"lease":session.lease.as_str()}))
    }

    pub fn frame_after(
        &self,
        after: u64,
        timeout: Duration,
    ) -> Result<Option<(u64, u64, Vec<u8>)>, String> {
        let guard = self
            .session
            .lock()
            .map_err(|_| "Desktop state unavailable")?;
        let session = guard
            .as_ref()
            .ok_or("No managed desktop session is running")?;
        let deadline = Instant::now() + timeout;
        loop {
            if STOPPING.load(std::sync::atomic::Ordering::Acquire) {
                return Err("Connect desktop service is stopping".into());
            }
            let frames = session
                .frames
                .lock()
                .map_err(|_| "Desktop frame state unavailable")?;
            if let Some(frame) = frames.current.as_ref().filter(|frame| frame.seq > after) {
                if frame.captured_at.elapsed() > FRAME_STALE_AFTER {
                    return Err("Desktop frame source is stale".into());
                }
                return Ok(Some((frame.seq, frame.since_start_ms, frame.jpeg.clone())));
            }
            if let Some(error) = frames.error.as_ref() {
                return Err(error.clone());
            }
            drop(frames);
            if Instant::now() >= deadline {
                return Ok(None);
            }
            thread::sleep(Duration::from_millis(40));
        }
    }

    /// Run the private, same-uid bridge used by Core's agent tools. It never
    /// listens on TCP and exposes only input and status operations.
    pub fn serve_agent_socket(self: Arc<Self>) -> Result<(), String> {
        let path = crate::config::data_dir().join(LEASE_SOCKET);
        std::fs::create_dir_all(crate::config::data_dir()).map_err(|e| e.to_string())?;
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            if !meta.file_type().is_socket() {
                return Err("Desktop agent socket path is occupied by a non-socket".into());
            }
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        let listener =
            UnixListener::bind(&path).map_err(|e| format!("Desktop agent socket: {e}"))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        while !STOPPING.load(std::sync::atomic::Ordering::Acquire) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let desktop = self.clone();
                    thread::spawn(move || {
                        let _ = handle_agent_request(stream, desktop);
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(30));
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(format!("Desktop agent socket stopped: {error}")),
            }
        }
        drop(listener);
        if std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_socket()) {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    pub fn agent_input(value: &Value) -> Result<Value, String> {
        Self::agent_request(&json!({"action":"input","value":value}))
    }

    fn agent_status() -> Result<Value, String> {
        Self::agent_request(&json!({"action":"status"}))
    }

    fn agent_frame() -> Result<Value, String> {
        Self::agent_request(&json!({"action":"frame"}))
    }

    fn agent_request(request: &Value) -> Result<Value, String> {
        let mut stream = UnixStream::connect(crate::config::data_dir().join(LEASE_SOCKET))
            .map_err(|e| format!("Connect desktop service is unavailable: {e}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut stream, request).map_err(|e| e.to_string())?;
        stream.write_all(b"\n").map_err(|e| e.to_string())?;
        let mut response = String::new();
        BufReader::new(stream)
            .take(AGENT_MESSAGE_MAX)
            .read_to_string(&mut response)
            .map_err(|e| e.to_string())?;
        let result: Value = serde_json::from_str(&response)
            .map_err(|_| "Invalid desktop service response".to_string())?;
        if result.get("available").is_none()
            && let Some(error) = result.get("error").and_then(Value::as_str)
        {
            Err(error.to_owned())
        } else {
            Ok(result)
        }
    }
}

fn scroll_ticks(delta_pixels: f64, remainder: f64) -> Result<(i64, f64), InputError> {
    if !delta_pixels.is_finite() || delta_pixels.abs() > 800.0 {
        return Err(InputError::Invalid(
            "scroll delta must be a finite value between -800 and 800 pixels",
        ));
    }
    let accumulated = delta_pixels / 40.0 + remainder;
    let ticks = accumulated.trunc().clamp(-20.0, 20.0) as i64;
    let next = accumulated - ticks as f64;
    Ok((ticks, next))
}

fn require_lease(caller: Lease, owner: Lease) -> Result<(), InputError> {
    if caller == owner {
        Ok(())
    } else {
        Err(InputError::LeaseConflict(owner.as_str()))
    }
}

pub fn is_tool(name: &str) -> bool {
    matches!(
        name,
        "desktop_status" | "desktop_screenshot" | "desktop_input"
    )
}

pub fn execute_tool(call: &glass_mod_api::ToolCall, workspace: &Path) -> crate::tools::Outcome {
    let args = match serde_json::from_str::<Value>(&call.arguments) {
        Ok(value) if value.is_object() => value,
        Ok(_) => return crate::tools::Outcome::err("arguments must be a JSON object"),
        Err(error) => {
            return crate::tools::Outcome::err(format!("invalid JSON arguments: {error}"));
        }
    };
    let result = match call.name.as_str() {
        "desktop_status" => Desktop::agent_status().map(|value| value.to_string()),
        "desktop_input" => Desktop::agent_input(&args).map(|value| value.to_string()),
        "desktop_screenshot" => {
            Desktop::agent_frame().and_then(|value| save_agent_frame(workspace, &value))
        }
        _ => Err("unknown desktop tool".into()),
    };
    match result {
        Ok(body) => crate::tools::Outcome::ok(body),
        Err(error) => crate::tools::Outcome::err(error),
    }
}

fn save_agent_frame(workspace: &Path, value: &Value) -> Result<String, String> {
    let encoded = value["jpeg"]
        .as_str()
        .ok_or("Desktop service returned no screenshot")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "Desktop service returned an invalid screenshot")?;
    if bytes.len() > MAX_FRAME
        || !bytes.starts_with(&[0xff, 0xd8])
        || !bytes.ends_with(&[0xff, 0xd9])
    {
        return Err("Desktop screenshot is invalid or exceeds its size limit".into());
    }
    let root = workspace
        .canonicalize()
        .map_err(|e| format!("Workspace unavailable: {e}"))?;
    let desktop_root = value["workspaceRoot"]
        .as_str()
        .map(Path::new)
        .ok_or("Connect desktop did not report its workspace")?
        .canonicalize()
        .map_err(|e| format!("Connect desktop workspace is unavailable: {e}"))?;
    if root != desktop_root {
        return Err("This GC thread and the Connect desktop use different workspace roots; screenshot not saved".into());
    }
    let artifacts = root.join(".gc-artifacts");
    create_private_directory(&artifacts)?;
    let directory = artifacts.join("desktop");
    create_private_directory(&directory)?;
    let session_id = value["sessionId"].as_str().unwrap_or("session");
    let seq = value["seq"]
        .as_u64()
        .ok_or("Desktop service returned no frame sequence")?;
    let filename = format!(
        "desktop-{}-{seq}.jpg",
        session_id
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .take(48)
            .collect::<String>()
    );
    let path = directory.join(&filename);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .map_err(|e| format!("Could not save desktop screenshot: {e}"))?;
    file.write_all(&bytes)
        .map_err(|e| format!("Could not save desktop screenshot: {e}"))?;
    file.sync_all()
        .map_err(|e| format!("Could not save desktop screenshot: {e}"))?;
    prune_agent_frames(&directory, &format!("desktop-{session_id}-"), &path);
    Ok(format!(
        "Fresh desktop frame {} saved to `.gc-artifacts/desktop/{filename}`. Use `view_image` with that relative path to inspect its pixels.",
        seq
    ))
}

fn prune_agent_frames(directory: &Path, prefix: &str, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut frames = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let path = entry.path();
            if !name.to_string_lossy().starts_with(prefix)
                || entry.file_type().ok()?.is_symlink()
                || !entry.file_type().ok()?.is_file()
            {
                return None;
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((path, modified))
        })
        .collect::<Vec<_>>();
    if frames.len() <= 8 {
        return;
    }
    frames.sort_by_key(|(_, modified)| *modified);
    let remove_count = frames.len().saturating_sub(8);
    for (path, _) in frames.into_iter().take(remove_count) {
        if path != keep {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn create_private_directory(path: &Path) -> Result<(), String> {
    let created = match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_dir() => false,
        Ok(_) => return Err("Desktop artifact path is not a directory".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(path)
                .map_err(|e| format!("Could not create desktop artifact directory: {e}"))?;
            true
        }
        Err(error) => {
            return Err(format!(
                "Could not inspect desktop artifact directory: {error}"
            ));
        }
    };
    if created {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("Could not secure desktop artifact directory: {e}"))?;
    }
    Ok(())
}

#[derive(Debug)]
pub enum InputError {
    Invalid(&'static str),
    Unavailable,
    NoSession,
    LeaseConflict(&'static str),
    Display(String),
}

impl InputError {
    pub fn message(&self) -> &str {
        match self {
            Self::Invalid(message) => message,
            Self::Unavailable => "Desktop state unavailable",
            Self::NoSession => "No managed desktop session is running",
            Self::LeaseConflict(_) => "Desktop input lease is held by the other participant",
            Self::Display(message) => message,
        }
    }

    pub fn lease(&self) -> Option<&'static str> {
        match self {
            Self::LeaseConflict(lease) => Some(lease),
            _ => None,
        }
    }
}

fn handle_agent_request(mut stream: UnixStream, desktop: Arc<Desktop>) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(stream.try_clone().map_err(|e| e.to_string())?)
        .take(16 * 1024)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let request: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
    let result = match request["action"].as_str() {
        Some("input") => desktop.input(Lease::Agent, &request["value"]).map_err(|error| error.message().to_owned()),
        Some("status") => Ok(desktop.status()),
        Some("frame") => desktop.frame_after(0, Duration::from_secs(2)).map(|frame| {
            match frame {
                Some((seq, captured, jpeg)) => json!({"sessionId":desktop.status()["sessionId"],"workspaceRoot":desktop.status()["workspaceRoot"],"seq":seq,"capturedAtMs":captured,"jpeg":base64::engine::general_purpose::STANDARD.encode(jpeg)}),
                None => json!({"error":"Desktop frame source did not produce a fresh frame"}),
            }
        }),
        _ => Err("Unsupported desktop agent operation".to_owned()),
    };
    let response = match result {
        Ok(value) => value,
        Err(error) => json!({"error":error}),
    };
    serde_json::to_writer(&mut stream, &response).map_err(|e| e.to_string())?;
    stream.write_all(b"\n").map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_mapping_bounds_to_managed_framebuffer() {
        assert_eq!(
            input::coordinates(&json!({"x":0.0,"y":0.0})).unwrap(),
            (0, 0)
        );
        assert_eq!(
            input::coordinates(&json!({"x":1.0,"y":1.0})).unwrap(),
            (1279, 799)
        );
        assert!(input::coordinates(&json!({"x":1.1,"y":0.5})).is_err());
    }

    #[test]
    fn text_input_rejects_unbounded_and_non_ascii_values() {
        assert!("hello".is_ascii());
        assert!(!"こんにちは".is_ascii());
        assert_eq!(input::bounded_delta(1000.0), 20);
        assert_eq!(input::bounded_delta(-1000.0), -20);
    }

    #[test]
    fn generated_display_does_not_select_login_display() {
        let id = "00000000000000000000000000000000";
        assert!(process::choose_display(id) >= 100);
    }

    #[test]
    fn input_is_rejected_when_the_other_participant_holds_the_lease() {
        assert!(require_lease(Lease::Agent, Lease::Agent).is_ok());
        assert!(matches!(
            require_lease(Lease::Agent, Lease::Human),
            Err(InputError::LeaseConflict("human"))
        ));
        assert!(matches!(
            require_lease(Lease::Human, Lease::Agent),
            Err(InputError::LeaseConflict("agent"))
        ));
    }

    #[test]
    fn scroll_pixels_accumulate_into_bounded_wheel_steps() {
        assert_eq!(scroll_ticks(20.0, 0.0).unwrap(), (0, 0.5));
        assert_eq!(scroll_ticks(20.0, 0.5).unwrap(), (1, 0.0));
        assert_eq!(scroll_ticks(-20.0, 0.0).unwrap(), (0, -0.5));
        assert_eq!(scroll_ticks(-20.0, -0.5).unwrap(), (-1, 0.0));
        assert_eq!(scroll_ticks(800.0, 0.0).unwrap(), (20, 0.0));
        assert!(scroll_ticks(800.1, 0.0).is_err());
        assert!(scroll_ticks(f64::NAN, 0.0).is_err());
    }
}
