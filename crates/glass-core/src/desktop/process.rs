use std::{
    collections::{HashMap, HashSet},
    io::Read,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use super::{Frames, Lease, ManagedSession};

// This reduces measured 1280x800 UI frame size by about 20% versus q:v 6
// while retaining roughly 31 dB PSNR on a real managed desktop capture.
const JPEG_QUALITY: &str = "8";
pub(super) const MANAGED_CHILD_PATH: &str =
    "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
pub(super) const MANAGED_XDG_DATA_DIRS: &str = "/usr/local/share:/usr/share:/var/lib/snapd/desktop";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Environment {
    Xfce,
    Gnome,
}

impl Environment {
    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "xfce" => Some(Self::Xfce),
            "gnome" => Some(Self::Gnome),
            _ => None,
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Xfce => "xfce",
            Self::Gnome => "gnome",
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Xfce => "XFCE",
            Self::Gnome => "GNOME",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Lifecycle {
    Disposable,
    Reusable,
}

struct SessionDirectoryCleanup {
    path: PathBuf,
    keep: bool,
}

impl Drop for SessionDirectoryCleanup {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

struct ChildCleanup(Option<Child>);

impl ChildCleanup {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }

    fn child_mut(&mut self) -> &mut Child {
        self.0.as_mut().expect("child cleanup guard is populated")
    }

    fn take(&mut self) -> Child {
        self.0.take().expect("child cleanup guard is populated")
    }
}

impl Drop for ChildCleanup {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            super::capture::stop_child(child);
        }
    }
}

impl Lifecycle {
    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "disposable" => Some(Self::Disposable),
            "reusable" => Some(Self::Reusable),
            _ => None,
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Disposable => "disposable",
            Self::Reusable => "reusable",
        }
    }
}

pub(super) fn start_session_with_environment(
    workspace: &Path,
    root: &Path,
    environment: Environment,
    lifecycle: Lifecycle,
    reusable_home: Option<&Path>,
) -> Result<ManagedSession, String> {
    start_session_impl(
        workspace,
        root,
        environment,
        lifecycle,
        reusable_home,
        true,
        super::SessionOwner::Gateway,
    )
}

pub(super) fn start_supervised_session(
    workspace: &Path,
    root: &Path,
    reusable_home: &Path,
    environment: Environment,
) -> Result<ManagedSession, String> {
    start_session_impl(
        workspace,
        root,
        environment,
        Lifecycle::Reusable,
        Some(reusable_home),
        false,
        super::SessionOwner::Supervisor,
    )
}

