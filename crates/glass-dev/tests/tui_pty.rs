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
    }
    assert_eq!(
        client_ids.len(),
        3,
        "each message needs a distinct client ID"
    );

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
    session.settle(Duration::from_millis(300));
    let visible = session.visible_output();
    assert!(
        visible.contains("gamma"),
        "the selected gamma conversation disappeared after alpha completed\n{visible}"
    );
    assert!(
        !visible.contains("handled alpha-job"),
        "alpha's background response replaced the selected gamma conversation\n{visible}"
    );
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
