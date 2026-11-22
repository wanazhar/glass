//! Window listing, focus and close for the managed X11 session, through
//! `wmctrl` on the session's private display.

use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

use super::{Desktop, HEIGHT, Lease, WIDTH, clipboard::wait_bounded};

const WMCTRL_MISSING: &str = "wmctrl is not installed; install it (for example `sudo apt install wmctrl`) to use desktop_windows";

#[derive(Debug, PartialEq)]
enum Request<'a> {
    List,
    Focus(&'a str),
    Close(&'a str),
}

fn parse(value: &Value) -> Result<(&str, Request<'_>), String> {
    let object = value
        .as_object()
        .ok_or("desktop_windows arguments must be an object")?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "action" | "sessionId" | "id"))
    {
        return Err("desktop_windows accepts only action, sessionId, and id".into());
    }
    let session_id = value["sessionId"]
        .as_str()
        .ok_or("sessionId is required from desktop_status")?;
    let id = || {
        value["id"]
            .as_str()
            .filter(|id| {
                id.len() > 2
                    && id.len() <= 10
                    && id.starts_with("0x")
                    && id[2..].bytes().all(|b| b.is_ascii_hexdigit())
            })
            .ok_or("id must be a window id such as \"0x01a00007\" from desktop_windows list")
    };
    let request = match value["action"].as_str() {
        Some("list") => Request::List,
        Some("focus") => Request::Focus(id()?),
        Some("close") => Request::Close(id()?),
        _ => return Err("action must be `list`, `focus`, or `close`".into()),
    };
    Ok((session_id, request))
}

/// Parses `wmctrl -l -x -G`: id, desktop, x, y, width, height, class, host,
/// title. Desktop -1 includes ordinary applications pinned to all workspaces.
fn parse_list(output: &str) -> Vec<Value> {
    let round = |value: f64| (value * 10_000.0).round() / 10_000.0;
    output
        .lines()
        .filter_map(|line| {
            let mut rest = line;
            let mut fields = Vec::with_capacity(8);
            for _ in 0..8 {
                rest = rest.trim_start();
                let end = rest.find(char::is_whitespace)?;
                fields.push(&rest[..end]);
                rest = &rest[end..];
            }
            let number = |index: usize| fields[index].parse::<f64>().ok();
            let (x, y, w, h) = (number(2)?, number(3)?, number(4)?, number(5)?);
            Some(json!({
                "id": fields[0],
                "title": rest.trim(),
                "app": fields[6],
                "x": round((x / WIDTH as f64).clamp(0.0, 1.0)),
                "y": round((y / HEIGHT as f64).clamp(0.0, 1.0)),
                "width": round((w / WIDTH as f64).clamp(0.0, 1.0)),
                "height": round((h / HEIGHT as f64).clamp(0.0, 1.0)),
            }))
        })
        .collect()
}

fn wmctrl(display: &str, xauthority: &Path, args: &[&str]) -> Result<String, String> {
    x11_command("wmctrl", display, xauthority, args)
}

fn x11_command(
    program: &str,
    display: &str,
    xauthority: &Path,
    args: &[&str],
) -> Result<String, String> {
    let mut child = Command::new(program)
        .args(args)
        .env("DISPLAY", display)
        .env("XAUTHORITY", xauthority)
        .env_remove("WAYLAND_DISPLAY")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound && program == "wmctrl" {
                WMCTRL_MISSING.to_owned()
            } else {
                format!("Could not run {program}: {error}")
            }
        })?;
    let status = wait_bounded(&mut child)?;
    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        let _ = std::io::Read::read_to_string(&mut pipe, &mut stdout);
    }
    if status.success() {
        Ok(stdout)
    } else {
        Err(
            "The desktop window manager did not answer; call desktop_screenshot to see the desktop"
                .into(),
        )
    }
}

fn application_windows(
    windows: Vec<Value>,
    mut properties: impl FnMut(&str) -> Result<String, String>,
) -> Result<Vec<Value>, String> {
    let mut applications = Vec::new();
    for window in windows {
        let types = properties(window["id"].as_str().ok_or("missing window id")?)?;
        if !types
            .split(|c: char| c.is_whitespace() || matches!(c, ',' | '='))
            .any(|atom| {
                matches!(
                    atom,
                    "_NET_WM_WINDOW_TYPE_DOCK" | "_NET_WM_WINDOW_TYPE_DESKTOP"
                )
            })
        {
            applications.push(window);
        }
    }
    Ok(applications)
}

impl Desktop {
    pub(super) fn windows(&self, value: &Value) -> Result<Value, String> {
        self.windows_with(value, run)
    }

