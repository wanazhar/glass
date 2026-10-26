use std::process::Command;

#[test]
fn model_help_explains_interactive_selection_without_requiring_a_tty() {
    let binary = env!("CARGO_BIN_EXE_glass-core");
    for command in ["model", "models"] {
        let output = Command::new(binary)
            .arg(command)
            .arg("--help")
            .current_dir(std::env::temp_dir())
            .output()
            .expect("run static help without a terminal");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "{command} --help should explain its disposition; stderr: {stderr}"
        );
        assert!(
            stdout.contains("There is no standalone model command"),
            "{stdout}"
        );
        assert!(stdout.contains("/model"), "{stdout}");
        assert!(!stderr.contains("No interactive terminal"), "{stderr}");
    }
}
