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
use std::os::unix::fs::PermissionsExt;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

struct Session {
    child: Child,
    master: std::fs::File,
    buffer: Vec<u8>,
}

struct FixtureChildGuard {
    pid_path: std::path::PathBuf,
    armed: bool,
}

impl FixtureChildGuard {
    fn new(pid_path: std::path::PathBuf) -> Self {
        Self {
            pid_path,
            armed: true,
        }
    }

    fn terminate(&mut self) {
        if !self.armed {
            return;
        }
        self.armed = false;
        let Ok(pid) = std::fs::read_to_string(&self.pid_path) else {
            return;
        };
        let Ok(pid) = pid.trim().parse::<i32>() else {
            return;
        };
        // SAFETY: this PID is written by the scoped fake runtime launched for
        // this test; the guard exists only inside the Unix PTY test process.
        let _ = unsafe { libc::kill(pid, libc::SIGKILL) };
    }
}

impl Drop for FixtureChildGuard {
    fn drop(&mut self) {
        self.terminate();
    }
}

impl Session {
    fn start(binary: &str, root: &std::path::Path, cols: u16, rows: u16) -> Self {
        Self::start_with_options(binary, root, cols, rows, "glass-core", None)
    }

    fn start_with_core_binary(
        binary: &str,
        root: &std::path::Path,
        cols: u16,
        rows: u16,
        core_binary: Option<&std::path::Path>,
    ) -> Self {
        Self::start_with_options(binary, root, cols, rows, "glass-core", core_binary)
    }

    fn start_with_options(
        binary: &str,
        root: &std::path::Path,
        cols: u16,
        rows: u16,
        harness: &str,
        core_binary: Option<&std::path::Path>,
    ) -> Self {
        Self::start_with_data_home_options(binary, root, cols, rows, harness, core_binary, None)
    }

    fn start_with_data_home_options(
        binary: &str,
        root: &std::path::Path,
        cols: u16,
        rows: u16,
        harness: &str,
        core_binary: Option<&std::path::Path>,
        data_home: Option<&std::path::Path>,
    ) -> Self {
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
        let mut command = Command::new(binary);
        command
            .current_dir(root)
            .env("GLASS_AGENT_HARNESS", harness)
            .env("GLASS_TRUST_STORE_PATH", root.join(".glass/pty-trust.json"));
        if let Some(data_home) = data_home {
            command.env("XDG_DATA_HOME", data_home);
        }
        if let Some(core_binary) = core_binary {
            command.env("GLASS_CORE_BINARY", core_binary);
        }
        let child = command
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
            if self.visible_output().contains(probe) {
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

    fn wait_for_raw(&mut self, probe: &str, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            self.settle(Duration::from_millis(200));
            if self.output().contains(probe) {
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
            self.settle(Duration::from_millis(300));
        }
    }

    fn send_slowly(&mut self, keys: &[u8], interval: Duration) {
        for key in keys {
            self.send(std::slice::from_ref(key));
            std::thread::sleep(interval);
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

    fn clear_output(&mut self) {
        self.buffer.clear();
    }

    fn trust_project(&mut self) {
        self.trust_project_with_probe("trusted project · guarded");
    }

    fn trust_project_with_probe(&mut self, probe: &str) {
        if !self.wait_for_visible("I inspect", Duration::from_millis(300)) {
            self.send(b":trust inspect\r");
            assert!(
                self.wait_for_visible("I inspect", Duration::from_secs(5)),
                "trust inspection surface did not render\n{}",
                self.output_tail()
            );
        }
        self.send(b"T");
        assert!(
            self.wait_for_raw(probe, Duration::from_secs(5)),
            "project trust did not complete\n{}",
            self.output_tail()
        );
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

fn fake_core_binary(_root: &std::path::Path) -> std::path::PathBuf {
    let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "glass-e2e-fake-core-{}-{sequence}",
        std::process::id()
    ));
    std::fs::create_dir_all(path.parent().expect("fake binary parent")).unwrap();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fake_glass_core_runtime.py");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then printf 'glass-core 0.1.0\\n'; exit 0; fi\nexec python3 '{}' \"$@\"\n",
            fixture.display()
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).unwrap();
    path
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

fn seed_attention_proposal(root: &std::path::Path, data_home: &std::path::Path) {
    use sha2::Digest;

    let root = std::fs::canonicalize(root).expect("canonicalize attention workspace");
    let digest = sha2::Sha256::digest(root.to_string_lossy().as_bytes());
    let project_id = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let timeline_path = data_home
        .join("glass/development")
        .join(project_id)
        .join("timeline.jsonl");
    let mut timeline = glass_dev::development::Timeline::open(timeline_path)
        .expect("open isolated attention timeline");
    let actor = glass_dev::development::Actor::external("pty-fixture");
    timeline
        .record(
            actor.clone(),
            glass_dev::development::DevelopmentEventKind::EditorProposalCreated,
            root.to_string_lossy(),
            serde_json::json!({
                "id": "proposal-pty-inbox",
                "path": "a-first.txt",
                "summary": "inbox-review",
                "actor": actor,
                "baseHash": "fixture-base",
                "baseRevision": 1,
                "original": "before\na-first change\n",
                "proposed": "before\nproposal change\n",
                "state": "pending",
                "createdRevision": 1,
                "updatedRevision": 1
            }),
        )
        .expect("seed attention proposal event");
}

fn binary() -> String {
    std::env::var("GLASS_E2E_BINARY").unwrap_or_else(|_| {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/release/glass")
            .display()
            .to_string()
    })
}

fn send_ctrl_shift_a(session: &mut Session) {
    // CSI-u reports the physical A key together with Ctrl+Shift. A plain
    // Ctrl+A byte loses Shift and cannot exercise this shortcut.
    session.send(b"\x1b[65;6u");
}

fn send_ctrl_shift_t(session: &mut Session) {
    // CSI-u preserves the physical T key with Ctrl+Shift modifiers.
    session.send(b"\x1b[84;6u");
}

fn paste_command(session: &mut Session, command: &str) {
    session.send(b":");
    session.settle(Duration::from_millis(100));
    session.send(b"\x1b[200~");
    session.send(command.as_bytes());
    session.send(b"\x1b[201~");
    session.send(b"\r");
}

fn select_project_thread(session: &mut Session, current: usize, target: usize) {
    send_ctrl_shift_t(session);
    assert!(session.wait_for_visible("PROJECT THREADS", Duration::from_secs(5)));
    if target < current {
        session.send(&b"\x1b[A".repeat(current - target));
    } else {
        session.send(&b"\x1b[B".repeat(target - current));
    }
    session.send(b"\r");
    assert!(session.wait_for_visible("CONVERSATION", Duration::from_secs(5)));
}

fn wait_for_saved_thread_draft(
    session: &mut Session,
    registry_path: &std::path::Path,
    role: &str,
    text: &str,
    cursor: usize,
) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(registry_path).unwrap()).unwrap();
        let saved = registry["threads"]
            .as_array()
            .and_then(|threads| threads.iter().find(|thread| thread["role"] == role))
            .is_some_and(|thread| {
                thread["composerDraft"]["text"] == text
                    && thread["composerDraft"]["cursor"] == cursor
            });
        if saved {
            return registry;
        }
        assert!(
            Instant::now() < deadline,
            "{role} draft/cursor was not persisted: {registry}"
        );
        session.settle(Duration::from_millis(100));
    }
}

fn wait_for_saved_thread_draft_id(
    session: &mut Session,
    registry_path: &std::path::Path,
    thread_id: &str,
    text: &str,
    cursor: usize,
) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(registry_path).unwrap()).unwrap();
        let saved = registry["threads"].as_array().is_some_and(|threads| {
            threads.iter().any(|thread| {
                thread["threadId"] == thread_id
                    && thread["composerDraft"]["text"] == text
                    && thread["composerDraft"]["cursor"] == cursor
            })
        });
        if saved {
            return registry;
        }
        assert!(
            Instant::now() < deadline,
            "thread {thread_id} draft/cursor was not persisted: {registry}"
        );
        session.settle(Duration::from_millis(100));
    }
}

fn verify_selected_thread_draft_cursor(
    session: &mut Session,
    registry_path: &std::path::Path,
    role: &str,
    text: &str,
    cursor: usize,
) {
    let mut inserted = text.to_owned();
    inserted.insert(cursor, 'X');
    session.send(b"X");
    wait_for_saved_thread_draft(session, registry_path, role, &inserted, cursor + 1);
    session.send(b"\x7f");
    wait_for_saved_thread_draft(session, registry_path, role, text, cursor);
}

fn seed_project_threads(root: &std::path::Path) {
    use std::os::unix::fs::MetadataExt;

    let canonical_root = std::fs::canonicalize(root).unwrap();
    let metadata = std::fs::metadata(&canonical_root).unwrap();
    let threads = ["First thread", "Second thread", "Third thread"]
        .into_iter()
        .enumerate()
        .map(|(index, title)| {
            serde_json::json!({
                "threadId": format!("thread-{:032x}", index + 1),
                "title": title,
                "role": title,
                "createdAtMs": 1,
                "runs": [{
                    "runId": format!("run-{:032x}", index + 1),
                    "engine": "glass-core",
                    "coreSessionId": null,
                    "classification": "completed"
                }],
                "sendAttempts": [],
                "composerDraft": null,
                "followUpQueue": null
            })
        })
        .collect::<Vec<_>>();
    let registry = serde_json::json!({
        "version": 1,
        "projectIdentity": {
            "canonicalRoot": canonical_root,
            "device": metadata.dev(),
            "inode": metadata.ino()
        },
        "threads": threads
    });
    let directory = root.join(".glass/threads");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("registry.json"),
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();
}

