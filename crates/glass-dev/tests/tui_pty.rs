#![cfg(unix)]

//! End-to-end PTY interaction tests for the Glass Dev TUI.
//!
//! Opt-in like the browser smoke test: run with `GLASS_E2E=1`. These tests
//! drive the real release binary in a pseudo-terminal at real sizes and
//! assert on rendered output, closing the gap between unit-render tests
//! and interactive behavior (key handling in every mode, menus, composer,
//! action menus, recovery, and quit conventions).

use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::fd::FromRawFd;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

struct Session {
    child: Child,
    master: std::fs::File,
    buffer: Vec<u8>,
}

impl Session {
    fn start(binary: &str, root: &std::path::Path, cols: u16, rows: u16) -> Self {
        let mut master_fd: libc::c_int = 0;
        let mut slave_fd: libc::c_int = 0;
        let window = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        #[cfg(target_os = "macos")]
        let window_ptr: *mut libc::winsize = std::ptr::addr_of!(window).cast_mut();
        #[cfg(not(target_os = "macos"))]
        let window_ptr: *const libc::winsize = std::ptr::addr_of!(window);

        let result = unsafe {
            libc::openpty(
                &mut master_fd,
                &mut slave_fd,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                window_ptr,
            )
        };
        assert_eq!(result, 0, "openpty failed");
        let child = Command::new(binary)
            .current_dir(root)
            .stdin(unsafe { Stdio::from(std::fs::File::from_raw_fd(slave_fd)) })
            .stdout(unsafe { Stdio::from(std::fs::File::from_raw_fd(libc::dup(slave_fd))) })
            .stderr(unsafe { Stdio::from(std::fs::File::from_raw_fd(libc::dup(slave_fd))) })
            .spawn()
            .expect("spawn glass");
        let master = unsafe { std::fs::File::from_raw_fd(master_fd) };
        let mut session = Self {
            child,
            master,
            buffer: Vec::new(),
        };
        // CI and cold release binaries can spend a few seconds discovering
        // the workspace before the first frame; do not send Ctrl+C into a
        // process that has not entered its event loop yet.
        session.settle(Duration::from_millis(4000));
        session
    }

    /// Poll output until `probe` appears or the deadline passes.
    fn wait_for(&mut self, probe: &str, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            self.settle(Duration::from_millis(200));
            if self.output().contains(probe) {
                return true;
            }
        }
        false
    }

    fn wait_for_visible(&mut self, probe: &str, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            self.settle(Duration::from_millis(200));
            if self.visible_output().contains(probe) {
                return true;
            }
        }
        false
    }

    fn visible_output(&self) -> String {
        let raw = String::from_utf8_lossy(&self.buffer);
        let mut visible = String::with_capacity(raw.len());
        let mut characters = raw.chars();
        while let Some(character) = characters.next() {
            if character != '\x1b' {
                if character == '\r' {
                    visible.push('\n');
                } else if !character.is_control() {
                    visible.push(character);
                }
                continue;
            }
            let Some(next) = characters.next() else {
                break;
            };
            if next == '[' {
                for sequence_character in characters.by_ref() {
                    if ('@'..='~').contains(&sequence_character) {
                        break;
                    }
                }
            } else if next == ']' {
                for sequence_character in characters.by_ref() {
                    if sequence_character == '\x07' {
                        break;
                    }
                }
            }
        }
        visible
    }

    fn wait_for_exit(&mut self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.child.try_wait().unwrap_or(None).is_some() {
                return true;
            }
            self.settle(Duration::from_millis(50));
            std::thread::sleep(Duration::from_millis(10));
        }
        self.child.try_wait().unwrap_or(None).is_some()
    }

    fn set_window(&mut self, cols: u16, rows: u16) {
        unsafe {
            libc::ioctl(
                self.master.as_raw_fd(),
                libc::TIOCSWINSZ,
                &libc::winsize {
                    ws_row: rows,
                    ws_col: cols,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                },
            );
        }
    }

    fn send(&mut self, keys: &[u8]) {
        self.master.write_all(keys).expect("write keys");
        self.master.flush().expect("flush");
        // Let crossterm finish an isolated Escape before the next test key.
        // Otherwise a fast Escape + printable byte can be parsed as one
        // sequence and exercise the wrong input mode.
        if keys.contains(&0x1b) {
            self.settle(Duration::from_millis(150));
        }
    }

    fn settle(&mut self, wait: Duration) {
        let deadline = Instant::now() + wait;
        let mut chunk = [0u8; 8192];
        while Instant::now() < deadline {
            let timeout = libc::timeval {
                tv_sec: 0,
                tv_usec: 50_000,
            };
            let mut polls = [libc::pollfd {
                fd: self.master.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            }];
            let ready = unsafe { libc::poll(polls.as_mut_ptr(), 1, 50) };
            if ready > 0 {
                match self.master.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => self.buffer.extend_from_slice(&chunk[..n]),
                    Err(_) => break,
                }
            }
            let _ = timeout;
        }
    }

    fn output(&self) -> String {
        String::from_utf8_lossy(&self.buffer).into_owned()
    }

    fn output_tail(&self) -> String {
        let output = self.output();
        let visible = output
            .chars()
            .map(|character| {
                if character == '\n' || character == '\r' || !character.is_control() {
                    character
                } else {
                    ' '
                }
            })
            .collect::<String>();
        visible
            .chars()
            .rev()
            .take(3000)
            .collect::<String>()
            .chars()
            .rev()
            .collect()
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.kill();
    }
}

