#![cfg(target_os = "linux")]

use std::{
    io::Write,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[test]
fn tui_enters_and_leaves_a_real_terminal_cleanly() {
    let binary = env!("CARGO_BIN_EXE_glass");
    let command = format!("exec {}", shell_quote(binary));
    let mut child = Command::new("script")
        .args(["-qec", &command, "/dev/null"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("util-linux script must be installed for the Linux PTY smoke test");

    child
        .stdin
        .take()
        .expect("script stdin must be piped")
        .write_all(b"q")
        .expect("quit key must reach the pseudo-terminal");

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().expect("PTY child status must be readable") {
            assert!(
                status.success(),
                "TUI did not leave the pseudo-terminal cleanly: {status}"
            );
            break;
        }
        if Instant::now() >= deadline {
            child.kill().expect("timed-out PTY child must be killable");
            let _ = child.wait();
            panic!("TUI did not handle the quit key within five seconds");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
