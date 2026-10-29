//! A Connect-owned graphical session shared by the browser and GC tools.
//!
//! The session is deliberately isolated from the host login desktop. It is
//! started only by an explicit Connect request and remains alive when a viewer
//! disconnects.

use base64::Engine;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
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
mod apps;
mod capture;
mod input;
mod process;
mod supervisor;
use capture::Frames;

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 800;
const MAX_FRAME: usize = 4 * 1024 * 1024;
const FRAME_STALE_AFTER: Duration = Duration::from_millis(1800);
const LEASE_SOCKET: &str = "desktop-agent.sock";
const MAX_CACHED_ACCESSIBILITY_WINDOWS: usize = 64;
const AGENT_MESSAGE_MAX: u64 = 6 * 1024 * 1024;
const DEFAULT_PROFILE_ID: &str = "default";
const GNOME_XVFB_ACCEPTED: bool = false;
static STOPPING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn random_accessibility_revision() -> Result<u64, String> {
    let mut bytes = [0u8; 8];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes)
        .map_err(|_| "Secure random unavailable")?;
    Ok((u64::from_be_bytes(bytes) & ((1u64 << 62) - 1)) | (1u64 << 62))
}

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

pub(crate) fn run_supervisor(
    profile: &Path,
    workspace: &Path,
    environment: &str,
) -> Result<(), String> {
    let environment =
        process::Environment::parse(environment).ok_or("environment must be gnome or xfce")?;
    supervisor::run(profile, workspace, environment)
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
    environment: process::Environment,
    lifecycle: process::Lifecycle,
    profile_id: Option<String>,
    workspace: PathBuf,
    owner: SessionOwner,
    auth_cookie: [u8; 16],
    directory: PathBuf,
    xvfb: Option<Child>,
    desktop: Option<Child>,
    capture: Option<Child>,
    frames: Arc<Mutex<Frames>>,
    session_bus_address: String,
    /// A process-wide lock keeps one Connect gateway authoritative for the
    /// active desktop and its cached lease/input state.
    _profile_lock: Option<File>,
    accessibility_revision: u64,
    accessibility_registry_ready: bool,
    accessibility_targets: HashMap<String, accessibility::Target>,
    accessibility_windows: HashMap<String, accessibility::WindowRoot>,
    lease: Lease,
    held_buttons: HashSet<u8>,
    held_keys: HashSet<String>,
    scroll_x_remainder: f64,
    scroll_y_remainder: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SessionOwner {
    Gateway,
    Supervisor,
    Attached,
}

impl Drop for ManagedSession {
    fn drop(&mut self) {
        if let Some(capture) = self.capture.as_mut() {
            capture::stop_child(capture);
        }
        if self.owner != SessionOwner::Attached {
            if let Some(desktop) = self.desktop.as_mut() {
                capture::stop_child(desktop);
            }
            if let Some(xvfb) = self.xvfb.as_mut() {
                capture::stop_child(xvfb);
            }
            let _ = std::fs::remove_dir_all(&self.directory);
        }
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
    /// Ensures concurrent first app launches share one registry warm-up.
    accessibility_registry_init: Mutex<()>,
    session: Mutex<Option<ManagedSession>>,
}

impl Desktop {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            root: crate::config::data_dir().join("desktop"),
            control: Mutex::new(()),
            accessibility_registry_init: Mutex::new(()),
            session: Mutex::new(None),
        }
    }

    pub fn status(&self) -> Value {
        let restore_error = self.restore_reusable_session().err();
        let capabilities = self.capabilities();
        let session = self.session.lock().unwrap();
        let Some(session) = session.as_ref() else {
            return json!({
                "available": false,
                "sessionId": null,
                "mode": null,
                "environment": null,
                "lifecycle": null,
                "profileId": null,
                "displayId": null,
                "workspaceRoot": self.workspace,
                "lease": "agent",
                "width": null,
                "height": null,
                "frameSeq": null,
                "frameAgeMs": null,
                "frameTimeMs": null,
                "error": restore_error,
                "capabilities": capabilities
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
            "environment": session.environment.as_str(),
            "lifecycle": session.lifecycle.as_str(),
            "profileId": session.profile_id,
            "displayId": session.display,
            "workspaceRoot": session.workspace,
            "lease": session.lease.as_str(),
            "width": WIDTH,
            "height": HEIGHT,
            "frameSeq": frames.current.as_ref().map(|frame| frame.seq),
            "frameAgeMs": age.map(|age| age.as_millis()),
            "frameTimeMs": frames.current.as_ref().map(|frame| frame.since_start_ms),
            "error": if !fresh {
                frames.error.as_deref().or(Some("Desktop frame source is stale"))
            } else {
                restore_error.as_deref()
            },
            "capabilities": capabilities
        })
    }

    fn capabilities(&self) -> Value {
        let xfce_reason = process::environment_dependency_error(process::Environment::Xfce);
        let gnome_reason = process::environment_dependency_error(process::Environment::Gnome)
            .or_else(|| {
                (!GNOME_XVFB_ACCEPTED).then_some(
                    "GNOME has not passed isolated Xvfb desktop acceptance on this host".into(),
                )
            });
        json!([
            {
                "environment":"gnome",
                "lifecycle":"reusable",
                "label":"Reusable GNOME workspace",
                "available":gnome_reason.is_none(),
                "reason":gnome_reason,
                "profileId":DEFAULT_PROFILE_ID,
                "profileExists":self.profile_dir().is_dir()
            },
            {
                "environment":"xfce",
                "lifecycle":"disposable",
                "label":"Quick XFCE desktop",
                "available":xfce_reason.is_none(),
                "reason":xfce_reason,
                "profileId":null,
                "profileExists":false
            }
        ])
    }

    fn profile_dir(&self) -> PathBuf {
        self.root.join("profiles").join(DEFAULT_PROFILE_ID)
    }

    fn lock_managed_profile(&self) -> Result<File, String> {
        match std::fs::symlink_metadata(&self.root) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err("Desktop storage path must be a real directory".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir_all(&self.root)
                    .map_err(|error| format!("Desktop storage: {error}"))?;
            }
            Err(error) => return Err(format!("Checking desktop storage: {error}")),
        }
        let metadata = std::fs::symlink_metadata(&self.root)
            .map_err(|error| format!("Checking desktop storage: {error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("Desktop storage path must be a real directory".into());
        }
        std::fs::set_permissions(&self.root, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("Desktop storage permissions: {error}"))?;
        let directory = self.root.join("profile-locks");
        match std::fs::symlink_metadata(&directory) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err("Desktop profile lock path must be a private directory".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&directory)
                    .map_err(|error| format!("Desktop profile locks: {error}"))?;
            }
            Err(error) => return Err(format!("Checking desktop profile locks: {error}")),
        }
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("Desktop profile lock permissions: {error}"))?;
        let lock_path = directory.join(format!("{DEFAULT_PROFILE_ID}.lock"));
        let mut options = std::fs::OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        let lock = options
            .open(&lock_path)
            .map_err(|error| format!("Opening desktop profile lock: {error}"))?;
        lock.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("Setting desktop profile lock permissions: {error}"))?;
        fs2::FileExt::try_lock_exclusive(&lock)
            .map_err(|_| "Another Connect gateway owns the managed desktop".to_owned())?;
        Ok(lock)
    }

    pub fn start(&self) -> Result<Value, String> {
        self.start_environment(process::Environment::Xfce, process::Lifecycle::Disposable)
    }

    pub fn start_selected(&self, environment: &str, lifecycle: &str) -> Result<Value, String> {
        let environment =
            process::Environment::parse(environment).ok_or("environment must be gnome or xfce")?;
        let lifecycle = process::Lifecycle::parse(lifecycle)
            .ok_or("lifecycle must be reusable or disposable")?;
        self.start_environment(environment, lifecycle)
    }

    fn start_environment(
        &self,
        environment: process::Environment,
        lifecycle: process::Lifecycle,
    ) -> Result<Value, String> {
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
        let profile_lock = self.lock_managed_profile()?;
        let session = self.start_session(environment, lifecycle, profile_lock)?;
        let id = session.id.clone();
        let display = session.display.clone();
        let profile_id = session.profile_id.clone();
        *guard = Some(session);
        Ok(
            json!({"started":true,"sessionId":id,"mode":"managedVirtualSession","environment":environment.as_str(),"lifecycle":lifecycle.as_str(),"profileId":profile_id,"displayId":display,"width":WIDTH,"height":HEIGHT}),
        )
    }

    fn start_session(
        &self,
        environment: process::Environment,
        lifecycle: process::Lifecycle,
        profile_lock: File,
    ) -> Result<ManagedSession, String> {
        if let Some(reason) = process::environment_dependency_error(environment) {
            return Err(reason);
        }
        if environment == process::Environment::Gnome && !GNOME_XVFB_ACCEPTED {
            return Err(
                "GNOME has not passed isolated Xvfb desktop acceptance on this host".into(),
            );
        }
        if lifecycle == process::Lifecycle::Disposable
            && supervisor::status(&self.profile_dir())?.is_some()
        {
            return Err("Stop the reusable desktop before starting a quick desktop".into());
        }
        match lifecycle {
            process::Lifecycle::Disposable => {
                let mut session = process::start_session_with_environment(
                    &self.workspace,
                    &self.root,
                    environment,
                    lifecycle,
                    None,
                )?;
                session._profile_lock = Some(profile_lock);
                Ok(session)
            }
            process::Lifecycle::Reusable => {
                let profile = self.profile_dir();
                let workspace = profile.join("workspace");
                let descriptor = supervisor::start_or_attach(&profile, &workspace, environment)?;
                self.attach_reusable_session(&descriptor, profile_lock)
            }
        }
    }

    fn restore_reusable_session(&self) -> Result<(), String> {
        let _control = self
            .control
            .lock()
            .map_err(|_| "Desktop control unavailable")?;
        {
            let guard = self
                .session
                .lock()
                .map_err(|_| "Desktop state unavailable")?;
            if guard.is_some() {
                return Ok(());
            }
        }
        let profile = self.profile_dir();
        let profile_lock = self.lock_managed_profile()?;
        if let Some(descriptor) = supervisor::status(&profile)? {
            let session = self.attach_reusable_session(&descriptor, profile_lock)?;
            let mut guard = self
                .session
                .lock()
                .map_err(|_| "Desktop state unavailable")?;
            if guard.is_none() {
                *guard = Some(session);
            }
        }
        Ok(())
    }

    fn attach_reusable_session(
        &self,
        descriptor: &Value,
        profile_lock: File,
    ) -> Result<ManagedSession, String> {
        let session_id = descriptor["sessionId"]
            .as_str()
            .ok_or("Reusable desktop session ID is unavailable")?
            .to_owned();
        let display = descriptor["displayId"]
            .as_str()
            .ok_or("Reusable desktop display is unavailable")?
            .to_owned();
        let directory = PathBuf::from(
            descriptor["sessionDirectory"]
                .as_str()
                .ok_or("Reusable desktop session directory is unavailable")?,
        );
        if !directory.starts_with(
            self.root
                .join("profiles")
                .join(DEFAULT_PROFILE_ID)
                .join("sessions"),
        ) {
            return Err("Reusable desktop session directory is outside its profile".into());
        }
        let environment = descriptor["environment"]
            .as_str()
            .and_then(process::Environment::parse)
            .ok_or("Reusable desktop environment is unavailable")?;
        if descriptor["lifecycle"].as_str() != Some("reusable") {
            return Err("Reusable desktop lifecycle is invalid".into());
        }
        let workspace = PathBuf::from(
            descriptor["workspaceRoot"]
                .as_str()
                .ok_or("Reusable desktop workspace is unavailable")?,
        );
        let session_bus_address = descriptor["sessionBusAddress"]
            .as_str()
            .ok_or("Reusable desktop session bus is unavailable")?
            .to_owned();
        let cookie = descriptor["cookie"]
            .as_str()
            .ok_or("Reusable desktop Xauthority cookie is unavailable")?;
        let auth_cookie = input::decode_cookie(cookie)?;
        input::release_all(&display, &auth_cookie)
            .map_err(|error| format!("Releasing stale desktop input state: {}", error.message()))?;
        let frames = Arc::new(Mutex::new(Frames::default()));
        let auth_file = directory.join("Xauthority");
        let capture = process::start_capture(&display, &auth_file, &frames)?;
        Ok(ManagedSession {
            id: session_id,
            display,
            environment,
            lifecycle: process::Lifecycle::Reusable,
            profile_id: Some(DEFAULT_PROFILE_ID.to_owned()),
            workspace,
            owner: SessionOwner::Attached,
            auth_cookie,
            directory,
            xvfb: None,
            desktop: None,
            capture: Some(capture),
            frames,
            session_bus_address,
            _profile_lock: Some(profile_lock),
            accessibility_revision: random_accessibility_revision()?,
            accessibility_registry_ready: false,
            accessibility_targets: HashMap::new(),
            accessibility_windows: HashMap::new(),
            lease: if descriptor["lease"].as_str() == Some("human") {
                Lease::Human
            } else {
                Lease::Agent
            },
            held_buttons: HashSet::new(),
            held_keys: HashSet::new(),
            scroll_x_remainder: 0.0,
            scroll_y_remainder: 0.0,
        })
    }

    pub fn delete_profile(&self, profile_id: &str) -> Result<Value, String> {
        if profile_id != DEFAULT_PROFILE_ID {
            return Err("Only the default reusable desktop profile can be deleted".into());
        }
        let _control = self
            .control
            .lock()
            .map_err(|_| "Desktop control unavailable")?;
        let guard = self
            .session
            .lock()
            .map_err(|_| "Desktop state unavailable")?;
        if guard
            .as_ref()
            .is_some_and(|session| session.profile_id.as_deref() == Some(DEFAULT_PROFILE_ID))
        {
            return Err("Stop the reusable desktop before deleting its saved profile".into());
        }
        let profile = self.profile_dir();
        let _profile_lock = self.lock_managed_profile()?;
        if supervisor::status(&profile)?.is_some() {
            return Err("Stop the reusable desktop before deleting its saved profile".into());
        }
        match std::fs::symlink_metadata(&profile) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("Reusable desktop profile path must not be a symlink".into());
            }
            Ok(metadata) if metadata.is_dir() => {
                std::fs::remove_dir_all(&profile)
                    .map_err(|error| format!("Deleting reusable desktop profile: {error}"))?;
            }
            Ok(_) => return Err("Reusable desktop profile path is not a directory".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Checking reusable desktop profile: {error}")),
        }
        Ok(json!({"deleted":true,"profileId":DEFAULT_PROFILE_ID}))
    }

    pub fn stop(&self) -> Result<Value, String> {
        self.restore_reusable_session()?;
        let _control = self
            .control
            .lock()
            .map_err(|_| "Desktop control unavailable")?;
        let mut guard = self
            .session
            .lock()
            .map_err(|_| "Desktop state unavailable")?;
        if guard
            .as_ref()
            .is_some_and(|session| session.lifecycle == process::Lifecycle::Reusable)
        {
            supervisor::stop(&self.profile_dir())?;
        }
        if guard.take().is_some() {
            Ok(json!({"stopped":true}))
        } else {
            Err("No managed desktop session is running".into())
        }
    }

    pub fn gateway_shutdown(&self) {
        let Ok(_control) = self.control.lock() else {
            return;
        };
        let Ok(mut guard) = self.session.lock() else {
            return;
        };
        if let Some(session) = guard.as_mut()
            && session.lifecycle == process::Lifecycle::Reusable
        {
            let _ = input::release_held(
                &session.display,
                &session.auth_cookie,
                &session.held_buttons,
                &session.held_keys,
            );
            session.held_buttons.clear();
            session.held_keys.clear();
            let _ = supervisor::write_lease(&self.profile_dir(), session.lease.as_str());
        }
        guard.take();
    }

    pub fn set_lease(&self, action: &str) -> Result<Value, String> {
        self.restore_reusable_session()?;
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
        if session.lifecycle == process::Lifecycle::Reusable {
            supervisor::write_lease(&self.profile_dir(), session.lease.as_str())?;
        }
        Ok(json!({"lease":session.lease.as_str()}))
    }

    pub fn input(&self, role: Lease, value: &Value) -> Result<Value, InputError> {
        self.restore_reusable_session()
            .map_err(|_| InputError::Unavailable)?;
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

    #[cfg(test)]
    pub fn observe_accessibility(
        &self,
        expected_session_id: Option<&str>,
        query: Option<&str>,
    ) -> Result<Value, String> {
        self.observe_accessibility_scoped(expected_session_id, query, None)
    }

    pub fn observe_accessibility_scoped(
        &self,
        expected_session_id: Option<&str>,
        query: Option<&str>,
        requested_window_id: Option<&str>,
    ) -> Result<Value, String> {
        self.restore_reusable_session()?;
        let (session_id, session_bus_address, revision, window_root) = {
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
            let window_root = if let Some(window_id) = requested_window_id {
                Some(
                    session
                        .accessibility_windows
                        .get(window_id)
                        .cloned()
                        .ok_or(
                            "windowId is stale or belongs to another observation; observe again",
                        )?,
                )
            } else {
                None
            };
            session.accessibility_revision = session.accessibility_revision.saturating_add(1);
            session.accessibility_targets.clear();
            (
                session.id.clone(),
                session.session_bus_address.clone(),
                session.accessibility_revision,
                window_root,
            )
        };
        // D-Bus can take up to two seconds. Keep the manager mutex free so
        // frame/status requests and human input are not held behind AT-SPI.
        let snapshot = accessibility::observe_scoped(
            &session_bus_address,
            &session_id,
            revision,
            query,
            window_root,
        )?;
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
        session.accessibility_registry_ready |= accessibility_registry_is_ready(&snapshot.value);
        if requested_window_id.is_some() {
            session.accessibility_windows.extend(snapshot.windows);
            let mut retained = 0usize;
            session.accessibility_windows.retain(|window_id, _| {
                if Some(window_id.as_str()) == requested_window_id {
                    true
                } else if retained < MAX_CACHED_ACCESSIBILITY_WINDOWS - 1 {
                    retained += 1;
                    true
                } else {
                    false
                }
            });
        } else {
            session.accessibility_windows = snapshot.windows;
        }
        Ok(snapshot.value)
    }

    pub fn act_accessibility(&self, value: &Value) -> Result<Value, String> {
        self.restore_reusable_session()?;
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
            session.accessibility_windows = snapshot.windows;
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
        self.restore_reusable_session()?;
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

    pub fn agent_launch_app(value: &Value) -> Result<Value, String> {
        Self::agent_request(&json!({"action":"launch_app","value":value}))
    }

    pub fn agent_observe(value: &Value) -> Result<Value, String> {
        Self::agent_request(&json!({"action":"observe","value":value}))
    }

    pub fn agent_act(value: &Value) -> Result<Value, String> {
        Self::agent_request(&json!({"action":"act","value":value}))
    }

    fn launch_app(&self, value: &Value) -> Result<Value, String> {
        let object = value
            .as_object()
            .ok_or("desktop_launch_app arguments must be an object")?;
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "action" | "sessionId" | "desktopEntryId"))
        {
            return Err("desktop_launch_app does not accept extra arguments".into());
        }
        let action = value
            .get("action")
            .and_then(Value::as_str)
            .ok_or("action must be list or launch")?;
        let expected_session_id = value
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or("sessionId is required from desktop_status")?;
        match action {
            "list" if value.get("desktopEntryId").is_some() => {
                return Err("desktopEntryId is only valid when action is launch".into());
            }
            "launch"
                if value
                    .get("desktopEntryId")
                    .and_then(Value::as_str)
                    .is_none() =>
            {
                return Err("desktopEntryId is required when action is launch".into());
            }
            "list" | "launch" => {}
            _ => return Err("action must be list or launch".into()),
        }
        self.restore_reusable_session()?;
        if action == "launch" {
            self.ensure_accessibility_registry_ready(expected_session_id)?;
        }
        let _control = self
            .control
            .lock()
            .map_err(|_| "Desktop control unavailable")?;
        let (session_id, context, lease) = {
            let guard = self
                .session
                .lock()
                .map_err(|_| "Desktop state unavailable")?;
            let session = guard
                .as_ref()
                .ok_or("No managed desktop session is running")?;
            if expected_session_id != session.id {
                return Err(
                    "The managed desktop session changed; call desktop_status and retry".into(),
                );
            }
            let home = match session.lifecycle {
                process::Lifecycle::Disposable => session.directory.join("home"),
                process::Lifecycle::Reusable => self.profile_dir().join("home"),
            };
            let atspi_bus_address =
                std::fs::read_to_string(session.directory.join("atspi-bus.address"))
                    .map_err(|_| "Managed desktop AT-SPI bus address is unavailable".to_owned())?;
            let atspi_bus_address = atspi_bus_address.trim().to_owned();
            if atspi_bus_address.is_empty()
                || atspi_bus_address.len() > 4096
                || !atspi_bus_address.starts_with("unix:")
                || atspi_bus_address.contains(['\n', '\r'])
            {
                return Err("Managed desktop AT-SPI bus address is invalid".into());
            }
            (
                session.id.clone(),
                apps::LaunchContext {
                    environment: session.environment,
                    display: session.display.clone(),
                    home,
                    #[cfg(test)]
                    session_directory: session.directory.clone(),
                    xauthority: session.directory.join("Xauthority"),
                    runtime_directory: session.directory.join("runtime"),
                    session_bus_address: session.session_bus_address.clone(),
                    atspi_bus_address,
                },
                session.lease,
            )
        };
        match action {
            "list" => {
                let listed = apps::list(&context.home);
                Ok(json!({
                    "sessionId": session_id,
                    "applications": listed["applications"],
                    "truncated": listed["truncated"]
                }))
            }
            "launch" => {
                require_lease(Lease::Agent, lease).map_err(|error| error.message().to_owned())?;
                let entry_id = value
                    .get("desktopEntryId")
                    .and_then(Value::as_str)
                    .ok_or("desktopEntryId is required when action is launch")?;
                let mut result = apps::launch(&context, entry_id)?;
                if let Some(object) = result.as_object_mut() {
                    object.insert("sessionId".into(), json!(session_id));
                }
                Ok(result)
            }
            _ => Err("action must be list or launch".into()),
        }
    }

    fn ensure_accessibility_registry_ready(&self, expected_session_id: &str) -> Result<(), String> {
        let _initializing = self
            .accessibility_registry_init
            .lock()
            .map_err(|_| "Desktop accessibility initialization unavailable")?;
        let ready = {
            let guard = self
                .session
                .lock()
                .map_err(|_| "Desktop state unavailable")?;
            let session = guard
                .as_ref()
                .ok_or("No managed desktop session is running")?;
            if session.id != expected_session_id {
                return Err(
                    "The managed desktop session changed; call desktop_status and retry".into(),
                );
            }
            require_lease(Lease::Agent, session.lease)
                .map_err(|error| error.message().to_owned())?;
            session.accessibility_registry_ready
        };
        if ready {
            return Ok(());
        }

        // The first AT-SPI request activates the private registry. Doing one
        // bounded observation before starting an app lets its accessibility
        // bridge connect to a ready registry instead of racing activation.
        let snapshot =
            match self.observe_accessibility_scoped(Some(expected_session_id), None, None) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    let guard = self
                        .session
                        .lock()
                        .map_err(|_| "Desktop state unavailable")?;
                    if guard.as_ref().is_some_and(|session| {
                        session.id == expected_session_id && session.accessibility_registry_ready
                    }) {
                        return Ok(());
                    }
                    return Err(error);
                }
            };
        if !accessibility_registry_is_ready(&snapshot) {
            return Err(
                "The managed desktop accessibility registry is not ready; observe the desktop and retry the launch".into(),
            );
        }
        Ok(())
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
            .set_read_timeout(Some(Duration::from_secs(12)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(12)))
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

