//! Owns a reusable desktop outside the short-lived Connect gateway.

use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    os::fd::AsRawFd,
    os::unix::{
        fs::{FileTypeExt, OpenOptionsExt, PermissionsExt},
        net::{UnixListener, UnixStream},
        process::CommandExt,
    },
    path::{Component, Path},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(super) const SOCKET_NAME: &str = "supervisor.sock";
const STATE_NAME: &str = "session.json";
const MAX_MESSAGE: usize = 64 * 1024;

fn valid_profile_path(profile: &Path) -> Result<bool, String> {
    let parent = profile
        .parent()
        .ok_or("Reusable desktop profile has no parent directory")?;
    let root = parent
        .parent()
        .ok_or("Reusable desktop profiles path has no parent directory")?;
    let root_metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("Checking desktop storage: {error}")),
    };
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err("Desktop storage path must be a real directory".into());
    }
    let parent_metadata = match fs::symlink_metadata(parent) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("Checking reusable profile parent: {error}")),
    };
    if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
        return Err("Reusable desktop profiles path must be a real directory".into());
    }
    match fs::symlink_metadata(profile) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err("Reusable desktop profile path must be a real directory".into())
        }
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("Checking reusable desktop profile: {error}")),
    }
}

fn ensure_profile_path(profile: &Path) -> Result<(), String> {
    let parent = profile
        .parent()
        .ok_or("Reusable desktop profile has no parent directory")?;
    let root = parent
        .parent()
        .ok_or("Reusable desktop profiles path has no parent directory")?;
    ensure_private_directory(root, "Desktop storage")?;
    ensure_private_directory(parent, "Reusable desktop profiles")?;
    ensure_private_directory(profile, "Reusable desktop profile")
}

fn ensure_private_directory(path: &Path, label: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(format!("{label} path must be a real directory"));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|error| format!("Creating {label}: {error}"))?;
        }
        Err(error) => return Err(format!("Checking {label}: {error}")),
    }
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("Checking {label}: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!("{label} path must be a real directory"));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("Setting {label} permissions: {error}"))
}

pub(super) fn status(profile: &Path) -> Result<Option<Value>, String> {
    if !valid_profile_path(profile)? {
        return Ok(None);
    }
    let socket = profile.join(SOCKET_NAME);
    match fs::symlink_metadata(&socket) {
        Ok(metadata) if metadata.file_type().is_socket() => {}
        Ok(_) => return Err("Reusable supervisor socket path is not a socket".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if !supervisor_process_alive(profile) {
                discard_stale_state(profile);
            }
            return Ok(None);
        }
        Err(error) => return Err(format!("Checking reusable supervisor socket: {error}")),
    }
    match request(&socket, json!({"action":"status"})) {
        Ok(value) => Ok(Some(value)),
        Err(error) => {
            if supervisor_process_alive(profile) {
                Err(error)
            } else {
                discard_stale_state(profile);
                Ok(None)
            }
        }
    }
}

pub(super) fn start_or_attach(
    profile: &Path,
    workspace: &Path,
    environment: super::process::Environment,
) -> Result<Value, String> {
    if let Some(value) = status(profile).unwrap_or(None) {
        return requested_environment(value, environment);
    }
    if supervisor_process_alive(profile) {
        let deadline = Instant::now() + Duration::from_secs(45);
        while Instant::now() < deadline {
            if let Some(value) = status(profile).unwrap_or(None) {
                return requested_environment(value, environment);
            }
            if !supervisor_process_alive(profile) {
                discard_stale_state(profile);
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        if supervisor_process_alive(profile) {
            return Err("Reusable desktop supervisor is still starting".into());
        }
    }
    ensure_profile_path(profile)?;
    let home = profile.join("home");
    ensure_private_directory(&home, "Reusable desktop HOME")?;
    ensure_private_directory(workspace, "Reusable desktop workspace")?;
    let log_path = profile.join("supervisor.log");
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&log_path)
        .map_err(|error| format!("Reusable desktop supervisor log: {error}"))?;
    log.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("Reusable desktop supervisor log permissions: {error}"))?;
    let executable = std::env::current_exe()
        .map_err(|error| format!("Locating Core for desktop supervisor: {error}"))?;
    let mut command = Command::new(executable);
    command
        .args([
            "connect",
            "_desktop-supervisor",
            profile.to_str().ok_or("Invalid reusable profile path")?,
            workspace
                .to_str()
                .ok_or("Invalid reusable workspace path")?,
            environment.as_str(),
        ])
        .env_clear()
        .env("PATH", super::process::MANAGED_CHILD_PATH)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("RUST_LOG", "warn")
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            log.try_clone().map_err(|error| error.to_string())?,
        ))
        .stderr(Stdio::from(log));
    for name in [
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "LC_MESSAGES",
        "LC_COLLATE",
        "LC_TIME",
        "LC_NUMERIC",
        "LC_MONETARY",
        "LANGUAGE",
        "TZ",
        "USER",
        "LOGNAME",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("Starting reusable desktop supervisor: {error}"))?;
    let socket = profile.join(SOCKET_NAME);
    let deadline = Instant::now() + Duration::from_secs(45);
    while Instant::now() < deadline {
        if let Ok(Some(value)) = status(profile) {
            return requested_environment(value, environment);
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Checking reusable desktop supervisor: {error}"))?
        {
            let details = fs::read_to_string(&log_path).unwrap_or_default();
            let details = details.lines().rev().take(8).collect::<Vec<_>>().join("\n");
            return Err(if details.is_empty() {
                format!("Reusable desktop supervisor exited with {status}")
            } else {
                format!("Reusable desktop supervisor exited with {status}: {details}")
            });
        }
        thread::sleep(Duration::from_millis(100));
    }
    terminate_supervisor(&mut child, profile);
    Err(format!(
        "Reusable desktop supervisor did not become ready at {}",
        socket.display()
    ))
}

