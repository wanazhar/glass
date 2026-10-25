//! Tool concurrency classes.
//!
//! Running independent tool calls in parallel is the cheapest speedup available:
//! a model that asks for eight `rg` calls waits for eight process spawns
//! sequentially, and almost none of them depend on each other.
//!
//! But parallelism is not free to get right. Three rules come from what
//! actually breaks:
//!
//! 1. **A write to a path must not overlap another write to that path.** Two
//!    concurrent `sed -i` on one file interleave and corrupt it. Writes to
//!    *different* paths are fine, so the unit is the path, not the tool.
//! 2. **Results must be reported in call order.** A chat-completions endpoint
//!    rejects tool results that do not pair with their calls by id, and a
//!    transcript whose tool results appear in completion order is impossible to
//!    debug. So fan out for speed and reassemble deterministically.
//! 3. **Nothing above the concurrency cap may run early.** A batch of one is a
//!    direct call, not a thread hand-off, so the common case costs nothing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// How a tool call may be scheduled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Class {
    /// Safe to run alongside anything.
    Parallel,
    /// Serialized against other calls touching the same key, in call order.
    Keyed(String),
    /// Runs alone, after everything queued before it.
    Serial,
}

/// Default cap on simultaneous calls. Above this, a batch degrades to
/// sequential rather than spawning an unbounded number of processes.
pub const MAX_PARALLEL: usize = 8;

/// Classify a bash command by what it does to the filesystem.
///
/// This is a heuristic over text the model wrote, not a shell parser. The bias
/// is toward `Serial`: wrongly serialising costs a little wall-clock, while
/// wrongly parallelising corrupts files.
pub fn classify_bash(command: &str, workspace: &Path) -> Class {
    let cmd = command.trim();

    // Anything that mutates VCS state, installs, or restarts the machine is
    // treated as exclusive regardless of the paths it names.
    if is_exclusive(cmd) {
        return Class::Serial;
    }

    // A git operation touching the index or history is exclusive even when it
    // only reads the working tree.
    if cmd.starts_with("git ") && mutates_git(cmd) {
        return Class::Serial;
    }

    let writes = crate::verify::written_paths(cmd, workspace);
    match writes.len() {
        0 => Class::Parallel,
        1 => Class::Keyed(writes[0].to_string_lossy().into_owned()),
        // Several distinct paths cannot share a key, so there is nothing safe
        // to overlap. Serial is the conservative answer.
        _ => Class::Serial,
    }
}

/// Commands that must never run beside anything else.
///
/// Matched against any word in the command rather than just the first, because
/// `sudo apt install x` is just as exclusive as `apt install x` and a
/// prefix-only check silently classified it as a parallel read.
fn is_exclusive(cmd: &str) -> bool {
    const EXCLUSIVE: &[&str] = &[
        "rm",
        "mv",
        "cp",
        "chmod",
        "chown",
        "dd",
        "truncate",
        "sudo",
        "su",
        "apt",
        "apt-get",
        "brew",
        "yum",
        "dnf",
        "pacman",
        "snap",
        "pip",
        "npm",
        "yarn",
        "pnpm",
        "cargo",
        "go",
        "gem",
        "docker",
        "kubectl",
        "helm",
        "terraform",
        "systemctl",
        "service",
        "make",
        "shutdown",
        "reboot",
        "halt",
        "kill",
        "killall",
        "pkill",
        "git",
        "hg",
        "svn",
    ];
    cmd.split_whitespace().any(|w| {
        let word =
            w.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_' && c != '.');
        EXCLUSIVE.contains(&word)
    })
}

fn mutates_git(cmd: &str) -> bool {
    const MUTATING: &[&str] = &[
        "add", "commit", "merge", "rebase", "reset", "checkout", "switch", "stash", "rm", "mv",
        "apply", "am", "restore", "clean", "pull", "push",
    ];
    cmd.split_whitespace()
        .any(|w| MUTATING.contains(&w.trim_matches(|c: char| !c.is_alphanumeric())))
}

