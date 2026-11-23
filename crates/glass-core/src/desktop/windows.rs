//! Window listing, focus and close for the managed X11 session, through
//! `wmctrl` on the session's private display.

use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

use super::{Desktop, HEIGHT, Lease, WIDTH, clipboard::wait_bounded};

const WMCTRL_MISSING: &str = "wmctrl is not installed; install it (for example `sudo apt install wmctrl`) to use desktop_windows or labelled screenshots";

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

fn window_number(window: &Value) -> Option<u64> {
    u64::from_str_radix(window["id"].as_str()?.strip_prefix("0x")?, 16).ok()
}

/// Keeps listed windows that are in the stacking order, bottom to top.
fn order_by_stacking(windows: Vec<Value>, order: &[u64]) -> Vec<Value> {
    let mut stacked: Vec<(usize, Value)> = windows
        .into_iter()
        .filter_map(|window| {
            let number = window_number(&window)?;
            Some((order.iter().position(|id| *id == number)?, window))
        })
        .collect();
    stacked.sort_by_key(|(position, _)| *position);
    stacked.into_iter().map(|(_, window)| window).collect()
}

fn stack(display: &str, xauthority: &Path) -> Result<Vec<Value>, String> {
    let mut windows = parse_list(&wmctrl(display, xauthority, &["-l", "-x", "-G"])?);
    let output = x11_command(
        "xprop",
        display,
        xauthority,
        &["-root", "_NET_CURRENT_DESKTOP"],
    )?;
    let desktop = property_numbers(&output, "_NET_CURRENT_DESKTOP")
        .first()
        .copied()
        .ok_or("X11 current desktop is unavailable")?;
    for window in &mut windows {
        let id = window["id"].as_str().ok_or("missing window id")?;
        let properties = x11_command(
            "xprop",
            display,
            xauthority,
            &[
                "-id",
                id,
                "_NET_WM_PID",
                "_NET_WM_DESKTOP",
                "_NET_WM_STATE",
                "_NET_FRAME_EXTENTS",
            ],
        )?;
        let geometry = x11_command_lossy("xwininfo", display, xauthority, &["-id", id])?;
        apply_stack_metadata(window, &properties, &geometry, desktop)?;
    }
    let tree = x11_command_bytes("xwininfo", display, xauthority, &["-root", "-tree"])?;
    let mut windows = include_popup_occluders(windows, &tree, desktop, |id| {
        x11_command_lossy(
            "xwininfo",
            display,
            xauthority,
            &["-id", &format!("0x{id:x}")],
        )
    })?;
    for window in windows
        .iter_mut()
        .filter(|window| window["occluderOnly"] == true)
    {
        let id = window_number(window).ok_or("missing popup window id")?;
        window["pid"] = json!(popup_process_id(id, |id| {
            x11_command(
                "xprop",
                display,
                xauthority,
                &[
                    "-id",
                    &format!("0x{id:x}"),
                    "_NET_WM_PID",
                    "WM_CLIENT_LEADER",
                ],
            )
        })?);
    }
    Ok(windows)
}

fn popup_process_id(
    id: u64,
    mut properties: impl FnMut(u64) -> Result<String, String>,
) -> Result<Option<i64>, String> {
    let output = properties(id)?;
    let pid = |output: &str| {
        property_numbers(output, "_NET_WM_PID")
            .first()
            .copied()
            .filter(|pid| *pid > 0)
    };
    if let Some(pid) = pid(&output) {
        return Ok(Some(pid));
    }
    let leader = output.lines().find_map(|line| {
        let (property, value) = line.split_once('#')?;
        if property.trim() != "WM_CLIENT_LEADER(WINDOW): window id" {
            return None;
        }
        u64::from_str_radix(value.trim().strip_prefix("0x")?, 16).ok()
    });
    match leader.filter(|leader| *leader != 0 && *leader != id) {
        Some(leader) => Ok(pid(&properties(leader)?)),
        None => Ok(None),
    }
}

fn root_stacking(tree: &[u8], clients: &[u64]) -> Result<Vec<u64>, String> {
    let tree = String::from_utf8_lossy(tree);
    let mut groups: Vec<(u64, Vec<u64>)> = Vec::new();
    let mut root_indent = None;
    for line in tree.lines() {
        let trimmed = line.trim_start();
        let Some(id) = trimmed
            .split_whitespace()
            .next()
            .and_then(|id| id.strip_prefix("0x"))
            .and_then(|id| u64::from_str_radix(id, 16).ok())
        else {
            continue;
        };
        let indent = line.len() - trimmed.len();
        if indent == *root_indent.get_or_insert(indent) {
            groups.push((id, Vec::new()));
        }
        if clients.contains(&id)
            && let Some((_, group)) = groups.last_mut()
        {
            group.push(id);
        }
    }
    let found = groups
        .iter()
        .flat_map(|(_, group)| group)
        .collect::<Vec<_>>();
    if clients.iter().any(|id| !found.contains(&id)) {
        return Err("X11 client list changed while reading the root window tree".into());
    }
    Ok(groups
        .into_iter()
        .rev()
        .flat_map(|(root, group)| {
            if group.is_empty() {
                vec![root]
            } else {
                group.into_iter().rev().collect()
            }
        })
        .collect())
}

