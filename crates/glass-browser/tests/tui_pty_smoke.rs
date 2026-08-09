#![cfg(target_os = "linux")]

use std::{
    fs::File,
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

mod support;

#[test]
fn phone_tui_renders_and_leaves_a_real_terminal_cleanly() {
    let (mut master, slave) = open_pty();
    let controlling_terminal = slave.as_raw_fd();
    let mut command = Command::new(support::glass_binary());
    command
        .args(["--tui-layout", "mobile", "--tui-live", "off"])
        .env("TERM", "xterm-256color")
        .stdin(Stdio::from(
            slave.try_clone().expect("PTY slave must clone"),
        ))
        .stdout(Stdio::from(
            slave.try_clone().expect("PTY slave must clone"),
        ))
        .stderr(Stdio::from(
            slave.try_clone().expect("PTY slave must clone"),
        ));
    // SAFETY: this closure calls only async-signal-safe libc functions between
    // fork and exec. The PTY descriptor remains open in the child at this point.
    unsafe {
        command.pre_exec(move || {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::ioctl(controlling_terminal, libc::TIOCSCTTY, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .expect("Glass must start on the pseudo-terminal");
    drop(slave);
    thread::sleep(Duration::from_millis(250));
    master
        .write_all(b"q")
        .expect("quit key must reach the pseudo-terminal");

    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().expect("PTY child status must be readable") {
            break status;
        }
        if Instant::now() >= deadline {
            // Glass called setsid, so its PID is also the process-group ID.
            // SAFETY: the negative PID targets only the child process group.
            unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
            let _ = child.wait();
            panic!("TUI did not handle the quit key within five seconds");
        }
        thread::sleep(Duration::from_millis(20));
    };
    assert!(
        status.success(),
        "TUI did not leave the pseudo-terminal cleanly: {status}"
    );

    let output = read_available(&mut master);
    assert_sequence(&output, b"\x1b[?1049h", "enter alternate screen");
    assert_sequence(&output, b"\x1b[?25l", "hide cursor");
    assert_sequence(&output, b"GLASS", "phone cockpit title");
    assert_sequence(&output, b"Overview", "phone overview card");
    assert_sequence(&output, b"COMMAND", "printable command composer");
    assert_sequence(&output, b"\x1b[?1049l", "leave alternate screen");
    assert_sequence(&output, b"\x1b[?25h", "show cursor");
}

fn open_pty() -> (File, File) {
    let mut master = -1;
    let mut slave = -1;
    let size = libc::winsize {
        ws_row: 20,
        ws_col: 40,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: all pointers refer to initialized storage valid for this call.
    let result = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            &size,
        )
    };
    assert_eq!(result, 0, "openpty failed: {}", io::Error::last_os_error());
    // SAFETY: openpty returned two newly owned descriptors.
    unsafe { (File::from_raw_fd(master), File::from_raw_fd(slave)) }
}

fn read_available(master: &mut File) -> Vec<u8> {
    let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
    assert!(flags >= 0, "PTY flags must be readable");
    assert_eq!(
        unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
        0
    );
    let mut output = Vec::new();
    loop {
        let mut chunk = [0_u8; 4096];
        match master.read(&mut chunk) {
            Ok(0) => break,
            Ok(length) => output.extend_from_slice(&chunk[..length]),
            Err(error) if matches!(error.kind(), io::ErrorKind::WouldBlock) => break,
            Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
            Err(error) => panic!("PTY output must be readable: {error}"),
        }
    }
    output
}

fn assert_sequence(output: &[u8], expected: &[u8], label: &str) {
    assert!(
        output
            .windows(expected.len())
            .any(|window| window == expected),
        "missing {label} sequence"
    );
}