fn workspace_root() -> std::path::PathBuf {
    let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
    let root =
        std::env::temp_dir().join(format!("glass-e2e-pty-{}-{sequence}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='e2e'\nversion='0.1.0'\n",
    )
    .unwrap();
    root
}

fn git_workspace_root() -> std::path::PathBuf {
    let root = workspace_root();
    for (args, message) in [
        (vec!["init", "-q"], "git init"),
        (
            vec!["config", "user.email", "glass@example.test"],
            "git email",
        ),
        (vec!["config", "user.name", "Glass E2E"], "git name"),
    ] {
        let status = Command::new("git")
            .current_dir(&root)
            .args(args)
            .status()
            .unwrap_or_else(|error| panic!("{message} failed to start: {error}"));
        assert!(status.success(), "{message} failed with {status}");
    }
    std::fs::write(root.join("a-first.txt"), "before\n").unwrap();
    std::fs::write(root.join("b-second.txt"), "before\n").unwrap();
    let status = Command::new("git")
        .current_dir(&root)
        .args(["add", "."])
        .status()
        .expect("git add failed to start");
    assert!(status.success(), "git add failed with {status}");
    let status = Command::new("git")
        .current_dir(&root)
        .args(["commit", "-qm", "initial"])
        .status()
        .expect("git commit failed to start");
    assert!(status.success(), "git commit failed with {status}");
    std::fs::write(root.join("a-first.txt"), "before\na-first change\n").unwrap();
    std::fs::write(root.join("b-second.txt"), "before\nb-second change\n").unwrap();
    root
}

fn binary() -> String {
    std::env::var("GLASS_E2E_BINARY").unwrap_or_else(|_| {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/release/glass")
            .display()
            .to_string()
    })
}

#[test]
fn ctrl_c_requires_confirmation_from_every_input_mode() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let bin = binary();

    // Plain navigation mode.
    let mut session = Session::start(&bin, &root, 100, 28);
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "navigation frame never rendered"
    );
    session.send(b"\x03");
    assert!(
        session.wait_for("QUIT?", Duration::from_secs(5)),
        "Ctrl+C must open quit confirmation from navigation mode"
    );
    assert!(
        !session.wait_for_exit(Duration::from_millis(300)),
        "Ctrl+C must not exit before confirmation"
    );
    session.send(b"\x1b");
    assert!(
        session.wait_for("Quit dismissed", Duration::from_secs(3)),
        "Escape must dismiss quit confirmation"
    );
    session.send(b"\x03");
    assert!(
        session.wait_for("QUIT?", Duration::from_secs(3)),
        "Ctrl+C must reopen quit confirmation"
    );
    session.send(b"y");
    assert!(
        session.wait_for_exit(Duration::from_secs(5)),
        "confirmed quit must exit navigation mode; output bytes={}",
        session.buffer.len()
    );
    session.kill();

    // Composer mode: Ctrl+C must ask before quitting, not insert a literal 'c'.
    let mut session = Session::start(&bin, &root, 100, 28);
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "composer navigation frame never rendered"
    );
    session.send(b"T");
    assert!(
        session.wait_for("CONVERSATION", Duration::from_secs(5)),
        "agent surface never rendered after trust"
    );
    assert!(
        session.wait_for("✓ Ready", Duration::from_secs(5)),
        "Pi runtime never became ready"
    );
    session.send(b"i");
    assert!(
        session.wait_for("▌", Duration::from_secs(5)),
        "composer never opened"
    );
    session.send(b"\x03");
    assert!(
        session.wait_for("QUIT?", Duration::from_secs(5)),
        "Ctrl+C must open quit confirmation from composer mode"
    );
    assert!(
        !session.wait_for_exit(Duration::from_millis(300)),
        "Ctrl+C must not exit composer mode before confirmation"
    );
    session.send(b"y");
    assert!(
        session.wait_for_exit(Duration::from_secs(5)),
        "confirmed quit must exit composer mode; output bytes={}",
        session.buffer.len()
    );
    session.kill();
}

