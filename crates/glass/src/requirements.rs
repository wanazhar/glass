//! Admin requirements: forced settings user and project config cannot
//! override.
//!
//! Ported from codex's `allow_managed_hooks_only` / `requirements.toml`
//! pattern, scoped to the settings an operator can meaningfully force in
//! glass-core: the sandbox policy and credential-scrub switch. Project
//! settings cannot relax user restrictions, so this layer sits above every
//! other source: whatever `requirements.toml` says wins, and corruption
//! fails closed (a corrupted file forces nothing and warns — it never
//! silently passes a setting through as if it were an operator choice).
//!
//! Unknown keys are skipped with a warning rather than rejected: the point
//! of the layer is a small, auditable force list, not a second schema.

use std::path::Path;

use crate::sandbox;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Requirements {
    /// Forced `sandbox` policy, when the file specified one.
    pub sandbox: Option<sandbox::Policy>,
    /// Forced `sandbox_env` value.
    pub sandbox_env: Option<String>,
}

/// Load `requirements.toml` from the config directory. A missing file is
/// the normal no-requirements case; a corrupted file is refused loudly
/// (the layer forces nothing) rather than guessed at.
pub fn load(dir: &Path) -> Result<Requirements, String> {
    let path = dir.join("requirements.toml");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Requirements::default()),
        Err(e) => return Err(format!("requirements.toml unreadable: {e}")),
    };

    #[derive(serde::Deserialize)]
    struct Raw {
        sandbox: Option<String>,
        sandbox_env: Option<String>,
        allow_managed_hooks_only: Option<bool>,
    }
    let raw: Raw = toml::from_str(&text)
        .map_err(|e| format!("requirements.toml parse: {e}"))?;

    let mut req = Requirements::default();
    if let Some(v) = raw.sandbox {
        req.sandbox = Some(sandbox::Policy::parse(&v).ok_or_else(|| {
            format!("requirements.toml: sandbox must be off, workspace, or strict (got `{v}`)")
        })?);
    }
    if let Some(v) = raw.sandbox_env {
        if v != "scrub" && v != "inherit" && !v.is_empty() {
            return Err(format!(
                "requirements.toml: sandbox_env must be empty, scrub, or inherit (got `{v}`)"
            ));
        }
        req.sandbox_env = Some(v);
    }
    // Recognized but intentionally unused: glass-core has no hook
    // config layers yet. Accepted so a codex-style file is not a parse
    // error, and ignored so nothing pretends to be enforced here.
    let _ = raw.allow_managed_hooks_only;
    Ok(req)
}

/// Apply requirements over an already-assembled config. Returns a
/// human-readable list of what was forced, for the settings provenance
/// view and the startup banner when `GLASS_REQUIREMENTS=1`.
pub fn apply(req: &Requirements, cfg: &mut crate::config::Config) -> Vec<String> {
    let mut forced = Vec::new();
    if let Some(policy) = req.sandbox
        && cfg.sandbox != policy {
        let name = match policy {
            sandbox::Policy::Off => "off",
            sandbox::Policy::Workspace => "workspace",
            sandbox::Policy::Strict => "strict",
        };
        forced.push(format!("sandbox forced to `{name}`"));
        cfg.sandbox = policy;
    }
    if let Some(env) = &req.sandbox_env
        && cfg.sandbox_env != *env {
        forced.push(format!("sandbox_env forced to `{env}`"));
        cfg.sandbox_env = env.clone();
    }
    forced
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("gc-req-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_is_the_normal_no_requirements_case() {
        let dir = scratch("missing");
        assert_eq!(load(&dir).unwrap(), Requirements::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn forced_sandbox_beats_everything() {
        let dir = scratch("force");
        std::fs::write(dir.join("requirements.toml"), "sandbox = \"strict\"\n").unwrap();
        let req = load(&dir).unwrap();
        let mut cfg = crate::config::Config {
            sandbox: sandbox::Policy::Off,
            ..Default::default()
        };
        let forced = apply(&req, &mut cfg);
        assert_eq!(cfg.sandbox, sandbox::Policy::Strict);
        assert!(forced.iter().any(|l| l.contains("sandbox forced to `strict`")), "{forced:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_change_forces_nothing() {
        let dir = scratch("noop");
        std::fs::write(dir.join("requirements.toml"), "sandbox = \"off\"\n").unwrap();
        let req = load(&dir).unwrap();
        let mut cfg = crate::config::Config {
            sandbox: sandbox::Policy::Off,
            ..Default::default()
        };
        assert!(apply(&req, &mut cfg).is_empty());
        assert_eq!(cfg.sandbox, sandbox::Policy::Off);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_fails_closed() {
        let dir = scratch("corrupt");
        std::fs::write(dir.join("requirements.toml"), "sandbox = [not toml").unwrap();
        let err = load(&dir).unwrap_err();
        assert!(err.contains("parse"), "{err}");
        // A failed load must apply nothing.
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_value_fails_closed_not_to_the_user_choice() {
        let dir = scratch("invalid");
        std::fs::write(dir.join("requirements.toml"), "sandbox = \"permissive\"\n").unwrap();
        let err = load(&dir).unwrap_err();
        assert!(err.contains("off, workspace, or strict"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_recognized_keys_do_not_break_the_file() {
        let dir = scratch("hooks");
        std::fs::write(
            dir.join("requirements.toml"),
            "allow_managed_hooks_only = true\n",
        )
        .unwrap();
        let req = load(&dir).unwrap();
        assert_eq!(req, Requirements::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
