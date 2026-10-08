//! Delegated tasks: fan-out, isolation, and the nested agent loop.
//!
//! One extension is one serial pipe, so fan-out cannot come from overlapping
//! tool calls — the host must run several nested agents inside a single
//! `run_subagents` request. That is what this module owns: the wave scheduler,
//! the per-task runner (fresh provider, fresh session, fresh context), and
//! optional git-worktree isolation.
//!
//! Delegation is deliberately coarse. A task gets `bash` and `state` and
//! nothing else, because anything else would be the parent conversation
//! leaking into a context that is supposed to be isolated.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::agent::Agent;
use crate::ext;
use crate::ext::protocol::SubagentTask;
use crate::provider::Provider;
use crate::session::{Record, Session};
use crate::tools;

/// How many nested agents run at once.
///
/// More tasks than this queue in waves. The cap exists because every
/// nested agent holds a provider request open and runs real commands;
/// an unbounded fan-out is a fork bomb with extra steps.
pub const MAX_CONCURRENT: usize = 4;

/// Run every task and return one report per task, in request order.
///
/// A task that fails — including one whose worker panics — produces an
/// error string for its own slot and never aborts its siblings. The
/// ordering guarantee matters for the same reason it does for tool calls:
/// the model reads reports against the tasks it delegated.
pub fn run_batch(
    tasks: &[SubagentTask],
    cap: usize,
    f: impl Fn(&SubagentTask) -> String + Sync,
) -> Vec<String> {
    let cap = cap.max(1);
    let mut out = Vec::with_capacity(tasks.len());
    for wave in tasks.chunks(cap) {
        if wave.len() == 1 {
            out.push(f(&wave[0]));
            continue;
        }
        let mut results: Vec<String> = std::thread::scope(|scope| {
            let f = &f;
            let handles: Vec<_> = wave.iter().map(|t| scope.spawn(move || f(t))).collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| "subagent failed: worker panicked".into())
                })
                .collect()
        });
        out.append(&mut results);
    }
    out
}

/// Builds one metered provider, optionally for a named model.
///
/// Boxed rather than generic so the REPL's factory can be handed to the
/// batch runner without threading type parameters through every call site.
pub type MakeProvider<'a> =
    dyn Fn(Option<&str>) -> Result<Box<dyn Provider>, String> + Sync + 'a;

/// Run one delegated task in a fresh context and return its report.
///
/// `make_provider` receives the task's model override; `None` means the
/// active connection's model.
pub fn run_task(
    task: &SubagentTask,
    workspace: &Path,
    scratch: &Path,
    make_provider: &MakeProvider<'_>,
) -> String {
    let result = (|| -> Result<String, String> {
        let provider = make_provider(task.model.as_deref())?;
        let (run_workspace, note) = if task.worktree {
            let dir = worktree(workspace, &scratch.join("worktrees"), &unique_id())?;
            let note = format!("\n\n[isolated worktree: {}]", dir.display());
            (dir, note)
        } else {
            (workspace.to_path_buf(), String::new())
        };
        let mut session = Session::create(scratch).map_err(|e| e.to_string())?;
        session.append(Record::User(task.task.clone()));
        let mut state = tools::State::new();
        let mods: Vec<Box<dyn glass_mod_api::Mod_>> = Vec::new();
        let mut registry = ext::Registry::empty();
        let mut agent = Agent::new(
            provider.as_ref(),
            &mut session,
            &mut state,
            &mods,
            &mut registry,
            &run_workspace,
        )
        .with_prompt(compose_prompt(task), None, None);
        if task.read_only {
            // The inspection tools only: a second-opinion review can look at
            // everything and change nothing.
            agent = agent.with_plan_mode(true);
        }
        match agent.run_turn(|_| {}) {
            Ok((out, _)) => Ok(format!("{}{note}", out.text)),
            Err(e) => Err(e.to_string()),
        }
    })();
    result.unwrap_or_else(|e| format!("subagent failed: {e}"))
}

/// The prompt a delegated task runs with.
///
/// An agent definition body replaces the default framing; the task text
/// is always appended so a custom persona cannot lose the assignment.
pub fn compose_prompt(task: &SubagentTask) -> String {
    let tools = if task.read_only {
        "the read-only inspection tools (list, read, search). You cannot modify anything"
    } else {
        "bash and state"
    };
    if let Some(body) = task
        .prompt
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty())
    {
        return format!(
            "{body}\n\nDelegated task: {task}\n\n\
             Do the task using {tools}. Do not ask the user questions. \
             Reply with a concise report of what you found or changed.",
            task = task.task
        );
    }
    format!(
        "You are a subagent working on a delegated task.\n\n\
         Task: {task}\n\n\
         Do the task using {tools}. Do not ask the user \
         questions. When you are done, reply with a concise report \
         of what you found or changed.",
        task = task.task
    )
}