fn start_session_impl(
    workspace: &Path,
    root: &Path,
    environment: Environment,
    lifecycle: Lifecycle,
    reusable_home: Option<&Path>,
    capture_frames: bool,
    owner: super::SessionOwner,
) -> Result<ManagedSession, String> {
    if lifecycle == Lifecycle::Reusable && reusable_home.is_none() {
        return Err("A reusable desktop requires its named profile home".into());
    }
    if let Some(reason) = environment_dependency_error(environment) {
        return Err(reason);
    }
    ensure_storage_root(root)?;
    let mut random = [0u8; 24];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut random)
        .map_err(|_| "Secure random unavailable")?;
    let id = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let cookie = &id[..32];
    let auth_cookie = super::input::decode_cookie(cookie)?;
    let directory = root.join(&id);
    std::fs::create_dir(&directory).map_err(|e| format!("Desktop session directory: {e}"))?;
    let mut cleanup = SessionDirectoryCleanup {
        path: directory.clone(),
        keep: false,
    };
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("Desktop session permissions: {e}"))?;
    let session_home = match lifecycle {
        Lifecycle::Disposable => directory.join("home"),
        Lifecycle::Reusable => reusable_home.unwrap().to_path_buf(),
    };
    std::fs::create_dir_all(&session_home).map_err(|e| format!("Desktop home directory: {e}"))?;
    std::fs::set_permissions(&session_home, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("Desktop home permissions: {e}"))?;
    // Give session-bus-activated services a runtime path that belongs only to
    // this managed desktop. The separately exported AT-SPI bus address below
    // directs GTK accessibility clients to the observer's private registry.
    let runtime_directory = directory.join("runtime");
    std::fs::create_dir(&runtime_directory)
        .map_err(|e| format!("Desktop runtime directory: {e}"))?;
    std::fs::set_permissions(&runtime_directory, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("Desktop runtime permissions: {e}"))?;
    if environment == Environment::Xfce {
        disable_session_autostart(&session_home)?;
    }
    let auth_file = directory.join("Xauthority");
    let display_number = choose_display(&id);
    let display = format!(":{display_number}");
    let auth = command("xauth", &[])
        .args([
            "-f",
            auth_file.to_str().ok_or("Invalid Xauthority path")?,
            "add",
            &display,
            "MIT-MAGIC-COOKIE-1",
            cookie,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Starting xauth: {e}"))?;
    if !auth.success() {
        let _ = std::fs::remove_dir_all(&directory);
        return Err("Could not create managed desktop Xauthority".into());
    }
    let xvfb = match command(
        "Xvfb",
        &[
            &display,
            "-screen",
            "0",
            "1280x800x24",
            "-nolisten",
            "tcp",
            "-auth",
            auth_file.to_str().ok_or("Invalid Xauthority path")?,
            "-noreset",
        ],
    )
    .env("DISPLAY", &display)
    .env("XAUTHORITY", &auth_file)
    .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&directory);
            return Err(format!("Starting isolated X server: {error}"));
        }
    };
    let mut xvfb = ChildCleanup::new(xvfb);
    wait_for_display(&display, &auth_file, xvfb.child_mut())?;
    let background = command("xsetroot", &[])
        .args(["-solid", "#202124"])
        .env("DISPLAY", &display)
        .env("XAUTHORITY", &auth_file)
        .status();
    let background = match background {
        Ok(status) => status,
        Err(error) => {
            return Err(format!("Setting managed desktop background: {error}"));
        }
    };
    if !background.success() {
        return Err("Could not initialize managed desktop background".into());
    }
    let session_bus_file = directory.join("session-bus.address");
    let atspi_bus_file = directory.join("atspi-bus.address");
    let session_bus_file_arg = session_bus_file
        .to_str()
        .ok_or("Invalid session bus address path")?;
    let atspi_bus_file_arg = atspi_bus_file
        .to_str()
        .ok_or("Invalid AT-SPI bus address path")?;
    let mut desktop_args = vec![
        "--",
        "sh",
        "-c",
        r#"umask 077; atspi_address=$(gdbus call --session --dest org.a11y.Bus --object-path /org/a11y/bus --method org.a11y.Bus.GetAddress | sed -n "s/^('\(.*\)',)$/\1/p") || exit 1; test -n "$atspi_address" || exit 1; printf '%s' "$DBUS_SESSION_BUS_ADDRESS" > "$1"; printf '%s' "$atspi_address" > "$2"; export AT_SPI_BUS_ADDRESS="$atspi_address"; dbus-update-activation-environment AT_SPI_BUS_ADDRESS HOME XDG_CONFIG_HOME XDG_CACHE_HOME XDG_DATA_HOME XDG_CONFIG_DIRS XDG_DATA_DIRS XDG_RUNTIME_DIR DISPLAY XAUTHORITY PATH GTK_A11Y XDG_CURRENT_DESKTOP DESKTOP_SESSION XDG_SESSION_TYPE XDG_SESSION_DESKTOP || exit 1; program=$3; shift 3; exec "$program" "$@"; : "#,
        "glass-managed-session",
        session_bus_file_arg,
        atspi_bus_file_arg,
    ];
    match environment {
        Environment::Xfce => desktop_args.push("xfce4-session"),
        Environment::Gnome => desktop_args.extend(["gnome-session", "--session=ubuntu"]),
    }
    let startup_log = directory.join("startup.stderr");
    let startup_stderr = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&startup_log)
        .map_err(|error| format!("Creating private desktop startup log: {error}"))?;
    let mut desktop_command = command("dbus-run-session", &desktop_args);
    desktop_command
        .env("DISPLAY", &display)
        .env("XAUTHORITY", &auth_file)
        .env("XDG_SESSION_TYPE", "x11")
        .env("GTK_A11Y", "atspi")
        .env("HOME", &session_home)
        .env("XDG_CONFIG_HOME", session_home.join(".config"))
        .env("XDG_CACHE_HOME", session_home.join(".cache"))
        .env("XDG_DATA_HOME", session_home.join(".local/share"))
        .env("XDG_CONFIG_DIRS", "/etc/xdg")
        .env("XDG_DATA_DIRS", MANAGED_XDG_DATA_DIRS)
        .env(
            "XDG_CURRENT_DESKTOP",
            match environment {
                Environment::Xfce => "XFCE",
                Environment::Gnome => "ubuntu:GNOME",
            },
        )
        .env(
            "DESKTOP_SESSION",
            match environment {
                Environment::Xfce => "xfce",
                Environment::Gnome => "ubuntu",
            },
        )
        .env(
            "XDG_SESSION_DESKTOP",
            match environment {
                Environment::Xfce => "xfce",
                Environment::Gnome => "ubuntu",
            },
        )
        .env("XDG_RUNTIME_DIR", &runtime_directory)
        .env("GDK_BACKEND", "x11")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .env("GTK_THEME", "Adwaita")
        .env_remove("NO_AT_BRIDGE")
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .stderr(Stdio::from(startup_stderr))
        .current_dir(workspace);
    if environment == Environment::Gnome {
        desktop_command
            .env("GNOME_SHELL_SESSION_MODE", "ubuntu")
            .env("LIBGL_ALWAYS_SOFTWARE", "1")
            .env("MUTTER_DEBUG_DISABLE_HW_CURSORS", "1");
    } else {
        desktop_command.env_remove("GNOME_SHELL_SESSION_MODE");
    }
    let mut desktop = match desktop_command.spawn() {
        Ok(child) => ChildCleanup::new(child),
        Err(error) => {
            return Err(format!(
                "Starting isolated {} desktop session: {error}",
                environment.label()
            ));
        }
    };
    let session_bus_address = match wait_for_session_bus(&session_bus_file, desktop.child_mut()) {
        Ok(address) => address,
        Err(error) => {
            let diagnostic = startup_diagnostic(&startup_log, &directory);
            return Err(format!("{error}{diagnostic}"));
        }
    };
    if let Err(error) = wait_for_atspi_bus(&atspi_bus_file, desktop.child_mut()) {
        let diagnostic = startup_diagnostic(&startup_log, &directory);
        return Err(format!("{error}{diagnostic}"));
    }
    let frames = Arc::new(Mutex::new(Frames::default()));
    let capture = if capture_frames {
        match start_capture(&display, &auth_file, &frames) {
            Ok(capture) => Some(capture),
            Err(error) => {
                let diagnostic = startup_diagnostic(&startup_log, &directory);
                return Err(format!("{error}{diagnostic}"));
            }
        }
    } else {
        None
    };
    let _ = std::fs::remove_file(&startup_log);
    let session = ManagedSession {
        id,
        display,
        environment,
        lifecycle,
        profile_id: (lifecycle == Lifecycle::Reusable)
            .then(|| super::DEFAULT_PROFILE_ID.to_owned()),
        workspace: workspace.to_path_buf(),
        owner,
        auth_cookie,
        directory,
        xvfb: Some(xvfb.take()),
        desktop: Some(desktop.take()),
        capture,
        frames,
        session_bus_address,
        _profile_lock: None,
        accessibility_revision: 0,
        accessibility_registry_ready: false,
        accessibility_targets: HashMap::new(),
        accessibility_windows: HashMap::new(),
        lease: Lease::Agent,
        held_buttons: HashSet::new(),
        held_keys: HashSet::new(),
        scroll_x_remainder: 0.0,
        scroll_y_remainder: 0.0,
    };
    cleanup.keep = true;
    Ok(session)
}

