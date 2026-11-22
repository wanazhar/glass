//! Clipboard access for the managed X11 session, through `xclip` on the
//! session's private display.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{Desktop, Lease};

pub(super) const MAX_CLIPBOARD_BYTES: usize = 4096;
const XCLIP: &str = "xclip";
const XCLIP_MISSING: &str = "xclip is not installed; install it (for example `sudo apt install xclip`) to use desktop_clipboard";

enum Request<'a> {
    Read,
    Write(&'a str),
}

fn parse(value: &Value) -> Result<(&str, Request<'_>), String> {
    let object = value
        .as_object()
        .ok_or("desktop_clipboard arguments must be an object")?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "action" | "sessionId" | "text"))
    {
        return Err("desktop_clipboard accepts only action, sessionId, and text".into());
    }
    let session_id = value["sessionId"]
        .as_str()
        .ok_or("sessionId is required from desktop_status")?;
    let request = match value["action"].as_str() {
        Some("read") => Request::Read,
        Some("write") => {
            let text = value["text"]
                .as_str()
                .ok_or("text is required when action is write")?;
            if text.len() > MAX_CLIPBOARD_BYTES {
                return Err(format!(
                    "text is limited to {MAX_CLIPBOARD_BYTES} UTF-8 bytes; write it in smaller parts"
                ));
            }
            if text
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
            {
                return Err(
                    "text may contain newline and tab but no other control characters".into(),
                );
            }
            Request::Write(text)
        }
        _ => return Err("action must be `read` or `write`".into()),
    };
    Ok((session_id, request))
}

