use std::{
    collections::{HashMap, HashSet},
    os::unix::fs::PermissionsExt,
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use super::{Frames, Lease, ManagedSession};

pub(super) fn start_session(workspace: &Path, root: &Path) -> Result<ManagedSession, String> {
    for tool in [
        "Xvfb",
        "xauth",
        "xsetroot",
        "dbus-run-session",
        "sh",
        "xfce4-session",
        "xfwm4",
        "xfdesktop",
        "xfce4-panel",
        "ffmpeg",
        "xdpyinfo",
    ] {
        if !command_exists(tool) {
            return Err(format!("Desktop dependency is unavailable: {tool}"));
        }
    }
    std::fs::create_dir_all(root).map_err(|e| format!("Desktop storage: {e}"))?;
    std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("Desktop storage permissions: {e}"))?;
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
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("Desktop session permissions: {e}"))?;
    let session_home = directory.join("home");
    std::fs::create_dir(&session_home).map_err(|e| format!("Desktop home directory: {e}"))?;
    std::fs::set_permissions(&session_home, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("Desktop home permissions: {e}"))?;
    disable_session_autostart(&session_home)?;
    let auth_file = directory.join("Xauthority");
    let display_number = choose_display(&id);
    let display = format!(":{display_number}");
    let auth = Command::new("xauth")
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
    let mut xvfb = match command(
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
    if let Err(error) = wait_for_display(&display, &auth_file, &mut xvfb) {
        super::capture::stop_child(&mut xvfb);
        let _ = std::fs::remove_dir_all(&directory);
        return Err(error);
    }
    let background = Command::new("xsetroot")
        .args(["-solid", "#202124"])
        .env("DISPLAY", &display)
        .env("XAUTHORITY", &auth_file)
        .status();
    let background = match background {
        Ok(status) => status,
        Err(error) => {
            super::capture::stop_child(&mut xvfb);
            let _ = std::fs::remove_dir_all(&directory);
            return Err(format!("Setting managed desktop background: {error}"));
        }
    };
    if !background.success() {
        super::capture::stop_child(&mut xvfb);
        let _ = std::fs::remove_dir_all(&directory);
        return Err("Could not initialize managed desktop background".into());
    }
    let session_bus_file = directory.join("session-bus.address");
    let session_bus_file_arg = session_bus_file
        .to_str()
        .ok_or("Invalid session bus address path")?;
    let desktop_args = [
        "--",
        "sh",
        "-c",
        "umask 077; printf '%s' \"$DBUS_SESSION_BUS_ADDRESS\" > \"$1\"; exec xfce4-session",
        "glass-xfce-session",
        session_bus_file_arg,
    ];
    let mut desktop = match command("dbus-run-session", &desktop_args)
        .env("DISPLAY", &display)
        .env("XAUTHORITY", &auth_file)
        .env("XDG_SESSION_TYPE", "x11")
        .env("GTK_A11Y", "atspi")
        .env("HOME", &session_home)
        .env("XDG_CONFIG_HOME", session_home.join(".config"))
        .env("XDG_CACHE_HOME", session_home.join(".cache"))
        .env("XDG_DATA_HOME", session_home.join(".local/share"))
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .current_dir(workspace)
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            super::capture::stop_child(&mut xvfb);
            let _ = std::fs::remove_dir_all(&directory);
            return Err(format!("Starting isolated XFCE session: {error}"));
        }
    };
    let session_bus_address = match wait_for_session_bus(&session_bus_file, &mut desktop) {
        Ok(address) => address,
        Err(error) => {
            super::capture::stop_child(&mut desktop);
            super::capture::stop_child(&mut xvfb);
            let _ = std::fs::remove_dir_all(&directory);
            return Err(error);
        }
    };
    let frames = Arc::new(Mutex::new(Frames::default()));
    let mut capture = match command(
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
            "6",
            "-f",
            "image2pipe",
            "pipe:1",
        ],
    )
    .env("DISPLAY", &display)
    .env("XAUTHORITY", &auth_file)
    .stdout(Stdio::piped())
    .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let mut desktop = desktop;
            super::capture::stop_child(&mut desktop);
            super::capture::stop_child(&mut xvfb);
            let _ = std::fs::remove_dir_all(&directory);
            return Err(format!("Starting desktop capture: {error}"));
        }
    };
    let Some(stdout) = capture.stdout.take() else {
        super::capture::stop_child(&mut capture);
        let mut desktop = desktop;
        super::capture::stop_child(&mut desktop);
        super::capture::stop_child(&mut xvfb);
        let _ = std::fs::remove_dir_all(&directory);
        return Err("Desktop capture output unavailable".into());
    };
    let frame_sink = frames.clone();
    let started = Instant::now();
    thread::spawn(move || super::capture::read_frames(stdout, frame_sink, started));
    if !super::capture::wait_for_first_frame(&frames, &mut capture) {
        let mut desktop = desktop;
        super::capture::stop_child(&mut capture);
        super::capture::stop_child(&mut desktop);
        super::capture::stop_child(&mut xvfb);
        let _ = std::fs::remove_dir_all(&directory);
        return Err("Desktop capture did not produce a fresh frame".into());
    }
    Ok(ManagedSession {
        id,
        display,
        auth_cookie,
        directory,
        xvfb,
        desktop,
        capture,
        frames,
        session_bus_address,
        accessibility_revision: 0,
        accessibility_targets: HashMap::new(),
        lease: Lease::Agent,
        held_buttons: HashSet::new(),
        held_keys: HashSet::new(),
        scroll_x_remainder: 0.0,
        scroll_y_remainder: 0.0,
    })
}

fn wait_for_session_bus(path: &Path, child: &mut Child) -> Result<String, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if child
            .try_wait()
            .map_err(|error| format!("Checking isolated XFCE startup: {error}"))?
            .is_some()
        {
            return Err("The isolated XFCE session exited during startup".into());
        }
        if let Ok(address) = std::fs::read_to_string(path) {
            let address = address.trim();
            if (address.starts_with("unix:path=") || address.starts_with("unix:abstract="))
                && atspi::zbus::address::Address::try_from(address).is_ok()
            {
                return Ok(address.to_owned());
            }
            return Err(
                "The isolated XFCE session returned an invalid private D-Bus address".into(),
            );
        }
        thread::sleep(Duration::from_millis(25));
    }
    Err("The isolated XFCE session did not publish its private D-Bus address".into())
}

fn command(program: &str, args: &[&str]) -> Command {
    let mut command = Command::new(program);
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

fn disable_session_autostart(session_home: &Path) -> Result<(), String> {
    let autostart = session_home.join(".config/autostart");
    std::fs::create_dir_all(&autostart)
        .map_err(|e| format!("Desktop autostart configuration: {e}"))?;
    std::fs::set_permissions(&autostart, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("Desktop autostart permissions: {e}"))?;
    for name in ["light-locker.desktop", "update-notifier.desktop"] {
        let path = autostart.join(name);
        std::fs::write(&path, "[Desktop Entry]\nType=Application\nHidden=true\n")
            .map_err(|e| format!("Disable desktop autostart {name}: {e}"))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("Desktop autostart file permissions: {e}"))?;
    }
    Ok(())
}

fn command_exists(program: &str) -> bool {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .any(|directory| directory.join(program).is_file())
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
        let status = Command::new("xdpyinfo")
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
