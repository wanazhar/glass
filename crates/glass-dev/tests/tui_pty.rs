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

fn binary() -> String {
    std::env::var("GLASS_E2E_BINARY").unwrap_or_else(|_| "target/release/glass".to_string())
}

#[test]
fn ctrl_c_quits_from_every_input_mode() {
    if std::env::var("GLASS_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let root = workspace_root();
    let bin = binary();

    // Plain navigation mode.
    let mut session = Session::start(&bin, &root, 100, 28);
    session.send(b"\x03");
    session.settle(Duration::from_millis(600));
    assert!(
        session.child.try_wait().unwrap_or(None).is_some(),
        "Ctrl+C must quit from navigation mode"
    );
    session.kill();

    // Composer mode: Ctrl+C must quit, not insert a literal 'c'.
    let mut session = Session::start(&bin, &root, 100, 28);
    session.send(b"i");
    session.settle(Duration::from_millis(400));
    session.send(b"\x03");
    session.settle(Duration::from_millis(600));
    assert!(
        session.child.try_wait().unwrap_or(None).is_some(),
        "Ctrl+C must quit from composer mode"
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
        session.wait_for("Composer", Duration::from_secs(5)),
        "action menu entries missing"
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
    session.settle(Duration::from_millis(500));
    session.send(b"help");
    assert!(
        session.wait_for("agent", Duration::from_secs(5)),
        "palette suggestions missing"
    );
    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));
    session.send(b"\x1b");
    session.settle(Duration::from_millis(300));
    // Surfaces switch with number keys and each renders its header.
    for (key, header) in [('2', "FILES"), ('3', "APP"), ('4', "Task")] {
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
    session.kill();
}