fn requested_environment(
    value: Value,
    environment: super::process::Environment,
) -> Result<Value, String> {
    if value["environment"].as_str() != Some(environment.as_str()) {
        return Err(format!(
            "The reusable desktop profile is already running {}",
            value["environment"]
                .as_str()
                .unwrap_or("an unknown environment")
        ));
    }
    Ok(value)
}

fn terminate_supervisor(child: &mut Child, profile: &Path) {
    if let Ok(pid) = i32::try_from(child.id()) {
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() {
            discard_stale_state(profile);
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    discard_stale_state(profile);
}

pub(super) fn stop(profile: &Path) -> Result<bool, String> {
    let socket = profile.join(SOCKET_NAME);
    if !valid_profile_path(profile)? {
        return Ok(false);
    }
    match fs::symlink_metadata(&socket) {
        Ok(metadata) if metadata.file_type().is_socket() => {}
        Ok(_) => return Err("Reusable supervisor socket path is not a socket".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            discard_stale_state(profile);
            return Ok(false);
        }
        Err(error) => return Err(format!("Checking reusable supervisor socket: {error}")),
    }
    request(&socket, json!({"action":"stop"}))?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while socket.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }
    if socket.exists() {
        return Err("Reusable desktop supervisor did not stop within the timeout".into());
    }
    Ok(true)
}

pub(super) fn run(
    profile: &Path,
    workspace: &Path,
    environment: super::process::Environment,
) -> Result<(), String> {
    super::arm_shutdown();
    ensure_profile_path(profile)?;
    let _supervisor_lock = lock_supervisor(profile)?;
    let home = profile.join("home");
    let saved_workspace = profile.join("workspace");
    ensure_private_directory(&home, "Reusable desktop HOME")?;
    ensure_private_directory(&saved_workspace, "Reusable desktop workspace")?;
    let _initial_workspace = workspace;
    let workspace = saved_workspace.as_path();
    let session_root = profile.join("sessions");
    ensure_private_directory(&session_root, "Reusable desktop sessions")?;
    let session =
        super::process::start_supervised_session(workspace, &session_root, &home, environment)?;
    let descriptor = json!({
        "available": true,
        "sessionId": session.id,
        "environment": session.environment.as_str(),
        "lifecycle": "reusable",
        "profileId": super::DEFAULT_PROFILE_ID,
        "displayId": session.display,
        "workspaceRoot": session.workspace,
        "sessionDirectory": session.directory,
        "cookie": session.auth_cookie.iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
        "sessionBusAddress": session.session_bus_address,
        "lease": session.lease.as_str(),
        "xvfbPid": session.xvfb.as_ref().map(Child::id),
        "xvfbStartTime": session.xvfb.as_ref().and_then(|child| process_start_time(child.id())),
        "desktopPid": session.desktop.as_ref().map(Child::id),
        "desktopStartTime": session.desktop.as_ref().and_then(|child| process_start_time(child.id())),
        "supervisorPid": std::process::id(),
        "supervisorStartTime": process_start_time(std::process::id())
    });
    write_state(profile, &descriptor)?;
    let socket = profile.join(SOCKET_NAME);
    match fs::symlink_metadata(&socket) {
        Ok(metadata) if metadata.file_type().is_socket() => {
            fs::remove_file(&socket)
                .map_err(|error| format!("Removing stale reusable supervisor socket: {error}"))?;
        }
        Ok(_) => return Err("Reusable desktop supervisor socket is not a socket".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Checking reusable desktop supervisor socket: {error}"
            ));
        }
    }
    let listener = UnixListener::bind(&socket)
        .map_err(|error| format!("Starting reusable desktop supervisor socket: {error}"))?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("Reusable desktop supervisor socket permissions: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("Configuring reusable desktop supervisor: {error}"))?;
    let mut session = session;
    while !super::stopping() {
        if session
            .desktop
            .as_mut()
            .is_none_or(|desktop| desktop.try_wait().ok().flatten().is_some())
            || session
                .xvfb
                .as_mut()
                .is_none_or(|xvfb| xvfb.try_wait().ok().flatten().is_some())
        {
            return Err("GNOME desktop session exited while its supervisor was active".into());
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                if !peer_is_current_user(&stream).unwrap_or(false)
                    || stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .is_err()
                    || stream
                        .set_write_timeout(Some(Duration::from_secs(2)))
                        .is_err()
                {
                    continue;
                }
                let mut line = String::new();
                let Ok(read_stream) = stream.try_clone() else {
                    continue;
                };
                if BufReader::new(read_stream)
                    .take(MAX_MESSAGE as u64 + 1)
                    .read_line(&mut line)
                    .is_err()
                    || line.len() > MAX_MESSAGE
                    || !line.ends_with('\n')
                {
                    let _ = write_error(&mut stream, "Invalid reusable desktop request");
                    continue;
                }
                let request: Value = match serde_json::from_str(&line) {
                    Ok(request) => request,
                    Err(_) => {
                        let _ = write_error(&mut stream, "Invalid reusable desktop request");
                        continue;
                    }
                };
                let result = match request["action"].as_str() {
                    Some("status") => read_state(profile),
                    Some("stop") => Ok(json!({"stopped":true})),
                    _ => Err("Unsupported reusable desktop supervisor action".to_owned()),
                };
                let should_stop = request["action"].as_str() == Some("stop") && result.is_ok();
                let response = result.unwrap_or_else(|error| json!({"error":error}));
                if serde_json::to_writer(&mut stream, &response).is_err()
                    || stream.write_all(b"\n").is_err()
                {
                    continue;
                }
                if should_stop {
                    break;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(format!("Reusable desktop supervisor failed: {error}")),
        }
    }
    drop(session);
    let _ = fs::remove_file(profile.join(STATE_NAME));
    let _ = fs::remove_file(socket);
    Ok(())
}

fn lock_supervisor(profile: &Path) -> Result<File, String> {
    let path = profile.join("supervisor.lock");
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let file = options
        .open(path)
        .map_err(|error| format!("Opening reusable supervisor lock: {error}"))?;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("Setting reusable supervisor lock permissions: {error}"))?;
    fs2::FileExt::try_lock_exclusive(&file)
        .map_err(|_| "Another reusable desktop supervisor is already active".to_owned())?;
    Ok(file)
}

