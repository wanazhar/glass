#![cfg(all(unix, feature = "native-engine"))]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BROWSER: &str = env!("CARGO_BIN_EXE_glass-browser");
const CONTENT_WORKER: &str = env!("CARGO_BIN_EXE_glass-native-content-worker");

struct SessionCleanup {
    config_home: PathBuf,
    name: String,
}

impl Drop for SessionCleanup {
    fn drop(&mut self) {
        let _ = cli(
            &self.config_home,
            &["--browser-runtime", "native", "session", "stop", &self.name],
        );
    }
}

fn cli(config_home: &Path, args: &[&str]) -> Output {
    Command::new(BROWSER)
        .env("GLASS_CONFIG_HOME", config_home)
        .env("GLASS_NATIVE_CONTENT_WORKER", CONTENT_WORKER)
        .args(args)
        .output()
        .expect("browser CLI should start")
}

fn proc_group_and_session(pid: u32) -> (i32, i32) {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .expect("persistent browser owner should remain alive");
    let (_, fields) = stat
        .rsplit_once(')')
        .expect("process stat should contain a command field");
    let fields = fields.split_whitespace().collect::<Vec<_>>();
    let process_group = fields[2].parse().expect("valid process group ID");
    let session = fields[3].parse().expect("valid session ID");
    (process_group, session)
}

fn owner_status_during_command(config_home: &Path, name: &str) -> Option<serde_json::Value> {
    use std::os::unix::net::UnixStream;

    let socket = config_home
        .join("glass/sessions")
        .join(format!("{name}.sock"));
    let mut stream = UnixStream::connect(socket).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(1))).ok()?;
    stream.write_all(b"{\"op\":\"status\"}\n").ok()?;
    let mut response = String::new();
    BufReader::new(stream).read_line(&mut response).ok()?;
    serde_json::from_str(&response).ok()
}

