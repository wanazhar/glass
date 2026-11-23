//! MP4 recordings of the managed X11 desktop, written into the workspace so
//! the model's file tools and the user can reach them.

use std::ffi::{CStr, CString};
use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{Desktop, HEIGHT, WIDTH, capture::stop_child, process};

pub(super) const RECORDING_DIRECTORY: &str = ".gc-artifacts/desktop/recordings";
const DEFAULT_SECONDS: u64 = 300;
const MAX_SECONDS: u64 = 600;

struct Recording {
    session_id: String,
    child: Child,
    file: File,
    relative: String,
    started: Instant,
    limit: u64,
}

static ACTIVE: Mutex<Option<Recording>> = Mutex::new(None);

fn recording_directory(workspace: &Path) -> std::io::Result<File> {
    let mut directory = File::open(workspace)?;
    let mut components = RECORDING_DIRECTORY.split('/').peekable();
    while let Some(component) = components.next() {
        let metadata = directory.metadata()?;
        // SAFETY: geteuid has no preconditions or side effects.
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "recording parent must be owned by this user and not writable by others",
            ));
        }
        let leaf = components.peek().is_none();
        let name = CString::new(component).unwrap();
        // SAFETY: the live parent descriptor and component name remain valid.
        if unsafe {
            libc::mkdirat(
                directory.as_raw_fd(),
                name.as_ptr(),
                if leaf { 0o700 } else { 0o755 },
            )
        } != 0
        {
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(error);
            }
        }
        // SAFETY: openat returns a new descriptor and refuses symlink components.
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: successful openat returned a uniquely owned descriptor.
        directory = unsafe { File::from_raw_fd(fd) };
        // SAFETY: geteuid has no preconditions or side effects.
        if directory.metadata()?.uid() != unsafe { libc::geteuid() } {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "recording directory belongs to another user",
            ));
        }
        if leaf {
            directory.set_permissions(std::fs::Permissions::from_mode(0o700))?;
        }
    }
    Ok(directory)
}