pub(super) fn environment_dependency_error(environment: Environment) -> Option<String> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = environment;
        return Some("Managed desktop sessions are supported on Linux only".into());
    }
    #[cfg(target_os = "linux")]
    {
        let mut required_tools = vec![
            "Xvfb",
            "xauth",
            "xsetroot",
            "dbus-run-session",
            "dbus-update-activation-environment",
            "sh",
            "gdbus",
            "sed",
            "ffmpeg",
            "xdpyinfo",
        ];
        match environment {
            Environment::Xfce => {
                required_tools.extend(["xfce4-session", "xfwm4", "xfdesktop", "xfce4-panel"])
            }
            Environment::Gnome => required_tools.extend(["gnome-session", "gnome-shell"]),
        }
        for tool in required_tools {
            if !command_exists(tool) {
                return Some(format!(
                    "{} desktop dependency is unavailable: {tool}",
                    environment.label()
                ));
            }
        }
        if environment == Environment::Gnome
            && !Path::new("/usr/share/gnome-session/sessions/ubuntu.session").is_file()
        {
            return Some("GNOME desktop session definition is unavailable: ubuntu".into());
        }
        if environment == Environment::Xfce
            && !Path::new("/etc/xdg/xfce4/panel/default.xml").is_file()
        {
            return Some("XFCE panel configuration template is unavailable".into());
        }
        None
    }
}