fn seed_uncertain_send_attempt(root: &std::path::Path, text: &str) {
    use std::os::unix::fs::MetadataExt;

    let canonical_root = std::fs::canonicalize(root).unwrap();
    let metadata = std::fs::metadata(&canonical_root).unwrap();
    let directory = root.join(".glass/threads");
    std::fs::create_dir_all(&directory).unwrap();
    let registry = serde_json::json!({
        "version": 1,
        "projectIdentity": {
            "canonicalRoot": canonical_root,
            "device": metadata.dev(),
            "inode": metadata.ino()
        },
        "threads": [{
            "threadId": "thread-00000000000000000000000000000001",
            "title": "Uncertain delivery",
            "role": "assistant",
            "createdAtMs": 1,
            "runs": [{
                "runId": "run-00000000000000000000000000000001",
                "engine": "glass-core",
                "coreSessionId": "fixture-session",
                "classification": "working"
            }],
            "sendAttempts": [{
                "attemptId": "attempt-00000000000000000000000000000001",
                "clientMessageId": "client-e2e-uncertain",
                "targetRunId": "run-00000000000000000000000000000001",
                "mode": "prompt",
                "text": text,
                "status": "working",
                "engineRequestId": "fixture-request-1",
                "createdAtMs": 2
            }],
            "composerDraft": null,
            "followUpQueue": null
        }]
    });
    std::fs::write(
        directory.join("registry.json"),
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();
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
    session.clear_output();
    session.send(b"\x1b");
    session.send(b"y");
    assert!(
        !session.wait_for_exit(Duration::from_millis(300)),
        "Escape must dismiss quit confirmation before a later key is processed\n{}",
        session.output_tail()
    );
    session.clear_output();
    session.send(b"\x03");
    assert!(
        session.wait_for_raw("QUIT?", Duration::from_secs(3)),
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
    let fake_core = fake_core_binary(&root);
    let mut session = Session::start_with_core_binary(&bin, &root, 100, 28, Some(&fake_core));
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "composer navigation frame never rendered"
    );
    session.trust_project();
    assert!(
        session.wait_for_raw("trusted project · guarded", Duration::from_secs(5)),
        "agent surface never rendered after trust"
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
    let _ = std::fs::remove_file(fake_core);
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
    session.send(b"4");
    assert!(
        session.wait_for("TERMINAL", Duration::from_secs(5)),
        "terminal surface never rendered"
    );
    session.send(b"a");
    assert!(
        session.wait_for("Start detected suite", Duration::from_secs(5)),
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
    session.trust_project();
    assert!(
        session.wait_for("trusted project · guarded", Duration::from_secs(5)),
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
fn agent_enter_and_typing_open_composer_with_offline_core_fixture() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let fake_core = fake_core_binary(&root);
    let mut session = Session::start_with_core_binary(&binary(), &root, 118, 32, Some(&fake_core));
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "first frame never rendered"
    );
    session.trust_project();
    assert!(
        session.wait_for("CONVERSATION", Duration::from_secs(5)),
        "Agent surface never rendered after trust"
    );
    assert!(
        session.wait_for("✓ Ready", Duration::from_secs(5)),
        "offline Glass Core fixture never became ready\n{}",
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
    let _ = std::fs::remove_file(fake_core);
}

#[test]
fn spawned_resident_agent_dispatches_an_offline_prompt_from_the_composer() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let fixture_state = root.join(".glass");
    std::fs::create_dir_all(&fixture_state).unwrap();
    std::fs::write(fixture_state.join("e2e-core-request-log"), "enabled").unwrap();
    let fake_core = fake_core_binary(&root);
    let mut session = Session::start_with_core_binary(&binary(), &root, 80, 24, Some(&fake_core));
    assert!(session.wait_for("GLASS", Duration::from_secs(8)));
    session.trust_project();
    assert!(session.wait_for("✓ Ready", Duration::from_secs(8)));

    session.send(b":agent spawn assistant offline task\r");
    assert!(
        session.wait_for("Spawned agent-0001", Duration::from_secs(5)),
        "agent command did not create the resident session\n{}",
        session.output_tail()
    );
    send_ctrl_shift_t(&mut session);
    assert!(session.wait_for("PROJECT THREADS", Duration::from_secs(5)));
    assert!(
        session.wait_for("idle", Duration::from_secs(12)),
        "spawned agent never became ready\n{}",
        session.output_tail()
    );
    session.send(b"\r");
    session.send(b"\r");
    assert!(session.wait_for("CONVERSATION", Duration::from_secs(5)));
    session.send(b"offline prompt round trip");
    session.send(b"\r");
    assert!(
        session.wait_for(
            "Glass Agent ready · response received",
            Duration::from_secs(12)
        ),
        "offline Core did not return to its ready state after the prompt\n{}",
        session.output_tail()
    );
    let requests = std::fs::read_to_string(fixture_state.join("e2e-core-requests.jsonl"))
        .expect("fake Core request log is present");
    assert!(
        requests.lines().any(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .is_some_and(|request| {
                    request["method"] == "turn/prompt"
                        && request["params"]["text"]
                            .as_str()
                            .is_some_and(|text| text.contains("offline prompt round trip"))
                })
        }),
        "fake Core did not receive a turn/prompt"
    );
    let registry_path = fixture_state.join("threads/registry.json");
    let deadline = Instant::now() + Duration::from_secs(8);
    let registry = loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        if registry["threads"][0]["sendAttempts"][0]["status"] == "settled"
            || Instant::now() >= deadline
        {
            break registry;
        }
        session.settle(Duration::from_millis(100));
    };
    let attempts = registry["threads"][0]["sendAttempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(
        attempts[0]["status"], "settled",
        "attempt never settled: {registry}"
    );
    assert!(
        attempts[0]["engineRequestId"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );
    assert!(
        attempts[0]["clientMessageId"]
            .as_str()
            .unwrap()
            .starts_with("chat-")
    );
    assert!(
        attempts[0]["text"]
            .as_str()
            .unwrap()
            .contains("offline prompt round trip")
    );
    session.kill();
    let _ = std::fs::remove_file(fake_core);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn three_fake_core_tasks_complete_without_stealing_selected_thread() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let fixture_state = root.join(".glass");
    std::fs::create_dir_all(&fixture_state).unwrap();
    std::fs::write(fixture_state.join("e2e-core-request-log"), "enabled").unwrap();
    std::fs::write(fixture_state.join("e2e-core-concurrency"), "enabled").unwrap();
    let fake_core = fake_core_binary(&root);
    let mut session = Session::start_with_core_binary(&binary(), &root, 100, 30, Some(&fake_core));
    assert!(session.wait_for("GLASS", Duration::from_secs(8)));
    session.trust_project();
    assert!(session.wait_for("✓ Ready", Duration::from_secs(8)));

    for (role, task, expected_id) in [
        ("alpha", "alpha task", "agent-0001"),
        ("beta", "beta task", "agent-0002"),
        ("gamma", "gamma task", "agent-0003"),
    ] {
        session.send(format!(":agent spawn {role} {task}\r").as_bytes());
        assert!(
            session.wait_for(&format!("Spawned {expected_id}"), Duration::from_secs(5)),
            "resident agent {role} was not created\n{}",
            session.output_tail()
        );
        send_ctrl_shift_t(&mut session);
        assert!(session.wait_for_visible("PROJECT THREADS", Duration::from_secs(5)));
        let deadline = Instant::now() + Duration::from_secs(12);
        while !session
            .visible_output()
            .lines()
            .any(|line| line.contains(role) && line.contains("idle"))
            && Instant::now() < deadline
        {
            session.settle(Duration::from_millis(100));
        }
        assert!(
            session
                .visible_output()
                .lines()
                .any(|line| line.contains(role) && line.contains("idle")),
            "resident agent {role} never became idle\n{}",
            session.output_tail()
        );
        session.send(b"\x1b");
    }

    for (index, tag) in ["alpha-job", "beta-job", "gamma-job"]
        .into_iter()
        .enumerate()
    {
        send_ctrl_shift_t(&mut session);
        assert!(session.wait_for("PROJECT THREADS", Duration::from_secs(5)));
        if index > 0 {
            let down = b"\x1b[B".repeat(index);
            session.send(&down);
        }
        session.send(b"\r\r");
        assert!(session.wait_for("CONVERSATION", Duration::from_secs(5)));
        session.send(tag.as_bytes());
        session.send(b"\r");
        let started = fixture_state.join(format!("e2e-core-started-{tag}"));
        let deadline = Instant::now() + Duration::from_secs(12);
        while !started.exists() && Instant::now() < deadline {
            session.settle(Duration::from_millis(50));
        }
        assert!(
            started.exists(),
            "fake Core did not start {tag}\n{}",
            session.output_tail()
        );
        let role = ["alpha", "beta", "gamma"][index];
        let draft = format!("{role} private draft");
        let cursor = draft.len() - "draft".len();
        session.send(draft.as_bytes());
        session.send(b"\x1b[D\x1b[D\x1b[D\x1b[D\x1b[D");
        wait_for_saved_thread_draft(
            &mut session,
            &fixture_state.join("threads/registry.json"),
            role,
            &draft,
            cursor,
        );
    }

    let registry_path = fixture_state.join("threads/registry.json");
    let deadline = Instant::now() + Duration::from_secs(8);
    let registry = loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        let all_working = registry["threads"].as_array().is_some_and(|threads| {
            threads.len() == 3
                && threads
                    .iter()
                    .all(|thread| thread["sendAttempts"][0]["status"] == "working")
        });
        if all_working || Instant::now() >= deadline {
            break registry;
        }
        session.settle(Duration::from_millis(100));
    };
    let threads = registry["threads"].as_array().unwrap();
    assert_eq!(
        threads.len(),
        3,
        "three spawned agents need separate threads"
    );
    let mut client_ids = std::collections::BTreeSet::new();
    for (tag, role) in [
        ("alpha-job", "alpha"),
        ("beta-job", "beta"),
        ("gamma-job", "gamma"),
    ] {
        let thread = threads
            .iter()
            .find(|thread| thread["role"] == role)
            .unwrap_or_else(|| panic!("missing {role} thread: {registry}"));
        let attempts = thread["sendAttempts"].as_array().unwrap();
        assert_eq!(attempts.len(), 1, "each thread owns one active attempt");
        assert_eq!(
            attempts[0]["status"], "working",
            "{role} should still be gated"
        );
        assert!(attempts[0]["text"].as_str().unwrap().contains(tag));
        client_ids.insert(attempts[0]["clientMessageId"].as_str().unwrap().to_owned());
        assert!(
            attempts[0]["engineRequestId"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
        );
        assert_eq!(
            attempts[0]["targetRunId"], thread["runs"][0]["runId"],
            "{role}'s attempt must target its own run"
        );
    }
    assert_eq!(
        client_ids.len(),
        3,
        "each message needs a distinct client ID"
    );

    let registry = wait_for_saved_thread_draft(
        &mut session,
        &registry_path,
        "gamma",
        "gamma private draft",
        "gamma private ".len(),
    );
    for (role, text, cursor) in [
        ("alpha", "alpha private draft", "alpha private ".len()),
        ("beta", "beta private draft", "beta private ".len()),
        ("gamma", "gamma private draft", "gamma private ".len()),
    ] {
        let thread = registry["threads"]
            .as_array()
            .unwrap()
            .iter()
            .find(|thread| thread["role"] == role)
            .unwrap();
        assert_eq!(
            thread["composerDraft"]["text"], text,
            "{role} owns its draft"
        );
        assert_eq!(
            thread["composerDraft"]["cursor"], cursor,
            "{role} owns its draft cursor"
        );
    }
    let gamma_view = session.visible_output();
    assert!(
        gamma_view.contains("gamma"),
        "gamma remains selected\n{gamma_view}"
    );

    session.buffer.clear();
    std::fs::write(fixture_state.join("e2e-core-release-alpha-job"), "release").unwrap();
    let alpha_completed = fixture_state.join("e2e-core-completed-alpha-job");
    let deadline = Instant::now() + Duration::from_secs(12);
    while !alpha_completed.exists() && Instant::now() < deadline {
        session.settle(Duration::from_millis(50));
    }
    assert!(
        alpha_completed.exists(),
        "alpha did not finish after release"
    );
    let deadline = Instant::now() + Duration::from_secs(8);
    let after_alpha_registry = loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        let alpha_settled = registry["threads"].as_array().is_some_and(|threads| {
            threads
                .iter()
                .find(|thread| thread["role"] == "alpha")
                .is_some_and(|thread| thread["sendAttempts"][0]["status"] == "settled")
        });
        if alpha_settled || Instant::now() >= deadline {
            break registry;
        }
        session.settle(Duration::from_millis(100));
    };
    assert_eq!(
        after_alpha_registry["threads"]
            .as_array()
            .unwrap()
            .iter()
            .find(|thread| thread["role"] == "alpha")
            .unwrap()["sendAttempts"][0]["status"],
        "settled",
        "alpha must settle before its background completion is reviewed"
    );
    send_ctrl_shift_t(&mut session);
    assert!(session.wait_for_visible("PROJECT THREADS", Duration::from_secs(5)));
    let visible = session.visible_output();
    assert!(
        visible
            .lines()
            .any(|line| line.contains("▸") && line.contains("gamma")),
        "gamma must remain the selected project thread after alpha completes\n{visible}"
    );
    session.buffer.clear();
    session.send(b"\x1b");
    assert!(session.wait_for_visible("gamma-job", Duration::from_secs(5)));
    let gamma_after_alpha = session.visible_output();
    assert!(
        !gamma_after_alpha.contains("handled alpha-job"),
        "alpha's background response replaced the selected gamma conversation\n{gamma_after_alpha}"
    );
    assert!(
        gamma_after_alpha.contains("gamma-job"),
        "Escape from the picker must return to gamma's conversation\n{gamma_after_alpha}"
    );
    let registry: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
    for (role, text, cursor) in [
        ("alpha", "alpha private draft", "alpha private ".len()),
        ("beta", "beta private draft", "beta private ".len()),
        ("gamma", "gamma private draft", "gamma private ".len()),
    ] {
        let thread = registry["threads"]
            .as_array()
            .unwrap()
            .iter()
            .find(|thread| thread["role"] == role)
            .unwrap();
        assert_eq!(
            thread["composerDraft"]["text"], text,
            "{role} draft after background completion"
        );
        assert_eq!(
            thread["composerDraft"]["cursor"], cursor,
            "{role} cursor after background completion"
        );
    }
    let alpha_thread = registry["threads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|thread| thread["role"] == "alpha")
        .unwrap();
    let gamma_thread = registry["threads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|thread| thread["role"] == "gamma")
        .unwrap();
    assert_eq!(alpha_thread["sendAttempts"][0]["status"], "settled");
    assert!(
        alpha_thread["sendAttempts"][0]["text"]
            .as_str()
            .unwrap()
            .contains("alpha-job")
    );
    assert_eq!(gamma_thread["sendAttempts"][0]["status"], "working");
    assert!(
        gamma_thread["sendAttempts"][0]["text"]
            .as_str()
            .unwrap()
            .contains("gamma-job")
    );

    session.buffer.clear();
    select_project_thread(&mut session, 2, 0);
    let alpha_view = session.visible_output();
    assert!(
        alpha_view.contains("alpha-job"),
        "the completed alpha conversation remains selectable\n{alpha_view}"
    );
    verify_selected_thread_draft_cursor(
        &mut session,
        &registry_path,
        "alpha",
        "alpha private draft",
        "alpha private ".len(),
    );
    session.buffer.clear();
    select_project_thread(&mut session, 0, 1);
    let beta_view = session.visible_output();
    assert!(
        beta_view.contains("beta-job"),
        "beta is selected\n{beta_view}"
    );
    verify_selected_thread_draft_cursor(
        &mut session,
        &registry_path,
        "beta",
        "beta private draft",
        "beta private ".len(),
    );
    session.buffer.clear();
    select_project_thread(&mut session, 1, 2);
    let gamma_view = session.visible_output();
    assert!(
        gamma_view.contains("gamma-job"),
        "gamma is selected again\n{gamma_view}"
    );
    verify_selected_thread_draft_cursor(
        &mut session,
        &registry_path,
        "gamma",
        "gamma private draft",
        "gamma private ".len(),
    );
    let registry: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
    for (role, text, cursor) in [
        ("alpha", "alpha private draft", "alpha private ".len()),
        ("beta", "beta private draft", "beta private ".len()),
        ("gamma", "gamma private draft", "gamma private ".len()),
    ] {
        let thread = registry["threads"]
            .as_array()
            .unwrap()
            .iter()
            .find(|thread| thread["role"] == role)
            .unwrap();
        assert_eq!(
            thread["composerDraft"]["text"], text,
            "{role} draft ownership"
        );
        assert_eq!(
            thread["composerDraft"]["cursor"], cursor,
            "{role} cursor ownership"
        );
    }
    for tag in ["beta-job", "gamma-job"] {
        std::fs::write(
            fixture_state.join(format!("e2e-core-release-{tag}")),
            "release",
        )
        .unwrap();
    }
    for tag in ["beta-job", "gamma-job"] {
        let completed = fixture_state.join(format!("e2e-core-completed-{tag}"));
        let deadline = Instant::now() + Duration::from_secs(12);
        while !completed.exists() && Instant::now() < deadline {
            session.settle(Duration::from_millis(50));
        }
        assert!(completed.exists(), "{tag} did not complete after release");
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    let registry = loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        let all_settled = registry["threads"].as_array().is_some_and(|threads| {
            threads.len() == 3
                && threads
                    .iter()
                    .all(|thread| thread["sendAttempts"][0]["status"] == "settled")
        });
        if all_settled || Instant::now() >= deadline {
            break registry;
        }
        session.settle(Duration::from_millis(100));
    };
    for thread in registry["threads"].as_array().unwrap() {
        assert_eq!(thread["sendAttempts"][0]["status"], "settled", "{registry}");
    }
    session.kill();
    let _ = std::fs::remove_file(fake_core);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn task_scheduler_threads_keep_task_identity_drafts_and_background_focus() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let fixture_state = root.join(".glass");
    std::fs::create_dir_all(&fixture_state).unwrap();
    std::fs::write(fixture_state.join("e2e-core-request-log"), "enabled").unwrap();
    std::fs::write(fixture_state.join("e2e-core-concurrency"), "enabled").unwrap();
    let fake_core = fake_core_binary(&root);
    let mut session = Session::start_with_core_binary(&binary(), &root, 120, 50, Some(&fake_core));
    assert!(session.wait_for("GLASS", Duration::from_secs(8)));
    session.trust_project();
    assert!(session.wait_for("✓ Ready", Duration::from_secs(8)));

    for (title, tag) in [
        ("Alpha task", "alpha-job"),
        ("Beta task", "beta-job"),
        ("Gamma task", "gamma-job"),
    ]
    .into_iter()
    {
        let arguments = serde_json::json!({
            "title": title,
            "goal": tag,
            "prompt": tag,
            "verification": {"kind": "settled"}
        });
        let command = format!("tool glass.task.create {arguments}");
        paste_command(&mut session, &command);
        assert!(
            session.wait_for("one-use confirmation required", Duration::from_secs(5)),
            "task creation did not request its normal one-use confirmation\n{}",
            session.output_tail()
        );
        session.send(b"y");
        assert!(
            session.wait_for("glass.task.create", Duration::from_secs(8)),
            "task creation did not reach the workspace scheduler\n{}",
            session.output_tail()
        );
        let started = fixture_state.join(format!("e2e-core-started-{tag}"));
        let deadline = Instant::now() + Duration::from_secs(15);
        while !started.exists() && Instant::now() < deadline {
            session.settle(Duration::from_millis(50));
        }
        assert!(
            started.exists(),
            "TaskScheduler did not start task {tag}\n{}",
            session.output_tail()
        );
    }

    let registry_path = fixture_state.join("threads/registry.json");
    session.send(b"1");
    assert!(session.wait_for_visible("CONVERSATION", Duration::from_secs(5)));
    let mut last_selected = 0;
    for (index, (task_id, title, tag, draft)) in [
        ("task-0001", "Alpha task", "alpha-job", "alpha task draft"),
        ("task-0002", "Beta task", "beta-job", "beta task draft"),
        ("task-0003", "Gamma task", "gamma-job", "gamma task draft"),
    ]
    .into_iter()
    .enumerate()
    {
        send_ctrl_shift_t(&mut session);
        assert!(
            session.wait_for_visible("PROJECT THREADS", Duration::from_secs(5)),
            "project thread picker did not open from the Agent surface\n{}",
            session.output_tail()
        );
        if index < last_selected {
            session.send(&b"\x1b[A".repeat(last_selected - index));
        } else if index > last_selected {
            session.send(&b"\x1b[B".repeat(index - last_selected));
        }
        let picker = session.visible_output();
        let label = format!("developer · {task_id} · {title}");
        assert!(
            picker.lines().any(|line| line.contains(&label)),
            "project thread picker must show task identity {label}\n{picker}"
        );
        session.send(b"\r");
        assert!(session.wait_for_visible("CONVERSATION", Duration::from_secs(5)));
        last_selected = index;
        let conversation = session.visible_output();
        assert!(
            conversation.contains(tag),
            "selected task prompt should be visible\n{conversation}"
        );
        session.send(draft.as_bytes());
        session.send(b"\x1b[D\x1b[D\x1b[D\x1b[D\x1b[D");

        let deadline = Instant::now() + Duration::from_secs(8);
        let registry = loop {
            let registry: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
            let has_draft = registry["threads"].as_array().is_some_and(|threads| {
                threads.iter().any(|thread| {
                    thread["composerDraft"]["text"] == draft
                        && thread["composerDraft"]["cursor"] == draft.len() - 5
                })
            });
            if has_draft || Instant::now() >= deadline {
                break registry;
            }
            session.settle(Duration::from_millis(100));
        };
        assert!(
            registry["threads"].as_array().is_some_and(|threads| {
                threads.iter().any(|thread| {
                    thread["composerDraft"]["text"] == draft
                        && thread["composerDraft"]["cursor"] == draft.len() - 5
                })
            }),
            "task thread did not persist its draft and cursor: {registry}"
        );
    }

    // Verify the scheduler's assignment projection and all three durable
    // conversation records while gamma remains selected.
    session.send(b"\x1b");
    session.send(b"5");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let visible = session.visible_output();
        let all_running = [
            ("task-0001", "Alpha task", "agent-0001"),
            ("task-0002", "Beta task", "agent-0002"),
            ("task-0003", "Gamma task", "agent-0003"),
        ]
        .into_iter()
        .all(|(id, title, agent)| {
            visible.lines().any(|line| {
                let compact = line
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .collect::<String>();
                compact.contains(id)
                    && compact.contains("running")
                    && compact.contains(&title.replace(' ', ""))
            }) && visible.contains(agent)
        });
        if all_running || Instant::now() >= deadline {
            assert!(all_running, "task snapshots are missing:\n{visible}");
            break;
        }
        session.settle(Duration::from_millis(100));
    }
    let registry: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
    let threads = registry["threads"].as_array().unwrap();
    assert_eq!(threads.len(), 3, "one durable thread per scheduled task");
    assert!(threads.iter().all(|thread| thread["role"] == "developer"));
    let mut thread_ids = std::collections::BTreeSet::new();
    let mut draft_threads = std::collections::BTreeMap::new();
    for draft in ["alpha task draft", "beta task draft", "gamma task draft"] {
        let thread = threads
            .iter()
            .find(|thread| thread["composerDraft"]["text"] == draft)
            .unwrap_or_else(|| panic!("missing durable task-thread draft {draft}: {registry}"));
        let thread_id = thread["threadId"].as_str().unwrap().to_owned();
        thread_ids.insert(thread_id.clone());
        draft_threads.insert(draft, thread_id);
        assert_eq!(thread["runs"].as_array().unwrap().len(), 1);
    }
    assert_eq!(
        thread_ids.len(),
        3,
        "task assignments use distinct durable threads"
    );
    session.send(b"1");
    assert!(session.wait_for_visible("CONVERSATION", Duration::from_secs(5)));
    let mut previous_selection = 2;
    for (target, tag, draft) in [
        (0, "alpha-job", "alpha task draft"),
        (1, "beta-job", "beta task draft"),
        (2, "gamma-job", "gamma task draft"),
    ] {
        select_project_thread(&mut session, previous_selection, target);
        previous_selection = target;
        session.send(b"\r");
        let cursor = draft.len() - 5;
        let (draft_before_cursor, draft_after_cursor) = draft.split_at(cursor);
        assert!(
            session.wait_for_visible(draft_before_cursor, Duration::from_secs(5)),
            "restored composer draft prefix {draft_before_cursor} should be visible\n{}",
            session.output_tail()
        );
        let visible = session.visible_output();
        assert!(
            visible.contains(tag),
            "selected task {tag} should be visible\n{visible}"
        );
        assert!(
            visible.contains(draft_before_cursor) && visible.contains(draft_after_cursor),
            "restored composer draft around its cursor should be visible\n{visible}"
        );

        let mut inserted = draft.to_owned();
        inserted.insert(cursor, 'X');
        session.send(b"X");
        let thread_id = draft_threads[draft].as_str();
        wait_for_saved_thread_draft_id(
            &mut session,
            &registry_path,
            thread_id,
            &inserted,
            cursor + 1,
        );
        session.send(b"\x7f");
        wait_for_saved_thread_draft_id(&mut session, &registry_path, thread_id, draft, cursor);
        session.send(b"\x1b");
    }
    assert_eq!(
        previous_selection, 2,
        "gamma remains selected before alpha release"
    );
    // Release alpha only. Task completion is observed through the task surface;
    // gamma remains selected in the project-thread picker and conversation.
    session.buffer.clear();
    std::fs::write(fixture_state.join("e2e-core-release-alpha-job"), "release").unwrap();
    let alpha_completed = fixture_state.join("e2e-core-completed-alpha-job");
    let deadline = Instant::now() + Duration::from_secs(12);
    while !alpha_completed.exists() && Instant::now() < deadline {
        session.settle(Duration::from_millis(50));
    }
    assert!(
        alpha_completed.exists(),
        "alpha task did not settle after release"
    );
    session.settle(Duration::from_millis(500));
    let deadline = Instant::now() + Duration::from_secs(10);
    let settled = loop {
        session.send(b"5");
        session.settle(Duration::from_millis(300));
        if session.wait_for_visible("task-0001", Duration::from_secs(2)) {
            let visible = session.visible_output();
            if visible.contains("succeeded") || Instant::now() >= deadline {
                break visible;
            }
        }
        if Instant::now() >= deadline {
            panic!(
                "alpha task did not reach succeeded state\n{}",
                session.output_tail()
            );
        }
    };
    assert!(
        settled.contains("succeeded"),
        "settled-task verification should report alpha completion\n{settled}"
    );
    assert!(settled.contains("Alpha task") && settled.contains("agent-0001"));
    for (id, title, agent) in [
        ("task-0002", "Beta task", "agent-0002"),
        ("task-0003", "Gamma task", "agent-0003"),
    ] {
        assert!(
            settled.lines().any(|line| {
                let compact = line
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .collect::<String>();
                compact.contains(id)
                    && compact.contains("running")
                    && compact.contains(&title.replace(' ', ""))
            }) && settled.contains(agent),
            "{id} should remain running on {agent}\n{settled}"
        );
    }

    session.send(b"1");
    assert!(session.wait_for_visible("CONVERSATION", Duration::from_secs(5)));
    assert!(
        session.visible_output().contains("gamma-job"),
        "gamma should remain the selected conversation while task snapshots refresh\n{}",
        session.visible_output()
    );
    send_ctrl_shift_t(&mut session);
    assert!(session.wait_for_visible("PROJECT THREADS", Duration::from_secs(5)));
    let picker = session.visible_output();
    assert!(
        picker.lines().any(|line| line.contains("▸")
            && line.contains("task-0003")
            && line.contains("Gamma task")),
        "gamma must stay selected after alpha completed in the background\n{picker}"
    );
    session.send(b"\x1b");
    assert!(session.wait_for_visible("gamma-job", Duration::from_secs(5)));
    let gamma_view = session.visible_output();
    assert!(
        gamma_view.contains("gamma-job"),
        "gamma conversation stays focused\n{gamma_view}"
    );
    assert!(
        !gamma_view.contains("handled alpha-job"),
        "alpha response stole gamma focus\n{gamma_view}"
    );
    let registry: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
    for (tag, draft) in [
        ("alpha-job", "alpha task draft"),
        ("beta-job", "beta task draft"),
        ("gamma-job", "gamma task draft"),
    ] {
        let thread = registry["threads"]
            .as_array()
            .unwrap()
            .iter()
            .find(|thread| thread["composerDraft"]["text"] == draft)
            .unwrap();
        assert_eq!(thread["composerDraft"]["cursor"], draft.len() - 5);
        let expected_classification = if tag == "alpha-job" {
            "completed"
        } else {
            "working"
        };
        assert_eq!(thread["runs"][0]["classification"], expected_classification);
        assert!(
            thread["runs"][0]["runId"]
                .as_str()
                .is_some_and(|id| !id.is_empty()),
            "{tag} needs a durable run identity"
        );
    }
    session.kill();
    let _ = std::fs::remove_file(fake_core);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn follow_up_queue_edits_persist_without_automatic_dispatch() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let fixture_state = root.join(".glass");
    std::fs::create_dir_all(&fixture_state).unwrap();
    std::fs::write(fixture_state.join("e2e-core-request-log"), "enabled").unwrap();
    let fake_core = fake_core_binary(&root);
    let mut session = Session::start_with_core_binary(&binary(), &root, 100, 30, Some(&fake_core));
    assert!(session.wait_for("GLASS", Duration::from_secs(8)));
    session.trust_project();
    assert!(session.wait_for("✓ Ready", Duration::from_secs(8)));
    session.send(b":agent spawn queue-owner queue persistence\r");
    assert!(
        session.wait_for("Spawned agent-0001", Duration::from_secs(5)),
        "queue-owner agent did not spawn\n{}",
        session.output_tail()
    );
    send_ctrl_shift_t(&mut session);
    assert!(session.wait_for("PROJECT THREADS", Duration::from_secs(5)));
    assert!(session.wait_for("queue-owner", Duration::from_secs(5)));
    session.send(b"\r\r");
    assert!(session.wait_for("CONVERSATION", Duration::from_secs(5)));

    session.send(b"\x11"); // Ctrl-Q opens this thread's follow-up queue.
    assert!(session.wait_for("THREAD FOLLOW-UP QUEUE", Duration::from_secs(5)));
    session.send(b"a");
    session.send(b"first queued follow-up");
    session.send(b"\r");
    assert!(session.wait_for("first queued follow-up", Duration::from_secs(5)));
    session.send(b"a");
    session.send(b"second queued follow-up");
    session.send(b"\r");
    assert!(session.wait_for("second queued follow-up", Duration::from_secs(5)));
    session.send(b"e");
    session.send(&[0x7f; 23]); // Remove the existing row text with Backspace.
    session.send(b"edited queued follow-up");
    session.send(b"\r");
    assert!(
        session.wait_for("edited queued follow-up", Duration::from_secs(5)),
        "queue row did not render after edit\n{}",
        session.output_tail()
    );

    // Reorder the second row ahead of the first, then remove the old first row.
    session.send(b"<");
    session.send(b"j");
    session.send(b"d");
    let registry_path = fixture_state.join("threads/registry.json");
    let deadline = Instant::now() + Duration::from_secs(8);
    let registry = loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        let queue = registry["threads"][0]["followUpQueue"].as_array();
        if queue.is_some_and(|entries| entries.len() == 1) || Instant::now() >= deadline {
            break registry;
        }
        session.settle(Duration::from_millis(100));
    };
    let queue = registry["threads"][0]["followUpQueue"]
        .as_array()
        .expect("queue persists in the owning thread record");
    assert_eq!(
        queue.len(),
        1,
        "only the retained row should remain: {registry}"
    );
    assert_eq!(queue[0]["text"], "edited queued follow-up");
    assert_eq!(queue[0]["mode"], "followUp");
    assert_eq!(queue[0]["state"], "ready");

    // Queue edits are local preparation. Only Enter on a selected ready row dispatches.
    let requests = std::fs::read_to_string(fixture_state.join("e2e-core-requests.jsonl"))
        .expect("fake Core request log is present");
    assert!(
        !requests.lines().any(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .is_some_and(|request| {
                    request["method"] == "turn/prompt"
                        && request["params"]["text"] == "edited queued follow-up"
                })
        }),
        "editing a queue must not dispatch a prompt"
    );
    session.settle(Duration::from_millis(300));
    session.send(b"\r");
    assert!(
        session.wait_for("awaiting acknowledgement", Duration::from_secs(8)),
        "explicit Enter did not begin delivery\n{}",
        session.output_tail()
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let registry = loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        if registry["threads"][0]["followUpQueue"][0]["state"] == "accepted"
            || Instant::now() >= deadline
        {
            break registry;
        }
        session.settle(Duration::from_millis(100));
    };
    assert_eq!(
        registry["threads"][0]["followUpQueue"][0]["state"], "accepted",
        "the row should change only after a positive send acknowledgement: {registry}"
    );
    let requests = std::fs::read_to_string(fixture_state.join("e2e-core-requests.jsonl"))
        .expect("fake Core request log is present");
    assert!(
        requests.lines().any(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .is_some_and(|request| {
                    request["method"] == "turn/prompt"
                        && request["params"]["text"] == "edited queued follow-up"
                })
        }),
        "explicit dispatch did not reach the owning Core session"
    );
    session.kill();
    let _ = std::fs::remove_file(fake_core);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn task_thread_queue_restart_requires_explicit_core_recovery() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let fixture_state = root.join(".glass");
    let data_home = root.join(".glass/xdg-data");
    std::fs::create_dir_all(&fixture_state).unwrap();
    std::fs::write(fixture_state.join("e2e-core-request-log"), "enabled").unwrap();
    std::fs::write(fixture_state.join("e2e-core-task-recovery-gate"), "enabled").unwrap();
    std::fs::write(fixture_state.join("e2e-core-unique-session-ids"), "enabled").unwrap();
    let fake_core = fake_core_binary(&root);
    let mut fixture_child =
        FixtureChildGuard::new(fixture_state.join("e2e-core-task-recovery-pid"));
    let mut resumed_fixture_child =
        FixtureChildGuard::new(fixture_state.join("e2e-core-task-recovery-resumed-pid"));
    let mut session = Session::start_with_data_home_options(
        &binary(),
        &root,
        100,
        30,
        "glass-core",
        Some(&fake_core),
        Some(&data_home),
    );
    assert!(session.wait_for("GLASS", Duration::from_secs(8)));
    session.trust_project();
    assert!(session.wait_for("✓ Ready", Duration::from_secs(8)));

    let task_spec = serde_json::json!({
        "title": "Recovery task",
        "goal": "issue64-task-recovery",
        "prompt": "issue64-task-recovery-prompt",
        "verification": {"kind": "settled"}
    });
    paste_command(&mut session, &format!("tool glass.task.create {task_spec}"));
    assert!(session.wait_for("one-use confirmation required", Duration::from_secs(5)));
    session.send(b"y");
    assert!(session.wait_for("glass.task.create", Duration::from_secs(8)));
    let old_turn_started = fixture_state.join("e2e-core-task-recovery-started");
    let deadline = Instant::now() + Duration::from_secs(15);
    while !old_turn_started.exists() && Instant::now() < deadline {
        session.settle(Duration::from_millis(50));
    }
    assert!(
        old_turn_started.exists(),
        "fake Core did not hold the task prompt open\n{}",
        session.output_tail()
    );

    let scheduler_path = fixture_state.join("tasks/scheduler.json");
    let thread_registry_path = fixture_state.join("threads/registry.json");
    let initial_scheduler: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&scheduler_path).unwrap()).unwrap();
    let initial_task = &initial_scheduler["tasks"][0]["snapshot"];
    assert_eq!(initial_task["id"], "task-0001");
    assert_eq!(initial_task["state"], "running");
    let old_agent_id = initial_task["assignedAgent"].as_str().unwrap().to_owned();
    let initial_threads: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&thread_registry_path).unwrap()).unwrap();
    assert_eq!(initial_threads["threads"].as_array().unwrap().len(), 1);
    let old_thread = &initial_threads["threads"][0];
    let old_thread_id = old_thread["threadId"].as_str().unwrap().to_owned();
    let old_run_id = old_thread["runs"][0]["runId"].as_str().unwrap().to_owned();
    let old_core_session_id = old_thread["runs"][0]["coreSessionId"]
        .as_str()
        .unwrap()
        .to_owned();
    let old_process_id =
        std::fs::read_to_string(fixture_state.join("e2e-core-task-recovery-pid")).unwrap();
    assert_eq!(
        old_core_session_id,
        format!("fixture-session-{}", old_process_id.trim())
    );

    // Persist a real queue row through the selected task thread's queue UI.
    session.send(b"1");
    assert!(session.wait_for("CONVERSATION", Duration::from_secs(5)));
    send_ctrl_shift_t(&mut session);
    assert!(session.wait_for("PROJECT THREADS", Duration::from_secs(5)));
    assert!(session.wait_for("task-0001 · Recovery task", Duration::from_secs(5)));
    session.send(b"\r");
    assert!(session.wait_for("CONVERSATION", Duration::from_secs(5)));
    session.send(b"\x11");
    assert!(session.wait_for("THREAD FOLLOW-UP QUEUE", Duration::from_secs(5)));
    assert!(session.wait_for("No queued follow-ups", Duration::from_secs(5)));
    session.send(b"a");
    assert!(
        session.wait_for("EDIT FOLLOW-UP", Duration::from_secs(5)),
        "queue add did not open its editor\n{}",
        session.output_tail()
    );
    session.send(b"queued recovery follow-up");
    session.send(b"\r");
    let deadline = Instant::now() + Duration::from_secs(8);
    let queued = loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&thread_registry_path).unwrap()).unwrap();
        let row = registry["threads"][0]["followUpQueue"][0].clone();
        if row["text"] == "queued recovery follow-up"
            && row["targetRunId"] == old_run_id
            && row["state"] == "ready"
        {
            break registry;
        }
        assert!(
            Instant::now() < deadline,
            "task queue row did not persist against its initial RunId: {registry}"
        );
        session.settle(Duration::from_millis(100));
    };
    let old_queue_entry_id = queued["threads"][0]["followUpQueue"][0]["entryId"]
        .as_str()
        .unwrap()
        .to_owned();
    session.send(b"\x1b");
    session.kill();
    fixture_child.terminate();
    let _ = std::fs::remove_file(fixture_state.join("e2e-core-task-recovery-gate"));

    let mut reopened = Session::start_with_data_home_options(
        &binary(),
        &root,
        100,
        30,
        "glass-core",
        Some(&fake_core),
        Some(&data_home),
    );
    assert!(reopened.wait_for("GLASS", Duration::from_secs(8)));
    assert!(reopened.wait_for("✓ Ready", Duration::from_secs(8)));
    let recovered_scheduler: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&scheduler_path).unwrap()).unwrap();
    let recovered_task = &recovered_scheduler["tasks"][0]["snapshot"];
    assert_eq!(recovered_task["state"], "paused");
    assert_eq!(recovered_task["recoveryPending"], true);
    assert_eq!(recovered_task["assignedAgent"], old_agent_id);

    let recovered_threads: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&thread_registry_path).unwrap()).unwrap();
    assert_eq!(recovered_threads["threads"].as_array().unwrap().len(), 1);
    let recovered_old_thread = &recovered_threads["threads"][0];
    assert_eq!(recovered_old_thread["threadId"], old_thread_id);
    assert_eq!(recovered_old_thread["runs"][0]["runId"], old_run_id);
    assert_eq!(
        recovered_old_thread["runs"][0]["coreSessionId"],
        old_core_session_id
    );
    assert_eq!(recovered_old_thread["runs"][0]["classification"], "working");
    assert_eq!(
        recovered_old_thread["followUpQueue"][0]["entryId"],
        old_queue_entry_id
    );
    assert_eq!(
        recovered_old_thread["followUpQueue"][0]["targetRunId"],
        old_run_id
    );
    assert_eq!(recovered_old_thread["followUpQueue"][0]["state"], "ready");

    // Assert the scheduler's persisted AgentId still joins to the recovered
    // thread before allowing task recovery to create another assignment.
    send_ctrl_shift_t(&mut reopened);
    assert!(reopened.wait_for("PROJECT THREADS", Duration::from_secs(5)));
    assert!(
        reopened.wait_for(
            "developer · task-0001 · Recovery task",
            Duration::from_secs(5)
        ),
        "reconstructed AgentRegistry ID did not join to the recovered task assignment\n{}",
        reopened.output_tail()
    );
    assert!(reopened.wait_for("recovery pending", Duration::from_secs(5)));
    reopened.send(b"\r");
    assert!(reopened.wait_for("CONVERSATION", Duration::from_secs(5)));

    // Keep the interrupted conversation queue pinned to its original RunId.
    // Task resume below must not implicitly retarget or dispatch it.
    reopened.send(b"\x11");
    assert!(reopened.wait_for("THREAD FOLLOW-UP QUEUE", Duration::from_secs(5)));
    assert!(reopened.wait_for("queued recovery follow-up", Duration::from_secs(5)));
    let requests_path = fixture_state.join("e2e-core-requests.jsonl");
    let prompt_count = |requests: &str| {
        requests
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|request| request["method"] == "turn/prompt")
            .count()
    };
    let requests_before_task_resume = std::fs::read_to_string(&requests_path).unwrap();
    assert_eq!(prompt_count(&requests_before_task_resume), 1);
    assert!(
        !requests_before_task_resume.contains("queued recovery follow-up"),
        "restart replayed the queued follow-up"
    );
    let queue_before_task_resume: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&thread_registry_path).unwrap()).unwrap();
    assert_eq!(
        queue_before_task_resume["threads"][0]["followUpQueue"][0]["targetRunId"],
        old_run_id
    );
    assert_eq!(
        queue_before_task_resume["threads"][0]["followUpQueue"][0]["entryId"],
        old_queue_entry_id
    );
    assert_eq!(
        queue_before_task_resume["threads"][0]["followUpQueue"][0]["state"],
        "ready"
    );
    reopened.send(b"\x1b");

    // Explicit task recovery must produce one new worker, Core session, and
    // prompt request, without touching the old thread's queue.
    std::fs::write(
        fixture_state.join("e2e-core-task-recovery-resumed"),
        "enabled",
    )
    .unwrap();
    std::fs::write(
        fixture_state.join("e2e-core-task-recovery-resume-gate"),
        "enabled",
    )
    .unwrap();
    paste_command(
        &mut reopened,
        "tool glass.task.resume {\"taskId\":\"task-0001\"}",
    );
    assert!(reopened.wait_for("one-use confirmation required", Duration::from_secs(5)));
    reopened.send(b"y");
    assert!(reopened.wait_for("glass.task.resume", Duration::from_secs(8)));
    let resumed_turn_started = fixture_state.join("e2e-core-task-recovery-resume-started");
    let deadline = Instant::now() + Duration::from_secs(15);
    while !resumed_turn_started.exists() && Instant::now() < deadline {
        reopened.settle(Duration::from_millis(50));
    }
    assert!(
        resumed_turn_started.exists(),
        "explicit task resume did not reach a new Core runtime\n{}",
        reopened.output_tail()
    );
    let resumed_pid_path = fixture_state.join("e2e-core-task-recovery-resumed-pid");
    let resumed_pid = std::fs::read_to_string(&resumed_pid_path).unwrap();
    assert_ne!(resumed_pid.trim(), old_process_id.trim());
    let deadline = Instant::now() + Duration::from_secs(12);
    let resumed_scheduler = loop {
        let scheduler: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&scheduler_path).unwrap()).unwrap();
        let snapshot = &scheduler["tasks"][0]["snapshot"];
        if snapshot["state"] == "running" || Instant::now() >= deadline {
            break scheduler;
        }
        reopened.settle(Duration::from_millis(100));
    };
    let resumed_task = &resumed_scheduler["tasks"][0]["snapshot"];
    assert_eq!(resumed_task["state"], "running");
    assert_eq!(resumed_task["recoveryPending"], false);
    let new_agent_id = resumed_task["assignedAgent"].as_str().unwrap();
    assert_ne!(new_agent_id, old_agent_id);
    let requests_after_task_resume = std::fs::read_to_string(&requests_path).unwrap();
    assert_eq!(prompt_count(&requests_after_task_resume), 2);
    assert!(
        requests_after_task_resume.contains("issue64-task-recovery-prompt"),
        "Core did not receive the explicit task recovery prompt"
    );
    assert!(
        !requests_after_task_resume.contains("queued recovery follow-up"),
        "explicit task resume dispatched the old thread's queue row"
    );
    let after_task_resume_threads: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&thread_registry_path).unwrap()).unwrap();
    assert_eq!(
        after_task_resume_threads["threads"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let new_task_thread = after_task_resume_threads["threads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|thread| thread["threadId"] != old_thread_id)
        .unwrap();
    assert_eq!(
        new_task_thread["runs"][0]["coreSessionId"],
        format!("fixture-session-{}", resumed_pid.trim())
    );
    let old_queue_after_resume = after_task_resume_threads["threads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|thread| thread["threadId"] == old_thread_id)
        .unwrap();
    assert_eq!(
        old_queue_after_resume["followUpQueue"][0]["entryId"],
        old_queue_entry_id
    );
    assert_eq!(
        old_queue_after_resume["followUpQueue"][0]["targetRunId"],
        old_run_id
    );
    assert_eq!(old_queue_after_resume["followUpQueue"][0]["state"], "ready");

    std::fs::write(
        fixture_state.join("e2e-core-task-recovery-resume-release"),
        "release",
    )
    .unwrap();
    reopened.kill();
    fixture_child.terminate();
    resumed_fixture_child.terminate();
    let _ = std::fs::remove_file(fake_core);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn active_run_queue_steer_reaches_its_run_and_settles_truthfully() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }

    let root = workspace_root();
    let fixture_state = root.join(".glass");
    std::fs::create_dir_all(&fixture_state).unwrap();
    std::fs::write(fixture_state.join("e2e-core-request-log"), "enabled").unwrap();
    std::fs::write(fixture_state.join("e2e-core-active-steer"), "enabled").unwrap();
    let fake_core = fake_core_binary(&root);
    let fixture_guard = FixtureChildGuard::new(fixture_state.join("e2e-core-active-steer-pid"));
    let mut session = Session::start_with_core_binary(&binary(), &root, 100, 30, Some(&fake_core));
    assert!(session.wait_for("GLASS", Duration::from_secs(8)));
    session.trust_project();
    assert!(session.wait_for("✓ Ready", Duration::from_secs(8)));
    session.send(b":agent spawn queue-owner active-run-queue-target\r");
    assert!(
        session.wait_for("Spawned agent-0001", Duration::from_secs(5)),
        "active queue target did not spawn\n{}",
        session.output_tail()
    );
    send_ctrl_shift_t(&mut session);
    assert!(session.wait_for("PROJECT THREADS", Duration::from_secs(5)));
    assert!(
        session.wait_for("idle", Duration::from_secs(12)),
        "spawned queue target never became ready\n{}",
        session.output_tail()
    );
    session.send(b"\r\r");
    assert!(session.wait_for("CONVERSATION", Duration::from_secs(5)));
    session.send(b"active-run-queue-target");
    session.send(b"\r");
    let started = fixture_state.join("e2e-core-active-steer-started");
    let deadline = Instant::now() + Duration::from_secs(8);
    while !started.exists() && Instant::now() < deadline {
        session.settle(Duration::from_millis(100));
    }
    assert!(
        started.exists(),
        "fake Core did not hold the target run active\n{}",
        session.output_tail()
    );
    let registry_path = fixture_state.join("threads/registry.json");
    let deadline = Instant::now() + Duration::from_secs(8);
    let active_registry = loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        if registry["threads"][0]["sendAttempts"][0]["status"] == "working"
            || Instant::now() >= deadline
        {
            break registry;
        }
        session.settle(Duration::from_millis(100));
    };
    assert_eq!(
        active_registry["threads"][0]["sendAttempts"][0]["status"], "working",
        "the target run must be recorded active before queue steering: {active_registry}"
    );

    send_ctrl_shift_t(&mut session);
    assert!(session.wait_for("PROJECT THREADS", Duration::from_secs(5)));
    assert!(session.wait_for("queue-owner", Duration::from_secs(5)));
    session.send(b"\r\r");
    assert!(session.wait_for("CONVERSATION", Duration::from_secs(5)));
    session.send(b"\x11");
    assert!(session.wait_for("THREAD FOLLOW-UP QUEUE", Duration::from_secs(5)));
    session.send(b"s");
    session.send(b"steer active queue target");
    session.send(b"\r");
    assert!(
        session.wait_for("steer active queue target", Duration::from_secs(5)),
        "active-run steer row was not added\n{}",
        session.output_tail()
    );

    let deadline = Instant::now() + Duration::from_secs(8);
    let registry = loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        let ready = registry["threads"].as_array().is_some_and(|threads| {
            threads.len() == 1
                && threads[0]["followUpQueue"][0]["state"] == "ready"
                && threads[0]["followUpQueue"][0]["mode"] == "steer"
        });
        if ready || Instant::now() >= deadline {
            break registry;
        }
        session.settle(Duration::from_millis(100));
    };
    let thread = &registry["threads"][0];
    let target_run_id = thread["runs"][0]["runId"]
        .as_str()
        .expect("the active run has a stable RunId")
        .to_owned();
    assert_eq!(thread["followUpQueue"][0]["targetRunId"], target_run_id);
    assert_eq!(thread["followUpQueue"][0]["state"], "ready");

    session.settle(Duration::from_millis(300));
    let mut dispatch_started = false;
    for _ in 0..4 {
        session.send(b"\r");
        if session.wait_for("awaiting acknowledgement", Duration::from_secs(2)) {
            dispatch_started = true;
            break;
        }
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        let still_ready = registry["threads"][0]["followUpQueue"][0]["state"] == "ready";
        let no_steer_attempt = registry["threads"][0]["sendAttempts"]
            .as_array()
            .is_some_and(|attempts| {
                !attempts.iter().any(|attempt| {
                    attempt["mode"] == "steer" && attempt["text"] == "steer active queue target"
                })
            });
        let requests = std::fs::read_to_string(fixture_state.join("e2e-core-requests.jsonl"))
            .expect("fake Core request log is present");
        let no_steer_request = !requests.lines().any(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .is_some_and(|request| {
                    request["method"] == "turn/steer"
                        && request["params"]["text"] == "steer active queue target"
                })
        });
        assert!(
            still_ready && no_steer_attempt && no_steer_request,
            "only a still-ready row with no attempt or Core request may be dispatched again: {registry}"
        );
        session.settle(Duration::from_millis(300));
    }
    assert!(
        dispatch_started,
        "explicit active-run queue delivery did not start\n{}",
        session.output_tail()
    );
    let received = fixture_state.join("e2e-core-active-steer-received");
    let deadline = Instant::now() + Duration::from_secs(8);
    while !received.exists() && Instant::now() < deadline {
        session.settle(Duration::from_millis(100));
    }
    assert!(
        received.exists(),
        "steering was not delivered while the target prompt remained active\n{}",
        session.output_tail()
    );
    let steer_params: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&received).unwrap()).unwrap();
    assert_eq!(steer_params["sessionId"], "fixture-session");
    assert_eq!(steer_params["text"], "steer active queue target");
    assert_eq!(steer_params["context"]["targetRunId"], target_run_id);

    let deadline = Instant::now() + Duration::from_secs(10);
    let registry = loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        let item_accepted = registry["threads"][0]["followUpQueue"][0]["state"] == "accepted";
        if item_accepted || Instant::now() >= deadline {
            break registry;
        }
        session.settle(Duration::from_millis(100));
    };
    assert_eq!(
        registry["threads"][0]["followUpQueue"][0]["state"], "accepted",
        "positive Core steer acknowledgement should accept the queue row: {registry}"
    );
    let deadline = Instant::now() + Duration::from_secs(8);
    let registry = loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        let attempts = registry["threads"][0]["sendAttempts"].as_array();
        let steer_settled = attempts.is_some_and(|attempts| {
            attempts.iter().any(|attempt| {
                attempt["targetRunId"] == target_run_id
                    && attempt["mode"] == "steer"
                    && attempt["status"] == "settled"
            })
        });
        if steer_settled || Instant::now() >= deadline {
            break registry;
        }
        session.settle(Duration::from_millis(100));
    };
    let attempts = registry["threads"][0]["sendAttempts"]
        .as_array()
        .expect("send attempts persist beside the queue");
    assert!(
        attempts.iter().any(|attempt| {
            attempt["targetRunId"] == target_run_id
                && attempt["mode"] == "steer"
                && attempt["status"] == "settled"
        }),
        "the exact turn/steer response must settle its attempt before the original prompt is released: {registry}"
    );
    assert!(
        attempts.iter().any(|attempt| {
            attempt["mode"] == "prompt"
                && attempt["status"] == "working"
                && attempt["targetRunId"] == target_run_id
        }),
        "the held prompt must remain active while the steer response settles: {registry}"
    );
    std::fs::write(
        fixture_state.join("e2e-core-active-steer-release"),
        "release",
    )
    .unwrap();

    let deadline = Instant::now() + Duration::from_secs(12);
    let registry = loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        let settled = registry["threads"].as_array().is_some_and(|threads| {
            threads[0]["sendAttempts"]
                .as_array()
                .is_some_and(|attempts| {
                    attempts.len() == 2
                        && attempts
                            .iter()
                            .all(|attempt| attempt["status"] == "settled")
                })
        });
        if settled || Instant::now() >= deadline {
            break registry;
        }
        session.settle(Duration::from_millis(100));
    };
    assert!(
        registry["threads"][0]["sendAttempts"]
            .as_array()
            .is_some_and(|attempts| {
                attempts.len() == 2
                    && attempts
                        .iter()
                        .all(|attempt| attempt["status"] == "settled")
            }),
        "accepted steer should settle only from its correlated response, with both run attempts terminal: {registry}"
    );
    let requests = std::fs::read_to_string(fixture_state.join("e2e-core-requests.jsonl"))
        .expect("fake Core request log is present");
    assert!(
        requests.lines().any(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .is_some_and(|request| {
                    request["method"] == "turn/steer"
                        && request["params"]["text"] == "steer active queue target"
                        && request["params"]["context"]["targetRunId"] == target_run_id
                })
        }),
        "the active run's exact target must reach Core as a steer"
    );
    session.kill();
    drop(fixture_guard);
    let _ = std::fs::remove_file(fake_core);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn follow_up_queue_old_run_retarget_and_review_retry_are_explicit() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }

    let root = workspace_root();
    let fixture_state = root.join(".glass");
    std::fs::create_dir_all(&fixture_state).unwrap();
    std::fs::write(fixture_state.join("e2e-core-request-log"), "enabled").unwrap();
    let fake_core = fake_core_binary(&root);
    let mut session = Session::start_with_core_binary(&binary(), &root, 100, 30, Some(&fake_core));
    assert!(session.wait_for("GLASS", Duration::from_secs(8)));
    session.trust_project();
    assert!(session.wait_for("✓ Ready", Duration::from_secs(8)));
    session.send(b":agent spawn queue-owner retarget-and-review\r");
    assert!(
        session.wait_for("Spawned agent-0001", Duration::from_secs(5)),
        "queue owner did not spawn\n{}",
        session.output_tail()
    );
    send_ctrl_shift_t(&mut session);
    assert!(session.wait_for("PROJECT THREADS", Duration::from_secs(5)));
    assert!(
        session.wait_for("queue-owner", Duration::from_secs(5)),
        "queue owner did not appear in the project picker\n{}",
        session.output_tail()
    );
    assert!(
        session.wait_for("idle", Duration::from_secs(12)),
        "queue owner did not reach its current idle run\n{}",
        session.output_tail()
    );
    session.send(b"\r\r");
    assert!(session.wait_for("CONVERSATION", Duration::from_secs(5)));

    let registry_path = fixture_state.join("threads/registry.json");
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut registry = loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        if registry["threads"].as_array().is_some_and(|threads| {
            threads.len() == 1
                && threads[0]["runs"]
                    .as_array()
                    .is_some_and(|runs| !runs.is_empty())
        }) || Instant::now() >= deadline
        {
            break registry;
        }
        session.settle(Duration::from_millis(100));
    };
    let thread = &mut registry["threads"][0];
    let current_run_id = thread["runs"][0]["runId"]
        .as_str()
        .expect("the current queue destination has a persisted RunId")
        .to_owned();
    let old_run_id = "run-00000000000000000000000000000000";
    assert_ne!(current_run_id, old_run_id);
    thread["runs"]
        .as_array_mut()
        .expect("run history is persisted")
        .push(serde_json::json!({
            "runId": old_run_id,
            "engine": "glass-core",
            "coreSessionId": null,
            "classification": "completed"
        }));
    thread["followUpQueue"] = serde_json::json!([
        {
            "entryId": "queue-00000000000000000000000000000001",
            "targetRunId": old_run_id,
            "mode": "followUp",
            "text": "old-run row must be retargeted",
            "state": "ready"
        },
        {
            "entryId": "queue-00000000000000000000000000000002",
            "targetRunId": current_run_id,
            "mode": "followUp",
            "text": "review this retry exactly once",
            "state": "needsReview"
        }
    ]);
    thread["sendAttempts"] = serde_json::json!([{
        "attemptId": "attempt-00000000000000000000000000000001",
        "clientMessageId": "queue-00000000000000000000000000000002",
        "targetRunId": current_run_id,
        "mode": "followUp",
        "text": "review this retry exactly once",
        "status": "uncertain",
        "engineRequestId": "fixture-prior-request",
        "createdAtMs": 2
    }]);
    std::fs::write(
        &registry_path,
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();

    session.send(b"\x11");
    assert!(
        session.wait_for("THREAD FOLLOW-UP QUEUE", Duration::from_secs(5)),
        "the persisted queue did not open\n{}",
        session.output_tail()
    );
    assert!(session.wait_for("old-run row must be retargeted", Duration::from_secs(5)));

    // A stale destination is visible but Enter cannot create an attempt or
    // reach Core. Retargeting is a separate, explicit user action.
    session.send(b"\r");
    assert!(
        session.wait_for("older run · press t", Duration::from_secs(5)),
        "dispatch to the old RunId was not blocked\n{}",
        session.output_tail()
    );
    session.settle(Duration::from_millis(300));
    let blocked: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
    assert_eq!(
        blocked["threads"][0]["followUpQueue"][0]["targetRunId"],
        old_run_id
    );
    assert_eq!(
        blocked["threads"][0]["followUpQueue"][0]["state"], "ready",
        "blocked dispatch must leave the old-run row ready for explicit retargeting"
    );
    assert_eq!(
        blocked["threads"][0]["sendAttempts"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "the stale row must not create another attempt"
    );
    let requests_path = fixture_state.join("e2e-core-requests.jsonl");
    let requests = std::fs::read_to_string(&requests_path).unwrap_or_default();
    assert!(
        !requests.contains("old-run row must be retargeted"),
        "a stale queue row reached Core"
    );

    session.send(b"t");
    let deadline = Instant::now() + Duration::from_secs(8);
    let retargeted = loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        if registry["threads"][0]["followUpQueue"][0]["targetRunId"] == current_run_id
            && registry["threads"][0]["followUpQueue"][0]["state"] == "ready"
        {
            break registry;
        }
        assert!(
            Instant::now() < deadline,
            "explicit retarget did not persist: {registry}"
        );
        session.settle(Duration::from_millis(100));
    };
    assert_eq!(
        retargeted["threads"][0]["followUpQueue"][0]["targetRunId"],
        current_run_id
    );
    assert_eq!(
        retargeted["threads"][0]["followUpQueue"][0]["state"],
        "ready"
    );

    // The second row models a prior dispatch whose runtime acceptance is
    // unknown. Canceling its duplicate-risk confirmation leaves both the row
    // and attempt ledger untouched; only the later confirmation sends it.
    session.send(b"j");
    session.send(b"\r");
    assert!(
        session.wait_for("RETRY UNRESOLVED DELIVERY?", Duration::from_secs(5)),
        "NeedsReview did not open its retry confirmation\n{}",
        session.output_tail()
    );
    session.clear_output();
    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));
    session.send(b"k");
    session.send(b"j");
    assert!(session.wait_for("review this retry exactly once", Duration::from_secs(5)));
    assert!(
        !session
            .visible_output()
            .contains("RETRY UNRESOLVED DELIVERY?"),
        "Escape must close retry confirmation before another key is handled"
    );
    let cancelled: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
    assert_eq!(
        cancelled["threads"][0]["followUpQueue"][1]["state"],
        "needsReview"
    );
    assert_eq!(
        cancelled["threads"][0]["sendAttempts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        cancelled["threads"][0]["sendAttempts"][0]["status"], "uncertain",
        "cancel leaves the existing unresolved attempt intact"
    );
    let requests = std::fs::read_to_string(&requests_path).unwrap_or_default();
    assert!(
        !requests.contains("review this retry exactly once"),
        "canceling retry confirmation sent a Core request"
    );

    session.send(b"\r");
    assert!(session.wait_for("RETRY UNRESOLVED DELIVERY?", Duration::from_secs(5)));
    session.send(b"\r");
    assert!(
        session.wait_for("awaiting acknowledgement", Duration::from_secs(8)),
        "confirmed retry did not dispatch\n{}",
        session.output_tail()
    );
    let deadline = Instant::now() + Duration::from_secs(12);
    let settled = loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        let accepted = registry["threads"][0]["followUpQueue"][1]["state"] == "accepted";
        let attempt_settled = registry["threads"][0]["sendAttempts"]
            .as_array()
            .is_some_and(|attempts| {
                attempts.len() == 2
                    && attempts[0]["status"] == "uncertain"
                    && attempts[1]["targetRunId"] == current_run_id
                    && attempts[1]["text"] == "review this retry exactly once"
                    && attempts[1]["status"] == "settled"
            });
        let queue_target_current =
            registry["threads"][0]["followUpQueue"][1]["targetRunId"] == current_run_id;
        if accepted && attempt_settled && queue_target_current {
            break registry;
        }
        assert!(
            Instant::now() < deadline,
            "confirmed retry did not settle exactly: {registry}"
        );
        session.settle(Duration::from_millis(100));
    };
    let attempts = settled["threads"][0]["sendAttempts"]
        .as_array()
        .expect("retry attempt remains in its thread ledger");
    assert_eq!(attempts.len(), 2, "cancel did not create an extra attempt");
    assert_eq!(
        attempts[0]["attemptId"],
        "attempt-00000000000000000000000000000001"
    );
    assert_ne!(attempts[0]["attemptId"], attempts[1]["attemptId"]);
    assert_eq!(
        attempts[0]["clientMessageId"],
        attempts[1]["clientMessageId"]
    );
    assert_eq!(attempts[0]["status"], "uncertain");
    assert_eq!(attempts[1]["mode"], "prompt");
    assert_eq!(attempts[1]["targetRunId"], current_run_id);
    assert_eq!(
        settled["threads"][0]["followUpQueue"][1]["targetRunId"],
        current_run_id
    );
    assert_eq!(
        settled["threads"][0]["followUpQueue"][1]["mode"],
        "followUp"
    );
    let requests = std::fs::read_to_string(&requests_path).unwrap();
    let retried_requests = requests
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|request| {
            matches!(
                request["method"].as_str(),
                Some("turn/prompt" | "turn/followUp")
            ) && request["params"]["text"] == "review this retry exactly once"
        })
        .collect::<Vec<_>>();
    assert_eq!(
        retried_requests.len(),
        1,
        "only the confirmed retry reaches Core"
    );
    assert_eq!(
        retried_requests[0]["params"]["sessionId"],
        "fixture-session"
    );

    session.kill();
    let _ = std::fs::remove_file(fake_core);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn agent_update_route_requires_confirmation_in_tui() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let mut session = Session::start_with_options(&binary(), &root, 118, 32, "pi", None);
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
    session.trust_project();
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
fn attention_inbox_keyboard_journey_keeps_composer_and_opens_proposal_context() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }

    // Exercise the normal desktop size and compact layout with a proposal
    // seeded into the isolated project timeline. No provider or service runs.
    for (cols, rows) in [(80, 24), (50, 16)] {
        let root = git_workspace_root();
        let data_home = root.join(".glass/xdg-data");
        seed_attention_proposal(&root, &data_home);
        let fake_core = fake_core_binary(&root);
        let mut session = Session::start_with_data_home_options(
            &binary(),
            &root,
            cols,
            rows,
            "glass-core",
            Some(&fake_core),
            Some(&data_home),
        );
        assert!(session.wait_for("GLASS", Duration::from_secs(8)));
        if cols <= 50 {
            session.trust_project_with_probe("trusted project");
        } else {
            session.trust_project();
        }
        assert!(
            session.wait_for("✓ Ready", Duration::from_secs(8)),
            "offline Core did not become ready at {cols}x{rows}\n{}",
            session.output_tail()
        );

        // Keep a composer draft underneath the asynchronous inbox refresh.
        // Closing the overlay must restore composer input, proven by appending
        // text before reopening the item list.
        session.send(b"\r");
        assert!(
            session.wait_for("CONVERSATION", Duration::from_secs(5)),
            "Agent composer did not open at {cols}x{rows}\n{}",
            session.output_tail()
        );
        session.send(b"draft");
        send_ctrl_shift_a(&mut session);
        assert!(
            session.wait_for("ATTENTION INBOX", Duration::from_secs(5)),
            "keyboard shortcut did not open inbox at {cols}x{rows}\n{}",
            session.output_tail()
        );
        assert!(
            session.wait_for("Review · inbox-review", Duration::from_secs(8)),
            "async inbox refresh did not show the proposal at {cols}x{rows}\n{}",
            session.output_tail()
        );
        session.send(b"\x1b");
        session.send(b"Z");
        assert!(
            session.wait_for("Z", Duration::from_secs(5)),
            "background refresh stole composer input at {cols}x{rows}\n{}",
            session.output_tail()
        );

        // Reopen the inbox and activate its selected item with Enter. It must
        // navigate to the proposal's exact file review context.
        send_ctrl_shift_a(&mut session);
        assert!(
            session.wait_for("ATTENTION INBOX", Duration::from_secs(5)),
            "inbox did not reopen at {cols}x{rows}\n{}",
            session.output_tail()
        );
        assert!(
            session.wait_for("Review · inbox-review", Duration::from_secs(5)),
            "inbox lost the pending proposal at {cols}x{rows}\n{}",
            session.output_tail()
        );
        session.send(b"\r");
        assert!(
            session.wait_for("pair-apply", Duration::from_secs(5)),
            "inbox item did not open its proposal review context at {cols}x{rows}\n{}",
            session.output_tail()
        );
        let owning_file_probe = if cols <= 50 {
            "FILES · a-first.txt"
        } else {
            "Context · @a-first.txt"
        };
        assert!(
            session.wait_for(owning_file_probe, Duration::from_secs(5)),
            "review context did not select the owning file at {cols}x{rows}\n{}",
            session.output_tail()
        );
        session.kill();
        let _ = std::fs::remove_file(fake_core);
    }
}

