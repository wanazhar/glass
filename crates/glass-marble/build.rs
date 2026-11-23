use std::path::Path;
use std::process::Command;

fn git(directory: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(directory)
        .args(arguments)
        .output()
        .ok()?;
    let value = String::from_utf8(output.stdout).ok()?;
    let value = value.trim();
    (output.status.success() && !value.is_empty()).then(|| value.to_string())
}

pub fn git_commit(directory: &Path) -> Option<String> {
    let root = std::path::PathBuf::from(git(directory, &["rev-parse", "--show-toplevel"])?);
    let root = root.canonicalize().ok()?;
    if directory.canonicalize().ok()? != root.join("crates/glass-marble")
        || !directory.join("Cargo.toml").is_file()
    {
        return None;
    }
    git(directory, &["rev-parse", "--verify", "HEAD"]).filter(|sha| {
        matches!(sha.len(), 40 | 64) && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PATH");
    let directory = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let commit = git_commit(&directory).unwrap_or_else(|| "unknown".into());
    let short: String = commit.chars().take(12).collect();
    println!("cargo:rustc-env=MARBLE_GIT_COMMIT={commit}");
    println!("cargo:rustc-env=MARBLE_GIT_COMMIT_SHORT={short}");
    let reference = git(&directory, &["symbolic-ref", "-q", "HEAD"]);
    for reference in [Some("HEAD"), Some("packed-refs"), reference.as_deref()]
        .into_iter()
        .flatten()
    {
        if let Some(path) = git(&directory, &["rev-parse", "--git-path", reference]) {
            let path = directory.join(path);
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}