#[test]
fn action_menu_opens_and_renders_entries() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let mut session = Session::start(&binary(), &root, 118, 32);
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "first frame never rendered"
    );
    session.send(b"a");
    assert!(
        session.wait_for("Compose message", Duration::from_secs(5)),
        "action menu entries missing"
    );
    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));
    session.kill();
}

#[test]
fn app_target_picker_failure_keeps_tui_recoverable() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let mut session = Session::start(&binary(), &root, 118, 32);
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "first frame never rendered"
    );
    session.send(b"T");
    assert!(
        session.wait_for("CONVERSATION", Duration::from_secs(5)),
        "Agent surface never rendered after trust"
    );
    session.send(b"3");
    assert!(
        session.wait_for("VISUAL PLANE", Duration::from_secs(5)),
        "App surface never rendered"
    );
    session.send(b"T");
    assert!(
        session.wait_for("RECOVERY", Duration::from_secs(8)),
        "target picker failure did not expose browser recovery\n{}",
        session.output_tail()
    );
    session.send(b"\x1b");
    assert!(
        session.wait_for("VISUAL PLANE", Duration::from_secs(5)),
        "recovery dismissal did not return to the App surface"
    );
    session.kill();
}

#[test]
fn agent_enter_and_typing_open_composer_without_cli_setup() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let mut session = Session::start(&binary(), &root, 118, 32);
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "first frame never rendered"
    );
    session.send(b"T");
    assert!(
        session.wait_for("CONVERSATION", Duration::from_secs(5)),
        "Agent surface never rendered after trust"
    );
    assert!(
        session.wait_for("✓ Ready", Duration::from_secs(5)),
        "Pi runtime never became ready\n{}",
        session.output_tail()
    );

    session.send(b"c");
    assert!(
        session.wait_for("CONVERSATION", Duration::from_secs(5)),
        "typing on Agent must open the composer instead of switching surfaces\n{}",
        session.output_tail()
    );
    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));

    session.send(b"\r");
    assert!(
        session.wait_for("CONVERSATION", Duration::from_secs(5)),
        "Enter on Agent must open the composer\n{}",
        session.output_tail()
    );
    session.kill();
}

#[test]
fn agent_update_route_requires_confirmation_in_tui() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let mut session = Session::start(&binary(), &root, 118, 32);
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "first frame never rendered"
    );
    session.send(b":agent update\r");
    assert!(
        session.wait_for("CONFIRMATION", Duration::from_secs(5)),
        "Pi update route did not open confirmation\n{}",
        session.output_tail()
    );
    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));
    session.kill();
}