#[test]
fn attention_review_keyboard_journey_preserves_uncertain_send_and_exits_cleanly() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }

    let prompt = "attention retry marker";
    for (cols, rows) in [(80, 24), (50, 16)] {
        let root = git_workspace_root();
        let data_home = root.join(".glass/xdg-data");
        seed_attention_proposal(&root, &data_home);
        let fixture_state = root.join(".glass");
        std::fs::create_dir_all(&fixture_state).unwrap();
        std::fs::write(fixture_state.join("e2e-core-request-log"), "enabled").unwrap();
        seed_uncertain_send_attempt(&root, prompt);
        let fake_core = fake_core_binary(&root);
        let mut session = Session::start_with_data_home_options(
            &binary(),
            &root,
            cols,
            rows,
            "glass-core",
            Some(&fake_core),
            Some(&data_home),
        );
        assert!(session.wait_for("GLASS", Duration::from_secs(8)));
        if cols <= 50 {
            session.trust_project_with_probe("trusted project");
        } else {
            session.trust_project();
        }
        assert!(session.wait_for("✓ Ready", Duration::from_secs(8)));

        send_ctrl_shift_a(&mut session);
        assert!(
            session.wait_for("ATTENTION INBOX", Duration::from_secs(5)),
            "inbox did not open at {cols}x{rows}\n{}",
            session.output_tail()
        );
        let inbox = session.visible_output();
        assert!(
            inbox.contains("Message delivery uncertain") && inbox.contains("Review · inbox-review"),
            "uncertain send and proposal did not appear together at {cols}x{rows}\n{}",
            session.output_tail()
        );
        assert!(
            inbox.contains(prompt),
            "inbox omitted the saved message at {cols}x{rows}\n{}",
            session.output_tail()
        );
        // Proposal sorts before the uncertain send; move to the latter.
        session.send(b"\x1b[B");
        session.send(b"\r");
        assert!(session.wait_for("Message delivery is uncertain", Duration::from_secs(5)));
        assert!(session.wait_for(prompt, Duration::from_secs(3)));
        let registry_path = fixture_state.join("threads/registry.json");
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        assert_eq!(
            registry["threads"][0]["sendAttempts"][0]["status"], "uncertain",
            "opening the attention item must preserve its uncertain ledger state"
        );
        let requests = std::fs::read_to_string(fixture_state.join("e2e-core-requests.jsonl"))
            .expect("fake Core request log is present");
        assert!(
            !requests.contains("\"method\":\"turn/prompt\""),
            "opening an uncertain send must not replay the prompt at {cols}x{rows}: {requests}"
        );

        send_ctrl_shift_a(&mut session);
        assert!(session.wait_for("ATTENTION INBOX", Duration::from_secs(5)));
        assert!(session.wait_for("Review · inbox-review", Duration::from_secs(5)));
        session.send(b"\x1b[A");
        session.send(b"\r");
        assert!(session.wait_for("pair-apply", Duration::from_secs(5)));
        let owning_file_probe = "a-first.txt";
        assert!(
            session.wait_for_raw(owning_file_probe, Duration::from_secs(5)),
            "proposal context did not focus its file at {cols}x{rows}\n{}",
            session.output_tail()
        );

        session.send(b"\x03");
        assert!(
            session.wait_for("UNSAVED CHANGES", Duration::from_secs(5)),
            "Ctrl-C did not open the review editor's exit confirmation at {cols}x{rows}\n{}",
            session.output_tail()
        );
        assert!(!session.wait_for_exit(Duration::from_millis(300)));
        session.clear_output();
        session.send(b"\x1b");
        assert!(!session.wait_for_exit(Duration::from_millis(300)));
        session.clear_output();
        session.send(b"\x03");
        assert!(session.wait_for("UNSAVED CHANGES", Duration::from_secs(3)));
        session.send(b"q");
        assert!(
            session.wait_for("UNSAVED EDITOR CHANGES", Duration::from_secs(3)),
            "Q did not open final discard-and-quit confirmation at {cols}x{rows}\n{}",
            session.output_tail()
        );
        session.send(b"\r");
        assert!(
            session.wait_for_exit(Duration::from_secs(5)),
            "confirmed exit did not finish cleanly at {cols}x{rows}\n{}",
            session.output_tail()
        );
        let _ = std::fs::remove_file(fake_core);
        let _ = std::fs::remove_dir_all(root);
    }
}

