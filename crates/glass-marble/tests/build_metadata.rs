#[allow(dead_code)]
#[path = "../build.rs"]
mod build_script;

use std::path::Path;
use std::process::Command;

fn git(directory: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(directory)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[test]
fn metadata_rejects_archives_inside_another_checkout() {
    let root = std::env::temp_dir().join(format!("marble-build-metadata-{}", std::process::id()));
    let workspace = root.join("workspace");
    let directory = workspace.join("crates/glass-marble");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("Cargo.toml"),
        "[package]\nname = 'glass-marble'\n",
    )
    .unwrap();
    git(&workspace, &["init", "--quiet", "--initial-branch=main"]);
    git(&workspace, &["add", "crates/glass-marble/Cargo.toml"]);
    git(
        &workspace,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    );
    let commit = git(&directory, &["rev-parse", "HEAD"]);
    assert_eq!(build_script::git_commit(&directory), Some(commit.clone()));

    let archive = workspace.join("vendor/archive/crates/glass-marble");
    std::fs::create_dir_all(&archive).unwrap();
    std::fs::copy(directory.join("Cargo.toml"), archive.join("Cargo.toml")).unwrap();
    assert_eq!(git(&archive, &["rev-parse", "HEAD"]), commit);
    assert_eq!(
        build_script::git_commit(&archive),
        None,
        "archive borrowed the outer checkout's commit"
    );

    let standalone = root.join("archive/crates/glass-marble");
    std::fs::create_dir_all(&standalone).unwrap();
    std::fs::copy(directory.join("Cargo.toml"), standalone.join("Cargo.toml")).unwrap();
    assert_eq!(build_script::git_commit(&standalone), None);

    std::fs::remove_file(directory.join("Cargo.toml")).unwrap();
    assert_eq!(build_script::git_commit(&directory), None);
    std::fs::remove_dir_all(root).unwrap();
}