#[test]
fn external_harness_routes_stay_inside_tui_until_confirmed() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let mut session = Session::start(&binary(), &root, 118, 32);
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "first frame never rendered"
    );
    session.send(b"T");
    assert!(
        session.wait_for("Workspace opened with", Duration::from_secs(5)),
        "workspace trust did not complete\n{}",
        session.output_tail()
    );
    assert!(
        session.wait_for("CONVERSATION", Duration::from_secs(5)),
        "Agent surface never rendered after trust\n{}",
        session.output_tail()
    );
    session.send(b":harness list\r");
    assert!(
        session.wait_for("External harnesses", Duration::from_secs(5)),
        "harness catalog did not render in TUI\n{}",
        session.output_tail()
    );
    session.send(
        b":harness delegate codex inspect current diff --sandbox read-only --timeout-secs 5\r",
    );
    assert!(
        session.wait_for("CONFIRMATION", Duration::from_secs(5)),
        "external delegate did not stop at Glass confirmation\n{}",
        session.output_tail()
    );
    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));
    session.kill();
}
#[test]
fn palette_and_help_survive_real_key_sequences() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let mut session = Session::start(&binary(), &root, 118, 32);
    session.send(b":");
    assert!(
        session.wait_for_visible("SELECT AN ACTION", Duration::from_secs(5)),
        "command palette did not open\n{}",
        session.output_tail()
    );
    assert!(
        session.wait_for_visible("Compose", Duration::from_secs(5)),
        "surface actions missing\n{}",
        session.output_tail()
    );
    for _ in 0..2 {
        session.send(b"\x1b[B");
    }
    session.send(b"\r");
    assert!(
        session.wait_for_visible("CONFIRMATION", Duration::from_secs(5)),
        "arrow selection did not route to the selected action\n{}",
        session.output_tail()
    );
    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));
    session.send(b":");
    assert!(
        session.wait_for_visible("SELECT AN ACTION", Duration::from_secs(5)),
        "palette did not reopen after cancelling confirmation"
    );
    session.send(b"help");
    assert!(
        session.wait_for_visible("Filter:", Duration::from_secs(5)),
        "palette filtering missing\n{}",
        session.output_tail()
    );
    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));
    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));
    // Surfaces switch with number keys and each renders its header.
    for (key, header) in [('2', "FILES"), ('3', "VISUAL PLANE"), ('4', "Task")] {
        session.send(&[key as u8]);
        assert!(
            session.wait_for(header, Duration::from_secs(5)),
            "{header} missing after {key}"
        );
    }
    session.kill();
}

#[test]
fn phone_size_keeps_primary_surfaces_reachable() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let mut session = Session::start(&binary(), &root, 48, 18);
    session.set_window(48, 18);
    assert!(
        session.wait_for("Agent", Duration::from_secs(5)),
        "phone layout must keep Agent reachable"
    );
    session.send(b":");
    assert!(
        session.wait_for_visible("SELECT AN ACTION", Duration::from_secs(5)),
        "phone palette did not open\n{}",
        session.output_tail()
    );
    session.send(b"\x1b[B");
    assert!(
        session.wait_for_visible("Setup Pi", Duration::from_secs(5)),
        "phone palette did not expose guided actions\n{}",
        session.output_tail()
    );
    session.send(b"\x1b");
    session.kill();
}

#[test]
fn desktop_surface_tabs_render_every_workbench() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let mut session = Session::start(&binary(), &root, 118, 32);
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "first frame never rendered"
    );
    session.send(b"T");
    assert!(
        session.wait_for("CONVERSATION", Duration::from_secs(5)),
        "Agent surface never rendered after trust"
    );
    for (key, marker) in [
        (b'2', "FILES"),
        (b'3', "VISUAL PLANE"),
        (b'4', "TERMINAL"),
        (b'5', "SUMMARY"),
        (b'6', "GIT"),
        (b'7', "TESTS"),
        (b'8', "ROUTES"),
    ] {
        session.send(&[key]);
        assert!(
            session.wait_for(marker, Duration::from_secs(5)),
            "{marker} missing after desktop surface key {}\n{}",
            key as char,
            session.output_tail()
        );
    }
    // Tab from the overflow surface wraps back to the first primary surface.
    session.send(b"\t");
    assert!(
        session.wait_for("CONVERSATION", Duration::from_secs(5)),
        "Tab did not wrap from More to Agent"
    );
    session.kill();
}

#[test]
fn phone_surface_tabs_keep_the_compact_workbench_reachable() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let mut session = Session::start(&binary(), &root, 48, 18);
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "first phone frame never rendered"
    );
    session.send(b"T");
    assert!(
        session.wait_for("CONVERSATION", Duration::from_secs(5)),
        "Agent surface never rendered after phone trust"
    );
    for (key, marker) in [
        (b'2', "FILES"),
        (b'3', "VISUAL PLANE"),
        (b'4', "SUMMARY"),
        (b'5', "ROUTES"),
    ] {
        session.send(&[key]);
        assert!(
            session.wait_for(marker, Duration::from_secs(5)),
            "{marker} missing after phone surface key {}\n{}",
            key as char,
            session.output_tail()
        );
    }
    session.send(b"\t");
    assert!(
        session.wait_for("CONVERSATION", Duration::from_secs(5)),
        "Tab did not wrap from phone More to Agent"
    );
    session.kill();
}

