#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn fixture_root() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("glass-cli-forward-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&root).expect("create fixture root");
    root
}

#[test]
fn direct_core_command_keeps_arguments_stdio_and_child_exit_status() {
    let root = fixture_root();
    let glass = root.join("glass");
    fs::copy(env!("CARGO_BIN_EXE_glass"), &glass).expect("copy Glass beside the fake Core");
    let binary = root.join(format!("glass-core{}", std::env::consts::EXE_SUFFIX));
    let arguments = root.join("arguments.txt");
    let standard_input = root.join("stdin.txt");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then printf 'glass-core {}\\n'; exit 0; fi\nprintf '%s\\n' \"$@\" > '{}'\nprintf '%s\\n' \"$GLASS_CORE_CLI_PRESENTATION\" > '{}'\ncat > '{}'\nprintf 'core stdout\\n'\nprintf 'core stderr\\n' >&2\nexit 37\n",
        env!("CARGO_PKG_VERSION"),
        arguments.display(),
        root.join("frontend.txt").display(),
        standard_input.display()
    );
    fs::write(&binary, script).expect("write fake Core executable");
    let mut permissions = fs::metadata(&binary)
        .expect("stat fake Core executable")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&binary, permissions).expect("make fake Core executable");

    let mut child = Command::new(&glass)
        .args(["connect", "--json", "--profile", "fixture profile"])
        .env("GLASS_CORE_CLI_PRESENTATION", "ambient-stale-value")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start Glass CLI");
    child
        .stdin
        .take()
        .expect("Glass stdin pipe")
        .write_all(b"stdin payload from caller")
        .expect("write to forwarded Core stdin");
    let output = child.wait_with_output().expect("wait for forwarded Core");

    assert_eq!(
        output.status.code(),
        Some(37),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "core stdout\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "core stderr\n");
    assert_eq!(
        fs::read_to_string(&arguments).expect("read forwarded arguments"),
        "connect\n--json\n--profile\nfixture profile\n"
    );
    assert_eq!(
        fs::read_to_string(&standard_input).expect("read forwarded stdin"),
        "stdin payload from caller"
    );
    assert_eq!(
        fs::read_to_string(root.join("frontend.txt")).expect("read forwarded presentation"),
        "glass\n"
    );
    fs::remove_dir_all(root).expect("remove fixture root");
}

#[test]
fn glass_help_points_to_core_command_discovery_without_starting_core() {
    let output = Command::new(env!("CARGO_BIN_EXE_glass"))
        .arg("--help")
        .output()
        .expect("run Glass help");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("glass <core-command>"), "{help}");
    assert!(help.contains("glass core <args...>"), "{help}");
}

#[test]
fn core_namespace_forwards_colliding_and_unlisted_commands_verbatim() {
    let root = fixture_root();
    let glass = root.join("glass");
    fs::copy(env!("CARGO_BIN_EXE_glass"), &glass).expect("copy Glass beside the fake Core");
    let binary = root.join(format!("glass-core{}", std::env::consts::EXE_SUFFIX));
    let arguments = root.join("arguments.txt");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then printf 'glass-core {}\\n'; exit 0; fi\nprintf '%s\\n' \"$@\" > '{}'\nprintf '%s\\n' \"$GLASS_CORE_CLI_PRESENTATION\" > '{}.frontend'\nprintf 'core help\\n'\n",
        env!("CARGO_PKG_VERSION"),
        arguments.display(),
        arguments.display()
    );
    fs::write(&binary, script).expect("write fake Core executable");
    let mut permissions = fs::metadata(&binary)
        .expect("stat fake Core executable")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&binary, permissions).expect("make fake Core executable");

    let output = Command::new(&glass)
        .args(["core", "browser", "status", "--future-core-option"])
        .env("GLASS_CORE_CLI_PRESENTATION", "ambient-stale-value")
        .output()
        .expect("run exact Core namespace");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "core help\n");
    assert_eq!(
        fs::read_to_string(&arguments).expect("read forwarded arguments"),
        "browser\nstatus\n--future-core-option\n"
    );
    assert_eq!(
        fs::read_to_string(format!("{}.frontend", arguments.display()))
            .expect("read forwarded presentation"),
        "glass core\n"
    );
    fs::remove_dir_all(root).expect("remove fixture root");
}
