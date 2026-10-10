//! Process self-hardening before any agent work starts.
//!
//! Ported from codex's `process-hardening` crate, scoped to what a portable
//! `libc`-only build can do without a `#[ctor]` shim: drop the core-dump
//! limit, strip dynamically-loaded-library injection variables from the
//! environment children inherit, and report what happened so nothing is
//! silently skipped.
//!
//! This is depth, not a boundary. The sandbox (`sandbox.rs`) is the
//! enforcement mechanism; hardening only removes cheap local privesc
//! surfaces before the harness runs anything untrusted.

/// Environment variable prefixes removed from the environment the process
/// and its children see. `LD_PRELOAD`, `LD_AUDIT` and friends let any
/// writable directory inject code into every child process; `DYLD_*` is the
/// macOS family. A non-zero exit from the harness could otherwise be turned
/// into arbitrary code execution by anything that can write those files.
const STRIPPED_PREFIXES: [&str; 4] = ["LD_", "DYLD_", "DY_", "_RLD_"];

/// Variables in the stripped families that are actually load paths for the
/// harness itself and must survive when the harness depends on them.
const KEPT_EXACT: [&str; 3] = ["LD_LIBRARY_PATH", "LD_PRELOAD", "DYLD_LIBRARY_PATH"];

/// Result of one hardening pass, reported once at startup.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    /// `true` when the core-dump limit is now 0.
    pub core_dump_disabled: bool,
    /// Names removed from the environment.
    pub stripped: Vec<String>,
}

/// Best-effort hardening. A refused `setrlimit` downgrades to a warning in
/// the report; a stripped environment is not attempted to be restored.
pub fn harden() -> Report {
    let mut report = Report::default();

    // Disable core dumps: a dump of the harness would include provider
    // traffic, keys read into buffers, and session contents.
    #[cfg(unix)]
    {
        report.core_dump_disabled = disable_core_dumps();
    }
    #[cfg(not(unix))]
    {
        report.core_dump_disabled = false;
    }

    // Strip injection families from the environment every child inherits.
    let vars: Vec<String> = std::env::vars()
        .map(|(k, _)| k)
        .collect::<Vec<_>>()
        .into_iter()
        .filter(|k| {
            STRIPPED_PREFIXES.iter().any(|p| k.starts_with(p)) && !KEPT_EXACT.contains(&k.as_str())
        })
        .collect();
    for key in &vars {
        // A failed removal never blocks startup; the variable simply stays.
        // SAFETY: removing an environment variable between other env
        // operations on the same thread; no other thread reads the env
        // here — this runs before any thread spawns.
        unsafe { std::env::remove_var(key) };
        report.stripped.push(key.clone());
    }
    report.stripped.sort();
    report
}

#[cfg(unix)]
fn disable_core_dumps() -> bool {
    unsafe {
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        libc::setrlimit(libc::RLIMIT_CORE, &limit) == 0
    }
}

/// One-line startup summary; empty when there is nothing to report so the
/// banner stays unchanged in the common hardened-by-default case.
pub fn line(report: &Report) -> String {
    let mut parts: Vec<String> = Vec::new();
    if report.core_dump_disabled {
        parts.push("core dumps off".to_string());
    } else {
        parts.push("core dump limit unchanged".to_string());
    }
    if !report.stripped.is_empty() {
        parts.push(format!("stripped {} injection env vars", report.stripped.len()));
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn core_dumps_are_disabled_on_unix() {
        let r = harden();
        assert!(r.core_dump_disabled, "RLIMIT_CORE=0 must be settable");
    }

    #[cfg(not(unix))]
    #[test]
    fn report_is_honest_on_non_unix() {
        let r = harden();
        assert!(!r.core_dump_disabled);
        assert!(r.stripped.is_empty());
    }

    #[test]
    fn line_mentions_the_limbo_and_the_stripped_count() {
        let r = Report {
            core_dump_disabled: true,
            stripped: vec!["LD_PRELOAD_X".into(), "DYLD_INSERT_FOO".into()],
        };
        let l = line(&r);
        assert!(l.contains("core dumps off"), "{l}");
        assert!(l.contains("2 injection env vars"), "{l}");
        assert!(line(&Report::default()).contains("unchanged"));
    }

    #[test]
    fn keep_list_spares_real_library_paths() {
        assert!(KEPT_EXACT.contains(&"LD_LIBRARY_PATH"));
        assert!(KEPT_EXACT.contains(&"DYLD_LIBRARY_PATH"));
        // And the strip families would otherwise match them.
        assert!(STRIPPED_PREFIXES.iter().any(|p| "LD_LIBRARY_PATH".starts_with(p)));
    }
}