fn include_popup_occluders(
    mut windows: Vec<Value>,
    tree: &[u8],
    desktop: i64,
    mut geometry: impl FnMut(u64) -> Result<String, String>,
) -> Result<Vec<Value>, String> {
    let clients = windows.iter().filter_map(window_number).collect::<Vec<_>>();
    let order = root_stacking(tree, &clients)?;
    for id in order.iter().filter(|id| !clients.contains(id)) {
        let geometry = geometry(*id)?;
        if geometry_field(&geometry, "Override Redirect State:") != Some("yes")
            || geometry_field(&geometry, "Map State:") != Some("IsViewable")
            || geometry_field(&geometry, "Class:") != Some("InputOutput")
        {
            continue;
        }
        let mut popup = json!({"id":format!("0x{id:x}"),"occluderOnly":true});
        apply_stack_metadata(&mut popup, "", &geometry, desktop)?;
        popup["visible"] = json!(true);
        windows.push(popup);
    }
    Ok(order_by_stacking(windows, &order))
}

fn geometry_field<'a>(geometry: &'a str, name: &str) -> Option<&'a str> {
    geometry
        .lines()
        .find_map(|line| line.trim().strip_prefix(name).map(str::trim))
}

fn property_numbers(output: &str, name: &str) -> Vec<i64> {
    output
        .lines()
        .find_map(|line| {
            let (property, values) = line.split_once('=')?;
            (property.split('(').next()?.trim() == name).then(|| {
                values
                    .split(',')
                    .filter_map(|value| value.trim().parse().ok())
                    .collect()
            })
        })
        .unwrap_or_default()
}

fn apply_stack_metadata(
    window: &mut Value,
    properties: &str,
    geometry: &str,
    desktop: i64,
) -> Result<(), String> {
    let field = |name: &str| geometry_field(geometry, name);
    let number = |name: &str| {
        field(name)
            .and_then(|value| value.parse::<i64>().ok())
            .ok_or("X11 window geometry is unavailable")
    };
    let extents = property_numbers(properties, "_NET_FRAME_EXTENTS");
    let extent = |index: usize| extents.get(index).copied().unwrap_or(0).max(0) as f64;
    let (left, right, top, bottom) = (extent(0), extent(1), extent(2), extent(3));
    let (x, y, width, height) = (
        number("Absolute upper-left X:")? as f64,
        number("Absolute upper-left Y:")? as f64,
        number("Width:")? as f64,
        number("Height:")? as f64,
    );
    if width <= 0.0 || height <= 0.0 {
        return Err("X11 window geometry is empty".into());
    }
    window["x"] = json!((x - left) / WIDTH as f64);
    window["y"] = json!((y - top) / HEIGHT as f64);
    window["width"] = json!((width + left + right) / WIDTH as f64);
    window["height"] = json!((height + top + bottom) / HEIGHT as f64);
    window["pid"] = json!(
        property_numbers(properties, "_NET_WM_PID")
            .first()
            .copied()
            .filter(|pid| *pid > 0)
    );
    let workspace = property_numbers(properties, "_NET_WM_DESKTOP")
        .first()
        .copied();
    window["desktop"] = json!(workspace);
    let hidden = properties
        .lines()
        .filter(|line| line.starts_with("_NET_WM_STATE("))
        .any(|line| {
            line.split(|c: char| c.is_whitespace() || c == ',')
                .any(|atom| atom == "_NET_WM_STATE_HIDDEN")
        });
    window["visible"] = json!(
        field("Map State:") == Some("IsViewable")
            && !hidden
            && workspace.is_some_and(|workspace| workspace == desktop
                || workspace == -1
                || workspace == u32::MAX as i64)
    );
    Ok(())
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
    String::from_utf8(x11_command_bytes(program, display, xauthority, args)?)
        .map_err(|error| error.to_string())
}