#[test]
fn project_thread_picker_is_discoverable_at_desktop_and_compact_sizes() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }

    for (cols, rows) in [(80, 24), (50, 16)] {
        let root = git_workspace_root();
        seed_project_threads(&root);
        let fake_core = fake_core_binary(&root);
        let mut session =
            Session::start_with_core_binary(&binary(), &root, cols, rows, Some(&fake_core));
        assert!(session.wait_for("GLASS", Duration::from_secs(8)));
        if cols <= 50 {
            session.trust_project_with_probe("trusted project");
        } else {
            session.trust_project();
        }
        assert!(session.wait_for("✓ Ready", Duration::from_secs(8)));

        send_ctrl_shift_t(&mut session);
        assert!(
            session.wait_for_visible("PROJECT THREADS", Duration::from_secs(5)),
            "thread picker did not open at {cols}x{rows}\n{}",
            session.output_tail()
        );
        assert!(
            session.wait_for_visible("First thread · completed", Duration::from_secs(5)),
            "readable thread title/status missing at {cols}x{rows}\n{}",
            session.output_tail()
        );
        assert!(
            session.wait_for_visible("Second thread · completed", Duration::from_secs(5)),
            "second thread missing at {cols}x{rows}\n{}",
            session.output_tail()
        );
        session.clear_output();
        session.send(b"\x1b[B\r");
        assert!(
            session.wait_for_visible("Second thread · completed", Duration::from_secs(5)),
            "Enter did not select the highlighted thread at {cols}x{rows}\n{}",
            session.output_tail()
        );
        assert!(
            !session.visible_output().contains("PROJECT THREADS"),
            "Enter left the thread picker open at {cols}x{rows}\n{}",
            session.output_tail()
        );
        assert!(
            !session.visible_output().contains("Sending"),
            "thread selection dispatched a prompt at {cols}x{rows}\n{}",
            session.output_tail()
        );
        session.send(b"\x1b");
        session.settle(Duration::from_millis(200));
        session.kill();
        let _ = std::fs::remove_file(fake_core);
        let _ = std::fs::remove_dir_all(root);
    }
}

