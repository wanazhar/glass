//! A Connect-owned graphical session shared by the browser and GC tools.
//!
//! The session is deliberately isolated from the host login desktop. It is
//! started only by an explicit Connect request and remains alive when a viewer
//! disconnects.

use base64::Engine;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
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

mod accessibility;
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
    session_bus_address: String,
    accessibility_revision: u64,
    accessibility_targets: HashMap<String, accessibility::Target>,
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
    /// Serializes user-visible acts, lease changes, and session lifecycle.
    /// Accessibility observation does not hold this lock.
    control: Mutex<()>,
    session: Mutex<Option<ManagedSession>>,
}

impl Desktop {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            root: crate::config::data_dir().join("desktop"),
            control: Mutex::new(()),
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
                "frameTimeMs": null,
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
            "frameTimeMs": frames.current.as_ref().map(|frame| frame.since_start_ms),
            "error": if fresh { None } else { frames.error.as_deref().or(Some("Desktop frame source is stale")) }
        })
    }

    pub fn start(&self) -> Result<Value, String> {
        if stopping() {
            return Err("Connect desktop service is stopping".into());
        }
        let _control = self
            .control
            .lock()
            .map_err(|_| "Desktop control unavailable")?;
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
        let _control = self
            .control
            .lock()
            .map_err(|_| "Desktop control unavailable")?;
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
        let _control = self
            .control
            .lock()
            .map_err(|_| "Desktop control unavailable")?;
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
                session.accessibility_revision = session.accessibility_revision.saturating_add(1);
                session.accessibility_targets.clear();
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
                session.accessibility_revision = session.accessibility_revision.saturating_add(1);
                session.accessibility_targets.clear();
            }
            _ => return Err("action must be take-control or return-to-agent".into()),
        }
        Ok(json!({"lease":session.lease.as_str()}))
    }

    pub fn input(&self, role: Lease, value: &Value) -> Result<Value, InputError> {
        let _control = self.control.lock().map_err(|_| InputError::Unavailable)?;
        let mut guard = self.session.lock().map_err(|_| InputError::Unavailable)?;
        let session = guard.as_mut().ok_or(InputError::NoSession)?;
        require_lease(role, session.lease)?;
        let action = value
            .get("action")
            .and_then(Value::as_str)
            .ok_or(InputError::Invalid("Missing action"))?;
        if action == "text" {
            let text = value
                .get("text")
                .and_then(Value::as_str)
                .ok_or(InputError::Invalid("text must be a string"))?;
            if unicode_text_conflicts_with_held_modifiers(text, &session.held_keys) {
                return Err(InputError::Invalid(
                    "release Control and Shift before entering Unicode text",
                ));
            }
        }
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
        session.accessibility_revision = session.accessibility_revision.saturating_add(1);
        session.accessibility_targets.clear();
        Ok(json!({"sent":true,"lease":session.lease.as_str()}))
    }

    pub fn observe_accessibility(
        &self,
        expected_session_id: Option<&str>,
        query: Option<&str>,
    ) -> Result<Value, String> {
        let (session_id, session_bus_address, revision) = {
            let mut guard = self
                .session
                .lock()
                .map_err(|_| "Desktop state unavailable")?;
            let session = guard
                .as_mut()
                .ok_or("No managed desktop session is running")?;
            if expected_session_id.is_some_and(|expected| expected != session.id) {
                return Err(
                    "The managed desktop session changed; call desktop_status and observe again"
                        .into(),
                );
            }
            session.accessibility_revision = session.accessibility_revision.saturating_add(1);
            session.accessibility_targets.clear();
            (
                session.id.clone(),
                session.session_bus_address.clone(),
                session.accessibility_revision,
            )
        };
        // D-Bus can take up to two seconds. Keep the manager mutex free so
        // frame/status requests and human input are not held behind AT-SPI.
        let snapshot = accessibility::observe(&session_bus_address, &session_id, revision, query)?;
        let mut guard = self
            .session
            .lock()
            .map_err(|_| "Desktop state unavailable")?;
        let session = guard
            .as_mut()
            .filter(|session| {
                session.id == session_id && session.accessibility_revision == revision
            })
            .ok_or("Accessibility observation changed while it was running; observe again")?;
        session.accessibility_targets = snapshot.targets;
        Ok(snapshot.value)
    }

    pub fn act_accessibility(&self, value: &Value) -> Result<Value, String> {
        let control = self
            .control
            .lock()
            .map_err(|_| "Desktop control unavailable")?;
        let (session_id, session_bus_address, revision, target, action, text) = {
            let mut guard = self
                .session
                .lock()
                .map_err(|_| "Desktop state unavailable")?;
            let session = guard
                .as_mut()
                .ok_or("No managed desktop session is running")?;
            let caller_session = value
                .get("sessionId")
                .and_then(Value::as_str)
                .ok_or("sessionId is required from desktop_observe")?;
            if caller_session != session.id {
                return Err(
                    "The managed desktop session changed; call desktop_status and observe again"
                        .into(),
                );
            }
            require_lease(Lease::Agent, session.lease)
                .map_err(|error| error.message().to_owned())?;
            let supplied_revision = value
                .get("revision")
                .and_then(Value::as_u64)
                .ok_or("revision is required from desktop_observe")?;
            if supplied_revision != session.accessibility_revision {
                return Err("Accessibility target is stale; call desktop_observe again".into());
            }
            let target_id = value
                .get("targetId")
                .and_then(Value::as_str)
                .ok_or("targetId is required from desktop_observe")?;
            let target = session
                .accessibility_targets
                .get(target_id)
                .cloned()
                .ok_or("Accessibility target is stale; call desktop_observe again")?;
            let action = value
                .get("action")
                .and_then(Value::as_str)
                .ok_or("action is required")?
                .to_owned();
            let text = value.get("text").and_then(Value::as_str).map(str::to_owned);
            // Reserve a new revision so every target from the old tree is
            // stale as soon as this action starts.
            session.accessibility_revision = session.accessibility_revision.saturating_add(1);
            session.accessibility_targets.clear();
            (
                session.id.clone(),
                session.session_bus_address.clone(),
                session.accessibility_revision,
                target,
                action,
                text,
            )
        };
        let result = accessibility::act(&session_bus_address, &target, &action, text.as_deref());
        drop(control);
        // Refresh without holding manager/control locks. Concurrent input or
        // observation advances the revision and prevents a stale commit.
        let observation = accessibility::observe(&session_bus_address, &session_id, revision, None);
        let mut guard = self
            .session
            .lock()
            .map_err(|_| "Desktop state unavailable")?;
        let current = guard.as_mut().filter(|session| {
            session.id == session_id && session.accessibility_revision == revision
        });
        let mut result = result?;
        if let (Some(session), Ok(snapshot)) = (current, observation) {
            session.accessibility_targets = snapshot.targets;
            if let Some(result) = result.as_object_mut() {
                result.insert("observation".into(), snapshot.value);
            }
        } else if let Some(result) = result.as_object_mut() {
            result.insert(
                "observationUnavailable".into(),
                json!("The desktop changed during this action. Call desktop_observe before continuing."),
            );
        }
        Ok(result)
    }

    pub fn frame_after(
        &self,
        after: u64,
        timeout: Duration,
    ) -> Result<Option<(u64, u64, Vec<u8>)>, String> {
        let (session_id, frame_state) = {
            let guard = self
                .session
                .lock()
                .map_err(|_| "Desktop state unavailable")?;
            let session = guard
                .as_ref()
                .ok_or("No managed desktop session is running")?;
            (session.id.clone(), session.frames.clone())
        };
        let deadline = Instant::now() + timeout;
        let result = loop {
            if STOPPING.load(std::sync::atomic::Ordering::Acquire) {
                return Err("Connect desktop service is stopping".into());
            }
            let frames = frame_state
                .lock()
                .map_err(|_| "Desktop frame state unavailable")?;
            if let Some(frame) = frames.current.as_ref().filter(|frame| frame.seq > after) {
                if frame.captured_at.elapsed() > FRAME_STALE_AFTER {
                    return Err("Desktop frame source is stale".into());
                }
                break Some((frame.seq, frame.since_start_ms, frame.jpeg.clone()));
            }
            if let Some(error) = frames.error.as_ref() {
                return Err(error.clone());
            }
            drop(frames);
            if Instant::now() >= deadline {
                return Ok(None);
            }
            thread::sleep(Duration::from_millis(40));
        };
        let guard = self
            .session
            .lock()
            .map_err(|_| "Desktop state unavailable")?;
        let session_is_current = guard.as_ref().is_some_and(|session| {
            session.id == session_id && Arc::ptr_eq(&session.frames, &frame_state)
        });
        if !session_is_current {
            return Err("Managed desktop session changed while waiting for a frame".into());
        }
        Ok(result)
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

    pub fn agent_observe(value: &Value) -> Result<Value, String> {
        Self::agent_request(&json!({"action":"observe","value":value}))
    }

    pub fn agent_act(value: &Value) -> Result<Value, String> {
        Self::agent_request(&json!({"action":"act","value":value}))
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

fn unicode_text_conflicts_with_held_modifiers(text: &str, held_keys: &HashSet<String>) -> bool {
    !text.is_ascii()
        && held_keys
            .iter()
            .any(|key| matches!(key.as_str(), "Control" | "Shift"))
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
        "desktop_status"
            | "desktop_screenshot"
            | "desktop_input"
            | "desktop_observe"
            | "desktop_act"
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
        "desktop_observe" => Desktop::agent_observe(&args).map(|value| value.to_string()),
        "desktop_act" => Desktop::agent_act(&args).map(|value| value.to_string()),
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
        Some("observe") => desktop.observe_accessibility(
            request["value"]["sessionId"].as_str(),
            request["value"]["query"].as_str(),
        ),
        Some("act") => desktop.act_accessibility(&request["value"]),
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
    use std::{
        process::{Command, Stdio},
        sync::atomic::{AtomicUsize, Ordering},
        thread,
    };

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
    fn desktop_status_includes_nullable_frame_time() {
        let desktop = Desktop::new(std::env::temp_dir());
        let status = desktop.status();
        assert_eq!(status["frameTimeMs"], Value::Null);
    }

    #[test]
    fn unicode_text_rejects_held_shortcut_modifiers() {
        let mut held_keys = HashSet::new();
        assert!(!unicode_text_conflicts_with_held_modifiers(
            "Crème", &held_keys
        ));
        held_keys.insert("Control".to_owned());
        assert!(unicode_text_conflicts_with_held_modifiers(
            "Crème", &held_keys
        ));
        assert!(!unicode_text_conflicts_with_held_modifiers(
            "Creme", &held_keys
        ));
        held_keys.clear();
        held_keys.insert("Shift".to_owned());
        assert!(unicode_text_conflicts_with_held_modifiers(
            "Crème", &held_keys
        ));
    }

    #[test]
    fn text_input_scroll_deltas_are_bounded() {
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

    #[test]
    fn managed_xfce_accessibility_observe_act_and_lease_e2e() {
        if std::env::var_os("GLASS_DESKTOP_A11Y_E2E").as_deref() != Some(std::ffi::OsStr::new("1"))
        {
            return;
        }

        let root = std::env::temp_dir().join(format!(
            "glass-desktop-a11y-e2e-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let mut desktop = Desktop::new(workspace);
        desktop.root = root.join("sessions");
        desktop.start().unwrap();
        let expected_saved_value = "glass desktop saved 7142";
        let saved_value_path = root.join("saved-value.txt");

        let (session_id, display, auth_file, session_bus, home) = {
            let guard = desktop.session.lock().unwrap();
            let session = guard.as_ref().unwrap();
            (
                session.id.clone(),
                session.display.clone(),
                session.directory.join("Xauthority"),
                session.session_bus_address.clone(),
                session.directory.join("home"),
            )
        };
        let mut registry_ready = false;
        let mut previous_applications = None;
        let registry_deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < registry_deadline {
            if let Ok(snapshot) = desktop.observe_accessibility(Some(&session_id), None) {
                let mut applications = snapshot["applications"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>();
                applications.sort();
                if !applications.is_empty() && previous_applications.as_ref() == Some(&applications)
                {
                    registry_ready = true;
                    break;
                }
                previous_applications = Some(applications);
            } else {
                previous_applications = None;
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert!(
            registry_ready,
            "isolated AT-SPI registry should accept a bounded observation before launching the test app"
        );
        let dialog_stderr_path = root.join("zenity-stderr.txt");
        let mut dialog = Command::new("zenity")
            .args([
                "--entry",
                "--title=Glass accessibility test",
                "--text=Enter a local test value",
                "--ok-label=Save",
            ])
            .env("DISPLAY", display)
            .env("XAUTHORITY", auth_file)
            .env("DBUS_SESSION_BUS_ADDRESS", session_bus)
            .env("GTK_A11Y", "atspi")
            .env("HOME", home)
            .stdin(Stdio::null())
            .stdout(std::fs::File::create(&saved_value_path).unwrap())
            .stderr(std::fs::File::create(&dialog_stderr_path).unwrap())
            .spawn()
            .expect("start local dialog in isolated managed Xfce session");

        thread::scope(|scope| {
            let observation =
                scope.spawn(|| desktop.observe_accessibility(Some(&session_id), None));
            let frame_wait =
                scope.spawn(|| desktop.frame_after(u64::MAX, Duration::from_millis(400)));
            thread::sleep(Duration::from_millis(40));
            let started = Instant::now();
            let status = desktop.status();
            let status_elapsed = started.elapsed();
            assert!(status["available"].as_bool().unwrap_or(false));
            assert!(status["frameTimeMs"].as_u64().is_some());
            assert!(
                status_elapsed < Duration::from_millis(250),
                "desktop_status should stay responsive during AT-SPI observation, took {status_elapsed:?}"
            );

            let started = Instant::now();
            desktop
                .input(
                    Lease::Agent,
                    &json!({"action":"pointer-move","x":0.5,"y":0.5}),
                )
                .unwrap();
            let input_elapsed = started.elapsed();
            assert!(
                input_elapsed < Duration::from_millis(250),
                "raw input should not wait on AT-SPI observation, took {input_elapsed:?}"
            );
            assert_eq!(frame_wait.join().unwrap().unwrap(), None);
            let observed = observation.join().unwrap();
            assert!(
                observed.is_ok()
                    || observed
                        .unwrap_err()
                        .contains("changed while it was running")
            );
        });

        (|| {
            let mut snapshot = None;
            let mut last_snapshot = None;
            let dialog_deadline = Instant::now() + Duration::from_secs(20);
            while Instant::now() < dialog_deadline {
                if dialog.try_wait().unwrap().is_some() {
                    panic!("test dialog exited before accessibility observation");
                }
                if let Ok(value) = desktop.observe_accessibility(Some(&session_id), None) {
                    last_snapshot = Some(value.clone());
                    let zenity_visible = value["applications"].as_array().is_some_and(|apps| {
                        apps.iter().any(|app| {
                            app.as_str()
                                .is_some_and(|name| name.to_ascii_lowercase().contains("zenity"))
                        })
                    });
                    let focused_dialog = value["focusedWindow"]["application"] == "zenity";
                    let editable_target = value["nodes"].as_array().is_some_and(|nodes| {
                        nodes.iter().any(|node| {
                            node["application"] == "zenity"
                                && !node["targetId"].is_null()
                                && !node["value"].is_null()
                        })
                    });
                    if zenity_visible && focused_dialog && editable_target {
                        snapshot = Some(value);
                        break;
                    }
                }
                thread::sleep(Duration::from_millis(100));
            }
            let first = snapshot.unwrap_or_else(|| {
                let stderr = std::fs::read_to_string(&dialog_stderr_path).unwrap_or_default();
                let diagnostic = stderr
                    .lines()
                    .filter(|line| {
                        let line = line.to_ascii_lowercase();
                        line.contains("at-spi") || line.contains("accessibility") || line.contains("error")
                    })
                    .take(8)
                    .collect::<Vec<_>>()
                    .join("\n");
                panic!(
                    "managed Xfce accessibility tree becomes observable; latest: {last_snapshot:?}; Zenity diagnostics: {diagnostic}"
                )
            });
            assert!(
                first["applications"]
                    .as_array()
                    .is_some_and(|applications| {
                        applications.iter().any(|application| {
                            application
                                .as_str()
                                .is_some_and(|name| name.to_ascii_lowercase().contains("zenity"))
                        })
                    }),
                "application list should identify Zenity: {first}"
            );
            assert_eq!(
                first["focusedWindow"]["name"], "Glass accessibility test",
                "the snapshot should identify the focused test dialog"
            );
            let first_target = test_target(&first, |node| !node["value"].is_null());

            let save_search = desktop
                .observe_accessibility(Some(&session_id), Some("Save"))
                .unwrap();
            assert_eq!(save_search["query"], "Save");
            assert!(
                save_search["nodes"].as_array().is_some_and(|nodes| {
                    nodes
                        .iter()
                        .any(|node| node["name"] == "Save" && !node["targetId"].is_null())
                }),
                "semantic search should return the actionable Save control: {save_search}"
            );

            let second = desktop
                .observe_accessibility(Some(&session_id), None)
                .unwrap();
            let stale = desktop.act_accessibility(&json!({
                "sessionId": session_id,
                "revision": first["revision"],
                "targetId": first_target["targetId"],
                "action": "set_text",
                "text": "glass stale token",
            }));
            assert!(stale.unwrap_err().contains("stale"));

            let field = test_target(&second, |node| !node["value"].is_null());
            let edited = desktop
                .act_accessibility(&json!({
                    "sessionId": session_id,
                    "revision": second["revision"],
                    "targetId": field["targetId"],
                    "action": "set_text",
                    "text": "glass accessibility probe",
                }))
                .unwrap_or_else(|error| panic!("could not edit target {field}: {error}"));
            assert_eq!(
                edited["verified"], true,
                "accessible text readback: {edited}"
            );
            assert_eq!(
                test_target(&edited["observation"], |node| !node["value"].is_null())["value"],
                "glass accessibility probe",
                "desktop_act should return its fresh readback snapshot"
            );

            let before_lease = desktop
                .observe_accessibility(Some(&session_id), None)
                .unwrap();
            let field = test_target(&before_lease, |node| !node["value"].is_null());
            desktop.set_lease("take-control").unwrap();
            let blocked = desktop.act_accessibility(&json!({
                "sessionId": session_id,
                "revision": before_lease["revision"],
                "targetId": field["targetId"],
                "action": "set_text",
                "text": "must not be entered",
            }));
            assert_eq!(
                blocked.unwrap_err(),
                "Desktop input lease is held by the other participant"
            );
            desktop.set_lease("return-to-agent").unwrap();

            let output = if std::env::var_os("GLASS_DESKTOP_A11Y_MODEL_E2E").as_deref()
                == Some(std::ffi::OsStr::new("1"))
            {
                run_live_accessibility_agent_turn(
                    &desktop,
                    &root,
                    &session_id,
                    expected_saved_value,
                )
            } else {
                run_accessibility_agent_turn(&desktop, &root, &session_id, expected_saved_value)
            };
            assert!(
                !output.trim().is_empty(),
                "the agent should report completion"
            );
            for _ in 0..30 {
                if let Some(status) = dialog.try_wait().unwrap() {
                    assert!(
                        status.success(),
                        "Save button should accept the local dialog"
                    );
                    assert_eq!(
                        std::fs::read_to_string(&saved_value_path).unwrap().trim(),
                        expected_saved_value,
                        "the app should save the exact value requested from the agent"
                    );
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }
            panic!("accessible Save action did not close the local dialog");
        })();
        if dialog.try_wait().unwrap().is_none() {
            let _ = dialog.kill();
            let _ = dialog.wait();
        }
        desktop.stop().unwrap();
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn test_target(snapshot: &Value, predicate: impl Fn(&Value) -> bool) -> Value {
        snapshot["nodes"]
            .as_array()
            .and_then(|nodes| {
                nodes
                    .iter()
                    .find(|node| !node["targetId"].is_null() && predicate(node))
            })
            .cloned()
            .unwrap_or_else(|| {
                panic!("expected accessible target missing from snapshot: {snapshot}")
            })
    }

    fn test_accessibility_tools() -> Vec<(String, String, String)> {
        crate::provider::desktop_tool_schemas(true)
            .into_iter()
            .filter(|(name, _, _)| matches!(name.as_str(), "desktop_observe" | "desktop_act"))
            .collect()
    }

    fn accessibility_test_prompt(session_id: &str) -> String {
        format!(
            "This is a test-owned isolated desktop session `{session_id}` with the agent input lease. Use only `desktop_observe` and `desktop_act` to inspect and operate its visible dialog. Target IDs are tied to the exact observation revision; use the fresh `observation` returned by each action before choosing another target. The dialog has one editable text field and a Save button. Do not use shell, files, screenshots, coordinates, or any other desktop session."
        )
    }

    fn run_live_accessibility_agent_turn(
        desktop: &Desktop,
        root: &Path,
        session_id: &str,
        expected_value: &str,
    ) -> String {
        let mut config = crate::config::load(&crate::config::default_path())
            .unwrap_or_else(|_| panic!("Could not load the configured free provider"));
        crate::auth::apply(&mut config)
            .unwrap_or_else(|_| panic!("Could not apply the configured provider connection"));
        let endpoint = url::Url::parse(&config.base_url)
            .unwrap_or_else(|_| panic!("The configured free provider endpoint is invalid"));
        assert_eq!(
            config.provider, "openai",
            "the guarded test expects OpenAI-compatible Kilo"
        );
        assert!(
            config.model.to_ascii_lowercase().contains("free"),
            "the live acceptance test will only call a model explicitly marked free"
        );
        assert_eq!(
            endpoint.host_str(),
            Some("127.0.0.1"),
            "the live acceptance test will only call the local provider proxy"
        );
        let key = crate::auth::resolve(&config)
            .unwrap_or_else(|_| panic!("Could not resolve the configured free provider"));
        let mut provider = crate::provider::build(
            &config.provider,
            &config.base_url,
            key,
            &config.model,
            config.adaptive_pacing,
        )
        .unwrap_or_else(|_| panic!("Could not construct the configured free provider"));
        provider.set_session_identity(config.provider_session.as_deref());

        let session_dir = root.join("live-agent-tool-turn");
        std::fs::create_dir_all(&session_dir).unwrap();
        let mut session = crate::session::Session::create(&session_dir).unwrap();
        session.append(crate::session::Record::User(format!(
            "In the isolated dialog, replace the text field with exactly `{expected_value}`, activate Save, then use the fresh accessibility observation to verify the dialog closed. Reply briefly when done."
        )));
        let mut state = crate::tools::State::new();
        let mods = Vec::new();
        let mut extensions = crate::ext::Registry::empty();
        let desktop_ref = desktop;
        let actions = Arc::new(Mutex::new(Vec::<String>::new()));
        let action_trace = Arc::clone(&actions);
        let mut agent = crate::agent::Agent::new(
            provider.as_ref(),
            &mut session,
            &mut state,
            &mods,
            &mut extensions,
            &session_dir,
        )
        .with_prompt(accessibility_test_prompt(session_id), None, None)
        .with_limits(1536, None, 8)
        .with_host_tools(
            test_accessibility_tools(),
            Box::new(move |call, _, _| {
                let args = serde_json::from_str::<Value>(&call.arguments).unwrap_or_default();
                if call.name == "desktop_act" {
                    action_trace.lock().unwrap().push(format!(
                        "desktop_act:{}",
                        args["action"].as_str().unwrap_or("unknown")
                    ));
                } else {
                    action_trace.lock().unwrap().push(call.name.clone());
                }
                let result = match call.name.as_str() {
                    "desktop_observe" => desktop_ref
                        .observe_accessibility(args["sessionId"].as_str(), args["query"].as_str())
                        .map(|value| value.to_string()),
                    "desktop_act" => desktop_ref
                        .act_accessibility(&args)
                        .map(|value| value.to_string()),
                    _ => Err("the test host exposes only accessible observe and act".to_owned()),
                };
                if call.name == "desktop_act"
                    && result.as_ref().is_ok_and(|output| {
                        serde_json::from_str::<Value>(output)
                            .is_ok_and(|value| value["observation"].is_object())
                    })
                {
                    action_trace.lock().unwrap().push(format!(
                        "desktop_act_result:{}:fresh",
                        args["action"].as_str().unwrap_or("unknown")
                    ));
                }
                crate::agent::HostToolOutcome::plain(match result {
                    Ok(output) => crate::tools::Outcome::ok(output),
                    Err(error) => crate::tools::Outcome::err(error),
                })
            }),
        );
        let turn = agent.run_turn(|_| {});
        assert!(
            turn.is_ok(),
            "the configured free model turn should complete"
        );
        let (output, stop) = turn.unwrap();
        assert_eq!(stop, crate::agent::Stop::Finished);
        let actions = actions.lock().unwrap();
        assert!(actions.iter().any(|action| action == "desktop_observe"));
        let text_position = actions
            .iter()
            .position(|action| action == "desktop_act:set_text")
            .expect("the configured free model should enter the requested value");
        let click_position = actions
            .iter()
            .position(|action| action == "desktop_act:click")
            .expect("the configured free model should activate Save");
        assert!(text_position < click_position);
        assert!(
            actions[click_position + 1..]
                .iter()
                .any(|action| action == "desktop_act_result:click:fresh"),
            "Save should return a fresh observation for the configured free model to inspect"
        );
        drop(actions);
        drop(agent);
        drop(extensions);
        drop(session);
        std::fs::remove_dir_all(session_dir).unwrap();
        output.text
    }

    fn run_accessibility_agent_turn(
        desktop: &Desktop,
        root: &Path,
        session_id: &str,
        expected_value: &str,
    ) -> String {
        #[derive(Default)]
        struct AccessState {
            observations: Vec<Value>,
            errors: Vec<String>,
            actions: Vec<String>,
        }

        struct AccessibilityTurnProvider {
            turn: AtomicUsize,
            state: Arc<Mutex<AccessState>>,
            session_id: String,
            expected_value: String,
        }

        impl crate::provider::Provider for AccessibilityTurnProvider {
            fn id(&self) -> &str {
                "offline-accessibility-turn"
            }

            fn stream(
                &self,
                request: &crate::provider::Request<'_>,
            ) -> Result<Box<dyn Iterator<Item = crate::provider::Event> + '_>, crate::provider::Error>
            {
                let event = match self.turn.fetch_add(1, Ordering::SeqCst) {
                    0 => {
                        for tool in ["desktop_observe", "desktop_act"] {
                            assert!(request.tools.iter().any(|(name, _, _)| name == tool));
                        }
                        crate::provider::Event::ToolCall {
                            id: "search-save-control".into(),
                            name: "desktop_observe".into(),
                            arguments: json!({"sessionId": self.session_id,"query":"Save"})
                                .to_string(),
                        }
                    }
                    1 => {
                        let state = self.state.lock().unwrap();
                        let target_snapshot = state.observations.last().ok_or_else(|| {
                            crate::provider::Error::Config("desktop_observe did not run".into())
                        })?;
                        assert!(
                            test_target(target_snapshot, |node| node["name"] == "Save")["targetId"]
                                .is_string()
                        );
                        crate::provider::Event::ToolCall {
                            id: "refresh-full-snapshot".into(),
                            name: "desktop_observe".into(),
                            arguments: json!({"sessionId": self.session_id}).to_string(),
                        }
                    }
                    2 => {
                        let state = self.state.lock().unwrap();
                        let stale = state.observations.first().ok_or_else(|| {
                            crate::provider::Error::Config("initial query did not run".into())
                        })?;
                        let target = test_target(stale, |node| node["name"] == "Save");
                        crate::provider::Event::ToolCall {
                            id: "deliberately-reuse-stale-target".into(),
                            name: "desktop_act".into(),
                            arguments: json!({
                                "sessionId": stale["sessionId"],
                                "revision": stale["revision"],
                                "targetId": target["targetId"],
                                "action": "click",
                            })
                            .to_string(),
                        }
                    }
                    3 => {
                        let state = self.state.lock().unwrap();
                        assert!(
                            state
                                .errors
                                .last()
                                .is_some_and(|error| error.contains("stale"))
                        );
                        crate::provider::Event::ToolCall {
                            id: "recover-with-fresh-observation".into(),
                            name: "desktop_observe".into(),
                            arguments: json!({"sessionId": self.session_id}).to_string(),
                        }
                    }
                    4 => {
                        let state = self.state.lock().unwrap();
                        let snapshot = state.observations.last().ok_or_else(|| {
                            crate::provider::Error::Config("fresh observation did not run".into())
                        })?;
                        let field = test_target(snapshot, |node| node["value"].is_string());
                        crate::provider::Event::ToolCall {
                            id: "enter-requested-value".into(),
                            name: "desktop_act".into(),
                            arguments: json!({
                                "sessionId": snapshot["sessionId"],
                                "revision": snapshot["revision"],
                                "targetId": field["targetId"],
                                "action": "set_text",
                                "text": self.expected_value,
                            })
                            .to_string(),
                        }
                    }
                    5 => {
                        let state = self.state.lock().unwrap();
                        let snapshot = state.observations.last().ok_or_else(|| {
                            crate::provider::Error::Config(
                                "set_text did not refresh the tree".into(),
                            )
                        })?;
                        let field =
                            test_target(snapshot, |node| node["value"] == self.expected_value);
                        let save = test_target(snapshot, |node| node["name"] == "Save");
                        assert!(field["targetId"].is_string());
                        crate::provider::Event::ToolCall {
                            id: "save-entered-value".into(),
                            name: "desktop_act".into(),
                            arguments: json!({
                                "sessionId": snapshot["sessionId"],
                                "revision": snapshot["revision"],
                                "targetId": save["targetId"],
                                "action": "click",
                            })
                            .to_string(),
                        }
                    }
                    _ => {
                        let state = self.state.lock().unwrap();
                        assert!(state.actions.iter().any(|action| action == "set_text"));
                        assert!(state.actions.iter().any(|action| action == "click"));
                        crate::provider::Event::Text(
                            "Saved the requested value using the accessible Save control.".into(),
                        )
                    }
                };
                Ok(Box::new(
                    vec![event, crate::provider::Event::Done].into_iter(),
                ))
            }

            fn context_window(&self) -> u32 {
                200_000
            }
        }

        let session_dir = root.join("agent-tool-turn");
        std::fs::create_dir_all(&session_dir).unwrap();
        let state = Arc::new(Mutex::new(AccessState::default()));
        let provider = AccessibilityTurnProvider {
            turn: AtomicUsize::new(0),
            state: state.clone(),
            session_id: session_id.to_owned(),
            expected_value: expected_value.to_owned(),
        };
        let mut session = crate::session::Session::create(&session_dir).unwrap();
        session.append(crate::session::Record::User(format!(
            "In the isolated dialog, replace the field with exactly `{expected_value}`, activate Save, and verify the dialog closes."
        )));
        let mut tool_state = crate::tools::State::new();
        let mods = Vec::new();
        let mut extensions = crate::ext::Registry::empty();
        let desktop_ref = desktop;
        let desktop_tools = test_accessibility_tools();
        let mut agent = crate::agent::Agent::new(
            &provider,
            &mut session,
            &mut tool_state,
            &mods,
            &mut extensions,
            &session_dir,
        )
        .with_prompt(accessibility_test_prompt(session_id), None, None)
        .with_host_tools(
            desktop_tools,
            Box::new(move |call, _, _| {
                let args = serde_json::from_str::<Value>(&call.arguments).unwrap();
                let result = match call.name.as_str() {
                    "desktop_observe" => desktop_ref
                        .observe_accessibility(args["sessionId"].as_str(), args["query"].as_str())
                        .map(|value| {
                            let mut access_state = state.lock().unwrap();
                            access_state.observations.push(value.clone());
                            value.to_string()
                        }),
                    "desktop_act" => {
                        let action = args["action"].as_str().unwrap_or("unknown").to_owned();
                        let result = desktop_ref.act_accessibility(&args);
                        let mut access_state = state.lock().unwrap();
                        match result {
                            Ok(value) => {
                                access_state.actions.push(action);
                                if let Some(observation) = value.get("observation") {
                                    access_state.observations.push(observation.clone());
                                }
                                Ok(value.to_string())
                            }
                            Err(error) => {
                                access_state.errors.push(error.clone());
                                Err(error)
                            }
                        }
                    }
                    _ => Err(format!("unexpected host tool {}", call.name)),
                };
                crate::agent::HostToolOutcome::plain(match result {
                    Ok(output) => crate::tools::Outcome::ok(output),
                    Err(error) => crate::tools::Outcome::err(error),
                })
            }),
        );
        let (output, stop) = agent.run_turn(|_| {}).unwrap();
        assert_eq!(stop, crate::agent::Stop::Finished);
        assert_eq!(provider.turn.load(Ordering::SeqCst), 7);
        drop(agent);
        drop(extensions);
        drop(session);
        std::fs::remove_dir_all(session_dir).unwrap();
        output.text
    }
}