fn x11_command_bytes(
    program: &str,
    display: &str,
    xauthority: &Path,
    args: &[&str],
) -> Result<Vec<u8>, String> {
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
    let mut pipe = child
        .stdout
        .take()
        .ok_or("X11 command produced no output")?;
    let reader = std::thread::spawn(move || {
        let mut stdout = Vec::new();
        std::io::Read::read_to_end(&mut pipe, &mut stdout).map(|_| stdout)
    });
    let status = wait_bounded(&mut child)?;
    let stdout = reader
        .join()
        .map_err(|_| "X11 command reader stopped")?
        .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(stdout)
    } else {
        Err(
            "The desktop window manager did not answer; call desktop_screenshot to see the desktop"
                .into(),
        )
    }
}

fn x11_command_lossy(
    program: &str,
    display: &str,
    xauthority: &Path,
    args: &[&str],
) -> Result<String, String> {
    Ok(
        String::from_utf8_lossy(&x11_command_bytes(program, display, xauthority, args)?)
            .into_owned(),
    )
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
    /// Windows bottom to top, for labelled screenshots to skip covered controls.
    pub(super) fn window_stack(&self, session_id: &str) -> Result<Value, String> {
        let (display, xauthority) = {
            let guard = self
                .session
                .lock()
                .map_err(|_| "Desktop state unavailable")?;
            let session = guard
                .as_ref()
                .filter(|session| session.id == session_id)
                .ok_or("The managed desktop session changed; call desktop_status again")?;
            (
                session.display.clone(),
                session.directory.join("Xauthority"),
            )
        };
        Ok(json!({"windows": stack(&display, &xauthority)?}))
    }

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

    #[test]
    fn stacking_orders_listed_windows_bottom_to_top() {
        let order = [0xe00003, 0x1a00007, 0x2200004];
        let windows = vec![
            json!({"id": "0x02200004", "title": "Files"}),
            json!({"id": "0x01a00007", "title": "Mousepad"}),
            json!({"id": "0x03300001", "title": "Gone"}),
        ];
        let titles: Vec<Value> = order_by_stacking(windows, &order)
            .into_iter()
            .map(|window| window["title"].clone())
            .collect();
        assert_eq!(titles, ["Mousepad", "Files"]);
    }

    #[test]
    fn x11_commands_drain_large_window_trees() {
        const DISPLAY: &str = "glass-large-window-tree-test";
        let tree = "x".repeat(256 * 1024);
        if std::env::var("DISPLAY").as_deref() == Ok(DISPLAY) {
            println!("{tree}");
            return;
        }
        let output = x11_command(
            std::env::current_exe().unwrap().to_str().unwrap(),
            DISPLAY,
            Path::new("/nonexistent-xauthority"),
            &[
                "--exact",
                "desktop::windows::tests::x11_commands_drain_large_window_trees",
                "--nocapture",
            ],
        )
        .unwrap();
        assert!(output.contains(&tree));
    }

    #[test]
    fn root_tree_reader_accepts_legacy_titles_on_unrelated_windows() {
        const DISPLAY: &str = "glass-legacy-window-tree-test";
        const TREE: &[u8] = b"  Root window id: 0xff\n     3 children:\n     0x3 \"Caf\xe9\" unrelated popup\n     0x2 frame\n        0x20 managed client\n     0x1 managed client\nxwininfo: Window id: 0x3 \"Caf\xe9\"\nAbsolute upper-left X: 0\nAbsolute upper-left Y: 0\nWidth: 200\nHeight: 100\nOverride Redirect State: yes\nMap State: IsViewable\nClass: InputOutput\n";
        if std::env::var("DISPLAY").as_deref() == Ok(DISPLAY) {
            std::io::Write::write_all(&mut std::io::stdout(), TREE).unwrap();
            return;
        }
        let output = x11_command_bytes(
            std::env::current_exe().unwrap().to_str().unwrap(),
            DISPLAY,
            Path::new("/nonexistent-xauthority"),
            &["--exact", "desktop::windows::tests::root_tree_reader_accepts_legacy_titles_on_unrelated_windows", "--nocapture"],
        ).unwrap();
        assert!(std::str::from_utf8(&output).is_err());
        let geometry = x11_command_lossy(
            std::env::current_exe().unwrap().to_str().unwrap(),
            DISPLAY,
            Path::new("/nonexistent-xauthority"),
            &["--exact", "desktop::windows::tests::root_tree_reader_accepts_legacy_titles_on_unrelated_windows", "--nocapture"],
        ).unwrap();
        assert!(geometry.contains("Caf\u{fffd}"));
        let stacked = include_popup_occluders(
            vec![json!({"id":"0x20"}), json!({"id":"0x1"})],
            &output,
            0,
            |id| {
                assert_eq!(id, 3);
                Ok(geometry.clone())
            },
        )
        .unwrap();
        assert_eq!(
            stacked.iter().filter_map(window_number).collect::<Vec<_>>(),
            [1, 0x20, 3]
        );
        assert_eq!(stacked[2]["occluderOnly"], true);
    }

    #[test]
    fn popup_process_ids_use_the_popup_or_its_client_leader() {
        assert_eq!(
            popup_process_id(3, |id| {
                assert_eq!(id, 3);
                Ok(
                    "_NET_WM_PID(CARDINAL) = 11\nWM_CLIENT_LEADER(WINDOW): window id # 0x4\n"
                        .into(),
                )
            })
            .unwrap(),
            Some(11)
        );
        assert_eq!(
            popup_process_id(3, |id| {
                Ok(match id {
                    3 => "_NET_WM_PID: not found.\nWM_CLIENT_LEADER(WINDOW): window id # 0x4\n",
                    4 => "_NET_WM_PID(CARDINAL) = 11\n",
                    _ => panic!("unexpected window"),
                }
                .into())
            })
            .unwrap(),
            Some(11)
        );
        for output in [
            "",
            "_NET_WM_PID(CARDINAL) = 0",
            "_NET_WM_PID(CARDINAL) = -1",
            "WM_CLIENT_LEADER(WINDOW): window id # 0x3",
            "WM_CLIENT_LEADER(WINDOW): window id # 0x0",
            "WM_CLIENT_LEADER(WINDOW): window id # invalid",
        ] {
            assert_eq!(popup_process_id(3, |_| Ok(output.into())).unwrap(), None);
        }
        assert!(popup_process_id(3, |_| Err("closed".into())).is_err());
        assert!(
            popup_process_id(3, |id| if id == 3 {
                Ok("WM_CLIENT_LEADER(WINDOW): window id # 0x4".into())
            } else {
                Err("closed leader".into())
            })
            .is_err()
        );
    }

    #[test]
    fn popup_occluders_follow_root_stacking_without_including_managed_frames() {
        let tree = "  Root window id: 0xff\n     5 children:\n     0x20 frame\n        0x2 client\n     0x3 popup\n     0x10 frame\n        0x11 wrapper\n           0x1 client\n     0x4 popup\n     0x5 hidden\n";
        let windows = vec![json!({"id":"0x1"}), json!({"id":"0x2"})];
        let geometry = "Absolute upper-left X: -10\nAbsolute upper-left Y: 50\nWidth: 200\nHeight: 100\nOverride Redirect State: yes\nMap State: IsViewable\nClass: InputOutput\n";
        let stacked = include_popup_occluders(windows, tree.as_bytes(), 0, |id| {
            assert!([3, 4, 5].contains(&id), "managed frames are not occluders");
            Ok(if id == 5 {
                geometry.replace("IsViewable", "IsUnMapped")
            } else {
                geometry.into()
            })
        })
        .unwrap();
        let ids = stacked.iter().filter_map(window_number).collect::<Vec<_>>();
        assert_eq!(ids, [4, 1, 3, 2]);
        assert_eq!(stacked[2]["visible"], true);
        assert_eq!(stacked[2]["occluderOnly"], true);
        assert_eq!(stacked[2]["x"], -10.0 / WIDTH as f64);
        assert_eq!(stacked[2]["width"], 200.0 / WIDTH as f64);
        assert!(stacked[2]["pid"].is_null());
    }

    #[test]
    fn popup_occluders_require_viewable_output_windows_and_propagate_errors() {
        let tree = "     0x1 popup\n";
        let geometry = "Absolute upper-left X: 0\nAbsolute upper-left Y: 0\nWidth: 100\nHeight: 100\nOverride Redirect State: yes\nMap State: IsViewable\nClass: InputOutput\n";
        for excluded in [
            geometry.replace("yes", "no"),
            geometry.replace("IsViewable", "IsUnviewable"),
            geometry.replace("IsViewable", "IsUnMapped"),
            geometry.replace("InputOutput", "InputOnly"),
        ] {
            assert!(
                include_popup_occluders(vec![], tree.as_bytes(), 0, |_| Ok(excluded.clone()))
                    .unwrap()
                    .is_empty()
            );
        }
        assert!(
            include_popup_occluders(vec![], b"", 0, |_| panic!("no windows"))
                .unwrap()
                .is_empty()
        );
        assert!(
            include_popup_occluders(vec![], tree.as_bytes(), 0, |_| Err("closed".into())).is_err()
        );
        assert!(
            include_popup_occluders(vec![], tree.as_bytes(), 0, |_| Ok(
                geometry.replace("Width: 100", "")
            ))
            .is_err()
        );
        assert!(
            include_popup_occluders(vec![json!({"id":"0x2"})], tree.as_bytes(), 0, |_| {
                panic!("missing client must fail closed")
            })
            .unwrap_err()
            .contains("root window tree")
        );
    }

    #[test]
    fn stack_metadata_preserves_negative_origins_and_adjusts_for_frames() {
        let mut window = json!({});
        let geometry = "  Absolute upper-left X: -100\n Absolute upper-left Y: -20\n Width: 800\n Height: 600\n Map State: IsViewable\n";
        let properties = "_NET_WM_PID(CARDINAL) = 123\n_NET_WM_DESKTOP(CARDINAL) = 2\n_NET_WM_STATE(ATOM) = \n_NET_FRAME_EXTENTS(CARDINAL) = 5, 7, 30, 9\n";
        apply_stack_metadata(&mut window, properties, geometry, 2).unwrap();
        assert_eq!(window["x"], -105.0 / WIDTH as f64);
        assert_eq!(window["y"], -50.0 / HEIGHT as f64);
        assert_eq!(window["width"], 812.0 / WIDTH as f64);
        assert_eq!(window["height"], 639.0 / HEIGHT as f64);
        assert_eq!(window["pid"], 123);
        assert_eq!(window["visible"], true);
        assert!(apply_stack_metadata(&mut window, properties, "", 2).is_err());
        assert!(
            apply_stack_metadata(
                &mut window,
                properties,
                &geometry.replace("Width: 800", "Width: 0"),
                2
            )
            .is_err()
        );
    }

    #[test]
    fn stack_metadata_filters_hidden_unmapped_and_other_workspaces_but_keeps_pinned() {
        let mut window = json!({});
        let geometry = "Absolute upper-left X: 0\nAbsolute upper-left Y: 0\nWidth: 100\nHeight: 100\nMap State: IsViewable";
        for (desktop, state, mapped, visible) in [
            (2, "", "IsViewable", true),
            (1, "", "IsViewable", false),
            (-1, "", "IsViewable", true),
            (u32::MAX as i64, "", "IsViewable", true),
            (2, "_NET_WM_STATE_HIDDEN", "IsViewable", false),
            (2, "", "IsUnMapped", false),
        ] {
            let properties =
                format!("_NET_WM_DESKTOP(CARDINAL) = {desktop}\n_NET_WM_STATE(ATOM) = {state}");
            apply_stack_metadata(
                &mut window,
                &properties,
                &geometry.replace("IsViewable", mapped),
                2,
            )
            .unwrap();
            assert_eq!(window["visible"], visible, "{properties} {mapped}");
        }
        apply_stack_metadata(&mut window, "", geometry, 2).unwrap();
        assert_eq!(window["visible"], false);
        assert!(window["pid"].is_null());
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

        {
            let _popup = spawn(
                &display,
                "xmessage",
                &[
                    "-title",
                    "GlassPopup",
                    "-xrm",
                    "*overrideRedirect: true",
                    "-geometry",
                    "200x100+200+150",
                    "popup",
                ],
                Stdio::null(),
            );
            let stacked = eventually(
                || {
                    let stacked = stack(&display, xauthority).ok()?;
                    stacked
                        .iter()
                        .any(|window| window["occluderOnly"] == true)
                        .then_some(stacked)
                },
                "the override-redirect popup in the occluder stack",
            );
            let owner = stacked
                .iter()
                .position(|window| window["id"] == id)
                .unwrap();
            assert!(stacked[owner + 1..].iter().any(|window| {
                window["occluderOnly"] == true
                    && super::super::window_covers(
                        window,
                        250.0 / WIDTH as f64,
                        200.0 / HEIGHT as f64,
                    )
            }));
        }

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
        let top = |display: &str| {
            stack(display, xauthority)
                .ok()?
                .iter()
                .rev()
                .find(|window| window["occluderOnly"] != true)
                .and_then(|window| window["title"].as_str().map(str::to_owned))
        };
        eventually(
            || (top(&display)? == "GlassClose").then_some(()),
            "the newest window on top of the stack",
        );
        run(&display, xauthority, "s", Request::Focus(&id)).unwrap();
        eventually(
            || (top(&display)? == "GlassProbe").then_some(()),
            "the focused window on top of the stack",
        );
        let closed = run(&display, xauthority, "s", Request::Close(&message)).unwrap();
        assert_eq!(closed["closed"], true);
        eventually(
            || find("GlassClose").is_none().then_some(()),
            "the xmessage window to close",
        );
        let _ = std::fs::remove_file(&log);
    }
}
