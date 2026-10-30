#![cfg(unix)]

use std::{
    fs,
    io::{Read, Write},
    os::fd::{AsRawFd, FromRawFd},
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct PtySession {
    master: fs::File,
    child: Child,
    output: Vec<u8>,
    root: std::path::PathBuf,
    data_dir: std::path::PathBuf,
}

impl PtySession {
    fn start() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "glass-core-quit-pty-{}-{stamp}",
            std::process::id()
        ));
        Self::spawn(root, None)
    }

    fn resume(root: std::path::PathBuf, session_id: &str) -> Self {
        Self::spawn(root, Some(session_id))
    }

    fn spawn(root: std::path::PathBuf, resume: Option<&str>) -> Self {
        let home = root.join("home");
        let config = root.join("config");
        let data = root.join("data");
        let cache = root.join("cache");
        let state = root.join("state");
        let runtime = root.join("runtime");
        let workspace = root.join("workspace");
        for path in [&home, &config, &data, &cache, &state, &runtime, &workspace] {
            fs::create_dir_all(path).expect("create isolated PTY directory");
        }
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))
            .expect("restrict PTY runtime directory");

        let mut master = -1;
        let mut slave = -1;
        let size = libc::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: both output pointers point to initialized local integers and
        // the optional name/termios pointers are null as permitted by openpty.
        let result = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
            )
        };
        assert_eq!(result, 0, "openpty failed");
        // SAFETY: openpty returned fresh owned file descriptors.
        let master = unsafe { fs::File::from_raw_fd(master) };
        // SAFETY: openpty returned a distinct fresh owned file descriptor.
        let slave = unsafe { fs::File::from_raw_fd(slave) };

        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut command = Command::new(env!("CARGO_BIN_EXE_glass-core"));
        if let Some(session_id) = resume {
            command.args(["resume", session_id]);
        }
        let child = command
            .env_clear()
            .env("PATH", path)
            .current_dir(&workspace)
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config)
            .env("XDG_DATA_HOME", &data)
            .env("XDG_CACHE_HOME", &cache)
            .env("XDG_STATE_HOME", &state)
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("TERM", "xterm-256color")
            .env("NO_COLOR", "1")
            .env("RUST_LOG", "off")
            .stdin(Stdio::from(slave.try_clone().expect("clone PTY stdin")))
            .stdout(Stdio::from(slave.try_clone().expect("clone PTY stdout")))
            .stderr(Stdio::from(slave))
            .spawn()
            .expect("start glass-core in isolated PTY");

        // The test reads through poll, so keep the master blocking.
        let _ = master.as_raw_fd();
        let mut session = Self {
            master,
            child,
            output: Vec::new(),
            root,
            data_dir: data.join("glass-core"),
        };
        session.wait_for("glass-core", Duration::from_secs(12));
        session
    }

    fn send(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).expect("write PTY input");
        self.master.flush().expect("flush PTY input");
    }

    fn wait_for(&mut self, needle: &str, timeout: Duration) -> usize {
        let start = self.output.len();
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if String::from_utf8_lossy(&self.output[start..]).contains(needle) {
                return start;
            }
            let mut fd = libc::pollfd {
                fd: self.master.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: fd points to one valid pollfd for the duration of poll.
            let ready = unsafe { libc::poll(&mut fd, 1, 100) };
            if ready <= 0 {
                if self.child.try_wait().expect("poll child").is_some() {
                    break;
                }
                continue;
            }
            let mut bytes = [0u8; 4096];
            match self.master.read(&mut bytes) {
                Ok(0) => break,
                Ok(count) => self.output.extend_from_slice(&bytes[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => panic!("read PTY output: {error}"),
            }
        }
        assert!(
            String::from_utf8_lossy(&self.output[start..]).contains(needle),
            "PTY did not display {needle:?}; output tail: {}",
            String::from_utf8_lossy(&self.output[self.output.len().saturating_sub(3000)..])
        );
        start
    }

    fn wait_exit(&mut self, timeout: Duration) -> std::process::ExitStatus {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait().expect("poll child") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "glass-core did not exit after confirmation"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn idle_ctrl_c_and_ctrl_d_share_the_confirm_cancel_and_exit_prompt() {
    let mut pty = PtySession::start();
    let root = pty.root.clone();
    let data_dir = pty.data_dir.clone();
    pty.send(b"draft to preserve");
    pty.send(&[0x03]);
    let first_prompt = pty.wait_for("QUIT?", Duration::from_secs(5));
    assert!(String::from_utf8_lossy(&pty.output[first_prompt..]).contains("Enter quit · Esc stay"));

    pty.send(&[0x1b]);
    let cancel_frame = pty.wait_for("draft to preserve", Duration::from_secs(5));
    assert!(String::from_utf8_lossy(&pty.output[cancel_frame..]).contains("draft to preserve"));

    pty.send(&[0x04]);
    let second_prompt = pty.wait_for("QUIT?", Duration::from_secs(5));
    assert!(
        String::from_utf8_lossy(&pty.output[second_prompt..]).contains("Enter quit · Esc stay")
    );

    pty.send(&[0x1b]);
    pty.wait_for("draft to preserve", Duration::from_secs(5));
    pty.send(&[0x03]);
    pty.wait_for("QUIT?", Duration::from_secs(5));
    pty.send(b"\r");
    assert!(pty.wait_exit(Duration::from_secs(5)).success());

    let draft_path = find_extension(&data_dir, "draft").expect("quit saves a session draft");
    assert_eq!(
        fs::read_to_string(&draft_path).unwrap(),
        "draft to preserve"
    );
    let session_id = draft_path
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let session_log = draft_path.with_extension("jsonl");
    let log = fs::read_to_string(session_log).expect("conversation log remains available");
    assert!(
        !log.contains("draft to preserve"),
        "draft is not recorded as a user turn"
    );

    let mut resumed = PtySession::resume(root, &session_id);
    resumed.wait_for("draft to preserve", Duration::from_secs(5));
    resumed.send(&[0x03]);
    resumed.wait_for("QUIT?", Duration::from_secs(5));
    resumed.send(b"\r");
    assert!(resumed.wait_exit(Duration::from_secs(5)).success());
}

fn find_extension(root: &std::path::Path, extension: &str) -> Option<std::path::PathBuf> {
    for entry in fs::read_dir(root).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_extension(&path, extension) {
                return Some(found);
            }
        } else if path.extension().is_some_and(|value| value == extension) {
            return Some(path);
        }
    }
    None
}