#[test]
fn arrow_keys_cycle_surfaces_without_numeric_shortcuts() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let mut session = Session::start(&binary(), &root, 118, 32);
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "first frame never rendered"
    );
    session.settle(Duration::from_millis(600));
    session.send(b"T");
    assert!(
        session.wait_for("CONVERSATION", Duration::from_secs(5)),
        "Agent surface never rendered after trust"
    );

    session.send(b"\x1b[C");
    assert!(
        session.wait_for("FILES", Duration::from_secs(5)),
        "Right arrow did not move to Code\n{}",
        session.output_tail()
    );
    session.send(b"\x1b[C");
    assert!(
        session.wait_for("VISUAL PLANE", Duration::from_secs(5)),
        "Right arrow did not move to App\n{}",
        session.output_tail()
    );
    session.send(b"\x1b[D");
    assert!(
        session.wait_for("FILES", Duration::from_secs(5)),
        "Left arrow did not return to Code\n{}",
        session.output_tail()
    );
    session.kill();
}

#[test]
fn compact_surface_tabs_keep_primary_workbenches_reachable() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let mut session = Session::start(&binary(), &root, 72, 24);
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "first compact frame never rendered"
    );
    session.send(b"T");
    assert!(
        session.wait_for("Workspace opened with", Duration::from_secs(5)),
        "workspace trust did not complete\n{}",
        session.output_tail()
    );
    for (key, marker) in [
        (b'2', "FILES"),
        (b'3', "VISUAL PLANE"),
        (b'4', "TERMINAL"),
        (b'5', "SUMMARY"),
        (b'6', "GIT"),
        (b'7', "TESTS"),
    ] {
        session.send(&[key]);
        assert!(
            session.wait_for(marker, Duration::from_secs(5)),
            "{marker} missing after compact surface key {}\n{}",
            key as char,
            session.output_tail()
        );
    }
    session.send(b"\t");
    assert!(
        session.wait_for("CONVERSATION", Duration::from_secs(5)),
        "Tab did not wrap from compact Debug to Agent"
    );
    session.kill();
}

#[test]
fn launcher_routes_and_surface_actions_remain_interactive() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let mut session = Session::start(&binary(), &root, 118, 32);
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "first frame never rendered"
    );
    session.send(b"T");
    assert!(
        session.wait_for("Workspace opened with", Duration::from_secs(5)),
        "workspace trust did not complete\n{}",
        session.output_tail()
    );
    assert!(
        session.wait_for("CONVERSATION", Duration::from_secs(5)),
        "Agent surface never rendered after trust"
    );

    session.send(b"a");
    assert!(
        session.wait_for("Compose message", Duration::from_secs(5)),
        "Agent launcher entries missing"
    );
    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));

    session.send(b":");
    assert!(
        session.wait_for("Command search", Duration::from_secs(5)),
        "command center did not open\n{}",
        session.output_tail()
    );
    session.send(b"agent setup");
    assert!(
        session.wait_for("agent setup", Duration::from_secs(5)),
        "command center route search did not render\n{}",
        session.output_tail()
    );
    session.send(b"\t");
    session.settle(Duration::from_millis(300));
    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));

    for (key, marker) in [
        (b'2', "FILES"),
        (b'3', "VISUAL PLANE"),
        (b'4', "TERMINAL"),
        (b'5', "SUMMARY"),
        (b'6', "GIT"),
        (b'7', "TESTS"),
    ] {
        session.send(&[key]);
        assert!(
            session.wait_for(marker, Duration::from_secs(5)),
            "{marker} missing before action test\n{}",
            session.output_tail()
        );
        session.send(b"a");
        assert!(
            session.wait_for("DETAILS", Duration::from_secs(5)),
            "launcher menu did not open on surface {}\n{}",
            key as char,
            session.output_tail()
        );
        session.send(b"\x1b");
        session.settle(Duration::from_millis(300));
    }
    session.kill();
}