    fn windows_with(
        &self,
        value: &Value,
        operation: impl FnOnce(&str, &Path, &str, Request<'_>) -> Result<Value, String>,
    ) -> Result<Value, String> {
        let (session_id, request) = parse(value)?;
        let mut guard = self
            .session
            .lock()
            .map_err(|_| "Desktop state unavailable")?;
        let session = guard
            .as_mut()
            .ok_or("No managed desktop session is running")?;
        if session.id != session_id {
            return Err(
                "The managed desktop session changed; call desktop_status and observe again".into(),
            );
        }
        if request != Request::List && session.lease != Lease::Agent {
            return Err("Desktop input lease is held by the other participant".into());
        }
        let changes_windows = request != Request::List;
        operation(
            &session.display,
            &session.directory.join("Xauthority"),
            session_id,
            request,
        )
        .inspect(|_| {
            if changes_windows {
                session.accessibility_revision = session.accessibility_revision.saturating_add(1);
                session.accessibility_targets.clear();
            }
        })
    }
}

fn run(
    display: &str,
    xauthority: &Path,
    session_id: &str,
    request: Request<'_>,
) -> Result<Value, String> {
    let windows = application_windows(
        parse_list(&wmctrl(display, xauthority, &["-l", "-x", "-G"])?),
        |id| {
            x11_command(
                "xprop",
                display,
                xauthority,
                &["-id", id, "_NET_WM_WINDOW_TYPE"],
            )
        },
    )?;
    let (flag, id, done) = match request {
        Request::List => return Ok(json!({"sessionId": session_id, "windows": windows})),
        Request::Focus(id) => ("-a", id, "focused"),
        Request::Close(id) => ("-c", id, "closed"),
    };
    let same = |other: &str| {
        u64::from_str_radix(&other[2..], 16).ok() == u64::from_str_radix(&id[2..], 16).ok()
    };
    if !windows
        .iter()
        .any(|window| window["id"].as_str().is_some_and(same))
    {
        return Err(format!(
            "no window has id {id}; call desktop_windows with action list for current ids"
        ));
    }
    wmctrl(display, xauthority, &["-i", flag, id])?;
    Ok(json!({"sessionId": session_id, "id": id, done: true}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_operations_hold_the_lease_and_only_mutations_invalidate_targets() {
        let desktop = super::super::tests::desktop_with_test_session(&std::env::temp_dir(), "s");
        let list = json!({"action":"list","sessionId":"s"});
        desktop
            .windows_with(&list, |_, _, _, _| {
                assert!(desktop.session.try_lock().is_err());
                Ok(json!({"windows":[]}))
            })
            .unwrap();
        assert_eq!(
            desktop
                .session
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .accessibility_revision,
            1
        );
        let focus = json!({"action":"focus","sessionId":"s","id":"0x01"});
        assert!(
            desktop
                .windows_with(&focus, |_, _, _, _| Err("failed".into()))
                .is_err()
        );
        assert_eq!(
            desktop
                .session
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .accessibility_revision,
            1
        );
        desktop
            .windows_with(&focus, |_, _, _, _| {
                assert!(desktop.session.try_lock().is_err());
                Ok(json!({"focused":true}))
            })
            .unwrap();
        {
            let mut guard = desktop.session.lock().unwrap();
            let session = guard.as_mut().unwrap();
            assert_eq!(session.accessibility_revision, 2);
            session.lease = Lease::Human;
        }
        assert!(
            desktop
                .windows_with(&focus, |_, _, _, _| panic!("stale holder acted"))
                .unwrap_err()
                .contains("lease")
        );
        desktop
            .windows_with(&list, |_, _, _, _| Ok(json!({"windows":[]})))
            .unwrap();
    }

    #[test]
    fn window_arguments_are_closed_and_ids_are_checked() {
        assert_eq!(
            parse(&json!({"action":"list","sessionId":"s"})).unwrap().1,
            Request::List
        );
        assert_eq!(
            parse(&json!({"action":"focus","sessionId":"s","id":"0x01a00007"}))
                .unwrap()
                .1,
            Request::Focus("0x01a00007")
        );
        let error = |value: Value| parse(&value).err().unwrap_or_default();
        assert!(error(json!({"action":"list"})).contains("desktop_status"));
        assert!(error(json!({"action":"close","sessionId":"s"})).contains("desktop_windows list"));
        assert!(
            error(json!({"action":"close","sessionId":"s","id":"1;rm"})).contains("0x01a00007")
        );
        assert!(error(json!({"action":"minimize","sessionId":"s"})).contains("`focus`"));
        assert!(
            error(json!({"action":"list","sessionId":"s","title":"x"})).contains("only action")
        );
    }

    #[test]
    fn wmctrl_listing_keeps_pinned_apps_but_excludes_panels_by_type() {
        let output = "0x00e00003 -1 0    0    1280 800  xfdesktop.Xfdesktop  host Desktop\n\
                      0x01a00007 -1 320  200  640  400  mousepad.Mousepad  host Untitled 1 - Mousepad\n";
        assert_eq!(parse_list(output).len(), 2);
        let windows = application_windows(parse_list(output), |id| {
            Ok(if id == "0x00e00003" {
                "_NET_WM_WINDOW_TYPE(ATOM) = _NET_WM_WINDOW_TYPE_DESKTOP".into()
            } else {
                "_NET_WM_WINDOW_TYPE(ATOM) = _NET_WM_WINDOW_TYPE_NORMAL".into()
            })
        })
        .unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0]["id"], "0x01a00007");
        assert_eq!(windows[0]["title"], "Untitled 1 - Mousepad");
        assert_eq!(windows[0]["app"], "mousepad.Mousepad");
        assert_eq!(windows[0]["x"], 0.25);
        assert_eq!(windows[0]["height"], 0.5);
        assert!(application_windows(parse_list(output), |_| Err("display closed".into())).is_err());
        assert!(
            application_windows(Vec::new(), |_| panic!("no windows"))
                .unwrap()
                .is_empty()
        );
        assert!(
            application_windows(parse_list(output), |_| Ok(
                "_NET_WM_WINDOW_TYPE(ATOM) = _NET_WM_WINDOW_TYPE_DOCK".into()
            ))
            .unwrap()
            .is_empty()
        );
    }
}

/// Drives zoom, windows and files through the real manager path when a
/// managed desktop is already running (for example after `gc desktop quick`).
#[cfg(test)]
mod managed_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn desktop_additions_reach_a_running_managed_desktop() {
        if std::env::var_os("GLASS_E2E").is_none() || !crate::desktop::service_running() {
            return;
        }
        let status = Desktop::agent_status().unwrap();
        let Some(session_id) = status["sessionId"].as_str() else {
            return;
        };
        let region = json!({"x":0.0,"y":0.0,"width":0.25,"height":0.25});
        let zoomed = Desktop::agent_frame(session_id, &region).unwrap();
        assert_eq!(zoomed["imageWidth"], 1280, "{zoomed}");
        assert_eq!(zoomed["region"], region);
        assert!(zoomed["jpeg"].as_str().is_some_and(|jpeg| jpeg.len() > 100));
        let full = Desktop::agent_frame(session_id, &Value::Null).unwrap();
        assert!(full.get("region").is_none());