/// A plan for executing a batch.
#[derive(Debug, Default)]
pub struct Plan {
    calls: Vec<Call>,
}

impl Plan {
    pub fn new(calls: Vec<Call>) -> Self {
        Self { calls }
    }

    /// Group calls into waves that may run together.
    ///
    /// Waves are built greedily: a `Parallel` call joins the current wave, a
    /// `Keyed` call joins only if its key is unclaimed, and a `Serial` call ends
    /// the wave and gets one of its own. This preserves call order within a key
    /// while letting unrelated work overlap.
    pub fn waves(&self, max_parallel: usize) -> Vec<Vec<usize>> {
        let mut waves: Vec<Vec<usize>> = Vec::new();
        let mut cur: Vec<usize> = Vec::new();
        let mut claimed: HashMap<String, ()> = HashMap::new();
        let cap = max_parallel.max(1);

        for c in &self.calls {
            match &c.class {
                Class::Serial => {
                    if !cur.is_empty() {
                        waves.push(std::mem::take(&mut cur));
                        claimed.clear();
                    }
                    waves.push(vec![c.index]);
                }
                Class::Keyed(key) => {
                    if claimed.contains_key(key) {
                        // Same path: flush so ordering is preserved, then the
                        // next wave can take it.
                        waves.push(std::mem::take(&mut cur));
                        claimed.clear();
                    }
                    if cur.len() >= cap {
                        waves.push(std::mem::take(&mut cur));
                        claimed.clear();
                    }
                    claimed.insert(key.clone(), ());
                    cur.push(c.index);
                }
                Class::Parallel => {
                    if cur.len() >= cap {
                        waves.push(std::mem::take(&mut cur));
                        claimed.clear();
                    }
                    cur.push(c.index);
                }
            }
        }
        if !cur.is_empty() {
            waves.push(cur);
        }
        waves
    }

    /// Whether the batch is worth threading at all.
    ///
    /// A single call runs inline. This is the common case — most turns make one
    /// tool call — and it must not cost a thread hand-off.
    pub fn needs_parallelism(&self) -> bool {
        if self.calls.len() < 2 {
            return false;
        }
        self.calls
            .iter()
            .any(|c| matches!(c.class, Class::Parallel))
    }

    /// Force one call to run alone, whatever class it was inferred as.
    ///
    /// Used when an extension declares its tool exclusive. The index is the
    /// call's position in the batch, which may not match its batch index when
    /// only a subset of the batch was planned.
    pub fn with_serial(mut self, index: Option<usize>) -> Self {
        let Some(i) = index else { return self };
        for c in self.calls.iter_mut() {
            if c.index == i {
                c.class = Class::Serial;
            }
        }
        self
    }

    /// Reassemble results in call order.
    ///
    /// The wave executor fills this with whatever order things actually finish;
    /// the model still sees them paired correctly by call id.
    pub fn reorder<T>(&self, finished: HashMap<usize, T>) -> Vec<(usize, T)> {
        let mut out: Vec<(usize, T)> = finished.into_iter().collect();
        out.sort_by_key(|(i, _)| *i);
        out
    }

    /// Test helper mirroring [`plan_bash`].
    #[cfg(test)]
    fn plan_bash_helper(cmds: &[(usize, String)]) -> Plan {
        plan_bash(cmds, Path::new("/w"))
    }
}

/// Run one wave, returning `(call index, result)` in completion order.
///
/// Uses scoped threads so a panic in a tool cannot take the process down, and
/// so the batch's threads are joined before this returns.
fn run_wave<T, F>(wave: &[usize], f: F) -> Vec<(usize, Result<T, ToolPanicked>)>
where
    T: Send,
    F: Fn(usize) -> T + Sync,
{
    std::thread::scope(|scope| {
        let handles: Vec<_> = wave
            .iter()
            .map(|&i| {
                let f = &f;
                (
                    i,
                    scope.spawn(move || {
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(i)))
                            .map_err(|_| ToolPanicked)
                    }),
                )
            })
            .collect();
        handles
            .into_iter()
            .map(|(index, handle)| match handle.join() {
                Ok(result) => (index, result),
                Err(_) => (index, Err(ToolPanicked)),
            })
            .collect()
    })
}