fn write_error(stream: &mut UnixStream, error: &str) -> Result<(), String> {
    serde_json::to_writer(&mut *stream, &json!({"error":error}))
        .map_err(|error| error.to_string())?;
    stream
        .write_all(b"\n")
        .map_err(|error| format!("Writing reusable desktop response: {error}"))
}

#[cfg(target_os = "linux")]
fn peer_is_current_user(stream: &UnixStream) -> std::io::Result<bool> {
    let mut credentials = std::mem::MaybeUninit::<libc::ucred>::zeroed();
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            credentials.as_mut_ptr().cast(),
            &mut length,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let credentials = unsafe { credentials.assume_init() };
    Ok(credentials.uid == unsafe { libc::geteuid() })
}

#[cfg(any(
    target_os = "macos",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
fn peer_is_current_user(stream: &UnixStream) -> std::io::Result<bool> {
    let mut uid = 0;
    let mut gid = 0;
    let result = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
    if result != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(uid == unsafe { libc::geteuid() })
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
)))]
fn peer_is_current_user(_: &UnixStream) -> std::io::Result<bool> {
    Ok(false)
}

pub(super) fn write_lease(profile: &Path, lease: &str) -> Result<(), String> {
    if !matches!(lease, "agent" | "human") {
        return Err("Invalid desktop lease state".into());
    }
    let Some(mut state) = status(profile)? else {
        return Err("Reusable desktop supervisor is not available".into());
    };
    state["lease"] = json!(lease);
    write_state(profile, &state)
}