#[test]
fn live_core_response_crash_reopens_as_uncertain_without_replay() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = git_workspace_root();
    let fixture_state = root.join(".glass");
    std::fs::create_dir_all(&fixture_state).unwrap();
    std::fs::write(fixture_state.join("e2e-core-request-log"), "enabled").unwrap();
    std::fs::write(fixture_state.join("e2e-core-recovery-gate"), "enabled").unwrap();
    let mut fixture_child = FixtureChildGuard::new(fixture_state.join("e2e-core-recovery-pid"));
    let fake_core = fake_core_binary(&root);
    let prompt = "live-response-crash-window";
    let mut session = Session::start_with_core_binary(&binary(), &root, 80, 24, Some(&fake_core));
    assert!(session.wait_for("GLASS", Duration::from_secs(8)));
    session.trust_project();
    assert!(session.wait_for("✓ Ready", Duration::from_secs(8)));
    session.send(b":agent spawn crash-owner recovery check\r");
    assert!(
        session.wait_for("Spawned agent-0001", Duration::from_secs(5)),
        "resident agent did not spawn\n{}",
        session.output_tail()
    );
    send_ctrl_shift_t(&mut session);
    assert!(session.wait_for("PROJECT THREADS", Duration::from_secs(5)));
    assert!(session.wait_for("crash-owner", Duration::from_secs(5)));
    session.send(b"\r\r");
    assert!(session.wait_for("CONVERSATION", Duration::from_secs(5)));
    session.send(prompt.as_bytes());
    session.send(b"\r");

    let started = fixture_state.join("e2e-core-recovery-started");
    let deadline = Instant::now() + Duration::from_secs(12);
    while !started.exists() && Instant::now() < deadline {
        session.settle(Duration::from_millis(50));
    }
    assert!(
        started.exists(),
        "fake Core did not enter the live response window\n{}",
        session.output_tail()
    );
    let registry_path = fixture_state.join("threads/registry.json");
    let deadline = Instant::now() + Duration::from_secs(8);
    let working_registry = loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        if registry["threads"][0]["sendAttempts"][0]["status"] == "working"
            || Instant::now() >= deadline
        {
            break registry;
        }
        session.settle(Duration::from_millis(50));
    };
    assert_eq!(
        working_registry["threads"][0]["sendAttempts"][0]["status"], "working",
        "the interruption must happen after Glass persisted an active delivery"
    );
    assert!(
        fixture_state.join("e2e-core-recovery-pid").exists(),
        "fixture recorded its exact child pid"
    );

    // Kill the Glass UI first, leaving the runtime blocked before its response;
    // then stop only this test's fake Core child so it cannot outlive the case.
    session.kill();
    fixture_child.terminate();

    let mut reopened = Session::start_with_core_binary(&binary(), &root, 80, 24, Some(&fake_core));
    assert!(reopened.wait_for("GLASS", Duration::from_secs(8)));
    reopened.trust_project();
    assert!(reopened.wait_for("✓ Ready", Duration::from_secs(8)));
    send_ctrl_shift_a(&mut reopened);
    assert!(reopened.wait_for("ATTENTION INBOX", Duration::from_secs(5)));
    assert!(
        reopened.wait_for("Message delivery uncertain", Duration::from_secs(8)),
        "the interrupted live send was not classified for inspection\n{}",
        reopened.output_tail()
    );
    reopened.send(b"\r");
    assert!(
        reopened.wait_for("Message delivery is uncertain", Duration::from_secs(5)),
        "attention item did not open its owning conversation\n{}",
        reopened.output_tail()
    );

    let registry: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
    let attempts = registry["threads"][0]["sendAttempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0]["status"], "uncertain");
    assert!(
        attempts[0]["text"]
            .as_str()
            .is_some_and(|text| text.contains(prompt)),
        "uncertain attempt lost the user prompt: {registry}"
    );
    let requests = std::fs::read_to_string(fixture_state.join("e2e-core-requests.jsonl"))
        .expect("fake Core request log is present");
    assert_eq!(
        requests
            .lines()
            .filter(|line| {
                serde_json::from_str::<serde_json::Value>(line)
                    .ok()
                    .is_some_and(|request| request["method"] == "turn/prompt")
            })
            .count(),
        1,
        "reopening must not replay the interrupted request"
    );
    reopened.kill();
    let _ = std::fs::remove_file(fake_core);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn uncertain_send_opens_attention_without_replaying_after_restart() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }

    let prompt = "inspect marker";
    for (cols, rows) in [(80, 24), (50, 16)] {
        let root = git_workspace_root();
        let data_home = root.join(".glass/xdg-data");
        let fixture_state = root.join(".glass");
        std::fs::create_dir_all(&fixture_state).unwrap();
        std::fs::write(fixture_state.join("e2e-core-request-log"), "enabled").unwrap();
        seed_uncertain_send_attempt(&root, prompt);
        let fake_core = fake_core_binary(&root);
        let mut session = Session::start_with_data_home_options(
            &binary(),
            &root,
            cols,
            rows,
            "glass-core",
            Some(&fake_core),
            Some(&data_home),
        );
        assert!(session.wait_for("GLASS", Duration::from_secs(8)));
        if cols <= 50 {
            session.trust_project_with_probe("trusted project");
        } else {
            session.trust_project();
        }
        assert!(session.wait_for("✓ Ready", Duration::from_secs(8)));
        send_ctrl_shift_a(&mut session);
        assert!(
            session.wait_for("ATTENTION INBOX", Duration::from_secs(5)),
            "reopened unresolved attempt was not reachable from the attention inbox at {cols}x{rows}\n{}",
            session.output_tail()
        );
        assert!(
            session.wait_for("Message delivery uncertain", Duration::from_secs(8)),
            "uncertain delivery was not shown after restart at {cols}x{rows}\n{}",
            session.output_tail()
        );
        assert!(
            session.wait_for(prompt, Duration::from_secs(5)),
            "attention item omitted the message needed for inspection at {cols}x{rows}\n{}",
            session.output_tail()
        );
        session.send(b"\r");
        assert!(
            session.wait_for("Message delivery is uncertain", Duration::from_secs(5)),
            "activating the attention item did not select its owning conversation at {cols}x{rows}\n{}",
            session.output_tail()
        );

        let registry: serde_json::Value = serde_json::from_slice(
            &std::fs::read(fixture_state.join("threads/registry.json")).unwrap(),
        )
        .unwrap();
        let attempts = registry["threads"][0]["sendAttempts"].as_array().unwrap();
        assert_eq!(attempts.len(), 1);
        assert_eq!(
            attempts[0]["attemptId"],
            "attempt-00000000000000000000000000000001"
        );
        assert_eq!(attempts[0]["clientMessageId"], "client-e2e-uncertain");
        assert_eq!(attempts[0]["text"], prompt);
        assert_eq!(attempts[0]["status"], "uncertain");

        let log = std::fs::read_to_string(fixture_state.join("e2e-core-requests.jsonl"))
            .expect("fake Core request log is present");
        let requests = log
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            requests
                .iter()
                .filter(|request| request["method"] == "turn/prompt")
                .count(),
            0,
            "opening an uncertain attempt must never replay the prompt"
        );
        session.kill();
        let _ = std::fs::remove_file(fake_core);
        let _ = std::fs::remove_dir_all(root);
    }
}