fn accessibility_registry_is_ready(snapshot: &Value) -> bool {
    snapshot["applications"]
        .as_array()
        .is_some_and(|applications| !applications.is_empty())
        && snapshot["nodesVisited"].as_u64().unwrap_or_default() > 0
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
            | "desktop_launch_app"
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
        "desktop_launch_app" => Desktop::agent_launch_app(&args).map(|value| value.to_string()),
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
        .set_read_timeout(Some(Duration::from_secs(12)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(12)))
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
        Some("observe") => desktop.observe_accessibility_scoped(
            request["value"]["sessionId"].as_str(),
            request["value"]["query"].as_str(),
            request["value"]["windowId"].as_str(),
        ),
        Some("act") => desktop.act_accessibility(&request["value"]),
        Some("launch_app") => desktop.launch_app(&request["value"]),
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
    fn accessibility_registry_readiness_requires_observed_applications_and_nodes() {
        assert!(!accessibility_registry_is_ready(&json!({
            "applications": [],
            "nodesVisited": 12
        })));
        assert!(!accessibility_registry_is_ready(&json!({
            "applications": ["xfce4-session"],
            "nodesVisited": 0
        })));
        assert!(accessibility_registry_is_ready(&json!({
            "applications": ["xfce4-session"],
            "nodesVisited": 1
        })));
    }

    #[test]
    fn gnome_reusable_capability_stays_disabled_with_a_reason() {
        let desktop = Desktop::new(std::env::temp_dir());
        let status = desktop.status();
        let gnome = status["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|capability| {
                capability["environment"] == "gnome" && capability["lifecycle"] == "reusable"
            })
            .expect("status should describe the reusable GNOME option");

        assert_eq!(gnome["available"], false);
        assert!(
            gnome["reason"]
                .as_str()
                .is_some_and(|reason| !reason.is_empty())
        );
    }

    #[test]
    fn profile_lock_rejects_symlinked_storage_before_writing() {
        let root = std::env::temp_dir().join(format!(
            "glass-desktop-lock-root-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let outside = root.join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o755)).unwrap();
        let storage = root.join("desktop");
        std::os::unix::fs::symlink(&outside, &storage).unwrap();
        let mut desktop = Desktop::new(root.join("workspace"));
        desktop.root = storage;

        assert!(desktop.lock_managed_profile().is_err());
        assert!(!outside.join("profile-locks").exists());
        assert_eq!(
            std::fs::symlink_metadata(&outside)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        let _ = std::fs::remove_dir_all(root);
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
    fn reusable_xfce_session_survives_gateway_restart_and_deletes_explicitly() {
        if std::env::var_os("GLASS_DESKTOP_REUSABLE_E2E").as_deref()
            != Some(std::ffi::OsStr::new("1"))
        {
            return;
        }

        let root = std::env::temp_dir().join(format!(
            "glass-desktop-reusable-e2e-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let configured_workspace = root.join("connect-workspace");
        std::fs::create_dir_all(&configured_workspace).unwrap();
        let desktop_root = root.join("desktop");
        let profile = desktop_root.join("profiles/default");
        let saved_home = profile.join("home");
        let saved_workspace = profile.join("workspace");
        let supervisor_profile = profile.clone();
        let supervisor_workspace = saved_workspace.clone();
        let supervisor = thread::spawn(move || {
            supervisor::run(
                &supervisor_profile,
                &supervisor_workspace,
                process::Environment::Xfce,
            )
        });
        let startup_deadline = Instant::now() + Duration::from_secs(60);
        let mut descriptor = None;
        while Instant::now() < startup_deadline {
            match supervisor::status(&profile) {
                Ok(Some(value)) => {
                    descriptor = Some(value);
                    break;
                }
                Ok(None) => thread::sleep(Duration::from_millis(100)),
                Err(error) => panic!("reusable XFCE supervisor status failed: {error}"),
            }
        }
        let descriptor = descriptor.expect("reusable XFCE supervisor starts within 60 seconds");
        let session_id = descriptor["sessionId"].as_str().unwrap().to_owned();

        let mut gateway_a = Desktop::new(configured_workspace.clone());
        gateway_a.root = desktop_root.clone();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut attached_a = Value::Null;
        while Instant::now() < deadline {
            let status = gateway_a.status();
            if status["sessionId"] == session_id && status["available"] == true {
                attached_a = status;
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(attached_a["sessionId"], session_id);
        assert_eq!(attached_a["environment"], "xfce");
        assert_eq!(attached_a["lifecycle"], "reusable");
        std::fs::write(saved_home.join("restart-marker"), "kept").unwrap();
        std::fs::write(saved_workspace.join("restart-marker"), "kept").unwrap();
        gateway_a.set_lease("take-control").unwrap();

        gateway_a.gateway_shutdown();
        drop(gateway_a);

        let mut gateway_b = Desktop::new(configured_workspace);
        gateway_b.root = desktop_root;
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut attached = Value::Null;
        while Instant::now() < deadline {
            let status = gateway_b.status();
            if status["sessionId"] == session_id && status["available"] == true {
                attached = status;
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(attached["sessionId"], session_id);
        assert_eq!(attached["lease"], "human");
        assert_eq!(attached["lifecycle"], "reusable");
        assert_eq!(attached["profileId"], DEFAULT_PROFILE_ID);
        assert_eq!(
            attached["workspaceRoot"],
            saved_workspace.to_string_lossy().as_ref()
        );
        assert!(saved_home.join("restart-marker").is_file());
        assert!(saved_workspace.join("restart-marker").is_file());

        assert_eq!(gateway_b.stop().unwrap()["stopped"], true);
        assert!(supervisor.join().unwrap().is_ok());
        assert!(saved_home.is_dir());
        assert!(saved_workspace.join("restart-marker").is_file());
        assert_eq!(
            gateway_b.delete_profile(DEFAULT_PROFILE_ID).unwrap()["deleted"],
            true
        );
        assert!(!profile.exists());
        let _ = std::fs::remove_dir_all(root);
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
        let mut desktop = Desktop::new(workspace.clone());
        desktop.root = root.join("sessions");
        desktop.start().unwrap();
        let expected_saved_value = "glass desktop saved 7142";
        let saved_value_path = root.join("saved-value.txt");

        let (session_id, display, auth_file, session_bus, atspi_bus, home, runtime_directory) = {
            let guard = desktop.session.lock().unwrap();
            let session = guard.as_ref().unwrap();
            (
                session.id.clone(),
                session.display.clone(),
                session.directory.join("Xauthority"),
                session.session_bus_address.clone(),
                std::fs::read_to_string(session.directory.join("atspi-bus.address")).unwrap(),
                session.directory.join("home"),
                session.directory.join("runtime"),
            )
        };
        if std::env::var_os("GLASS_DESKTOP_BROWSER_MODEL_E2E").as_deref()
            == Some(std::ffi::OsStr::new("1"))
        {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_live_browser_workflow(&desktop, &root, &session_id)
            }));
            let stop = desktop.stop().map_err(|error| error.to_string());
            let cleanup = if stop.is_ok() {
                std::fs::remove_dir_all(&root).map_err(|error| error.to_string())
            } else {
                Err("kept isolated test files because desktop.stop failed".to_owned())
            };
            if let Err(panic) = outcome {
                if let Err(error) = &stop {
                    eprintln!("managed browser test desktop cleanup failed: {error}");
                }
                if let Err(error) = &cleanup {
                    eprintln!("managed browser test file cleanup failed: {error}");
                }
                std::panic::resume_unwind(panic);
            }
            stop.unwrap_or_else(|error| panic!("stopping managed browser test desktop: {error}"));
            cleanup.unwrap_or_else(|error| panic!("removing managed browser test files: {error}"));
            return;
        }
        if std::env::var_os("GLASS_DESKTOP_A11Y_MODEL_E2E").as_deref()
            == Some(std::ffi::OsStr::new("1"))
        {
            run_live_mousepad_workflow(&desktop, &root, &session_id, &workspace);
            desktop.stop().unwrap();
            std::fs::remove_dir_all(&root).unwrap();
            return;
        }
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
            .env("DISPLAY", &display)
            .env("XAUTHORITY", &auth_file)
            .env("DBUS_SESSION_BUS_ADDRESS", &session_bus)
            .env("AT_SPI_BUS_ADDRESS", &atspi_bus)
            .env("GDK_BACKEND", "x11")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("WAYLAND_SOCKET")
            .env("GTK_A11Y", "atspi")
            .env("GTK_THEME", "Adwaita")
            .env_remove("NO_AT_BRIDGE")
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_CACHE_HOME", home.join(".cache"))
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("XDG_RUNTIME_DIR", &runtime_directory)
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
                if let Ok(value) =
                    desktop.observe_accessibility(Some(&session_id), Some("Zenity text"))
                {
                    last_snapshot = Some(value.clone());
                    let zenity_visible = value["applications"].as_array().is_some_and(|apps| {
                        apps.iter().any(|app| {
                            app.as_str()
                                .is_some_and(|name| name.to_ascii_lowercase().contains("zenity"))
                        })
                    });
                    let visible_dialog = value["nodes"].as_array().is_some_and(|nodes| {
                        nodes
                            .iter()
                            .any(|node| node["owningWindow"]["name"] == "Glass accessibility test")
                    });
                    let editable_target = value["nodes"].as_array().is_some_and(|nodes| {
                        nodes.iter().any(|node| {
                            node["application"] == "zenity"
                                && !node["targetId"].is_null()
                                && !node["value"].is_null()
                        })
                    });
                    if zenity_visible && visible_dialog && editable_target {
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
                    "managed Xfce accessibility tree becomes observable; latest: {last_snapshot:?}; diagnostics: {diagnostic}"
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
            assert!(
                first["nodes"].as_array().is_some_and(|nodes| {
                    nodes
                        .iter()
                        .any(|node| node["owningWindow"]["name"] == "Glass accessibility test")
                }),
                "the snapshot should identify the test dialog that owns the field: {first}"
            );
            let first_target = test_target(&first, |node| node["editable"] == true);

            let save_search = desktop
                .observe_accessibility(Some(&session_id), Some("Zenity Save"))
                .unwrap();
            assert_eq!(save_search["query"], "Zenity Save");
            assert!(
                save_search["nodes"].as_array().is_some_and(|nodes| {
                    nodes
                        .iter()
                        .any(|node| node["name"] == "Save" && !node["targetId"].is_null())
                }),
                "semantic search should return the actionable Save control: {save_search}"
            );

            let second = desktop
                .observe_accessibility(Some(&session_id), Some("Zenity text"))
                .unwrap();
            let stale = desktop.act_accessibility(&json!({
                "sessionId": session_id,
                "revision": first["revision"],
                "targetId": first_target["targetId"],
                "action": "set_text",
                "text": "glass stale token",
            }));
            assert!(stale.unwrap_err().contains("stale"));

            let field = test_target(&second, |node| node["editable"] == true);
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
                edited["value"], "glass accessibility probe",
                "desktop_act should return its fresh readback value"
            );

            let before_lease = desktop
                .observe_accessibility(Some(&session_id), Some("Zenity text"))
                .unwrap();
            let field = test_target(&before_lease, |node| node["editable"] == true);
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

            let output =
                run_accessibility_agent_turn(&desktop, &root, &session_id, expected_saved_value);
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

        if std::env::var_os("GLASS_DESKTOP_A11Y_MODEL_E2E").as_deref()
            == Some(std::ffi::OsStr::new("1"))
        {
            run_live_mousepad_workflow(&desktop, &root, &session_id, &workspace);
        } else {
            let mut mousepad = Command::new("mousepad")
                .args(["--disable-server"])
                .env("DISPLAY", &display)
                .env("XAUTHORITY", &auth_file)
                .env("DBUS_SESSION_BUS_ADDRESS", &session_bus)
                .env("AT_SPI_BUS_ADDRESS", &atspi_bus)
                .env("GDK_BACKEND", "x11")
                .env_remove("WAYLAND_DISPLAY")
                .env_remove("WAYLAND_SOCKET")
                .env("GTK_A11Y", "atspi")
                .env("GTK_THEME", "Adwaita")
                .env_remove("NO_AT_BRIDGE")
                .env("HOME", &home)
                .env("XDG_CONFIG_HOME", home.join(".config"))
                .env("XDG_CACHE_HOME", home.join(".cache"))
                .env("XDG_DATA_HOME", home.join(".local/share"))
                .env("XDG_RUNTIME_DIR", &runtime_directory)
                .current_dir(&workspace)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(std::fs::File::create(root.join("mousepad-stderr.txt")).unwrap())
                .spawn()
                .expect("start Mousepad in the isolated managed desktop");
            let mousepad_deadline = Instant::now() + Duration::from_secs(18);
            let mut mousepad_snapshot = None;
            let mut last_mousepad_snapshot = None;
            while Instant::now() < mousepad_deadline {
                if let Ok(snapshot) =
                    desktop.observe_accessibility(Some(&session_id), Some("Mousepad text"))
                {
                    last_mousepad_snapshot = Some(snapshot.clone());
                    let mousepad_focused = snapshot["focusedWindow"]["application"]
                        .as_str()
                        .is_some_and(|name| name.to_ascii_lowercase().contains("mousepad"));
                    let mousepad_editable = snapshot["nodes"].as_array().is_some_and(|nodes| {
                        nodes.iter().any(|node| {
                            node["application"]
                                .as_str()
                                .is_some_and(|name| name.to_ascii_lowercase().contains("mousepad"))
                                && !node["targetId"].is_null()
                                && !node["value"].is_null()
                        })
                    });
                    if mousepad_focused && mousepad_editable {
                        mousepad_snapshot = Some(snapshot);
                        break;
                    }
                }
                if mousepad.try_wait().unwrap().is_some() {
                    panic!("Mousepad exited before its focused accessibility view appeared");
                }
                thread::sleep(Duration::from_millis(100));
            }
            let mousepad_snapshot = mousepad_snapshot.unwrap_or_else(|| {
                let stderr =
                    std::fs::read_to_string(root.join("mousepad-stderr.txt")).unwrap_or_default();
                panic!(
                    "Mousepad focused editable view was not observed; latest: {last_mousepad_snapshot:?}; diagnostics: {stderr}"
                );
            });
            let editor = test_target(&mousepad_snapshot, |node| {
                node["application"]
                    .as_str()
                    .is_some_and(|name| name.to_ascii_lowercase().contains("mousepad"))
                    && !node["value"].is_null()
            });
            let edited = desktop
                .act_accessibility(&json!({
                    "sessionId": session_id,
                    "revision": mousepad_snapshot["revision"],
                    "targetId": editor["targetId"],
                    "action": "set_text",
                    "text": "Mousepad AT-SPI focus probe — café 東京",
                }))
                .unwrap_or_else(|error| panic!("could not edit Mousepad target {editor}: {error}"));
            assert_eq!(edited["verified"], true, "Mousepad readback: {edited}");
            let _ = mousepad.kill();
            let _ = mousepad.wait();
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

    fn live_desktop_tools() -> Vec<(String, String, String)> {
        crate::provider::desktop_tool_schemas(true)
            .into_iter()
            .filter(|(name, _, _)| {
                matches!(
                    name.as_str(),
                    "desktop_observe" | "desktop_act" | "desktop_input" | "desktop_launch_app"
                )
            })
            .collect()
    }

    fn run_live_browser_workflow(desktop: &Desktop, root: &Path, session_id: &str) {
        assert_eq!(
            desktop.status()["sessionId"].as_str(),
            Some(session_id),
            "browser workflow must use its own isolated managed desktop"
        );
        let config_path = crate::config::default_path();
        let mut config = if config_path.exists() {
            crate::config::load(&config_path)
                .unwrap_or_else(|_| panic!("Could not load the configured free provider"))
        } else {
            crate::config::Config::default()
        };
        crate::auth::apply(&mut config)
            .unwrap_or_else(|_| panic!("Could not apply the configured provider connection"));
        let endpoint = url::Url::parse(&config.base_url)
            .unwrap_or_else(|_| panic!("The configured free provider endpoint is invalid"));
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model.to_ascii_lowercase(), "kilo-auto/free");
        assert_eq!(endpoint.host_str(), Some("127.0.0.1"));
        assert_eq!(endpoint.port(), Some(20128));
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

        let session_dir = root.join("live-browser-agent-turn");
        std::fs::create_dir_all(&session_dir).unwrap();
        let mut session = crate::session::Session::create(&session_dir).unwrap();
        let browser_binary = std::env::var_os("GLASS_DESKTOP_BROWSER_BINARY")
            .map(PathBuf::from)
            .expect("set GLASS_DESKTOP_BROWSER_BINARY to the managed browser executable")
            .canonicalize()
            .expect("the managed browser executable must exist");
        let (managed_home, browser_profile) = {
            let guard = desktop.session.lock().unwrap();
            let managed_session = guard.as_ref().expect("managed desktop session is active");
            (
                managed_session.directory.join("home"),
                managed_session.directory.join("home/chromium-profile"),
            )
        };
        write_test_browser_entry(&managed_home, &browser_binary, &browser_profile)
            .expect("a private managed Chromium desktop entry should be created");
        session.append(crate::session::Record::User(
            r#"Use only desktop tools in this isolated managed desktop. First call desktop_launch_app list with this sessionId, then launch the listed Managed Chromium (Glass acceptance) entry; launch is unverified, so confirm the browser window with desktop_observe query Chrome. Navigate to the public Rust Book homepage with Control+L, type https://doc.rust-lang.org/book/, and press Enter. Use desktop_observe query Chrome for browser observations and desktop_input key names Control, Enter, ArrowLeft, and ArrowRight; desktop_input is bound to this session and takes no sessionId. The Rust Book homepage's table of contents is collapsed. Query `Chrome Toggle Table of Contents label`; only if the fresh result has pointerFallback:true, onscreen:true, targetId:null, queryAmbiguous:false, and truncated:false, use its pointerTarget in one pointer-click. Then query `Chrome The Rust Programming Language Understanding Ownership link` and choose only the exact Understanding Ownership link in the observed Rust Book window. If desktop_act rejects that link because it has no accessibility click action, repeat the exact query and use only a fresh pointerTarget from a complete, unique, onscreen result. After it opens, call desktop_observe query Chrome References and Borrowing link and click only that exact result. On that page call desktop_observe query Chrome The Slice Type link and use that exact result. Verify the heading and current address/title. From that observed page, find the The Slice Type link, focus it with desktop_act using its current sessionId/revision/targetId, then immediately use Control+Enter to open it in a second tab. Verify both tabs appear together and have distinct titles/addresses, with the original References page still present. In the Slice tab call desktop_observe query Chrome String slices link and follow only that exact returned link; verify the #string-slices address. Then use Alt+ArrowLeft and Alt+ArrowRight and verify the address returns to the base slice URL and then the #string-slices URL. Switch back with Control+Shift+Tab and verify References and Borrowing is still open in the first tab. Never type a deep-link URL directly; each chapter and section route must come from an observed link. Target IDs are bound to the latest observation revision; desktop_act requires sessionId, revision, and targetId from that same snapshot. Use only desktop_launch_app, desktop_observe, desktop_act, and desktop_input. Do not use terminal/shell, OS command entry, filesystem, network, CDP, Playwright, browser-specific automation, or any other desktop. Report only pages, tabs, and navigation steps actually verified."#
                .into(),
        ));
        let mut state = crate::tools::State::new();
        let mods = Vec::new();
        let mut extensions = crate::ext::Registry::empty();
        #[derive(Default)]
        struct Trace {
            routes: Vec<String>,
            observations: usize,
            errors: usize,
            browser_seen: bool,
            home_seen: bool,
            ownership_seen: bool,
            references_seen: bool,
            slices_seen: bool,
            slice_page_seen: bool,
            two_tab_snapshot_seen: bool,
            url_values: Vec<String>,
            url_sequence: Vec<String>,
            tab_snapshots: Vec<Vec<String>>,
            tab_names: Vec<String>,
            window_names: Vec<String>,
            browser_node_inventory: Vec<String>,
        }
        let trace = Arc::new(Mutex::new(Trace::default()));
        let action_trace = Arc::clone(&trace);
        let desktop_ref = desktop;
        let started_at = Instant::now();
        let mut agent = crate::agent::Agent::new(
            provider.as_ref(),
            &mut session,
            &mut state,
            &mods,
            &mut extensions,
            &session_dir,
        )
        .with_prompt(format!(
            r#"Work only in managed session {session_id}. Call desktop_launch_app list with that sessionId and launch the listed Managed Chromium (Glass acceptance) entry. Launch is unverified: confirm the browser window with desktop_observe query Chrome. Use Control+L, type https://doc.rust-lang.org/book/, then key-tap Enter. desktop_input is bound to this session and takes no sessionId. Use only desktop_observe query Chrome for browser observations. The homepage's table of contents is collapsed. Query `Chrome Toggle Table of Contents label`; only if the fresh result has pointerFallback:true, onscreen:true, targetId:null, queryAmbiguous:false, and truncated:false, use its pointerTarget in one pointer-click. Then query `Chrome The Rust Programming Language Understanding Ownership link` and choose only the exact Ownership link in the observed Rust Book window. If desktop_act rejects that link because it has no accessibility click action, repeat the exact query and use only a fresh pointerTarget from a complete, unique, onscreen result. After reaching Ownership, call desktop_observe query Chrome References and Borrowing link and follow only that exact result. On that page call desktop_observe query Chrome The Slice Type link and use only that exact result. Verify the current title/address. On the References page, use the observed The Slice Type link: focus it with desktop_act using its current sessionId/revision/targetId, then press Control+Enter to open it in a second tab. Verify both distinct tabs appear in one observation and the original References page remains open. In the Slice tab call desktop_observe query Chrome String slices link and follow only the exact returned link. Verify the #string-slices address, then use Alt+ArrowLeft and Alt+ArrowRight to verify the base slice URL and return to #string-slices. Switch back with Control+Shift+Tab and verify the first tab remains on References and Borrowing. Never type a deep-link URL directly; navigate deep pages only through observed links. Target IDs require sessionId/revision from the same latest snapshot. Use only desktop_launch_app, desktop_observe, desktop_act, and desktop_input. Do not use terminal/shell, OS command entry, filesystem, network, CDP, Playwright, browser-specific tools, or another desktop. Report only verified pages and tabs."#
        ), None, None)
        .with_limits(8192, None, 64)
        .with_host_tools(live_desktop_tools(), Box::new(move |call, _, _| {
            let args = serde_json::from_str::<Value>(&call.arguments).unwrap_or_default();
            let action = args["action"].as_str().unwrap_or_default();
            action_trace.lock().unwrap().routes.push(if action.is_empty() {
                call.name.clone()
            } else {
                format!("{}:{action}", call.name)
            });
            let result = match call.name.as_str() {
                "desktop_observe" => desktop_ref
                    .observe_accessibility(
                        args["sessionId"].as_str(),
                        args["query"].as_str(),
                    )
                    .map(|value| {
                        let active_app = value["focusedWindow"]["application"]
                            .as_str()
                            .map(str::to_ascii_lowercase)
                            .unwrap_or_default();
                        let focused_name = value["focusedWindow"]["name"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned();
                        let mut trace = action_trace.lock().unwrap();
                        trace.observations += 1;
                        if active_app.contains("chrome") {
                            trace.browser_seen = true;
                            if !focused_name.is_empty()
                                && !trace.window_names.iter().any(|old| old == &focused_name)
                            {
                                trace.window_names.push(focused_name.clone());
                            }
                            let title = focused_name.to_ascii_lowercase();
                            trace.home_seen |= title.contains("the rust programming language");
                            trace.ownership_seen |= title.contains("understanding ownership");
                            trace.references_seen |= title.contains("references and borrowing");
                            trace.slice_page_seen |= title.contains("the slice type");
                        }
                        if let Some(nodes) = value["nodes"].as_array() {
                            let mut current_tabs = Vec::new();
                            let mut current_urls = Vec::new();
                            for node in nodes {
                                let name = node["name"].as_str().unwrap_or_default();
                                let value_text = node["value"].as_str().unwrap_or_default();
                                let text = format!("{name} {value_text}").to_ascii_lowercase();
                                let role = node["role"].as_str().unwrap_or_default();
                                if matches!(
                                    role,
                                    "link"
                                        | "heading"
                                        | "paragraph"
                                        | "document web"
                                        | "page tab"
                                        | "combo box"
                                        | "text entry"
                                ) && trace.browser_node_inventory.len() < 80
                                {
                                    let item = format!(
                                        "{role}: {}",
                                        name.chars().take(120).collect::<String>()
                                    );
                                    if !trace.browser_node_inventory.contains(&item) {
                                        trace.browser_node_inventory.push(item);
                                    }
                                }
                                trace.home_seen |= text.contains("the rust programming language");
                                trace.ownership_seen |= text.contains("understanding ownership");
                                trace.references_seen |= text.contains("references and borrowing");
                                trace.slices_seen |= node["role"] == "heading"
                                    && (text.contains("the slice type")
                                        || text.contains("string slices"));
                                if matches!(
                                    role,
                                    "combo box" | "text entry" | "text box"
                                ) && ["address", "search"]
                                    .iter()
                                    .any(|part| name.to_ascii_lowercase().contains(part))
                                    && !value_text.is_empty()
                                {
                                    let observed_url = if value_text.starts_with("doc.rust-lang.org/") {
                                        format!("https://{value_text}")
                                    } else if value_text.starts_with("https://doc.rust-lang.org/book/") {
                                        value_text.to_owned()
                                    } else {
                                        String::new()
                                    };
                                    if !observed_url.is_empty() {
                                        current_urls.push(observed_url.clone());
                                        trace.url_sequence.push(observed_url.clone());
                                        trace.home_seen |= observed_url
                                            == "https://doc.rust-lang.org/book/";
                                        trace.ownership_seen |= observed_url
                                            .contains("ch04-00-understanding-ownership");
                                        trace.references_seen |= observed_url
                                            .contains("ch04-02-references-and-borrowing");
                                        trace.slice_page_seen |=
                                            observed_url.contains("ch04-03-slices");
                                        if !trace.url_values.iter().any(|url| url == &observed_url) {
                                            trace.url_values.push(observed_url);
                                        }
                                    }
                                }
                                if matches!(node["role"].as_str(), Some("page tab" | "tab"))
                                    && !name.is_empty()
                                {
                                    if !trace.tab_names.iter().any(|tab| tab == name) {
                                        trace.tab_names.push(name.to_owned());
                                    }
                                    if !current_tabs.iter().any(|tab| tab == name) {
                                        current_tabs.push(name.to_owned());
                                    }
                                }
                            }
                            let has_references = current_tabs
                                .iter()
                                .any(|tab| tab.to_ascii_lowercase().contains("references and borrowing"));
                            let has_slice = current_tabs
                                .iter()
                                .any(|tab| tab.to_ascii_lowercase().contains("slice type"));
                            trace.two_tab_snapshot_seen |= current_tabs.len() >= 2
                                && has_references
                                && has_slice;
                            trace.tab_snapshots.push(current_tabs);
                            if let Some(url) = current_urls.last()
                                && url.contains("ch04-03-slices")
                            {
                                trace.slice_page_seen = true;
                            }
                        }
                        value.to_string()
                    }),
                "desktop_act" => desktop_ref
                    .act_accessibility(&args)
                    .map(|value| value.to_string()),
                "desktop_launch_app" => desktop_ref
                    .launch_app(&args)
                    .map(|value| value.to_string()),
                "desktop_input" => desktop_ref
                    .input(Lease::Agent, &args)
                    .map(|value| value.to_string())
                    .map_err(|error| error.message().to_owned()),
                _ => Err("the test host exposes only managed desktop tools".to_owned()),
            };
            if result.is_err() {
                action_trace.lock().unwrap().errors += 1;
            }
            crate::agent::HostToolOutcome::plain(match result {
                Ok(output) => crate::tools::Outcome::ok(output),
                Err(error) => crate::tools::Outcome::err(error),
            })
        }));
        let turn = agent.run_turn(|_| {});
        if let Err(error) = &turn {
            let trace = trace.lock().unwrap();
            eprintln!(
                "GLASS_DESKTOP_BROWSER_MODEL_FAILURE {}",
                json!({
                    "error": error.to_string(),
                    "toolCalls": trace.routes.len(),
                    "observations": trace.observations,
                    "toolErrors": trace.errors,
                    "routes": trace.routes,
                    "windowNames": trace.window_names,
                    "tabNames": trace.tab_names,
                    "tabSnapshots": trace.tab_snapshots,
                    "urlSequence": trace.url_sequence,
                    "observedUrls": trace.url_values,
                    "browserNodeInventory": trace.browser_node_inventory
                })
            );
            panic!("configured free-model browser task must finish");
        }
        let (output, stop) = turn.unwrap();
        {
            let trace = trace.lock().unwrap();
            eprintln!(
                "GLASS_DESKTOP_BROWSER_MODEL_TRACE {}",
                json!({
                    "stop": format!("{stop:?}"),
                    "toolCalls": trace.routes.len(),
                    "observations": trace.observations,
                    "toolErrors": trace.errors,
                    "routes": trace.routes,
                    "windowNames": trace.window_names,
                    "tabNames": trace.tab_names,
                    "tabSnapshots": trace.tab_snapshots,
                    "observedUrls": trace.url_values,
                    "urlSequence": trace.url_sequence,
                    "browserNodeInventory": trace.browser_node_inventory,
                    "slicePageSeen": trace.slice_page_seen,
                    "twoTabSnapshotSeen": trace.two_tab_snapshot_seen
                })
            );
        }
        assert_eq!(stop, crate::agent::Stop::Finished);
        assert!(!output.text.trim().is_empty());
        let trace = trace.lock().unwrap();
        assert!(
            trace
                .routes
                .iter()
                .all(|route| route.starts_with("desktop_"))
        );
        assert!(
            trace.browser_seen,
            "the model should use the managed browser"
        );
        assert!(
            trace.home_seen,
            "the model should observe the Rust Book homepage"
        );
        assert!(
            trace.ownership_seen,
            "the ownership chapter link should be observed"
        );
        assert!(trace.references_seen, "the section link should be observed");
        assert!(trace.slices_seen, "the slice chapter should be observed");
        assert!(
            trace.slice_page_seen,
            "the slice chapter page should be open"
        );
        assert!(
            trace
                .url_values
                .iter()
                .any(|url| url.contains("ch04-00-understanding-ownership"))
                && trace
                    .url_values
                    .iter()
                    .any(|url| url.contains("ch04-03-slices")),
            "distinct chapter and slice URLs must appear in the visible address field: {:?}",
            trace.url_values
        );
        assert!(
            trace.tab_names.len() >= 2,
            "both tabs should be observable: {:?}",
            trace.tab_names
        );
        assert!(
            trace.two_tab_snapshot_seen,
            "a single observation should show both the References and Slice tabs: {:?}",
            trace.tab_snapshots
        );
        assert!(
            trace.window_names.len() >= 2,
            "switching tabs should change the browser title: {:?}",
            trace.window_names
        );
        assert!(
            trace.url_sequence.windows(4).any(|urls| {
                urls[0].contains("ch04-03-slices")
                    && urls[1].contains("#string-slices")
                    && urls[2].contains("ch04-03-slices")
                    && !urls[2].contains("#string-slices")
                    && urls[3].contains("#string-slices")
            }),
            "Back then Forward should traverse the slice-section URL in order: {:?}",
            trace.url_sequence
        );
        assert!(
            trace
                .url_sequence
                .last()
                .is_some_and(|url| url.contains("ch04-00-understanding-ownership")
                    && url.contains("#references-and-borrowing")),
            "the first tab should remain on the References and Borrowing section after returning from the slice tab: {:?}",
            trace.url_sequence
        );
        eprintln!(
            "GLASS_DESKTOP_BROWSER_MODEL_WORKFLOW {}",
            json!({
                "provider": config.provider,
                "model": config.model,
                "endpointHost": endpoint.host_str(),
                "endpointPort": endpoint.port(),
                "toolCalls": trace.routes.len(),
                "observations": trace.observations,
                "toolErrors": trace.errors,
                "elapsedMs": started_at.elapsed().as_millis(),
                "routes": trace.routes,
                "windowNames": trace.window_names,
                "tabNames": trace.tab_names,
                "observedUrls": trace.url_values,
                "urlSequence": trace.url_sequence,
                "browserNodeInventory": trace.browser_node_inventory,
                "homeSeen": trace.home_seen,
                "ownershipSeen": trace.ownership_seen,
                "referencesSeen": trace.references_seen,
                "slicesSeen": trace.slices_seen
            })
        );
        drop(trace);
        drop(agent);
        drop(extensions);
        drop(session);
        std::fs::remove_dir_all(session_dir).unwrap();
    }

    fn write_test_browser_entry(home: &Path, binary: &Path, profile: &Path) -> std::io::Result<()> {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let applications = home.join(".local/share/applications");
        std::fs::create_dir_all(&applications)?;
        std::fs::set_permissions(&applications, std::fs::Permissions::from_mode(0o700))?;
        std::fs::create_dir(profile)?;
        std::fs::set_permissions(profile, std::fs::Permissions::from_mode(0o700))?;
        for path in [binary, profile] {
            if !path.is_absolute()
                || path.to_string_lossy().chars().any(|character| {
                    !(character.is_ascii_alphanumeric() || "/._-".contains(character))
                })
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "browser test paths must be absolute and desktop-entry safe",
                ));
            }
        }
        use std::io::Write;
        let mut entry = std::fs::OpenOptions::new();
        entry
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        writeln!(
            entry.open(applications.join("glass-managed-browser.desktop"))?,
            "[Desktop Entry]\nType=Application\nName=Managed Chromium (Glass acceptance)\nExec=/usr/bin/env ACCESSIBILITY_ENABLED=1 {} --user-data-dir={} --no-first-run --no-default-browser-check --password-store=basic --force-renderer-accessibility=complete\nStartupWMClass=chrome\nTerminal=false\nNoDisplay=false\n",
            binary.display(),
            profile.display()
        )
    }

    fn run_live_mousepad_workflow(
        desktop: &Desktop,
        root: &Path,
        session_id: &str,
        workspace: &Path,
    ) {
        let config_path = crate::config::default_path();
        let mut config = if config_path.exists() {
            crate::config::load(&config_path)
                .unwrap_or_else(|_| panic!("Could not load the configured free provider"))
        } else {
            crate::config::Config::default()
        };
        crate::auth::apply(&mut config)
            .unwrap_or_else(|_| panic!("Could not apply the configured provider connection"));
        let endpoint = url::Url::parse(&config.base_url)
            .unwrap_or_else(|_| panic!("The configured free provider endpoint is invalid"));
        assert_eq!(
            config.provider, "openai",
            "the guarded test expects the configured OpenAI-compatible free provider"
        );
        assert_eq!(
            config.model.to_ascii_lowercase(),
            "kilo-auto/free",
            "the live acceptance test only calls the configured free model"
        );
        assert_eq!(
            endpoint.host_str(),
            Some("127.0.0.1"),
            "the live acceptance test will only call the local provider proxy"
        );
        assert_eq!(
            endpoint.port(),
            Some(20128),
            "the live acceptance test only calls the local free-provider proxy"
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

        let note_path = workspace.join("agent-workflow-notes.txt");
        let note_contents =
            "Shared desktop note\nCafé, 東京, and collaborative control.\nVerified in Mousepad.\n";
        let session_dir = root.join("live-mousepad-agent-turn");
        std::fs::create_dir_all(&session_dir).unwrap();
        let mut session = crate::session::Session::create(&session_dir).unwrap();
        session.append(crate::session::Record::User(format!(
            "This is a bounded desktop task in your private workspace `{}`. Open Mousepad through the visible XFCE application UI, create a note with exactly these three lines (including the final newline):\n```text\n{note_contents}```\nSave it as `agent-workflow-notes.txt` in that workspace. Then open Thunar through the visible UI, navigate to and reopen that file from Thunar, and verify that the note is visible in the opened editor. Use only desktop_status, desktop_observe, desktop_act, and desktop_input. Never use shell, filesystem tools, network tools, CDP, Playwright, browser-specific semantic tools, or another desktop. For shortcuts such as Alt+F2, Ctrl+S, and Ctrl+L, use desktop_input with action `key-chord` and ordered keys, for example `keys:[\"Alt\",\"F2\"]`; use `key-tap` for one key. Do not use `key` for a chord: that action is a held-key down/up event and requires `pressed`. Target IDs are tied to the exact observation revision; refresh after actions and after a rejected stale or lease-blocked action. A brief simulated human-control handoff may reject one input; continue only after the lease is returned and you have a fresh observation. Finish with a brief completion report."
            , workspace.display()
        )));
        let mut state = crate::tools::State::new();
        let mods = Vec::new();
        let mut extensions = crate::ext::Registry::empty();
        let desktop_ref = desktop;
        #[derive(Default)]
        struct Trace {
            routes: Vec<String>,
            observations: usize,
            errors: usize,
            mousepad_focused: bool,
            interruption_armed: bool,
            lease_blocked: bool,
            lease_resumed: bool,
            observed_after_resume: bool,
            successful_action_after_resume: bool,
            thunar_window_seen: bool,
            file_name_seen: bool,
            note_text_seen: bool,
        }
        let trace = Arc::new(Mutex::new(Trace::default()));
        let action_trace = Arc::clone(&trace);
        let model_started = Instant::now();
        let expected_filename = note_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("agent-workflow-notes.txt")
            .to_owned();
        let expected_note_fragment = "Café, 東京, and collaborative control.".to_owned();
        let mut agent = crate::agent::Agent::new(
            provider.as_ref(),
            &mut session,
            &mut state,
            &mods,
            &mut extensions,
            &session_dir,
        )
        .with_prompt(
            format!(
                "You are sharing only test-owned isolated desktop session `{session_id}` with the human. Use only desktop_observe, desktop_act, and desktop_input. Open Mousepad with the visible XFCE Run Program dialog: use key-chord Alt+F2, type `mousepad`, then key-tap Enter. Type the exact requested three-line UTF-8 note into Mousepad, open Save As with Ctrl+Shift+S, and save to `{}`/agent-workflow-notes.txt. Query `Mousepad text` to inspect its compact active window and editable text box; use the Save button target when available. Then launch Thunar via Alt+F2, use Ctrl+L to enter `{}`, and open the saved file from the file manager with its visible row plus Enter. Verify the reopened note by calling desktop_observe with query `Mousepad text` and checking the bounded editor text value. Do not launch apps through shell. For shortcuts, use desktop_input action `key-chord` with ordered keys (for example [\"Alt\",\"F2\"]); use `key-tap` for one key. Never send a chord as `key`. Refresh revision-bound target IDs after acts. If the human-control lease blocks an action, re-observe after it returns and continue. Do not use any file, shell, web, or other desktop tool.",
                workspace.display(), workspace.display()
            ),
            None,
            None,
        )
        .with_limits(8192, None, 64)
        .with_host_tools(
            live_desktop_tools(),
            Box::new(move |call, _, _| {
                let args = serde_json::from_str::<Value>(&call.arguments).unwrap_or_default();
                let action_name = args["action"].as_str().unwrap_or_default();
                let is_mutation = matches!(
                    call.name.as_str(),
                    "desktop_act" | "desktop_input" | "desktop_launch_app"
                );
                let intercept = {
                    let mut trace = action_trace.lock().unwrap();
                    trace.routes.push(if action_name.is_empty() {
                        call.name.clone()
                    } else {
                        format!("{}:{action_name}", call.name)
                    });
                    trace.interruption_armed && !trace.lease_blocked && is_mutation
                };
                if intercept {
                    desktop_ref.set_lease("take-control").unwrap();
                    thread::sleep(Duration::from_millis(200));
                    action_trace.lock().unwrap().lease_blocked = true;
                }
                let result = match call.name.as_str() {
                    "desktop_status" => Ok(desktop_ref.status().to_string()),
                    "desktop_observe" => desktop_ref.observe_accessibility(
                        args["sessionId"].as_str(),
                        args["query"].as_str(),
                    ).map(|value| {
                        let active_app = value["focusedWindow"]["application"]
                            .as_str()
                            .map(str::to_ascii_lowercase)
                            .or_else(|| {
                                value["nodes"].as_array()?.iter().find_map(|node| {
                                    let active = node["role"] == "frame"
                                        && node["states"]
                                            .as_array()?
                                            .iter()
                                            .any(|state| state == "active");
                                    active.then(|| {
                                        node["application"]
                                            .as_str()
                                            .unwrap_or_default()
                                            .to_ascii_lowercase()
                                    })
                                })
                            })
                            .unwrap_or_default();
                        let focused_window_name = value["focusedWindow"]["name"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned();
                        let mut trace = action_trace.lock().unwrap();
                        trace.observations += 1;
                        if active_app.contains("mousepad") {
                            trace.mousepad_focused = true;
                            trace.interruption_armed = !trace.lease_blocked;
                        }
                        if trace.lease_resumed {
                            trace.observed_after_resume = true;
                        }
                        if active_app.contains("thunar") {
                            trace.thunar_window_seen = true;
                        }
                        if value["nodes"].as_array().is_some_and(|nodes| nodes.iter().any(|node| {
                            node["name"].as_str().is_some_and(|name| name.contains(&expected_filename))
                        })) {
                            trace.file_name_seen = true;
                        }
                        if active_app.contains("mousepad")
                            && trace.thunar_window_seen
                            && trace.file_name_seen
                            && focused_window_name.contains(&expected_filename)
                            && value["nodes"].as_array().is_some_and(|nodes| nodes.iter().any(|node| {
                                node["value"].as_str().is_some_and(|text| text.contains(&expected_note_fragment))
                            }))
                        {
                            trace.note_text_seen = true;
                        }
                        value.to_string()
                    }),
                    "desktop_act" => desktop_ref
                        .act_accessibility(&args)
                        .map(|value| value.to_string()),
                    "desktop_input" => desktop_ref
                        .input(Lease::Agent, &args)
                        .map(|value| value.to_string())
                        .map_err(|error| error.message().to_owned()),
                    _ => Err("the test host exposes only managed desktop tools".to_owned()),
                };
                if intercept {
                    let error = result
                        .as_ref()
                        .err()
                        .cloned()
                        .unwrap_or_else(|| "lease handoff failed to block the action".to_owned());
                    if !error.contains("held by the other participant") {
                        let _ = desktop_ref.set_lease("return-to-agent");
                        return crate::agent::HostToolOutcome::plain(crate::tools::Outcome::err(error));
                    }
                    desktop_ref.set_lease("return-to-agent").unwrap();
                    action_trace.lock().unwrap().lease_resumed = true;
                    return crate::agent::HostToolOutcome::plain(crate::tools::Outcome::err(error));
                }
                if result.is_err() {
                    let mut trace = action_trace.lock().unwrap();
                    trace.errors += 1;
                } else if is_mutation && action_trace.lock().unwrap().lease_resumed {
                    action_trace.lock().unwrap().successful_action_after_resume = true;
                }
                crate::agent::HostToolOutcome::plain(match result {
                    Ok(output) => crate::tools::Outcome::ok(output),
                    Err(error) => crate::tools::Outcome::err(error),
                })
            }),
        );
        let started_at = Instant::now();
        let turn = agent.run_turn(|_| {});
        assert!(
            turn.is_ok(),
            "the configured free model turn should complete"
        );
        let (output, stop) = turn.unwrap();
        assert_eq!(stop, crate::agent::Stop::Finished);
        assert!(
            !output.text.trim().is_empty(),
            "model should report completion"
        );
        let saved = std::fs::read_to_string(&note_path)
            .unwrap_or_else(|error| panic!("GUI workflow did not create the note: {error}"));
        assert_eq!(
            saved, note_contents,
            "the GUI-saved UTF-8 note must match exactly"
        );
        let trace = trace.lock().unwrap();
        assert!(
            trace
                .routes
                .iter()
                .all(|route| route.starts_with("desktop_"))
        );
        assert!(
            trace.mousepad_focused,
            "the model should open focused Mousepad"
        );
        assert!(
            trace.lease_blocked && trace.lease_resumed,
            "the human lease handoff should block and resume input"
        );
        assert!(
            trace.observed_after_resume,
            "the model should refresh its snapshot after lease return"
        );
        assert!(
            trace.successful_action_after_resume,
            "the model should continue after lease return"
        );
        assert!(
            trace.thunar_window_seen,
            "the model should open the saved file through Thunar"
        );
        assert!(
            trace.file_name_seen,
            "the model should observe the saved filename in the UI"
        );
        assert!(
            trace.note_text_seen,
            "the model should observe the reopened note contents"
        );
        eprintln!(
            "GLASS_DESKTOP_MODEL_WORKFLOW {}",
            json!({
                "provider": config.provider,
                "model": config.model,
                "endpointHost": endpoint.host_str(),
                "endpointPort": endpoint.port(),
                "toolCalls": trace.routes.len(),
                "observations": trace.observations,
                "toolErrors": trace.errors,
                "elapsedMs": started_at.elapsed().as_millis(),
                "setupAndTurnMs": model_started.elapsed().as_millis(),
                "humanLeaseBlocked": trace.lease_blocked,
                "humanLeaseResumed": trace.lease_resumed,
                "routes": trace.routes,
                "savedBytes": saved.len(),
                "savedLines": saved.lines().count(),
                "focusedMousepad": trace.mousepad_focused,
                "thunarSeen": trace.thunar_window_seen,
                "fileNameSeen": trace.file_name_seen,
                "reopenedTextSeen": trace.note_text_seen
            })
        );
        drop(trace);
        drop(agent);
        drop(extensions);
        drop(session);
        std::fs::remove_dir_all(session_dir).unwrap();
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
                            arguments: json!({"sessionId": self.session_id,"query":"Zenity Save"})
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
                            arguments: json!({"sessionId": self.session_id,"query":"Zenity text"})
                                .to_string(),
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
                            arguments: json!({"sessionId": self.session_id,"query":"Zenity text"})
                                .to_string(),
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
                        assert!(state.actions.iter().any(|action| action == "set_text"));
                        crate::provider::Event::ToolCall {
                            id: "refresh-save-control".into(),
                            name: "desktop_observe".into(),
                            arguments: json!({"sessionId": self.session_id,"query":"Zenity Save"})
                                .to_string(),
                        }
                    }
                    6 => {
                        let state = self.state.lock().unwrap();
                        let snapshot = state.observations.last().ok_or_else(|| {
                            crate::provider::Error::Config("save query did not run".into())
                        })?;
                        let save = test_target(snapshot, |node| node["name"] == "Save");
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
        assert_eq!(provider.turn.load(Ordering::SeqCst), 8);
        drop(agent);
        drop(extensions);
        drop(session);
        std::fs::remove_dir_all(session_dir).unwrap();
        output.text
    }
}