        for action in [
            json!({"action":"pointer-double-click","sessionId":session_id,"x":0.6,"y":0.6}),
            json!({"action":"pointer-drag","sessionId":session_id,"x":0.6,"y":0.6,"toX":0.7,"toY":0.7}),
        ] {
            let sent = Desktop::agent_input(&action).unwrap();
            assert_eq!(sent["sent"], true, "{sent}");
        }
        let bad_drag = Desktop::agent_input(
            &json!({"action":"pointer-drag","sessionId":session_id,"x":0.6,"y":0.6}),
        )
        .err()
        .unwrap_or_default();
        assert!(bad_drag.contains("toX"), "{bad_drag}");

        let listed =
            Desktop::agent_windows(&json!({"action":"list","sessionId":session_id})).unwrap();
        assert!(listed["windows"].is_array(), "{listed}");
        let missing = Desktop::agent_windows(
            &json!({"action":"focus","sessionId":session_id,"id":"0x7fffffff"}),
        )
        .err()
        .unwrap_or_default();
        assert!(missing.contains("action list"), "{missing}");

        let name = format!("glass-files-{}.txt", std::process::id());
        let workspace = PathBuf::from(status["workspaceRoot"].as_str().unwrap());
        std::fs::write(workspace.join(&name), "desktop transfer").unwrap();
        let imported = Desktop::agent_files(
            &json!({"action":"import","sessionId":session_id,"path":name}),
            &workspace,
        )
        .unwrap();
        let exported = Desktop::agent_files(
            &json!({"action":"export","sessionId":session_id,"name":name}),
            &workspace,
        )
        .unwrap();
        let path = workspace.join(exported["path"].as_str().unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "desktop transfer");
        let _ = std::fs::remove_file(imported["desktopPath"].as_str().unwrap());
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(workspace.join(&name));
        eprintln!(
            "zoom, {} windows and file transfer verified on managed session {session_id}",
            listed["windows"].as_array().unwrap().len()
        );
    }
}