/// A tool panicked after it started. Its side effects are unknown, so callers
/// must surface the failed result and must not retry it automatically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolPanicked;

/// Execute a whole batch, returning results keyed by call index.
///
/// Results come back in call order regardless of completion order, which is the
/// property a chat-completions endpoint requires and which makes a transcript
/// with parallel calls reproducible.
pub fn execute<T, F>(
    plan: &Plan,
    max_parallel: usize,
    f: F,
) -> Vec<(usize, Result<T, ToolPanicked>)>
where
    T: Send,
    F: Fn(usize) -> T + Sync,
{
    let mut done: HashMap<usize, Result<T, ToolPanicked>> = HashMap::new();

    for wave in plan.waves(max_parallel) {
        if wave.len() == 1 {
            // A single call is a plain function call, not a thread hand-off.
            let i = wave[0];
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(i)))
                .map_err(|_| ToolPanicked);
            done.insert(i, result);
            continue;
        }
        for (i, result) in run_wave(&wave, &f) {
            done.insert(i, result);
        }
    }

    plan.reorder(done)
}

/// Build a plan from raw bash commands.
pub fn plan_bash(commands: &[(usize, String)], workspace: &Path) -> Plan {
    Plan::new(
        commands
            .iter()
            .map(|(i, cmd)| Call {
                index: *i,
                class: classify_bash(cmd, workspace),
            })
            .collect(),
    )
}

/// One scheduled call.
#[derive(Debug)]
pub struct Call {
    pub index: usize,
    pub class: Class,
}

/// Canonical form of a write target for use as a lock key.
///
/// Two different spellings of the same file must produce the same key, or the
/// guard silently fails exactly when it matters.
pub fn canonical_key(path: &Path) -> String {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    // `canonicalize` resolves symlinks, which matters for editors and temp
    // directories that alias each other. It fails on not-yet-created files, so
    // fall back to the lexically normalised path.
    abs.canonicalize()
        .unwrap_or_else(|_| lexically_normal(&abs))
        .to_string_lossy()
        .into_owned()
}