fn recording_file(directory: &File, name: &CStr) -> std::io::Result<File> {
    // SAFETY: exclusive creation is relative to the held private directory.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: successful openat returned a uniquely owned descriptor.
    let file = unsafe { File::from_raw_fd(fd) };
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

fn spawn(display: &str, xauthority: &Path, file: &File, seconds: u64) -> Result<Child, String> {
    let size = format!("{WIDTH}x{HEIGHT}");
    let input = format!("{display}.0");
    let limit = seconds.to_string();
    process::command(
        "ffmpeg",
        &[
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "x11grab",
            "-framerate",
            "10",
            "-video_size",
            &size,
            "-i",
            &input,
            "-t",
            &limit,
            "-an",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
            "-f",
            "mp4",
            "-y",
            "/dev/stdout",
        ],
    )
    .env("DISPLAY", display)
    .env("XAUTHORITY", xauthority)
    .stdout(Stdio::from(
        file.try_clone().map_err(|error| error.to_string())?,
    ))
    .spawn()
    .map_err(|error| {
        format!("Starting the desktop recording failed: {error}; install ffmpeg with libx264")
    })
}

/// Ask FFmpeg to finish the file (SIGINT writes the MP4 index), then reap it.
fn finish(recording: &mut Recording) -> Result<u64, String> {
    if recording.child.try_wait().ok().flatten().is_none() {
        unsafe {
            libc::kill(recording.child.id() as i32, libc::SIGINT);
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while recording.child.try_wait().ok().flatten().is_none() {
            if Instant::now() >= deadline {
                stop_child(&mut recording.child);
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let bytes = recording.file.metadata().map(|m| m.len()).unwrap_or(0);
    if bytes == 0 {
        return Err("The recording produced no video; check that ffmpeg has libx264".into());
    }
    Ok(bytes)
}

fn start(
    session_id: &str,
    display: &str,
    xauthority: &Path,
    workspace: &Path,
    seconds: u64,
) -> Result<Value, String> {
    let mut active = ACTIVE.lock().map_err(|_| "Recording state unavailable")?;
    if let Some(recording) = active.as_mut()
        && recording.child.try_wait().ok().flatten().is_none()
    {
        return Err(format!(
            "A recording is already running ({}); call desktop_record with action stop first",
            recording.relative
        ));
    }
    let directory = recording_directory(workspace).map_err(|error| error.to_string())?;
    let mut nonce = [0u8; 16];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut nonce)
        .map_err(|_| "Secure random unavailable".to_owned())?;
    let name = format!("desktop-{:032x}.mp4", u128::from_ne_bytes(nonce));
    let file = recording_file(&directory, &CString::new(name.as_str()).unwrap())
        .map_err(|error| error.to_string())?;
    let relative = format!("{RECORDING_DIRECTORY}/{name}");
    let child = spawn(display, xauthority, &file, seconds)?;
    *active = Some(Recording {
        session_id: session_id.to_owned(),
        child,
        file,
        relative: relative.clone(),
        started: Instant::now(),
        limit: seconds,
    });
    Ok(json!({"recording": true, "sessionId": session_id, "path": relative, "maxSeconds": seconds}))
}

fn stop(session_id: &str) -> Result<Value, String> {
    let mut active = ACTIVE.lock().map_err(|_| "Recording state unavailable")?;
    let Some(mut recording) = active.take() else {
        return Err("No recording is running; call desktop_record with action start first".into());
    };
    let seconds = recording.started.elapsed().as_secs().min(recording.limit);
    let bytes = finish(&mut recording)?;
    Ok(json!({
        "recording": false,
        "sessionId": session_id,
        "path": recording.relative,
        "seconds": seconds,
        "bytes": bytes
    }))
}

fn status() -> Value {
    let Ok(mut active) = ACTIVE.lock() else {
        return json!({"recording": false});
    };
    match active.as_mut() {
        Some(recording) => json!({
            "recording": recording.child.try_wait().ok().flatten().is_none(),
            "path": recording.relative,
            "seconds": recording.started.elapsed().as_secs().min(recording.limit),
            "maxSeconds": recording.limit,
            "sessionId": recording.session_id
        }),
        None => json!({"recording": false}),
    }
}

fn parse(value: &Value) -> Result<(&str, &str, u64), String> {
    let object = value
        .as_object()
        .ok_or("desktop_record arguments must be an object")?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "action" | "sessionId" | "maxSeconds"))
    {
        return Err("desktop_record accepts only action, sessionId, and maxSeconds".into());
    }
    let session_id = value["sessionId"]
        .as_str()
        .ok_or("sessionId is required from desktop_status")?;
    let action = value["action"]
        .as_str()
        .filter(|action| matches!(*action, "start" | "stop" | "status"))
        .ok_or("action must be `start`, `stop`, or `status`")?;
    let seconds = match value.get("maxSeconds") {
        None => DEFAULT_SECONDS,
        Some(seconds) => seconds
            .as_u64()
            .filter(|seconds| (1..=MAX_SECONDS).contains(seconds))
            .ok_or("maxSeconds must be an integer from 1 to 600")?,
    };
    Ok((session_id, action, seconds))
}

impl Desktop {
    pub(super) fn record(&self, value: &Value) -> Result<Value, String> {
        let (session_id, action, seconds) = parse(value)?;
        let (display, xauthority, workspace) = {
            let guard = self
                .session
                .lock()
                .map_err(|_| "Desktop state unavailable")?;
            let session = guard
                .as_ref()
                .ok_or("No managed desktop session is running")?;
            if session.id != session_id {
                return Err(
                    "The managed desktop session changed; call desktop_status and retry".into(),
                );
            }
            (
                session.display.clone(),
                session.directory.join("Xauthority"),
                session.workspace.clone(),
            )
        };
        match action {
            "start" => start(session_id, &display, &xauthority, &workspace, seconds),
            "stop" => stop(session_id),
            _ => Ok(status()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn scratch(label: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("glass-record-{label}-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn recording_permissions_are_private_under_shared_umask() {
        const MARKER: &str = "GLASS_RECORDING_UMASK_TEST";
        if std::env::var_os(MARKER).is_none() {
            for mask in ["022", "002"] {
                assert!(Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "desktop::recording::tests::recording_permissions_are_private_under_shared_umask", "--nocapture"])
                .env(MARKER, mask)
                .status().unwrap().success());
            }
            return;
        }
        let mask = libc::mode_t::from_str_radix(&std::env::var(MARKER).unwrap(), 8).unwrap();
        // SAFETY: this subprocess runs only this test, without other filesystem callers.
        unsafe {
            libc::umask(mask);
        }
        let root = scratch("permissions");
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!root.join(".gc-artifacts").exists());
        let directory = recording_directory(&root).unwrap();
        assert_eq!(std::fs::metadata(&root).unwrap().mode() & 0o777, 0o755);
        let name = c"private.mp4";
        let file = recording_file(&directory, name).unwrap();
        assert_eq!(file.metadata().unwrap().len(), 0);
        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        let mut path = root.clone();
        for component in RECORDING_DIRECTORY.split('/') {
            path.push(component);
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                if component == "recordings" {
                    0o700
                } else {
                    0o755
                }
            );
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(recording_directory(&root).is_ok());
        assert_eq!(
            directory.metadata().unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert!(recording_file(&directory, name).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recording_preserves_shared_parent_modes_and_rejects_writable_parents() {
        let root = scratch("shared-parents");
        let artifacts = root.join(".gc-artifacts");
        let desktop = artifacts.join("desktop");
        std::fs::create_dir_all(&desktop).unwrap();
        for parent in [&root, &artifacts, &desktop] {
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::write(desktop.join("invoice.pdf"), b"shared export").unwrap();
        let directory = recording_directory(&root).unwrap();
        assert_eq!(directory.metadata().unwrap().mode() & 0o777, 0o700);
        assert_eq!(
            recording_file(&directory, c"private.mp4")
                .unwrap()
                .metadata()
                .unwrap()
                .mode()
                & 0o777,
            0o600
        );
        for parent in [&root, &artifacts, &desktop] {
            assert_eq!(std::fs::metadata(parent).unwrap().mode() & 0o777, 0o755);
        }
        assert_eq!(
            std::fs::read(desktop.join("invoice.pdf")).unwrap(),
            b"shared export"
        );
        for mode in [0o775, 0o757] {
            std::fs::set_permissions(&artifacts, std::fs::Permissions::from_mode(mode)).unwrap();
            assert!(recording_directory(&root).is_err());
            assert_eq!(std::fs::metadata(&artifacts).unwrap().mode() & 0o777, mode);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recording_creation_stays_anchored_and_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let root = scratch("substitution");
        let outside = scratch("outside");
        symlink(&outside, root.join(".gc-artifacts")).unwrap();
        assert!(recording_directory(&root).is_err());
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
        std::fs::remove_file(root.join(".gc-artifacts")).unwrap();
        let directory = recording_directory(&root).unwrap();
        let original = root.join("owned");
        std::fs::rename(root.join(RECORDING_DIRECTORY), &original).unwrap();
        symlink(&outside, root.join(RECORDING_DIRECTORY)).unwrap();
        let mut file = recording_file(&directory, c"private.mp4").unwrap();
        std::io::Write::write_all(&mut file, b"private recording").unwrap();
        assert_eq!(
            std::fs::read(original.join("private.mp4")).unwrap(),
            b"private recording"
        );
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
        symlink(outside.join("victim"), original.join("blocked.mp4")).unwrap();
        assert!(recording_file(&directory, c"blocked.mp4").is_err());
        assert!(!outside.join("victim").exists());
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn record_rejects_unknown_fields_and_bounds_duration() {
        assert!(parse(&json!({"action":"start","sessionId":"s","path":"/tmp/x"})).is_err());
        assert!(parse(&json!({"action":"pause","sessionId":"s"})).is_err());
        assert!(parse(&json!({"action":"start","sessionId":"s","maxSeconds":601})).is_err());
        assert_eq!(
            parse(&json!({"action":"start","sessionId":"s"})).unwrap(),
            ("s", "start", DEFAULT_SECONDS)
        );
    }

    /// Records a private Xvfb display; runs only with `GLASS_E2E=1`.
    #[test]
    fn record_writes_a_playable_mp4_from_a_virtual_x11_display() {
        if std::env::var_os("GLASS_E2E").is_none() {
            return;
        }
        let number = 940 + std::process::id() % 40;
        let display = format!(":{number}");
        let mut xvfb = Command::new("Xvfb")
            .args([
                &display,
                "-nolisten",
                "tcp",
                "-screen",
                "0",
                &format!("{WIDTH}x{HEIGHT}x24"),
            ])
            .stderr(Stdio::null())
            .spawn()
            .expect("Xvfb is required");
        let socket = PathBuf::from(format!("/tmp/.X11-unix/X{number}"));
        let deadline = Instant::now() + Duration::from_secs(10);
        while !socket.exists() {
            assert!(Instant::now() < deadline, "Xvfb did not start");
            std::thread::sleep(Duration::from_millis(100));
        }
        let workspace = std::env::temp_dir().join(format!("glass-record-{number}"));
        let _ = std::fs::remove_dir_all(&workspace);
        std::fs::create_dir(&workspace).unwrap();
        let xauthority = Path::new("/nonexistent-xauthority");
        let started = start("s", &display, xauthority, &workspace, 30).unwrap();
        assert!(
            start("s", &display, xauthority, &workspace, 30)
                .unwrap_err()
                .contains("already running")
        );
        assert_eq!(status()["recording"], true);
        std::thread::sleep(Duration::from_secs(2));
        let stopped = stop("s").unwrap();
        let _ = xvfb.kill();
        let _ = xvfb.wait();
        assert_eq!(stopped["path"], started["path"]);
        assert!(
            stopped["path"]
                .as_str()
                .unwrap()
                .starts_with(RECORDING_DIRECTORY)
        );
        let file = workspace.join(stopped["path"].as_str().unwrap());
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let probe = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "csv=p=0",
            ])
            .arg(&file)
            .output()
            .unwrap();
        let duration: f64 = String::from_utf8_lossy(&probe.stdout)
            .trim()
            .parse()
            .unwrap();
        assert!(duration >= 1.0, "duration {duration}");
        assert!(stop("s").unwrap_err().contains("No recording"));
        let _ = std::fs::remove_dir_all(&workspace);
    }
}