/// Create a detached worktree at `root/<id>` from the workspace's HEAD.
///
/// The worktree is deliberately left in place after the task: the parent
/// needs somewhere to inspect what the isolated agent changed, and
/// silently deleting a checkout the report points at would be worse than
/// leaving a directory behind. Uncommitted parent changes are not
/// present — a worktree starts from HEAD, which is the point.
pub fn worktree(workspace: &Path, root: &Path, id: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(root).map_err(|e| format!("worktree root: {e}"))?;
    let dir = root.join(id);
    let out = Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(["worktree", "add", "--detach"])
        .arg(&dir)
        .arg("HEAD")
        .output()
        .map_err(|e| format!("git worktree: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "git worktree add failed (is this a git workspace?): {}",
            err.trim()
        ));
    }
    Ok(dir)
}

/// A filesystem-safe unique id, so two worktrees cannot collide.
fn unique_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    format!("{millis:x}-{:x}", SEQ.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;

    fn task(text: &str) -> SubagentTask {
        SubagentTask {
            task: text.into(),
            ..SubagentTask::default()
        }
    }

    #[test]
    fn batch_preserves_request_order_when_later_tasks_finish_first() {
        let tasks: Vec<SubagentTask> = (0..4).map(|i| task(&format!("t{i}"))).collect();
        let out = run_batch(&tasks, 8, |t| {
            // Later tasks finish first; reports must still follow request order.
            let n: u64 = t.task[1..].parse().unwrap();
            std::thread::sleep(Duration::from_millis((4 - n) * 20));
            t.task.clone()
        });
        assert_eq!(out, vec!["t0", "t1", "t2", "t3"]);
    }

    #[test]
    fn batch_runs_waves_concurrently_within_the_cap() {
        let tasks: Vec<SubagentTask> = (0..6).map(|i| task(&format!("t{i}"))).collect();
        let in_flight = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let _ = run_batch(&tasks, 2, |_| {
            let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(20));
            in_flight.fetch_sub(1, Ordering::SeqCst);
            "done".into()
        });
        assert!(
            peak.load(Ordering::SeqCst) <= 2,
            "concurrency cap was exceeded: {peak:?}"
        );
    }

    #[test]
    fn batch_keeps_one_output_per_task_even_when_a_worker_panics() {
        let tasks: Vec<SubagentTask> = (0..3).map(|i| task(&format!("t{i}"))).collect();
        let out = run_batch(&tasks, 3, |t| {
            if t.task == "t1" {
                panic!("worker blew up");
            }
            format!("ok {}", t.task)
        });
        assert_eq!(out.len(), 3, "a panicking worker must not lose its slot");
        assert_eq!(out[0], "ok t0");
        assert!(out[1].contains("panicked"), "{}", out[1]);
        assert_eq!(out[2], "ok t2");
    }

    #[test]
    fn a_single_task_runs_inline() {
        let log = StdMutex::new(Vec::new());
        let tasks = vec![task("solo")];
        let out = run_batch(&tasks, 4, |t| {
            log.lock().unwrap().push(t.task.clone());
            "report".into()
        });
        assert_eq!(out, vec!["report"]);
        assert_eq!(log.into_inner().unwrap(), vec!["solo"]);
    }

    #[test]
    fn an_agent_prompt_precedes_the_task_and_a_default_names_it() {
        let mut t = task("review the parser");
        let default = compose_prompt(&t);
        assert!(default.contains("review the parser"));
        assert!(default.contains("delegated task"), "{default}");

        t.prompt = Some("You are a security reviewer.".into());
        let custom = compose_prompt(&t);
        assert!(custom.starts_with("You are a security reviewer."));
        assert!(custom.contains("review the parser"));
    }

    #[test]
    fn worktree_isolation_creates_a_detached_checkout() {
        let base = std::env::temp_dir().join(format!("gc-wt-{}", std::process::id()));
        let repo = base.join("repo");
        let root = base.join("wts");
        let _ = std::fs::remove_dir_all(&base);
        if std::fs::create_dir_all(&repo).is_err() {
            return;
        }
        let run = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .output()
        };
        if run(&["init", "-q"]).is_err() {
            return; // no git available: skip like the python-dependent tests
        }
        std::fs::write(repo.join("a.txt"), "hi").unwrap();
        let _ = run(&["add", "a.txt"]);
        if !run(&["commit", "-qm", "init"]).unwrap().status.success() {
            return;
        }
        let dir = worktree(&repo, &root, "t1").expect("worktree");
        assert!(dir.join("a.txt").exists(), "HEAD contents must be present");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn worktree_outside_a_repo_fails_cleanly() {
        let base = std::env::temp_dir().join(format!("gc-wt-bare-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let err = worktree(&base, &base.join("wts"), "x").unwrap_err();
        assert!(err.contains("worktree"), "{err}");
        let _ = std::fs::remove_dir_all(&base);
    }
}