#[test]
fn uncertain_send_retry_requires_confirmation_and_reuses_client_message_id() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let prompt = "inspect marker before retry";
    let root = git_workspace_root();
    let data_home = root.join(".glass/xdg-data");
    let fixture_state = root.join(".glass");
    std::fs::create_dir_all(&fixture_state).unwrap();
    std::fs::write(fixture_state.join("e2e-core-request-log"), "enabled").unwrap();
    std::fs::write(
        fixture_state.join("e2e-core-preserve-resume-session"),
        "enabled",
    )
    .unwrap();
    seed_uncertain_send_attempt(&root, prompt);
    let fake_core = fake_core_binary(&root);
    let mut session = Session::start_with_data_home_options(
        &binary(),
        &root,
        80,
        24,
        "glass-core",
        Some(&fake_core),
        Some(&data_home),
    );
    assert!(session.wait_for("GLASS", Duration::from_secs(8)));
    session.trust_project();
    assert!(session.wait_for("✓ Ready", Duration::from_secs(8)));
    send_ctrl_shift_a(&mut session);
    assert!(session.wait_for("ATTENTION INBOX", Duration::from_secs(5)));
    assert!(session.wait_for(prompt, Duration::from_secs(8)));
    session.send(b"\r");
    assert!(session.wait_for("Message delivery is uncertain", Duration::from_secs(5)));

    session.send(b":agent resume\r");
    assert!(
        session.wait_for("CONFIRMATION", Duration::from_secs(5)),
        "resuming the exact Core session did not ask for its normal confirmation\n{}",
        session.output_tail()
    );
    session.send(b"\r");
    let registry_path = fixture_state.join("threads/registry.json");
    let deadline = Instant::now() + Duration::from_secs(12);
    let registry = loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        let latest_is_idle = registry["threads"][0]["runs"]
            .as_array()
            .and_then(|runs| runs.last())
            .is_some_and(|run| run["classification"] == "idle");
        if latest_is_idle || Instant::now() >= deadline {
            break registry;
        }
        session.settle(Duration::from_millis(100));
    };
    assert_eq!(
        registry["threads"][0]["runs"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["classification"],
        "idle",
        "Core session did not become ready for an explicit retry: {registry}"
    );

    session.send(b":agent retry\r");
    assert!(
        session.wait_for("RETRY UNCERTAIN DELIVERY", Duration::from_secs(5)),
        "retry did not show duplicate-risk confirmation\n{}",
        session.output_tail()
    );
    assert!(session.wait_for("same client message ID", Duration::from_secs(5)));
    session.send(b"\x1b");
    let requests_path = fixture_state.join("e2e-core-requests.jsonl");
    let requests = std::fs::read_to_string(&requests_path).unwrap();
    assert_eq!(requests.matches("\"method\":\"turn/prompt\"").count(), 0);
    let before_retry: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
    assert_eq!(
        before_retry["threads"][0]["sendAttempts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    session.send(b":agent retry\r");
    assert!(session.wait_for("RETRY UNCERTAIN DELIVERY", Duration::from_secs(5)));
    session.send(b"\r");
    let deadline = Instant::now() + Duration::from_secs(8);
    let after_retry = loop {
        let registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        let attempts = registry["threads"][0]["sendAttempts"].as_array().unwrap();
        if attempts.len() == 2 && attempts[1]["status"] == "settled" || Instant::now() >= deadline {
            break registry;
        }
        session.settle(Duration::from_millis(100));
    };
    let attempts = after_retry["threads"][0]["sendAttempts"]
        .as_array()
        .unwrap();
    assert_eq!(
        attempts.len(),
        2,
        "retry creates a distinct delivery record"
    );
    assert_eq!(attempts[0]["status"], "uncertain");
    assert_eq!(attempts[1]["status"], "settled");
    assert_eq!(
        attempts[0]["clientMessageId"],
        attempts[1]["clientMessageId"]
    );
    assert_ne!(attempts[0]["attemptId"], attempts[1]["attemptId"]);
    assert_eq!(attempts[1]["text"], prompt);
    assert!(
        attempts[1]["engineRequestId"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );
    let requests = std::fs::read_to_string(requests_path).unwrap();
    assert_eq!(requests.matches("\"method\":\"turn/prompt\"").count(), 1);
    assert!(
        requests.contains(prompt),
        "Core did not receive the saved retry text"
    );
    session.kill();
    let _ = std::fs::remove_file(fake_core);
    let _ = std::fs::remove_dir_all(root);
}
#[test]
fn palette_and_help_survive_real_key_sequences() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let mut session = Session::start(&binary(), &root, 118, 32);
    session.trust_project();
    assert!(
        session.wait_for_raw("trusted project · guarded", Duration::from_secs(5)),
        "workspace trust did not complete"
    );
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
    session.clear_output();
    session.send(b"\r");
    assert!(
        session.wait_for_raw("Glass Core is the resident engine", Duration::from_secs(5)),
        "arrow selection did not route to the Glass Core setup guidance\n{}",
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
        session.wait_for_raw("Readiness doctor", Duration::from_secs(5)),
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
    session.trust_project();
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
    session.trust_project();
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
    session.trust_project();
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
    session.trust_project();
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
    session.trust_project();
    assert!(
        session.wait_for("CONVERSATION", Duration::from_secs(5)),
        "Agent surface never rendered after trust"
    );

    session.send(b":");
    assert!(
        session.wait_for("Compose message", Duration::from_secs(5)),
        "Agent command center entries missing"
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
        (b'8', "ROUTES"),
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
    let fake_core = fake_core_binary(&root);
    let mut session = Session::start_with_core_binary(&binary(), &root, 118, 32, Some(&fake_core));
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "first frame never rendered"
    );
    session.trust_project();
    assert!(
        session.wait_for_raw("trusted project · guarded", Duration::from_secs(5)),
        "Agent surface never rendered after trust"
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
    let _ = std::fs::remove_file(fake_core);
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
    session.trust_project();
    assert!(
        session.wait_for_visible("CONVERSATION", Duration::from_secs(5)),
        "Agent surface never rendered after trust\n{}",
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

#[test]
fn code_editor_accepts_and_saves_typed_text_through_pty() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    std::fs::create_dir_all(root.join("src")).expect("create source directory");
    let source = root.join("src/main.rs");
    std::fs::write(&source, "fn main() {}\n").expect("write source file");
    let mut session = Session::start(&binary(), &root, 100, 28);
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "first frame never rendered"
    );
    session.trust_project();
    session.send(b"2");
    assert!(
        session.wait_for_visible("FILES", Duration::from_secs(5)),
        "Code surface did not render\n{}",
        session.output_tail()
    );
    session.send_slowly(b":open src/main.rs\r", Duration::from_millis(35));
    assert!(
        session.wait_for_visible("EDITOR", Duration::from_secs(5)),
        "file did not open in the editor\n{}",
        session.output_tail()
    );
    session.send(b"i");
    assert!(
        session.wait_for_visible("INSERT", Duration::from_secs(3)),
        "editor did not enter insert mode\n{}",
        session.output_tail()
    );
    session.send_slowly(b"// PTY saved\r", Duration::from_millis(35));
    session.send(b"\x13");
    assert!(
        session.wait_for_visible("saved", Duration::from_secs(5)),
        "Ctrl-S did not save the buffer\n{}",
        session.output_tail()
    );
    session.kill();
    assert_eq!(
        std::fs::read_to_string(&source).expect("read saved source"),
        "// PTY saved\nfn main() {}\n"
    );
    std::fs::remove_dir_all(root).expect("remove temporary workspace");
}

#[test]
fn git_stage_and_commit_work_in_an_isolated_local_repository() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = git_workspace_root();
    std::fs::write(root.join(".git/info/exclude"), ".glass/\n")
        .expect("exclude the fixture trust store from Git changes");
    let mut session = Session::start(&binary(), &root, 110, 30);
    assert!(
        session.wait_for("GLASS", Duration::from_secs(8)),
        "first frame never rendered"
    );
    session.trust_project();
    session.send(b"6");
    assert!(
        session.wait_for_visible("a-first.txt", Duration::from_secs(8)),
        "Git surface did not load the isolated repository\n{}",
        session.output_tail()
    );

    session.send_slowly(b":git stage .\r", Duration::from_millis(30));
    assert!(
        session.wait_for("one-use confirmation required", Duration::from_secs(5)),
        "staging did not request confirmation\n{}",
        session.output_tail()
    );
    session.send(b"y");
    assert!(
        session.wait_for("glass.git.stage", Duration::from_secs(8)),
        "staging did not report its result\n{}",
        session.output_tail()
    );
    let staged = Command::new("git")
        .current_dir(&root)
        .args(["diff", "--cached", "--name-only"])
        .output()
        .expect("inspect staged files");
    assert!(staged.status.success());
    assert_eq!(
        String::from_utf8_lossy(&staged.stdout).lines().count(),
        2,
        "the TUI stage action should stage both modified fixture files"
    );

    session.send_slowly(b":git commit PTY local commit\r", Duration::from_millis(30));
    assert!(
        session.wait_for("one-use confirmation required", Duration::from_secs(5)),
        "commit did not request confirmation\n{}",
        session.output_tail()
    );
    session.send(b"y");
    assert!(
        session.wait_for("glass.git.commit", Duration::from_secs(8)),
        "commit did not report its result\n{}",
        session.output_tail()
    );
    session.kill();

    let commit = Command::new("git")
        .current_dir(&root)
        .args(["log", "-1", "--format=%s"])
        .output()
        .expect("inspect local commit");
    assert!(commit.status.success());
    assert_eq!(
        String::from_utf8_lossy(&commit.stdout).trim(),
        "PTY local commit"
    );
    let status = Command::new("git")
        .current_dir(&root)
        .args(["status", "--porcelain"])
        .output()
        .expect("inspect local repository status");
    assert!(status.status.success());
    assert!(
        status.stdout.is_empty(),
        "the local commit should leave a clean tree"
    );
    std::fs::remove_dir_all(root).expect("remove temporary Git workspace");
}