fn request(socket: &Path, request: Value) -> Result<Value, String> {
    let mut stream = UnixStream::connect(socket)
        .map_err(|error| format!("Reusable desktop supervisor is unavailable: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| error.to_string())?;
    stream
        .write_all(format!("{request}\n").as_bytes())
        .map_err(|error| format!("Sending reusable desktop request: {error}"))?;
    let mut response = String::new();
    BufReader::new(stream)
        .take(MAX_MESSAGE as u64)
        .read_line(&mut response)
        .map_err(|error| format!("Reading reusable desktop response: {error}"))?;
    let response: Value = serde_json::from_str(&response)
        .map_err(|error| format!("Invalid reusable desktop response: {error}"))?;
    if let Some(error) = response["error"].as_str() {
        return Err(error.to_owned());
    }
    Ok(response)
}

fn write_state(profile: &Path, value: &Value) -> Result<(), String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = profile.join(format!("{STATE_NAME}.tmp-{}-{nonce}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|error| format!("Creating reusable desktop state: {error}"))?;
    serde_json::to_writer(&mut file, value)
        .map_err(|error| format!("Writing reusable desktop state: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("Syncing reusable desktop state: {error}"))?;
    fs::rename(&temporary, profile.join(STATE_NAME))
        .map_err(|error| format!("Saving reusable desktop state: {error}"))
}

fn read_state(profile: &Path) -> Result<Value, String> {
    let state_path = profile.join(STATE_NAME);
    let metadata = fs::symlink_metadata(&state_path)
        .map_err(|error| format!("Reading reusable desktop state: {error}"))?;
    if metadata.file_type().is_symlink() || metadata.len() > MAX_MESSAGE as u64 {
        return Err("Reusable desktop state file is invalid".into());
    }
    let state: Value = serde_json::from_slice(
        &fs::read(&state_path)
            .map_err(|error| format!("Reading reusable desktop state: {error}"))?,
    )
    .map_err(|error| format!("Parsing reusable desktop state: {error}"))?;
    Ok(state)
}

fn discard_stale_state(profile: &Path) {
    if !valid_profile_path(profile).unwrap_or(false) {
        return;
    }
    let Ok(state) = read_state(profile) else {
        return;
    };
    for name in ["xvfbPid", "desktopPid"] {
        let Some(pid) = state[name].as_u64().and_then(|pid| i32::try_from(pid).ok()) else {
            continue;
        };
        let Some(start_time) = state[format!("{name}StartTime")].as_u64() else {
            continue;
        };
        if process_group_leader_matches(pid, start_time) {
            unsafe {
                libc::kill(-pid, libc::SIGTERM);
            }
        }
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let alive = ["xvfbPid", "desktopPid"].into_iter().any(|name| {
            state[name]
                .as_u64()
                .and_then(|pid| i32::try_from(pid).ok())
                .zip(state[format!("{name}StartTime")].as_u64())
                .is_some_and(|(pid, started)| process_group_leader_matches(pid, started))
        });
        if !alive {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    for name in ["xvfbPid", "desktopPid"] {
        if let Some(pid) = state[name].as_u64().and_then(|pid| i32::try_from(pid).ok())
            && let Some(started) = state[format!("{name}StartTime")].as_u64()
            && process_group_leader_matches(pid, started)
        {
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
    }
    if let Some(directory) = state["sessionDirectory"].as_str() {
        let path = Path::new(directory);
        if is_managed_session_directory(profile, path) {
            let _ = fs::remove_dir_all(path);
        }
    }
    let _ = fs::remove_file(profile.join(STATE_NAME));
}

fn process_group_leader_matches(pid: i32, start_time: u64) -> bool {
    pid > 1
        && process_start_time_for(pid) == Some(start_time)
        && unsafe { libc::getpgid(pid) } == pid
}

fn is_managed_session_directory(profile: &Path, path: &Path) -> bool {
    if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return false;
    }
    let sessions = profile.join("sessions");
    if !path.starts_with(&sessions) {
        return false;
    }
    let Ok(sessions_metadata) = fs::symlink_metadata(&sessions) else {
        return false;
    };
    if sessions_metadata.file_type().is_symlink() || !sessions_metadata.is_dir() {
        return false;
    }
    let Ok(path_metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if path_metadata.file_type().is_symlink() || !path_metadata.is_dir() {
        return false;
    }
    let Ok(sessions) = fs::canonicalize(sessions) else {
        return false;
    };
    let Ok(path) = fs::canonicalize(path) else {
        return false;
    };
    path != sessions && path.starts_with(sessions)
}

fn process_start_time(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_comm = stat.rsplit_once(')')?.1;
    after_comm.split_whitespace().nth(19)?.parse().ok()
}

fn process_start_time_for(pid: i32) -> Option<u64> {
    u32::try_from(pid).ok().and_then(process_start_time)
}

fn supervisor_process_alive(profile: &Path) -> bool {
    let Ok(state) = read_state(profile) else {
        return false;
    };
    let Some(pid) = state["supervisorPid"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
    else {
        return false;
    };
    let Some(started) = state["supervisorStartTime"].as_u64() else {
        return false;
    };
    process_start_time(pid) == Some(started)
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_private_directory, ensure_profile_path, is_managed_session_directory,
        process_group_leader_matches, terminate_supervisor,
    };
    use std::{
        fs,
        os::unix::fs::symlink,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temporary_directory(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "glass-desktop-supervisor-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn reusable_profile_rejects_symlinked_parent_and_profile() {
        let root = temporary_directory("profile-links");
        let profiles = root.join("profiles");
        fs::create_dir(&profiles).unwrap();
        let outside = root.join("outside");
        fs::create_dir(&outside).unwrap();

        let profile_link = profiles.join("default");
        symlink(&outside, &profile_link).unwrap();
        assert!(ensure_profile_path(&profile_link).is_err());
        fs::remove_file(&profile_link).unwrap();

        let profiles_link = root.join("linked-profiles");
        symlink(&outside, &profiles_link).unwrap();
        assert!(ensure_profile_path(&profiles_link.join("default")).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reusable_profile_rejects_symlinked_desktop_storage_root() {
        let temp = temporary_directory("storage-root-link");
        let outside = temp.join("outside");
        let storage_parent = temp.join("storage-parent");
        fs::create_dir(&outside).unwrap();
        fs::create_dir(&storage_parent).unwrap();
        let storage = storage_parent.join("desktop");
        symlink(&outside, &storage).unwrap();
        let profile = storage.join("profiles/default");
        assert!(ensure_profile_path(&profile).is_err());
        assert!(super::valid_profile_path(&profile).is_err());
        assert!(!outside.join("profiles").exists());
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn stale_session_cleanup_rejects_traversal_and_symlinked_paths() {
        let root = temporary_directory("stale-session-path");
        let profile = root.join("profiles/default");
        let sessions = profile.join("sessions");
        let outside = root.join("outside");
        fs::create_dir_all(&sessions).unwrap();
        fs::create_dir(&outside).unwrap();
        let child = sessions.join("managed-session");
        fs::create_dir(&child).unwrap();

        assert!(is_managed_session_directory(&profile, &child));
        assert!(!is_managed_session_directory(
            &profile,
            &sessions.join("../outside")
        ));
        let linked_child = sessions.join("linked-session");
        symlink(&outside, &linked_child).unwrap();
        assert!(!is_managed_session_directory(&profile, &linked_child));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stale_cleanup_never_signals_pid_one_or_invalid_process_groups() {
        assert!(!process_group_leader_matches(0, 0));
        assert!(!process_group_leader_matches(1, 0));
        assert!(!process_group_leader_matches(-1, 0));
    }

    #[test]
    fn reusable_home_and_workspace_reject_symlink_paths() {
        let root = temporary_directory("home-workspace-links");
        let outside = root.join("outside");
        fs::create_dir(&outside).unwrap();
        let home_link = root.join("home");
        let workspace_link = root.join("workspace");
        symlink(&outside, &home_link).unwrap();
        symlink(&outside, &workspace_link).unwrap();

        assert!(ensure_private_directory(&home_link, "Reusable desktop HOME").is_err());
        assert!(ensure_private_directory(&workspace_link, "Reusable desktop workspace").is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn startup_timeout_termination_gracefully_reaps_supervisor_child() {
        let root = temporary_directory("timeout-child");
        let mut child = Command::new("sleep").arg("30").spawn().unwrap();

        terminate_supervisor(&mut child, &root);

        assert!(child.try_wait().unwrap().is_some());
        let _ = fs::remove_dir_all(root);
    }
}