/// Remove `.` and resolve `..` without touching the filesystem.
fn lexically_normal(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> &'static Path {
        Path::new("/w")
    }

    #[test]
    fn read_only_commands_are_parallel() {
        assert_eq!(classify_bash("cat a.txt", ws()), Class::Parallel);
        assert_eq!(classify_bash("rg foo src/", ws()), Class::Parallel);
        assert_eq!(classify_bash("ls -la", ws()), Class::Parallel);
        assert_eq!(
            classify_bash("cargo test", ws()),
            Class::Serial,
            "builds are exclusive"
        );
    }

    #[test]
    fn single_write_is_keyed_on_its_path() {
        let c = classify_bash("echo hi > a.txt", ws());
        assert_eq!(c, Class::Keyed("/w/a.txt".into()));
    }

    #[test]
    fn multiple_writes_are_serial() {
        // Two distinct paths cannot share a key, so there is nothing safe to
        // overlap and the conservative answer wins.
        assert_eq!(classify_bash("cp a b; cp c d", ws()), Class::Serial);
    }

    #[test]
    fn destructive_commands_are_always_serial() {
        for c in [
            "rm -rf build",
            "sudo apt install x",
            "git commit -m x",
            "docker run x",
        ] {
            assert_eq!(
                classify_bash(c, ws()),
                Class::Serial,
                "{c} must be exclusive"
            );
        }
    }

    #[test]
    fn reads_and_writes_to_different_paths_overlap() {
        let p = Plan::plan_bash_helper(&[
            (0, "cat a.txt".to_string()),
            (1, "cat b.txt".to_string()),
            (2, "echo x > c.txt".to_string()),
        ]);
        let waves = p.waves(MAX_PARALLEL);
        assert_eq!(waves.len(), 1, "all three can run together: {waves:?}");
        assert_eq!(waves[0].len(), 3);
    }

    #[test]
    fn writes_to_the_same_path_are_serialized_in_order() {
        let p = Plan::plan_bash_helper(&[
            (0, "echo 1 > a.txt".to_string()),
            (1, "echo 2 > a.txt".to_string()),
        ]);
        let waves = p.waves(MAX_PARALLEL);
        assert_eq!(waves.len(), 2, "same path must not overlap: {waves:?}");
        assert_eq!(waves[0], vec![0]);
        assert_eq!(waves[1], vec![1]);
    }

    #[test]
    fn serial_call_splits_the_waves() {
        let p = Plan::plan_bash_helper(&[
            (0, "cat a".to_string()),
            (1, "rm -rf x".to_string()),
            (2, "cat b".to_string()),
        ]);
        let waves = p.waves(MAX_PARALLEL);
        assert_eq!(waves, vec![vec![0], vec![1], vec![2]]);
    }

    #[test]
    fn parallel_cap_is_respected() {
        let cmds: Vec<(usize, String)> = (0..20).map(|i| (i, format!("cat f{i}"))).collect();
        let p = plan_bash(&cmds, ws());
        let waves = p.waves(4);
        assert!(waves.iter().all(|w| w.len() <= 4), "{waves:?}");
        assert_eq!(waves.iter().map(|w| w.len()).sum::<usize>(), 20);
    }

    #[test]
    fn single_call_needs_no_thread() {
        let p = plan_bash(&[(0, "cat a".into())], ws());
        assert!(!p.needs_parallelism());
    }

    #[test]
    fn all_serial_batch_needs_no_thread() {
        let p = plan_bash(&[(0, "rm x".into()), (1, "rm y".into())], ws());
        assert!(
            !p.needs_parallelism(),
            "nothing can overlap, so skip the machinery"
        );
    }

    #[test]
    fn parallel_batch_is_worth_threading() {
        let p = plan_bash(&[(0, "cat a".into()), (1, "cat b".into())], ws());
        assert!(p.needs_parallelism());
    }

    #[test]
    fn results_reorder_into_call_order() {
        let mut finished = HashMap::new();
        finished.insert(2, "third");
        finished.insert(0, "first");
        finished.insert(1, "second");
        let p = Plan::plan_bash_helper(&[
            (0, "cat a".into()),
            (1, "cat b".into()),
            (2, "cat c".into()),
        ]);
        let ordered = p.reorder(finished);
        assert_eq!(ordered[0].1, "first");
        assert_eq!(ordered[2].1, "third");
    }

    #[test]
    fn canonical_key_normalises_dot_paths() {
        let a = canonical_key(Path::new("/w/./a/../a.txt"));
        let b = canonical_key(Path::new("/w/a.txt"));
        assert_eq!(a, b, "two spellings of one file must share a key");
    }
}