pub(super) fn start_capture(
    display: &str,
    auth_file: &Path,
    frames: &Arc<Mutex<Frames>>,
) -> Result<Child, String> {
    let mut capture = command(
        "ffmpeg",
        &[
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "x11grab",
            "-framerate",
            "5",
            "-video_size",
            "1280x800",
            "-i",
            &format!("{display}.0"),
            "-an",
            "-c:v",
            "mjpeg",
            "-q:v",
            JPEG_QUALITY,
            "-f",
            "image2pipe",
            "pipe:1",
        ],
    )
    .env("DISPLAY", display)
    .env("XAUTHORITY", auth_file)
    .stdout(Stdio::piped())
    .spawn()
    .map_err(|error| format!("Starting desktop capture: {error}"))?;
    let Some(stdout) = capture.stdout.take() else {
        super::capture::stop_child(&mut capture);
        return Err("Desktop capture output unavailable".into());
    };
    let frame_sink = frames.clone();
    let started = Instant::now();
    thread::spawn(move || super::capture::read_frames(stdout, frame_sink, started));
    if !super::capture::wait_for_first_frame(frames, &mut capture) {
        super::capture::stop_child(&mut capture);
        return Err("Desktop capture did not produce a fresh frame".into());
    }
    Ok(capture)
}

fn wait_for_session_bus(path: &Path, child: &mut Child) -> Result<String, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let address = std::fs::read_to_string(path).ok();
        if let Some(address) = address {
            let address = address.trim();
            if (address.starts_with("unix:path=") || address.starts_with("unix:abstract="))
                && atspi::zbus::address::Address::try_from(address).is_ok()
            {
                if let Some(status) = child
                    .try_wait()
                    .map_err(|error| format!("Checking managed desktop startup: {error}"))?
                {
                    return Err(format!(
                        "The isolated desktop session exited during startup (exit status: {status}; session bus address published)"
                    ));
                }
                return Ok(address.to_owned());
            }
            return Err(
                "The isolated XFCE session returned an invalid private D-Bus address".into(),
            );
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Checking managed desktop startup: {error}"))?
        {
            return Err(format!(
                "The isolated desktop session exited before publishing its private D-Bus address (exit status: {status}; session bus address missing)",
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
    Err("The isolated desktop session did not publish its private D-Bus address".into())
}

fn wait_for_atspi_bus(path: &Path, child: &mut Child) -> Result<String, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let address = std::fs::read_to_string(path).ok();
        if let Some(address) = address {
            let address = address.trim();
            if (address.starts_with("unix:path=") || address.starts_with("unix:abstract="))
                && atspi::zbus::address::Address::try_from(address).is_ok()
            {
                if let Some(status) = child
                    .try_wait()
                    .map_err(|error| format!("Checking managed desktop startup: {error}"))?
                {
                    return Err(format!(
                        "The isolated desktop session exited during AT-SPI startup (exit status: {status}; AT-SPI address published)"
                    ));
                }
                return Ok(address.to_owned());
            }
            return Err(
                "The isolated desktop session returned an invalid private AT-SPI address".into(),
            );
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Checking managed desktop startup: {error}"))?
        {
            return Err(format!(
                "The isolated desktop session exited before publishing its AT-SPI address (exit status: {status}; AT-SPI address missing)"
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
    Err("The isolated desktop session did not publish its private AT-SPI address".into())
}

fn startup_diagnostic(log: &Path, session_directory: &Path) -> String {
    let Ok(file) = std::fs::File::open(log) else {
        return String::new();
    };
    let mut bytes = Vec::with_capacity(2048);
    if file.take(4096).read_to_end(&mut bytes).is_err() || bytes.is_empty() {
        return String::new();
    }
    let mut detail = String::from_utf8_lossy(&bytes).into_owned();
    if let Some(directory) = session_directory.to_str() {
        detail = detail.replace(directory, "<private-session>");
    }
    detail = detail.replace("unix:path=", "unix:path=<redacted>");
    if detail.len() > 2048 {
        let mut boundary = 2048;
        while !detail.is_char_boundary(boundary) {
            boundary -= 1;
        }
        detail.truncate(boundary);
    }
    format!("; bounded private startup stderr: {}", detail.trim())
}

fn command(program: &str, args: &[&str]) -> Command {
    let mut command = Command::new(program);
    apply_private_process_environment(&mut command);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command
}

fn apply_private_process_environment(command: &mut Command) {
    command.env_clear();
    command.env("PATH", MANAGED_CHILD_PATH);
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
}

fn disable_session_autostart(session_home: &Path) -> Result<(), String> {
    let config = session_home.join(".config");
    ensure_private_session_directory(&config, "Desktop configuration")?;
    let autostart = session_home.join(".config/autostart");
    ensure_private_session_directory(&autostart, "Desktop autostart configuration")?;
    for name in ["light-locker.desktop", "update-notifier.desktop"] {
        let path = autostart.join(name);
        write_private_file_if_missing(&path, "[Desktop Entry]\nType=Application\nHidden=true\n")
            .map_err(|error| format!("Disable desktop autostart {name}: {error}"))?;
    }
    Ok(())
}

fn ensure_private_session_directory(path: &Path, label: &str) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(format!("{label} path must be a real directory"));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(path).map_err(|error| format!("Creating {label}: {error}"))?;
        }
        Err(error) => return Err(format!("Checking {label}: {error}")),
    }
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| format!("Checking {label}: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!("{label} path must be a real directory"));
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("Setting {label} permissions: {error}"))
}

fn ensure_storage_root(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err("Desktop storage path must be a real directory".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(path).map_err(|error| format!("Desktop storage: {error}"))?;
        }
        Err(error) => return Err(format!("Checking desktop storage: {error}")),
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("Checking desktop storage: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Desktop storage path must be a real directory".into());
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("Desktop storage permissions: {error}"))
}

fn write_private_file_if_missing(path: &Path, contents: &str) -> Result<(), String> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(contents.as_bytes())
                .map_err(|error| format!("Writing private file: {error}"))?;
            file.sync_all()
                .map_err(|error| format!("Saving private file: {error}"))?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = std::fs::symlink_metadata(path)
                .map_err(|error| format!("Checking private file: {error}"))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("Private autostart entry must be a regular file".into());
            }
            Ok(())
        }
        Err(error) => Err(format!("Creating private file: {error}")),
    }
}