fn truncate(text: &str) -> &str {
    if text.len() <= MAX_CLIPBOARD_BYTES {
        return text;
    }
    let mut end = MAX_CLIPBOARD_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn xclip(display: &str, xauthority: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(XCLIP);
    command
        .args(["-selection", "clipboard"])
        .args(args)
        .env("DISPLAY", display)
        .env("XAUTHORITY", xauthority)
        .env_remove("WAYLAND_DISPLAY");
    command
}

fn spawn_error(error: std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::NotFound {
        XCLIP_MISSING.to_owned()
    } else {
        format!("Could not run xclip: {error}")
    }
}

fn wait_bounded(child: &mut std::process::Child) -> Result<std::process::ExitStatus, String> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("The desktop clipboard did not respond within 2s".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn read(display: &str, xauthority: &Path) -> Result<String, String> {
    let mut child = xclip(display, xauthority, &["-out"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(spawn_error)?;
    let mut stdout = child.stdout.take().ok_or("xclip produced no output")?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = std::io::Read::read_to_end(
            &mut std::io::Read::take(&mut stdout, (MAX_CLIPBOARD_BYTES * 4) as u64),
            &mut bytes,
        );
        bytes
    });
    let status = wait_bounded(&mut child)?;
    let bytes = reader.join().unwrap_or_default();
    if !status.success() {
        // xclip exits non-zero when nothing owns the clipboard.
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn write(display: &str, xauthority: &Path, text: &str) -> Result<(), String> {
    // xclip forks a background owner that serves the selection until another
    // client takes it, so the foreground process exits once stdin closes.
    let mut child = xclip(display, xauthority, &["-in"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(spawn_error)?;
    let mut stdin = child.stdin.take().ok_or("xclip accepted no input")?;
    stdin
        .write_all(text.as_bytes())
        .map_err(|e| format!("Could not write the desktop clipboard: {e}"))?;
    drop(stdin);
    if wait_bounded(&mut child)?.success() {
        Ok(())
    } else {
        Err("xclip could not take ownership of the desktop clipboard".into())
    }
}

impl Desktop {
    pub(super) fn clipboard(&self, value: &Value) -> Result<Value, String> {
        let (session_id, request) = parse(value)?;
        let (display, xauthority) = {
            let guard = self
                .session
                .lock()
                .map_err(|_| "Desktop state unavailable")?;
            let session = guard
                .as_ref()
                .ok_or("No managed desktop session is running")?;
            if session.id != session_id {
                return Err(
                    "The managed desktop session changed; call desktop_status and observe again"
                        .into(),
                );
            }
            if matches!(request, Request::Write(_)) && session.lease != Lease::Agent {
                return Err("Desktop input lease is held by the other participant".into());
            }
            (
                session.display.clone(),
                session.directory.join("Xauthority"),
            )
        };
        match request {
            Request::Read => {
                let text = read(&display, &xauthority)?;
                let kept = truncate(&text);
                Ok(json!({
                    "sessionId": session_id,
                    "text": kept,
                    "bytes": text.len(),
                    "truncated": kept.len() < text.len(),
                }))
            }
            Request::Write(text) => {
                write(&display, &xauthority, text)?;
                Ok(json!({"sessionId": session_id, "written": true, "bytes": text.len()}))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_arguments_are_closed_and_bounded() {
        assert!(parse(&json!({"action":"read","sessionId":"s"})).is_ok());
        assert!(parse(&json!({"action":"write","sessionId":"s","text":"a\tb\n"})).is_ok());
        let error = |value: Value| parse(&value).err().unwrap_or_default();
        assert!(error(json!({"action":"read"})).contains("desktop_status"));
        assert!(error(json!({"action":"copy","sessionId":"s"})).contains("`read` or `write`"));
        assert!(error(json!({"action":"write","sessionId":"s"})).contains("text is required"));
        assert!(error(json!({"action":"read","sessionId":"s","x":1})).contains("only action"));
        assert!(
            error(json!({"action":"write","sessionId":"s","text":"\u{7}"})).contains("control")
        );
        let long = "a".repeat(MAX_CLIPBOARD_BYTES + 1);
        assert!(
            error(json!({"action":"write","sessionId":"s","text":long})).contains("smaller parts")
        );
    }

    #[test]
    fn clipboard_reads_truncate_on_a_char_boundary() {
        let text = format!("{}é", "a".repeat(MAX_CLIPBOARD_BYTES - 1));
        assert_eq!(truncate(&text).len(), MAX_CLIPBOARD_BYTES - 1);
        assert_eq!(truncate("short"), "short");
    }
}

/// Opt-in with `GLASS_E2E=1`; needs `Xvfb` and `xclip`.
#[cfg(test)]
mod xvfb_tests {
    use super::*;

    struct Xvfb(std::process::Child);

    impl Drop for Xvfb {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn start_xvfb() -> (Xvfb, String) {
        let number = 940 + std::process::id() % 50;
        let display = format!(":{number}");
        let child = Command::new("Xvfb")
            .args([
                display.as_str(),
                "-nolisten",
                "tcp",
                "-screen",
                "0",
                "640x480x24",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Xvfb must be installed for GLASS_E2E desktop tests");
        let socket = format!("/tmp/.X11-unix/X{number}");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !Path::new(&socket).exists() {
            assert!(Instant::now() < deadline, "Xvfb did not start on {display}");
            std::thread::sleep(Duration::from_millis(50));
        }
        (Xvfb(child), display)
    }

    #[test]
    fn clipboard_round_trips_on_a_virtual_x11_display() {
        if std::env::var_os("GLASS_E2E").is_none() {
            return;
        }
        let (_xvfb, display) = start_xvfb();
        let xauthority = Path::new("/nonexistent-xauthority");
        assert_eq!(read(&display, xauthority).unwrap(), "");
        let text = "Quarterly report\nline two\tcol";
        write(&display, xauthority, text).unwrap();
        assert_eq!(read(&display, xauthority).unwrap(), text);
        write(&display, xauthority, "replaced").unwrap();
        assert_eq!(read(&display, xauthority).unwrap(), "replaced");
    }

    /// Drives the real manager path when a managed desktop is already running
    /// (for example after `gc desktop quick`); skipped otherwise.
    #[test]
    fn clipboard_tool_reaches_a_running_managed_desktop() {
        if std::env::var_os("GLASS_E2E").is_none() || !crate::desktop::service_running() {
            return;
        }
        let status = Desktop::agent_status().unwrap();
        let Some(session_id) = status["sessionId"].as_str() else {
            return;
        };
        assert_eq!(status["displayServer"], "x11");
        let written = Desktop::agent_clipboard(
            &json!({"action":"write","sessionId":session_id,"text":"from the agent"}),
        )
        .unwrap();
        assert_eq!(written["written"], true);
        let read =
            Desktop::agent_clipboard(&json!({"action":"read","sessionId":session_id})).unwrap();
        assert_eq!(read["text"], "from the agent");
        let stale = Desktop::agent_clipboard(&json!({"action":"read","sessionId":"stale"}))
            .err()
            .unwrap_or_default();
        assert!(stale.contains("session changed"), "{stale}");
        eprintln!("desktop_clipboard verified on managed session {session_id}");
    }
}