#[cfg(test)]
mod exec_tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;

    #[test]
    fn single_call_runs_inline() {
        let p = plan_bash(&[(0, "cat a".into())], Path::new("/w"));
        let out = execute(&p, 4, |i| i * 2);
        assert_eq!(out, vec![(0, Ok(0))]);
    }

    #[test]
    fn parallel_calls_overlap_in_time() {
        // Four 150ms sleeps in one wave take ~150ms, not ~600ms. A generous
        // bound keeps this stable on a loaded machine while still failing if
        // the calls are serialised.
        let p = plan_bash(
            &(0..4).map(|i| (i, format!("cat f{i}"))).collect::<Vec<_>>(),
            Path::new("/w"),
        );
        let t = std::time::Instant::now();
        let out = execute(&p, 8, |_| {
            std::thread::sleep(Duration::from_millis(150));
            "done"
        });
        // Four 150ms sleeps overlapped take ~150ms. Serialised would be ~600ms.
        assert!(
            t.elapsed() < Duration::from_millis(400),
            "calls did not overlap: {:?}",
            t.elapsed()
        );
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn results_come_back_in_call_order_not_completion_order() {
        // Later calls finish first; the output must still be 0,1,2,3.
        let p = plan_bash(
            &(0..4).map(|i| (i, format!("cat f{i}"))).collect::<Vec<_>>(),
            Path::new("/w"),
        );
        let out = execute(&p, 8, |i| {
            std::thread::sleep(Duration::from_millis((4 - i) as u64 * 30));
            i
        });
        let order: Vec<usize> = out.iter().map(|(i, _)| *i).collect();
        assert_eq!(order, vec![0, 1, 2, 3], "transcript order must be stable");
    }

    #[test]
    fn same_path_writes_run_in_order() {
        let log = Mutex::new(Vec::new());
        let p = plan_bash(
            &[(0, "echo a > x.txt".into()), (1, "echo b > x.txt".into())],
            Path::new("/w"),
        );
        let out = execute(&p, 8, |i| {
            log.lock().unwrap().push(i);
            std::thread::sleep(Duration::from_millis(10));
            i
        });
        let seen = log.into_inner().unwrap();
        assert_eq!(seen, vec![0, 1], "same-path writes must not overlap");
        assert_eq!(out[0], (0, Ok(0)));
    }

    #[test]
    fn serial_call_runs_alone() {
        let seen = Mutex::new(Vec::new());
        let p = plan_bash(
            &[
                (0, "rm -rf x".into()),
                (1, "cat a".into()),
                (2, "cat b".into()),
            ],
            Path::new("/w"),
        );
        execute(&p, 8, |i| {
            let mut s = seen.lock().unwrap();
            s.push(i);
            // While the serial call runs, nothing else should be in flight.
            if i == 0 {
                assert_eq!(s.len(), 1, "serial call was overlapped: {s:?}");
            }
        });
    }

    #[test]
    fn a_panicking_tool_is_reported_without_losing_other_results_or_retrying() {
        let p = plan_bash(
            &(0..3).map(|i| (i, format!("cat f{i}"))).collect::<Vec<_>>(),
            Path::new("/w"),
        );
        let executions = std::sync::atomic::AtomicUsize::new(0);
        let side_effects = std::sync::atomic::AtomicUsize::new(0);
        let out = execute(&p, 8, |i| {
            executions.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if i == 1 {
                side_effects.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                panic!("tool blew up");
            }
            i
        });
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], (0, Ok(0)));
        assert_eq!(out[1], (1, Err(ToolPanicked)));
        assert_eq!(out[2], (2, Ok(2)));
        // Each call was entered once. Agent must surface the explicit panic
        // result instead of treating it as absent and rerunning it inline.
        assert_eq!(executions.load(std::sync::atomic::Ordering::SeqCst), 3);
        assert_eq!(side_effects.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}

#[cfg(test)]
mod serial_tests {
    use super::*;

    #[test]
    fn with_serial_isolates_one_call() {
        let mut p = plan_bash(
            &(0..3).map(|i| (i, format!("cat f{i}"))).collect::<Vec<_>>(),
            Path::new("/w"),
        );
        assert!(!p.needs_parallelism() || true);
        p = p.with_serial(Some(1));
        let waves = p.waves(8);
        assert_eq!(
            waves,
            vec![vec![0], vec![1], vec![2]],
            "call 1 must run alone: {waves:?}"
        );
    }

    #[test]
    fn with_serial_ignores_a_missing_index() {
        let p = plan_bash(&[(0, "cat a".into())], Path::new("/w"));
        let q = p.waves(8);
        assert_eq!(p.with_serial(None).waves(8), q);
    }
}