fn command_exists(program: &str) -> bool {
    std::env::split_paths(MANAGED_CHILD_PATH).any(|directory| directory.join(program).is_file())
}

pub(super) fn choose_display(id: &str) -> u16 {
    let mut value = 0u16;
    for byte in id.bytes().take(4) {
        value = value.wrapping_mul(31).wrapping_add(byte as u16);
    }
    100 + value % 5000
}

fn wait_for_display(display: &str, auth_file: &Path, child: &mut Child) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("The isolated X server exited during startup".into());
        }
        let status = command("xdpyinfo", &[])
            .arg("-display")
            .arg(display)
            .env("DISPLAY", display)
            .env("XAUTHORITY", auth_file)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if status.is_ok_and(|status| status.success()) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err("The isolated X server did not become ready".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_storage_root_rejects_symlinks_without_changing_target_permissions() {
        let parent = std::env::temp_dir().join(format!(
            "glass-desktop-storage-root-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir(&parent).unwrap();
        let outside = parent.join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o755)).unwrap();
        let link = parent.join("desktop");
        std::os::unix::fs::symlink(&outside, &link).unwrap();

        assert!(ensure_storage_root(&link).is_err());
        assert_eq!(
            std::fs::symlink_metadata(&outside)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );

        let _ = std::fs::remove_dir_all(parent);
    }

    #[test]
    fn partial_session_startup_removes_its_private_directory() {
        let root = std::env::temp_dir().join(format!(
            "glass-desktop-session-cleanup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let directory = root.join("session");
        std::fs::create_dir_all(&directory).unwrap();
        {
            let _cleanup = SessionDirectoryCleanup {
                path: directory.clone(),
                keep: false,
            };
            std::fs::write(directory.join("partial"), b"partial").unwrap();
        }
        assert!(!directory.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn child_cleanup_guard_stops_and_reaps_started_processes() {
        let child = command("sleep", &["30"]).spawn().unwrap();
        let pid = i32::try_from(child.id()).unwrap();
        drop(ChildCleanup::new(child));

        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }

    #[test]
    fn autostart_override_preserves_existing_profile_data_and_rejects_symlinks() {
        let root = std::env::temp_dir().join(format!(
            "glass-desktop-autostart-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let existing = root.join("existing.desktop");
        std::fs::write(&existing, "user preference").unwrap();
        write_private_file_if_missing(&existing, "Hidden=true").unwrap();
        assert_eq!(
            std::fs::read_to_string(&existing).unwrap(),
            "user preference"
        );

        let outside = root.join("outside");
        std::fs::write(&outside, "untouched").unwrap();
        let link = root.join("linked.desktop");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        assert!(write_private_file_if_missing(&link, "Hidden=true").is_err());
        assert_eq!(std::fs::read_to_string(outside).unwrap(), "untouched");
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    fn process_environment_presence(process_name: &str) -> Option<serde_json::Value> {
        for entry in std::fs::read_dir("/proc").ok()?.flatten() {
            let pid = entry.file_name();
            if !pid
                .to_string_lossy()
                .bytes()
                .all(|byte| byte.is_ascii_digit())
            {
                continue;
            }
            let directory = entry.path();
            let Ok(comm) = std::fs::read_to_string(directory.join("comm")) else {
                continue;
            };
            if comm.trim() != process_name {
                continue;
            }
            let Ok(environment) = std::fs::read(directory.join("environ")) else {
                continue;
            };
            let present = |name: &str| {
                environment
                    .split(|byte| *byte == 0)
                    .filter_map(|field| field.split(|byte| *byte == b'=').next())
                    .any(|field| field == name.as_bytes())
            };
            return Some(serde_json::json!({
                "GTK_A11Y":present("GTK_A11Y"),
                "GTK_MODULES":present("GTK_MODULES"),
                "AT_SPI_BUS_ADDRESS":present("AT_SPI_BUS_ADDRESS"),
                "NO_AT_BRIDGE":present("NO_AT_BRIDGE")
            }));
        }
        None
    }

    #[test]
    fn managed_commands_clear_unapproved_environment() {
        let mut command = Command::new("sh");
        command
            .env("GLASS_DESKTOP_TEST_CANARY", "must-not-leak")
            .env("PATH", "/tmp/untrusted:/usr/bin");
        apply_private_process_environment(&mut command);
        let output = command
            .args(["-c", "env"])
            .output()
            .expect("run an environment probe in a managed child");
        let output = String::from_utf8_lossy(&output.stdout);
        assert!(!output.contains("GLASS_DESKTOP_TEST_CANARY"));
        assert!(
            output
                .lines()
                .any(|line| line == format!("PATH={MANAGED_CHILD_PATH}"))
        );
    }

    #[test]
    fn managed_xfce_panel_registers_with_private_atspi_bus() {
        if std::env::var_os("GLASS_DESKTOP_PANEL_A11Y_E2E").as_deref()
            != Some(std::ffi::OsStr::new("1"))
        {
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "glass-desktop-panel-a11y-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let workspace = root.join("workspace");
        let sessions = root.join("sessions");
        std::fs::create_dir_all(&workspace).unwrap();
        let session = match start_session_with_environment(
            &workspace,
            &sessions,
            Environment::Xfce,
            Lifecycle::Disposable,
            None,
        ) {
            Ok(session) => session,
            Err(error) => {
                let _ = std::fs::remove_dir_all(&root);
                panic!("managed XFCE session did not start: {error}");
            }
        };
        let snapshot = super::super::accessibility::observe(
            &session.session_bus_address,
            &session.id,
            1,
            None,
        )
        .unwrap();
        let applications = snapshot.value["applications"].clone();
        #[cfg(target_os = "linux")]
        let panel_environment = process_environment_presence("xfce4-panel");
        #[cfg(not(target_os = "linux"))]
        let panel_environment: Option<serde_json::Value> = None;
        eprintln!(
            "isolated AT-SPI panel diagnostic: roots={:?}; named_applications={}; panel_env_presence={panel_environment:?}; startup_stderr=not-retained-after-success",
            snapshot.application_root_names, applications
        );
        drop(session);
        std::fs::remove_dir_all(&root).unwrap();
        assert!(
            applications
                .as_array()
                .is_some_and(|applications| applications.iter().any(|name| name == "xfce4-panel")),
            "managed panel did not register in the private AT-SPI application tree: {}",
            applications
        );
    }
}
