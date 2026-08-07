use std::{path::PathBuf, process::Command, sync::OnceLock};

pub fn glass_binary() -> PathBuf {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            if let Some(path) = std::env::var_os("CARGO_BIN_EXE_glass-browser") {
                return PathBuf::from(path);
            }

            let test_executable =
                std::env::current_exe().expect("test executable path should be available");
            let target_debug = test_executable
                .parent()
                .and_then(|deps| deps.parent())
                .expect("test executable should live under target/debug/deps");
            let binary_name = if cfg!(windows) {
                "glass-browser.exe"
            } else {
                "glass-browser"
            };
            let path = target_debug.join(binary_name);
            if !path.exists() {
                let status = Command::new("cargo")
                    .args(["build", "-p", "glass-browser", "--locked"])
                    .current_dir(env!("CARGO_MANIFEST_DIR"))
                    .status()
                    .expect("cargo should be available to build glass-dev");
                assert!(
                    status.success(),
                    "glass-browser should build for integration tests"
                );
            }
            path
        })
        .clone()
}
