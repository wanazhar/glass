//! MP4 recordings of the managed X11 desktop, written into the workspace so
//! the model's file tools and the user can reach them.

use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use super::{Desktop, HEIGHT, WIDTH, capture::stop_child, process};

pub(super) const RECORDING_DIRECTORY: &str = ".gc-artifacts/desktop/recordings";
const DEFAULT_SECONDS: u64 = 300;
const MAX_SECONDS: u64 = 600;

struct Recording {
    session_id: String,
    child: Child,
    path: PathBuf,
    relative: String,
    started: Instant,
    limit: u64,
}

static ACTIVE: Mutex<Option<Recording>> = Mutex::new(None);

fn spawn(display: &str, xauthority: &Path, path: &Path, seconds: u64) -> Result<Child, String> {
    let size = format!("{WIDTH}x{HEIGHT}");
    let input = format!("{display}.0");
    let limit = seconds.to_string();
    let output = path.to_string_lossy();
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
            "-y",
            &output,
        ],
    )
    .env("DISPLAY", display)
    .env("XAUTHORITY", xauthority)
    .stdout(Stdio::null())
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
    let bytes = std::fs::metadata(&recording.path)
        .map(|m| m.len())
        .unwrap_or(0);
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
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let relative = format!("{RECORDING_DIRECTORY}/desktop-{stamp}.mp4");
    let path = workspace.join(&relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create {RECORDING_DIRECTORY}: {error}"))?;
    }
    let child = spawn(display, xauthority, &path, seconds)?;
    *active = Some(Recording {
        session_id: session_id.to_owned(),
        child,
        path,
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
    use std::process::Command;

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