/// Opt-in with `GLASS_E2E=1`; needs `Xvfb`, `xfwm4`, `wmctrl` and `xev`.
#[cfg(test)]
mod xvfb_tests {
    use super::*;
    use std::time::{Duration, Instant};

    struct Kill(std::process::Child);

    impl Drop for Kill {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn spawn(display: &str, program: &str, args: &[&str], stdout: Stdio) -> Kill {
        Kill(
            Command::new(program)
                .args(args)
                .env("DISPLAY", display)
                .stdout(stdout)
                .stderr(Stdio::null())
                .spawn()
                .unwrap_or_else(|error| panic!("{program} is required: {error}")),
        )
    }

    fn eventually<T>(mut check: impl FnMut() -> Option<T>, what: &str) -> T {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(value) = check() {
                return value;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    #[test]
    fn windows_drag_and_double_click_work_on_a_virtual_x11_display() {
        if std::env::var_os("GLASS_E2E").is_none() {
            return;
        }
        let number = 900 + std::process::id() % 40;
        let display = format!(":{number}");
        let geometry = format!("{WIDTH}x{HEIGHT}x24");
        let _xvfb = spawn(
            &display,
            "Xvfb",
            &[&display, "-nolisten", "tcp", "-screen", "0", &geometry],
            Stdio::null(),
        );
        let socket = format!("/tmp/.X11-unix/X{number}");
        eventually(|| Path::new(&socket).exists().then_some(()), "Xvfb");
        let _wm = spawn(&display, "xfwm4", &["--compositor=off"], Stdio::null());
        let log = std::env::temp_dir().join(format!("glass-xev-{number}.log"));
        let _xev = spawn(
            &display,
            "xev",
            &[
                "-event",
                "mouse",
                "-geometry",
                "400x300+200+150",
                "-name",
                "GlassProbe",
            ],
            Stdio::from(std::fs::File::create(&log).unwrap()),
        );
        let xauthority = Path::new("/nonexistent-xauthority");
        let list = || run(&display, xauthority, "s", Request::List).ok();
        let probe = eventually(
            || {
                list()?["windows"]
                    .as_array()?
                    .iter()
                    .find(|window| window["title"] == "GlassProbe")
                    .cloned()
            },
            "the xev window in desktop_windows list",
        );
        let id = probe["id"].as_str().unwrap().to_owned();
        let focused = run(&display, xauthority, "s", Request::Focus(&id)).unwrap();
        assert_eq!(focused["focused"], true);
        let missing = run(&display, xauthority, "s", Request::Focus("0x7fffffff"))
            .err()
            .unwrap();
        assert!(missing.contains("action list"), "{missing}");

        let center = |axis: &str, size: &str| {
            probe[axis].as_f64().unwrap() + probe[size].as_f64().unwrap() / 2.0
        };
        let (x, y) = (center("x", "width"), center("y", "height"));
        let cookie = [0u8; 16];
        super::super::input::inject(
            &display,
            &cookie,
            "pointer-double-click",
            &json!({"x": x, "y": y}),
        )
        .unwrap();
        super::super::input::inject(
            &display,
            &cookie,
            "pointer-triple-click",
            &json!({"x": x, "y": y}),
        )
        .unwrap();
        super::super::input::inject(
            &display,
            &cookie,
            "pointer-drag",
            &json!({"x": x - 0.05, "y": y, "toX": x + 0.05, "toY": y + 0.05}),
        )
        .unwrap();
        let events = eventually(
            || {
                let text = std::fs::read_to_string(&log).ok()?;
                (text.matches("ButtonRelease").count() >= 6).then_some(text)
            },
            "xev button events",
        );
        assert_eq!(events.matches("ButtonPress").count(), 6, "{events}");
        assert!(events.matches("MotionNotify").count() >= 10, "{events}");

        // xev ignores WM_DELETE_WINDOW, so close a window that honours it.
        let _message = spawn(
            &display,
            "xmessage",
            &["-title", "GlassClose", "close me"],
            Stdio::null(),
        );
        let find = |title: &str| {
            list()?["windows"]
                .as_array()?
                .iter()
                .find(|window| window["title"] == title)
                .and_then(|window| window["id"].as_str().map(str::to_owned))
        };
        let message = eventually(|| find("GlassClose"), "the xmessage window");
        let closed = run(&display, xauthority, "s", Request::Close(&message)).unwrap();
        assert_eq!(closed["closed"], true);
        eventually(
            || find("GlassClose").is_none().then_some(()),
            "the xmessage window to close",
        );
        let _ = std::fs::remove_file(&log);
    }
}