#[test]
fn agent_browser_code_terminal_tasks_git_and_debug_paths_respond() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let mut session = Session::start(&binary(), &root, 118, 32);
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "first frame never rendered"
    );
    session.send(b"T");
    assert!(
        session.wait_for("Workspace opened with", Duration::from_secs(5)),
        "workspace trust did not complete\n{}",
        session.output_tail()
    );
    assert!(
        session.wait_for("CONVERSATION", Duration::from_secs(5)),
        "Agent surface never rendered after trust"
    );
    assert!(
        session.wait_for("✓ Ready", Duration::from_secs(5)),
        "Pi runtime never became ready\n{}",
        session.output_tail()
    );

    session.send(b"i");
    assert!(
        session.wait_for("▌", Duration::from_secs(5)),
        "Agent composer did not open\n{}",
        session.output_tail()
    );
    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));

    session.send(b"2");
    assert!(
        session.wait_for("FILES", Duration::from_secs(5)),
        "Code surface did not render\n{}",
        session.output_tail()
    );
    session.send(b"i");
    assert!(
        session.wait_for("REVIEW", Duration::from_secs(5)),
        "Code collaboration panel did not render\n{}",
        session.output_tail()
    );
    assert!(
        session.wait_for("EDITOR", Duration::from_secs(1)),
        "Code editor did not open\n{}",
        session.output_tail()
    );
    session.send(b"\x1ba");
    assert!(
        session.wait_for("Do not edit files", Duration::from_secs(5)),
        "Alt-A did not hand the focused editor context to the agent\n{}",
        session.output_tail()
    );
    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));

    session.send(b"3");
    assert!(
        session.wait_for("VISUAL PLANE", Duration::from_secs(5)),
        "Browser surface did not render\n{}",
        session.output_tail()
    );
    session.send(b"v");
    assert!(
        session.wait_for("Semantic inspection", Duration::from_secs(5))
            || session.wait_for("Live view", Duration::from_secs(1))
            || session.wait_for("live pixels are disabled", Duration::from_secs(1)),
        "browser visual toggle did not report a result\n{}",
        session.output_tail()
    );

    session.send(b"4");
    assert!(
        session.wait_for("TERMINAL", Duration::from_secs(5)),
        "Terminal surface did not render\n{}",
        session.output_tail()
    );
    session.send(b"s");
    assert!(
        session.wait_for("No deve", Duration::from_secs(5))
            || session.wait_for("Development suite", Duration::from_secs(1)),
        "terminal development-suite action did not report a result\n{}",
        session.output_tail()
    );
    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));

    session.send(b"5");
    assert!(
        session.wait_for("SUMMARY", Duration::from_secs(5)),
        "Tasks surface did not render\n{}",
        session.output_tail()
    );
    session.send(b"a");
    assert!(
        session.wait_for("DETAILS", Duration::from_secs(5)),
        "task launcher did not open\n{}",
        session.output_tail()
    );
    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));

    session.send(b"6");
    assert!(
        session.wait_for("GIT", Duration::from_secs(5)),
        "Git surface did not render\n{}",
        session.output_tail()
    );
    session.send(b"d");
    assert!(
        session.wait_for("DIFF", Duration::from_secs(5)),
        "Git diff action did not report a result\n{}",
        session.output_tail()
    );
    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));

    session.send(b"7");
    assert!(
        session.wait_for("TESTS", Duration::from_secs(5)),
        "Debug surface did not render\n{}",
        session.output_tail()
    );
    session.send(b"a");
    assert!(
        session.wait_for("DETAILS", Duration::from_secs(5)),
        "debug launcher did not open\n{}",
        session.output_tail()
    );
    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));
    session.kill();
}

#[test]
fn git_file_selection_opens_the_focused_diff_in_place() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = git_workspace_root();
    let mut session = Session::start(&binary(), &root, 118, 32);
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "first frame never rendered"
    );
    session.send(b"T");
    assert!(
        session.wait_for_visible("CONVERSATION", Duration::from_secs(5)),
        "Agent surface never rendered after trust\n{}",
        session.output_tail()
    );
    assert!(
        session.wait_for_visible("✓ Ready", Duration::from_secs(5)),
        "Pi runtime never became ready\n{}",
        session.output_tail()
    );

    session.send(b"6");
    assert!(
        session.wait_for_visible("a-first.txt", Duration::from_secs(8)),
        "Git file list never rendered\n{}",
        session.output_tail()
    );
    session.send(b"\x1b[B");
    session.send(b"\r");
    assert!(
        session.wait_for_visible("b-second change", Duration::from_secs(8)),
        "Enter did not load the focused second-file diff\n{}",
        session.output_tail()
    );

    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));
    session.send(b"\x1b[A");
    session.send(b"\r");
    assert!(
        session.wait_for_visible("a-first change", Duration::from_secs(8)),
        "Up and Enter did not load the first-file diff\n{}",
        session.output_tail()
    );
    session.kill();
}