fn assert_success(output: &Output, action: &str) {
    assert!(
        output.status.success(),
        "{action} failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_owner_survives_caller_group_cleanup_and_keeps_cross_call_dom() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the epoch")
        .as_nanos();
    let config_home = std::env::temp_dir().join(format!(
        "glass-native-session-lifecycle-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&config_home).unwrap();
    let name = format!("lifecycle-{}", std::process::id());
    let cleanup = SessionCleanup {
        config_home: config_home.clone(),
        name: name.clone(),
    };

    // Simulate the isolated process group that Core's bash tool owns. The
    // named browser owner must not inherit it, because successful shell exit
    // kills any remaining members of that group.
    let mut start = Command::new(BROWSER);
    start
        .env("GLASS_CONFIG_HOME", &config_home)
        .env("GLASS_NATIVE_CONTENT_WORKER", CONTENT_WORKER)
        .args([
            "--browser-runtime",
            "native",
            "--policy-allow",
            "evaluate",
            "session",
            "start",
            &name,
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .process_group(0);
    let child = start.spawn().expect("session start command should start");
    let caller_group = child.id();
    let started = child
        .wait_with_output()
        .expect("session start should finish");
    assert_success(&started, "session start");

    let status = cli(
        &config_home,
        &["--browser-runtime", "native", "session", "status", &name],
    );
    assert_success(&status, "session status after start");
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["state"], "running");
    let owner_pid = status["pid"].as_u64().unwrap() as u32;
    let (owner_group, owner_session) = proc_group_and_session(owner_pid);
    assert_ne!(owner_group, caller_group as i32);
    assert_eq!(owner_session, owner_pid as i32);

    // This is the exact group cleanup performed after a Core bash command
    // returns. It must not signal the explicitly managed browser owner.
    unsafe {
        libc::kill(-(caller_group as i32), libc::SIGKILL);
    }
    std::thread::sleep(Duration::from_millis(40));

    let navigate = cli(
        &config_home,
        &[
            "--browser-runtime",
            "native",
            "--session",
            &name,
            "navigate",
            "about:blank",
        ],
    );
    assert_success(&navigate, "attached navigate");

    let evaluate = cli(
        &config_home,
        &[
            "--browser-runtime",
            "native",
            "--session",
            &name,
            "evaluate",
            "document.body.textContent = 'persistent native session marker'; true",
        ],
    );
    assert_success(&evaluate, "attached evaluate");

    let text = cli(
        &config_home,
        &["--browser-runtime", "native", "--session", &name, "text"],
    );
    assert_success(&text, "separate attached text call");
    assert!(
        String::from_utf8_lossy(&text.stdout).contains("persistent native session marker"),
        "the next CLI process must read the DOM written by the previous one: {}",
        String::from_utf8_lossy(&text.stdout)
    );

    let observe = cli(
        &config_home,
        &[
            "--browser-runtime",
            "native",
            "--session",
            &name,
            "observe",
            "--level",
            "interactive",
        ],
    );
    assert_success(&observe, "separate attached observation");

    let command_error = cli(
        &config_home,
        &[
            "--browser-runtime",
            "native",
            "--session",
            &name,
            "click",
            "id=missing-target",
        ],
    );
    assert!(
        !command_error.status.success(),
        "a missing target should return a command error"
    );
    let after_error = cli(
        &config_home,
        &["--browser-runtime", "native", "session", "status", &name],
    );
    assert_success(&after_error, "session status after command error");
    let after_error: serde_json::Value = serde_json::from_slice(&after_error.stdout).unwrap();
    assert_eq!(after_error["state"], "running");

    let mut waiting = Command::new(BROWSER);
    waiting
        .env("GLASS_CONFIG_HOME", &config_home)
        .env("GLASS_NATIVE_CONTENT_WORKER", CONTENT_WORKER)
        .args([
            "--browser-runtime",
            "native",
            "--session",
            &name,
            "wait",
            "js=false",
            "--timeout-ms",
            "10000",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0);
    let mut waiting = waiting.spawn().expect("attached wait should start");
    let caller_group = waiting.id();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let mut active = false;
    while std::time::Instant::now() < deadline {
        if owner_status_during_command(&config_home, &name)
            .is_some_and(|status| status.get("activeNavigationRevision").is_some())
        {
            active = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(active, "long wait should be executing in the native owner");
    let killed = unsafe { libc::kill(-(caller_group as i32), libc::SIGKILL) };
    assert_eq!(
        killed, 0,
        "the isolated attached-command group should be live"
    );
    assert!(
        !waiting.wait().expect("cancelled CLI should exit").success(),
        "the attached CLI should be cancelled"
    );

    let after_cancel = cli(
        &config_home,
        &["--browser-runtime", "native", "session", "status", &name],
    );
    assert_success(&after_cancel, "session status after cancelled command");
    let after_cancel: serde_json::Value = serde_json::from_slice(&after_cancel.stdout).unwrap();
    assert_eq!(after_cancel["state"], "running");

    let after_cancel_text = cli(
        &config_home,
        &["--browser-runtime", "native", "--session", &name, "text"],
    );
    assert_success(&after_cancel_text, "attached text after cancelled command");
    assert!(
        String::from_utf8_lossy(&after_cancel_text.stdout)
            .contains("persistent native session marker")
    );

    let stop = cli(
        &config_home,
        &["--browser-runtime", "native", "session", "stop", &name],
    );
    assert_success(&stop, "explicit session stop");
    std::thread::sleep(Duration::from_millis(50));
    assert!(!Path::new(&format!("/proc/{owner_pid}")).exists());
    let stopped = cli(
        &config_home,
        &["--browser-runtime", "native", "session", "status", &name],
    );
    assert_success(&stopped, "session status after stop");
    let stopped: serde_json::Value = serde_json::from_slice(&stopped.stdout).unwrap();
    assert_eq!(stopped["state"], "stopped");
    drop(cleanup);
    std::fs::remove_dir_all(config_home).unwrap();
}
