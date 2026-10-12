//! Shell execution and shared outcomes for core tool dispatch.

use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use glass_mod_api::ToolCall;

// Process signalling, without pulling in a dependency for two constants and
// one call. Only used on the timeout path.
const SIGTERM: i32 = 15;
const SIGKILL: i32 = 9;

unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

/// Hard ceiling on captured output, per command.
///
/// Tool output dominates context growth. An uncaptured `cat` of a large file
/// can add more tokens than the entire conversation, so truncation is not an
/// optimisation here, it is the difference between a session that survives and
/// one that needs compaction every few turns.
pub const MAX_OUTPUT_BYTES: usize = 60_000;

/// Default and maximum command timeout.
pub const DEFAULT_TIMEOUT_MS: u64 = 120_000;
pub const MAX_TIMEOUT_MS: u64 = 900_000;

pub struct Outcome {
    pub body: String,
    /// Bytes dropped by truncation, reported to the model so it knows the
    /// output it sees is partial rather than complete.
    pub truncated: bool,
    pub exit_code: Option<i32>,
    /// Where the full output was written, when it was too big to inline.
    pub spill: Option<PathBuf>,
}

impl Outcome {
    /// A successful result body returned to the model.
    pub fn ok(body: String) -> Self {
        Self {
            body,
            truncated: false,
            exit_code: Some(0),
            spill: None,
        }
    }

    /// A tool that did not run, with a message for the model.
    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            body: {
                let message = msg.into();
                if message.starts_with("error:") { message } else { format!("error: {message}") }
            },
            truncated: false,
            exit_code: None,
            spill: None,
        }
    }
}

/// Run `call` and capture its output.
///
/// Enforce the startup policy without inheriting another session's exceptions.
pub fn bash(call: &ToolCall, workspace: &std::path::Path) -> Outcome {
    bash_cancellable(call, workspace, &std::sync::atomic::AtomicBool::new(false))
}
pub fn bash_cancellable(
    call: &ToolCall,
    workspace: &std::path::Path,
    cancel: &std::sync::atomic::AtomicBool,
) -> Outcome {
    bash_with_approvals(
        call,
        workspace,
        cancel,
        &crate::sandbox::SessionApprovals::default(),
    )
}

pub fn bash_with_approvals(
    call: &ToolCall,
    workspace: &std::path::Path,
    cancel: &std::sync::atomic::AtomicBool,
    approvals: &crate::sandbox::SessionApprovals,
) -> Outcome {
    bash_owned(call, workspace, cancel, approvals, None)
}

pub fn bash_owned(
    call: &ToolCall,
    workspace: &std::path::Path,
    cancel: &std::sync::atomic::AtomicBool,
    approvals: &crate::sandbox::SessionApprovals,
    owner: Option<&str>,
) -> Outcome {
    if call.arg_raw("job").is_some() {
        if cancel.load(Ordering::Relaxed) {
            return Outcome::err("Cancelled before job control");
        }
        let Some(owner) = owner else {
            return Outcome::err("error: job control requires an owned agent session");
        };
        return match crate::jobs::dispatch(call, workspace, owner, approvals) {
            Ok(v) => Outcome::ok(format!("[exit 0]\n{}",crate::jobs::render(&v))),
            Err(e) => Outcome::err(format!("error: {e}")),
        };
    }
    let Some(command) = call.arg_str("command") else {
        return Outcome::err("error: `command` is required and must be a string".to_string());
    };
    if command.trim().is_empty() {
        return Outcome::err("error: `command` is empty".to_string());
    }
    let exception = approvals.permits_exact(&command);
    if !exception && let Some(reason) = crate::sandbox::refusal() {
        return Outcome::err(format!(
            "error: sandbox = strict is not available ({reason}); refusing to run commands unsandboxed"
        ));
    }

    let cwd = match call.arg_raw("cwd") {
        Some(_) => {
            let Some(rel) = call.arg_str("cwd").filter(|s| !s.trim().is_empty()) else {
                return Outcome::err("`cwd` must be a nonempty directory path string; omit it to use the workspace");
            };
            let joined = workspace.join(&rel);
            if !joined.is_dir() {
                return Outcome::err(format!("`cwd` is not an accessible directory: {}; nothing executed", joined.display()));
            }
            joined
        }
        None => workspace.to_path_buf(),
    };
    if exception
        && !matches!((cwd.canonicalize(), workspace.canonicalize()), (Ok(cwd), Ok(root)) if cwd == root)
    {
        return Outcome::err(
            "error: this exact-command exception applies only in the session workspace; use the default cwd and explicit paths",
        );
    }

    // Models send `timeout_ms` as a JSON number, not a string, so this has to
    // read the raw value. Using `arg_str` here silently returns None and every
    // command would quietly get the default timeout.
    let timeout = match call.arg_raw("timeout_ms") {
        None => DEFAULT_TIMEOUT_MS,
        Some(raw) => {
            let value = serde_json::from_str::<serde_json::Value>(&raw);
            let millis = match value.as_ref().ok() {
                Some(serde_json::Value::Number(n)) => n.as_u64(),
                Some(serde_json::Value::String(s)) => s.parse::<u64>().ok(),
                _ => None,
            };
            let Some(millis) = millis.filter(|n| *n > 0) else {
                return Outcome::err("`timeout_ms` must be a positive integer in milliseconds; nothing executed");
            };
            millis.clamp(100, MAX_TIMEOUT_MS)
        }
    };

    // Approved exceptions must bypass macOS's sandbox-exec wrapper as well as
    // Linux's Landlock pre-exec hook. They apply only to exact command bytes.
    let mut builder = if exception {
        let mut cmd = std::process::Command::new("/bin/sh");
        cmd.arg("-c").arg(&command);
        cmd
    } else {
        crate::sandbox::shell(&command)
    };
    builder
        .current_dir(&cwd)
        // Put the child in its own process group. Killing only the shell
        // leaves `sleep`, `cargo`, and `npm` running: the shell exits but its
        // children keep the stdout pipe open, so we would block on the drain
        // threads until they finished on their own. Signalling the group
        // terminates the whole tree, which is what a timeout has to mean.
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Kernel-level confinement, when a policy is active: applied after fork
    // and before exec so the shell and everything it spawns inherits it.
    if let Err(error) =
        crate::sandbox::harden_command(&mut builder, &command, approvals, exception, false)
    {
        return Outcome::err(format!("error: {error}"));
    }
    crate::sandbox::scrub_env(&mut builder);
    if let Ok(exe) = std::env::current_exe() {
        builder.env("GC_BIN", exe);
    }
    builder.env_remove("GC_AGENT_OWNER");
    if let Some(owner) = owner.filter(|id| crate::team::enabled(id)) {
        builder.env("GC_AGENT_OWNER", owner);
    }
    let mut child = match builder.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Outcome {
                body: format!("error: could not spawn shell: {e}"),
                truncated: false,
                exit_code: None,
                spill: None,
            };
        }
    };

    // Drain both pipes concurrently. Reading stdout to completion before
    // stderr will deadlock as soon as the child fills the stderr pipe buffer,
    // which `cargo build` and `git push` both do routinely.
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out_buf = std::thread::spawn(move || read_capped(&mut stdout));
    let err_buf = std::thread::spawn(move || read_capped(&mut stderr));

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout);
    let mut timed_out = false;
    let mut cancelled = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {
                if cancel.load(std::sync::atomic::Ordering::Acquire) {
                    kill_group(&mut child);
                    cancelled = true;
                    break child.wait().ok();
                }
                if std::time::Instant::now() >= deadline {
                    kill_group(&mut child);
                    timed_out = true;
                    break child.wait().ok();
                }
                // Poll rather than block so the timeout stays responsive.
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(_) => break None,
        }
    };

    // A shell may exit while a background descendant retains the pipes.
    // Foreground calls own their process group; persistent work uses jobs.
    // Kill remaining group members before joining readers, without adding the
    // graceful-shutdown delay to every successful command.
    let descendants_stopped = !cancelled && !timed_out && child.id() > 1
        && unsafe { kill(-(child.id() as i32), SIGKILL) } == 0;

    let (out, out_spill) = out_buf.join().unwrap_or_else(|_| (String::new(), None));
    let (err, err_spill) = err_buf.join().unwrap_or_else(|_| (String::new(), None));

    let mut body = String::new();
    if !out.is_empty() {
        body.push_str(&out);
        if !body.ends_with('\n') {
            body.push('\n');
        }
    }
    if !err.is_empty() {
        if !body.is_empty() && !body.ends_with('\n') {
            body.push('\n');
        }
        body.push_str("stderr:\n");
        body.push_str(&err);
    }

    // A stream that overran the cap left a file behind. Point the model at it
    // rather than discarding the remainder: losing the tail of a build log or a
    // search result is exactly the information the model came for.
    if out_spill.is_some() && let Some(path) = &err_spill {
        body.push_str(&format!("\n[stderr overflow saved to {} — read with `tail` or `rg`]\n", path.display()));
    }
    let spill = out_spill.or(err_spill);
    if let Some(p) = &spill {
        body.push_str(&format!(
            "\n[output truncated at {MAX_OUTPUT_BYTES} bytes; full output saved to {} — \
             read it with `head`, `tail`, or `rg`]\n",
            p.display()
        ));
    }
    if descendants_stopped {
        body.push_str("\n[shell exited; remaining owned background processes stopped; use bash job start for persistent work]\n");
    }
    if cancelled {
        body.push_str("\n[command cancelled; owned process group stopped]\n");
    }
    if timed_out {
        body.push_str(&format!("\n[command killed after {timeout}ms]\n"));
    }
    if body.is_empty() {
        body.push_str("(no output)");
    }

    // Exit code first, always. Interface research is unambiguous that a model
    // without it will re-run commands "to determine the effect of the prior
    // action" — the retry loops show up as duplicated commands in transcripts.
    let code = status.and_then(|s| s.code());
    let mut prefix = match code {
        Some(0) => "[exit 0]\n".to_string(),
        Some(c) => format!("[exit {c}]\n"),
        // Killed by a signal rather than exiting: name it, since the model
        // cannot infer it from output alone.
        None => "[killed by signal]\n".to_string(),
    };
    prefix.push_str(&body);

    // A bash command's writes are untracked at a path level: the conservative
    // cache answer (task six-wave-d-001) is invalidating everything, on
    // every completed command, success or failure (a failed `mv` still
    // Touches unknowable state on some filesystems).
    crate::fs_scan::invalidate_all();

    Outcome {
        truncated: spill.is_some(),
        body: prefix,
        exit_code: code,
        spill,
    }
}

/// Signal the child's entire process group.
///
/// The negative pid is what makes this a group signal rather than a signal to
/// one process. Without it the shell dies on its own but its children keep
/// running and keep the stdout pipe open, so a timed-out `cargo build` would
/// still block the harness until it finished on its own.
///
/// SIGTERM first so a well-behaved process can flush output, then SIGKILL for
/// anything that ignored it. The signal is repeated briefly because a child
/// forked just before the shell died may not have been in the group yet.
fn kill_group(child: &mut Child) {
    // Guard against pid 0 or 1: a negative group id of 0 means "our own
    // group" and -1 means "every process we may signal", neither of which is
    // ever intended here.
    let Some(pid) = (child.id() > 1).then_some(child.id()) else {
        return;
    };
    let pgid = -(pid as i32);

    unsafe {
        kill(pgid, SIGTERM);
    }
    std::thread::sleep(std::time::Duration::from_millis(100));
    // The shell may exit on TERM while a descendant ignores it and keeps
    // stdout open. Signal the owned group before reaping its leader, even
    // when that leader has already exited.
    unsafe {
        kill(pgid, SIGKILL);
    }
    for _ in 0..4 {
        if child.try_wait().ok().flatten().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
}

/// Read a stream, keeping the first [`MAX_OUTPUT_BYTES`] inline and spilling the
/// rest to a temp file.
///
/// Truncating without a spill throws away the part a build log or a search
/// sweep actually ends with, which is the part that says what went wrong. With
/// a spill the model can always recover the tail with `tail` or `rg` instead of
/// re-running the command. Temp files are the OS's to reap, so a crashed session
/// leaves nothing behind that needs cleaning up.
fn read_capped(r: &mut Option<impl Read>) -> (String, Option<PathBuf>) {
    let Some(reader) = r.as_mut() else {
        return (String::new(), None);
    };

    let mut head: Vec<u8> = Vec::new();
    let mut spill: Option<(std::fs::File, PathBuf)> = None;
    let mut chunk = [0u8; 8192];

    loop {
        let n = match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };

        let room = MAX_OUTPUT_BYTES.saturating_sub(head.len());
        if room >= n {
            head.extend_from_slice(&chunk[..n]);
            continue;
        }
        // This chunk straddles the cap: fill the inline budget, spill the rest.
        head.extend_from_slice(&chunk[..room]);
        spill = spill.or_else(open_spill);
        if let Some((f, _)) = spill.as_mut() {
            let _ = f.write_all(&chunk[room..]);
        }
    }

    let mut path = spill.map(|(mut f, p)| {
        let _ = f.flush();
        p
    });

    // Content addressing (task six-wave-d-001, blob-artifacts): once the
    // spill exists, its bytes also land in the content-addressed blob store
    // and the recorded path becomes the `blob:<sha256>` reference. The
    // spill file stays as the materialized copy for this session; a later
    // session or handoff re-reads by content through the reference.
    if let Some(p) = path.take() {
        path = match std::fs::read(&p).ok().and_then(|bytes| crate::blobs::put(&bytes).ok()) {
            Some((reference, _)) => {
                record_blob(reference, &p);
                Some(p)
            }
            None => Some(p),
        };
    }

    (String::from_utf8_lossy(&head).into_owned(), path)
}

/// Side channel from a spill write to its blob reference: one entry per
/// process, read back by the record binder. An entry that never binds is
/// harmless — the plain path remains the read path.
pub fn blob_reference_for(spill_path: &std::path::Path) -> Option<String> {
    blob_refs().lock().ok()?.get(spill_path).cloned()
}

static BLOB_REFS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<PathBuf, String>>> =
    std::sync::OnceLock::new();

fn blob_refs() -> &'static std::sync::Mutex<std::collections::HashMap<PathBuf, String>> {
    BLOB_REFS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn record_blob(reference: String, path: &std::path::Path) {
    if let Ok(mut map) = blob_refs().lock() {
        map.insert(path.to_path_buf(), reference);
    }
}

fn spill_dir() -> PathBuf {
    crate::config::data_dir().join("output")
}

/// Open a spill file, returning it with the path we will report to the model.
///
/// Returns `None` when the temp dir is unusable, which degrades to plain
/// truncation rather than failing the command.
fn open_spill() -> Option<(std::fs::File, PathBuf)> {
    let dir = spill_dir();
    std::fs::create_dir_all(&dir).ok()?;

    // pid plus a counter keeps concurrent commands from colliding.
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("out-{}-{n}.log", std::process::id()));

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let f = options.open(&path).ok()?;
    Some((f, path))
}

/// Durable working state, written by the model, read by the compactor.
///
/// The revision guard answers "did anyone write since I read?". The receipt
/// ledger answers the other half — "have I already applied this request?" — so
/// a retried write collapses onto its recorded receipt instead of failing as
/// stale or applying a second time. See `crate::session::state_receipts`.
#[derive(Clone, Default)]
pub struct State {
    items: Vec<String>,
    receipts: crate::session::state_receipts::Ledger,
    /// The session this state belongs to, when one was opened before it was
    /// built. `None` means unbound, and ownership says so rather than
    /// asserting a writer this process cannot verify.
    owner: Option<crate::session::state_receipts::SessionContext>,
}

impl State {
    pub fn new() -> Self {
        // Restore sites build state by replaying a session log, so adopt the
        // session that log belongs to and the receipts it already holds. An
        // adopted ledger is what makes a retry after a restart return a
        // receipt instead of writing the same notes a second time.
        let Some(owner) = crate::session::state_receipts::adopt() else {
            return Self::default();
        };
        Self {
            items: Vec::new(),
            receipts: crate::session::state_receipts::Ledger::restore(&owner.receipts),
            owner: Some(owner),
        }
    }

    pub fn items(&self) -> &[String] {
        &self.items
    }

    /// Revision a caller must present to replace the notes it just read.
    pub fn revision(&self) -> String {
        crate::session::state_receipts::revision_of(&self.items)
    }

    /// Handle a `state` tool call. No `items` argument means read-back.
    pub fn apply(&mut self, call: &ToolCall) -> Outcome {
        let base = self.revision();
        let request_id = call
            .arg_raw("request_id")
            .and_then(|raw| serde_json::from_str::<String>(&raw).ok())
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty());
        let items = match call.arg_raw("items") {
            Some(raw) if !raw.is_empty() && raw != "null" => match parse_items(&raw) {
                Ok(items) => Some(items),
                Err(outcome) => return outcome,
            },
            // Absent or null means read-back, which is how the model recovers
            // state after a compaction without having to guess.
            _ => None,
        };

        // A known request id is answered before the revision guard runs. A
        // retry presents the revision its own write replaced, which is now
        // stale by definition; answering that as stale would tell the caller
        // nothing about whether the work landed.
        if let (Some(id), Some(items)) = (&request_id, &items) {
            let content = crate::session::state_receipts::revision_of(items);
            if let Some(previous) = self.receipts.get(id) {
                if previous.revision == content {
                    return Outcome::ok(self.replayed_body(previous));
                }
                return Outcome::err(format!(
                    "request_id {id} was already applied with different content \
                     (recorded revision {}, this request {content}); \
                     send a new request_id for different notes",
                    previous.revision
                ));
            }
        }

        // A caller that read a revision must still hold it, so a concurrent
        // update cannot be silently overwritten by a stale replacement.
        if let Some(expected) = call.arg_raw("expected")
            && serde_json::from_str::<String>(&expected).ok().as_deref() != Some(base.as_str())
        {
            return Outcome::err("Stale state revision; read state with revision:true before replacing notes");
        }
        if items.is_none() && call.arg_raw("revision").as_deref() == Some("true") {
            return Outcome::ok(self.revision_read_back(&base));
        }
        let Some(items) = items else { return Outcome::ok(self.render()) };

        let count = items.len();
        let bytes: usize = items.iter().map(String::len).sum();
        self.items = items;
        let revision = self.revision();
        let mut body = if count == 0 {
            "state cleared".to_string()
        } else {
            format!("state recorded ({count} items)")
        };
        // A write that changed nothing needs no log record, so its receipt
        // lives in this session only. Say which, rather than implying a
        // durable record the log never received.
        let unchanged = revision == base;
        if let Some(id) = request_id {
            let receipt = crate::session::state_receipts::Receipt {
                request_id: id.clone(),
                revision: revision.clone(),
                base_revision: base.clone(),
                items: count,
                bytes,
                actor: self.actor(),
                applied_at: crate::loops::now(),
            };
            let evicted = self.receipts.record(receipt.clone());
            if !unchanged {
                crate::session::state_receipts::stage(crate::session::state_receipts::Staged {
                    base_revision: base,
                    items: self.items.clone(),
                    receipt,
                });
            }
            body.push_str(&format!(
                "\nreceipt: request {id} · revision {} · {count} items, {bytes} B · {}{}",
                short_revision(&revision),
                self.actor(),
                if unchanged {
                    " · notes were already identical, so no log record was needed"
                } else {
                    " · recorded with this change in the session log"
                },
            ));
            body.push_str(&match &evicted {
                Some(evicted) => format!(
                    "\nledger: {} of {} request ids retained; evicted oldest id {}",
                    self.receipts.len(),
                    self.receipts.bound(),
                    evicted.request_id
                ),
                None => format!(
                    "\nledger: {} of {} request ids retained; eviction oldest-first",
                    self.receipts.len(),
                    self.receipts.bound()
                ),
            });
        }
        Outcome::ok(body)
    }

    /// Who the log says accepted this process's writes, or that nobody is
    /// bound — never a guess presented as a fact.
    fn actor(&self) -> String {
        self.owner
            .as_ref()
            .map(|owner| owner.session.clone())
            .unwrap_or_else(|| "unbound writer".to_string())
    }

    /// The answer to a repeated request: what was applied, not a new write.
    fn replayed_body(&self, receipt: &crate::session::state_receipts::Receipt) -> String {
        format!(
            "state request {} was already applied — not applied again\n{}",
            receipt.request_id,
            receipt.render()
        )
    }

    fn revision_read_back(&self, revision: &str) -> String {
        let (summary, detail) = self.ownership(revision);
        serde_json::json!({
            "items": self.items,
            "revision": revision,
            "ownership": summary,
            "owner": detail,
            "requires_revision": revision,
            "receipt_ledger": self.receipts.report(),
        })
        .to_string()
    }

    /// Who can actually write these notes, computed rather than asserted.
    ///
    /// The writer lock is the fact behind it: `Session` holds an exclusive
    /// `flock` on its log, so "this process holds it" is a checked claim, and
    /// a read-only snapshot reports that appends are refused rather than
    /// pretending they are not.
    fn ownership(&self, revision: &str) -> (String, serde_json::Value) {
        let requires = serde_json::Value::from(revision);
        match &self.owner {
            Some(owner) if owner.durable_writer => (
                format!(
                    "session {}: this process holds the exclusive log writer lock; \
                     replace notes by sending expected=\"{}\"",
                    owner.session,
                    short_revision(revision)
                ),
                serde_json::json!({
                    "session": owner.session,
                    "durable_writer": true,
                    "log_append": "accepted: this process holds the session log writer lock",
                    "concurrent_writers": "excluded by the exclusive session log writer lock",
                    "requires_revision": requires,
                    "guard": "send expected = requires_revision; a stale value is rejected instead of overwriting",
                }),
            ),
            Some(owner) => (
                format!(
                    "session {}: opened read-only, so this process cannot append to its log; \
                     replace notes by sending expected=\"{}\"",
                    owner.session,
                    short_revision(revision)
                ),
                serde_json::json!({
                    "session": owner.session,
                    "durable_writer": false,
                    "log_append": "refused: this process holds no writer lock for the session log",
                    "concurrent_writers": "possible: another process may hold the session log writer lock",
                    "requires_revision": requires,
                    "guard": "send expected = requires_revision; a stale value is rejected instead of overwriting",
                }),
            ),
            None => (
                format!(
                    "unbound: no session log is bound to this state, so only this process can \
                     replace these notes and whoever appends them decides durability; \
                     replace notes by sending expected=\"{}\"",
                    short_revision(revision)
                ),
                serde_json::json!({
                    "session": serde_json::Value::Null,
                    "durable_writer": false,
                    "log_append": "unknown: this state is not bound to a session log writer",
                    "concurrent_writers": "possible: no writer lock is bound to this state",
                    "requires_revision": requires,
                    "guard": "send expected = requires_revision; a stale value is rejected instead of overwriting",
                }),
            ),
        }
    }

    fn render(&self) -> String {
        if self.items.is_empty() {
            return "(no state recorded)".to_string();
        }
        self.items
            .iter()
            .map(|s| format!("- {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Render as the system-visible block injected after compaction.
    pub fn as_context(&self) -> Option<String> {
        if self.items.is_empty() {
            return None;
        }
        Some(format!(
            "State recorded earlier in this session (your own notes, keep it accurate):\n{}",
            self.render()
        ))
    }

    /// Request ids the ledger still answers for, for tests that assert what
    /// it kept.
    #[cfg(test)]
    fn retained_receipts(&self) -> usize {
        self.receipts.len()
    }
}

fn short_revision(revision: &str) -> &str {
    revision.get(..8).unwrap_or(revision)
}

/// Validate and normalize `items`, preserving the existing bounds.
fn parse_items(raw: &str) -> Result<Vec<String>, Outcome> {
    match serde_json::from_str::<Vec<String>>(raw) {
        Ok(items) => {
            if items.len() > 128
                || items.iter().any(|s| s.len() > 4096)
                || items.iter().map(String::len).sum::<usize>() > 65536
            {
                return Err(Outcome::err(
                    "State limited to 128 items, 4 KiB each and 64 KiB total",
                ));
            }
            Ok(items
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect())
        }
        Err(e) => Err(Outcome {
            body: format!("error: `items` must be an array of strings: {e}"),
            truncated: false,
            exit_code: None,
            spill: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(args: &str) -> ToolCall {
        ToolCall {
            id: "t1".into(),
            name: "bash".into(),
            arguments: args.to_string(),
        }
    }

    #[test]
    fn runs_a_simple_command() {
        let out = bash(
            &call(r#"{"command":"echo hello"}"#),
            std::path::Path::new("."),
        );
        // The result always leads with the exit code so the model never has to
        // guess whether a command worked.
        assert_eq!(out.body, "[exit 0]\nhello\n");
        assert_eq!(out.exit_code, Some(0));
        assert!(!out.truncated);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn command_exceptions_require_an_explicit_session_snapshot() {
        const CHILD: &str = "GC_TEST_APPROVAL_SNAPSHOT_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "tools::tests::command_exceptions_require_an_explicit_session_snapshot",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        let root =
            std::env::temp_dir().join(format!("gc-approval-snapshot-{}", std::process::id()));
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let cfg = crate::config::Config {
            workspace: workspace.clone(),
            permission_profile: crate::sandbox::PermissionProfile::ReadOnly,
            ..Default::default()
        };
        crate::sandbox::init(&cfg);
        let outside = root.join("outside");
        let command = format!("printf approved > {}", outside.display());
        crate::permissions::approve_session_command(&command).unwrap();
        let tool = call(&serde_json::json!({"command":command}).to_string());
        let _ = bash(&tool, &workspace);
        assert!(
            !outside.exists(),
            "headless/delegated calls inherited an ambient grant"
        );
        let approvals = crate::permissions::approvals_for_command(&command).unwrap();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let result = bash_with_approvals(&tool, &workspace, &cancel, &approvals);
        assert_eq!(result.exit_code, Some(0), "{}", result.body);
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "approved");
        std::fs::remove_file(&outside).unwrap();
        let shifted = call(&serde_json::json!({"command":command,"cwd":root}).to_string());
        let result = bash_with_approvals(&shifted, &workspace, &cancel, &approvals);
        assert!(result.body.contains("session workspace"));
        assert!(!outside.exists());
        let _ = bash(&tool, &workspace);
        assert!(
            !outside.exists(),
            "explicit exception widened later unapproved commands"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn nonzero_exit_is_reported_in_the_body() {
        let out = bash(
            &call(r#"{"command":"echo bad >&2; exit 2"}"#),
            std::path::Path::new("."),
        );
        assert!(out.body.starts_with("[exit 2]"), "{}", out.body);
        assert_eq!(out.exit_code, Some(2));
    }

    #[test]
    fn successful_command_with_no_output_is_unambiguous() {
        // A silent success previously read identically to "nothing happened",
        // which is what makes models re-run commands to find out.
        let out = bash(&call(r#"{"command":"true"}"#), std::path::Path::new("."));
        assert_eq!(out.body, "[exit 0]\n(no output)");
    }

    #[test]
    fn captures_stderr() {
        let out = bash(
            &call(r#"{"command":"echo oops >&2"}"#),
            std::path::Path::new("."),
        );
        assert!(out.body.contains("stderr:"));
        assert!(out.body.contains("oops"));
    }

    #[test]
    fn reports_exit_code_in_nonzero_case() {
        let out = bash(&call(r#"{"command":"exit 3"}"#), std::path::Path::new("."));
        assert_eq!(out.exit_code, Some(3));
    }

    #[test]
    fn empty_command_is_rejected_without_spawning() {
        let out = bash(&call(r#"{"command":"   "}"#), std::path::Path::new("."));
        assert!(out.body.contains("empty"));
        assert_eq!(out.exit_code, None);
    }

    #[test]
    fn missing_command_is_rejected() {
        let out = bash(&call(r#"{"cwd":"/tmp"}"#), std::path::Path::new("."));
        assert!(out.body.contains("required"));
    }

    #[test]
    fn timeout_kills_the_child() {
        let started = std::time::Instant::now();
        let out = bash(
            &call(r#"{"command":"sleep 5","timeout_ms":150}"#),
            std::path::Path::new("."),
        );
        assert!(out.body.contains("killed"), "{}", out.body);
        assert!(out.exit_code.is_none() || out.exit_code != Some(0));
        // Proves the numeric timeout was honoured rather than defaulted to 120s.
        assert!(
            started.elapsed() < std::time::Duration::from_secs(3),
            "took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn numeric_timeout_is_honoured_not_defaulted() {
        // Regression: reading a JSON number with the string accessor returned
        // None, so every command silently ran with the 120s default.
        let started = std::time::Instant::now();
        let out = bash(
            &call(r#"{"command":"sleep 30","timeout_ms":200}"#),
            std::path::Path::new("."),
        );
        assert!(out.body.contains("killed"), "{}", out.body);
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }

    #[test]
    fn string_timeout_is_also_accepted() {
        let started = std::time::Instant::now();
        let out = bash(
            &call(r#"{"command":"sleep 30","timeout_ms":"200"}"#),
            std::path::Path::new("."),
        );
        assert!(out.body.contains("killed"), "{}", out.body);
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }

    #[test]
    fn non_numeric_timeout_is_rejected_before_execution() {
        let out = bash(
            &call(r#"{"command":"echo ok","timeout_ms":"soon"}"#),
            std::path::Path::new("."),
        );
        assert!(out.body.contains("timeout_ms"));
        assert!(out.body.contains("nothing executed"));
        assert_eq!(out.exit_code, None);
    }

    #[test]
    fn invalid_explicit_shell_inputs_do_not_spawn() {
        let root = std::env::temp_dir().join(format!("gc-input-validation-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let marker = root.join("should-not-exist");
        for extra in [
            serde_json::json!({"cwd":"missing"}),
            serde_json::json!({"cwd":""}),
            serde_json::json!({"cwd":42}),
            serde_json::json!({"timeout_ms":0}),
            serde_json::json!({"timeout_ms":1.5}),
            serde_json::json!({"timeout_ms":true}),
            serde_json::json!({"timeout_ms":null}),
        ] {
            let mut args = extra;
            args["command"] = serde_json::json!("touch should-not-exist");
            let out = bash(&call(&args.to_string()), &root);
            assert_eq!(out.exit_code, None, "{}", out.body);
            assert!(!marker.exists(), "invalid arguments executed the shell: {args}");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn child_processes_are_killed_with_the_group() {
        // The shell dies instantly but `sleep` would survive without a group
        // signal, holding the stdout pipe open past the timeout.
        let started = std::time::Instant::now();
        let out = bash(
            &call(r#"{"command":"sleep 30 & sleep 30","timeout_ms":200}"#),
            std::path::Path::new("."),
        );
        assert!(out.body.contains("killed"), "{}", out.body);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(3),
            "took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn timeout_kills_term_ignoring_descendants_after_shell_exit() {
        let started = std::time::Instant::now();
        let out = bash(
            &call(r#"{"command":"sh -c 'trap \"\" TERM; sleep 5' & sleep 5","timeout_ms":200}"#),
            std::path::Path::new("."),
        );
        assert!(out.body.contains("killed"), "{}", out.body);
        assert!(started.elapsed() < std::time::Duration::from_secs(3), "descendant retained the output pipe");
    }

    #[test]
    fn honours_cwd() {
        let dir = std::env::temp_dir().join("glass-bash-cwd-test");
        std::fs::create_dir_all(&dir).unwrap();
        let out = bash(&call(r#"{"command":"pwd","cwd":"."}"#), &dir);
        // Compare canonically: macOS temp dirs are symlinked via /private.
        let path = out
            .body
            .trim()
            .strip_prefix("[exit 0]")
            .unwrap_or(out.body.trim());
        let got = std::fs::canonicalize(path.trim()).unwrap_or_else(|_| path.trim().into());
        let want = std::fs::canonicalize(&dir).unwrap();
        assert_eq!(got, want);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn exited_shell_cannot_leave_descendants_holding_capture_pipes() {
        let start = std::time::Instant::now();
        let out = bash(&call(r#"{"command":"sleep 30 & echo done","timeout_ms":200}"#), std::path::Path::new("."));
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
        assert_eq!(out.exit_code, Some(0));
        assert!(out.body.contains("done"));
    }

    #[test]
    fn both_overflow_streams_remain_accessible() {
        let out = bash(&call(r#"{"command":"seq 1 40000; seq 40001 80000 >&2"}"#), std::path::Path::new("."));
        let stdout_path = out.spill.as_ref().unwrap();
        assert!(std::fs::read_to_string(stdout_path).unwrap().contains("40000"));
        let stderr_path = out.body.split("[stderr overflow saved to ").nth(1).unwrap().split(" — ").next().unwrap();
        assert!(std::fs::read_to_string(stderr_path).unwrap().contains("80000"));
        let _ = std::fs::remove_file(stdout_path);
        let _ = std::fs::remove_file(stderr_path);
    }

    #[test]
    fn large_output_spills_to_disk_and_keeps_the_tail() {
        let out = bash(
            &call(r#"{"command":"seq 1 40000"}"#),
            std::path::Path::new("."),
        );
        assert!(out.truncated, "expected truncation");
        let spill = out.spill.as_ref().expect("overflow must spill to a file");
        let msg = std::fs::read_to_string(spill).expect("spill file must exist");
        assert!(
            msg.contains("40000"),
            "the last line must be recoverable from the spill file"
        );
        // The inline body points the model at the file.
        assert!(out.body.contains("saved to"), "{}", out.body);
        assert!(
            out.body.len() < MAX_OUTPUT_BYTES + 400,
            "body grew to {}",
            out.body.len()
        );
        let _ = std::fs::remove_file(spill);
    }

    #[test]
    fn output_under_the_cap_does_not_spill() {
        let out = bash(
            &call(r#"{"command":"echo small"}"#),
            std::path::Path::new("."),
        );
        assert!(!out.truncated);
        assert!(out.spill.is_none());
    }

    #[test]
    fn spill_contains_exactly_the_overflow() {
        // The inline head and the spill file must partition the stream, with no
        // bytes lost at the boundary.
        let out = bash(
            &call(r#"{"command":"seq 1 40000"}"#),
            std::path::Path::new("."),
        );
        let spill = out.spill.as_ref().unwrap();
        let tail = std::fs::read_to_string(spill).unwrap();
        let head_lines = out
            .body
            .lines()
            .filter(|l| l.chars().all(|c| c.is_ascii_digit()) && !l.is_empty())
            .count();
        assert!(head_lines > 0, "head should carry some lines");
        assert!(tail.contains("40000"));
        // 40000 lines total; head + spill must account for all of them.
        let spill_lines = tail.lines().filter(|l| !l.trim().is_empty()).count();
        assert!(
            head_lines + spill_lines >= 39999,
            "head {head_lines} + spill {spill_lines} lost lines"
        );
        let _ = std::fs::remove_file(spill);
    }

    #[test]
    fn state_records_and_reads_back() {
        let mut s = State::new();
        let c = ToolCall {
            id: "t".into(),
            name: "state".into(),
            arguments: r#"{"items":["a is b","c is d"]}"#.into(),
        };
        s.apply(&c);
        assert_eq!(s.items().len(), 2);
        let read = ToolCall {
            id: "t".into(),
            name: "state".into(),
            arguments: "{}".into(),
        };
        let out = s.apply(&read);
        assert!(out.body.contains("a is b"));
        assert!(out.body.contains("c is d"));
    }

    #[test]
    fn state_rejects_non_array_items() {
        let mut s = State::new();
        let c = ToolCall {
            id: "t".into(),
            name: "state".into(),
            arguments: r#"{"items":"nope"}"#.into(),
        };
        let out = s.apply(&c);
        assert!(out.body.contains("array of strings"));
        assert!(s.items().is_empty());
    }

    #[test]
    fn state_drops_blank_entries() {
        let mut s = State::new();
        let c = ToolCall {
            id: "t".into(),
            name: "state".into(),
            arguments: r#"{"items":["  real  ","","   "]}"#.into(),
        };
        s.apply(&c);
        assert_eq!(s.items(), &["real".to_string()]);
    }

    #[test]
    fn empty_state_has_no_context_block() {
        assert!(State::new().as_context().is_none());
    }

    #[test]
    fn state_context_renders_items() {
        let mut s = State::new();
        s.apply(&ToolCall {
            id: "t".into(),
            name: "state".into(),
            arguments: r#"{"items":["step 1 done"]}"#.into(),
        });
        let ctx = s.as_context().unwrap();
        assert!(ctx.contains("step 1 done"));
    }

    fn state_call(args: &str) -> ToolCall {
        ToolCall {
            id: "t".into(),
            name: "state".into(),
            arguments: args.into(),
        }
    }

    #[test]
    fn a_retried_request_id_returns_its_receipt_without_applying_twice() {
        let mut s = State::new();
        let base = s.revision();
        let first = s.apply(&state_call(&format!(
            r#"{{"request_id":"req-1","items":["first"],"expected":"{base}"}}"#
        )));
        assert!(first.body.contains("state recorded (1 items)"));
        assert!(first.body.contains("receipt: request req-1"));

        // The retry presents the revision its own write replaced, which is now
        // stale by definition. It must still get the receipt, not a stale
        // error that says nothing about whether the work landed.
        let retry = s.apply(&state_call(&format!(
            r#"{{"request_id":"req-1","items":["first"],"expected":"{base}"}}"#
        )));
        assert!(
            retry.body.contains("already applied"),
            "a known id must answer with its receipt: {}",
            retry.body
        );
        assert!(retry.body.contains("req-1"));
        assert_eq!(s.items(), &["first".to_string()]);
        assert_eq!(s.retained_receipts(), 1, "the retry records nothing new");
    }

    #[test]
    fn a_clear_is_a_request_too_and_a_repeat_cannot_double_clear() {
        let mut s = State::new();
        s.apply(&state_call(r#"{"request_id":"set","items":["note"]}"#));
        let cleared = s.apply(&state_call(r#"{"request_id":"clear","items":[]}"#));
        assert!(cleared.body.contains("state cleared"));
        let repeat = s.apply(&state_call(r#"{"request_id":"clear","items":[]}"#));
        assert!(repeat.body.contains("already applied"), "{}", repeat.body);
        assert!(s.items().is_empty());
        assert_eq!(s.retained_receipts(), 2);
    }

    #[test]
    fn the_same_request_id_with_different_notes_fails_explicitly() {
        let mut s = State::new();
        s.apply(&state_call(r#"{"request_id":"req-2","items":["one"]}"#));
        let conflict = s.apply(&state_call(r#"{"request_id":"req-2","items":["two"]}"#));
        assert!(
            conflict.body.contains("already applied with different content"),
            "{}",
            conflict.body
        );
        // A client error changes nothing and is reported as one.
        assert_eq!(conflict.exit_code, None);
        assert_eq!(s.items(), &["one".to_string()]);
        assert_eq!(s.retained_receipts(), 1);
    }

    #[test]
    fn a_concurrent_update_still_trips_the_stale_revision_guard() {
        let mut s = State::new();
        let revision = s.revision();
        s.apply(&state_call(r#"{"request_id":"a","items":["someone else wrote this"]}"#));
        let stale = s.apply(&state_call(
            &format!(r#"{{"request_id":"b","items":["mine"],"expected":"{revision}"}}"#),
        ));
        assert!(stale.body.contains("Stale state revision"), "{}", stale.body);
        assert_eq!(s.items(), &["someone else wrote this".to_string()]);
    }

    #[test]
    fn the_revision_read_back_states_real_ownership_and_the_revision_to_send() {
        let mut unbound = State::new();
        let read = unbound.apply(&state_call(r#"{"revision":true}"#));
        let value: serde_json::Value = serde_json::from_str(&read.body).unwrap();
        assert_eq!(value["requires_revision"], value["revision"]);
        assert_eq!(value["owner"]["session"], serde_json::Value::Null);
        assert!(
            value["ownership"].as_str().unwrap().starts_with("unbound:"),
            "an unbound state must not claim a writer: {}",
            value["ownership"]
        );

        let dir = std::env::temp_dir().join(format!("gc-state-ownership-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let session = crate::session::Session::create(&dir).unwrap();
        let mut bound = State::new();
        let read = bound.apply(&state_call(r#"{"revision":true}"#));
        let value: serde_json::Value = serde_json::from_str(&read.body).unwrap();
        assert_eq!(value["owner"]["session"], session.id.as_str());
        assert_eq!(value["owner"]["durable_writer"], true);
        assert!(
            value["ownership"].as_str().unwrap().contains(&session.id),
            "ownership must name the session that can actually write: {}",
            value["ownership"]
        );
        drop(session);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_read_only_snapshot_reports_that_it_cannot_append() {
        let dir = std::env::temp_dir().join(format!("gc-state-readonly-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let id = {
            let session = crate::session::Session::create(&dir).unwrap();
            session.id.clone()
        };
        let readonly = crate::session::Session::open_readonly(&dir, &id).unwrap();
        let mut state = State::new();
        let value: serde_json::Value =
            serde_json::from_str(&state.apply(&state_call(r#"{"revision":true}"#)).body).unwrap();
        assert_eq!(value["owner"]["durable_writer"], false);
        assert!(
            value["owner"]["log_append"]
                .as_str()
                .unwrap()
                .starts_with("refused:"),
            "a snapshot must not claim it can write: {}",
            value["owner"]["log_append"]
        );
        drop(readonly);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_receipt_ledger_is_bounded_and_reports_what_it_evicted() {
        let mut s = State::new();
        // Walk past the ledger bound with requests the log never sees, so the
        // bound is what is under test rather than a session's log size.
        for i in 0..=crate::session::state_receipts::LEDGER_CAPACITY {
            s.apply(&state_call(&format!(
                r#"{{"request_id":"req-{i}","items":["note {i}"]}}"#
            )));
        }
        assert_eq!(s.retained_receipts(), crate::session::state_receipts::LEDGER_CAPACITY);
        // The evicted id is applied afresh rather than silently deduped.
        let evicted = s.apply(&state_call(
            r#"{"request_id":"req-0","items":["note 0"]}"#,
        ));
        assert!(
            evicted.body.contains("state recorded"),
            "an evicted id must not still be treated as applied: {}",
            evicted.body
        );
        assert!(evicted.body.contains("evicted oldest id"), "{}", evicted.body);
        assert!(evicted.body.contains("ledger:"), "{}", evicted.body);

        let read = s.apply(&state_call(r#"{"revision":true}"#));
        let value: serde_json::Value = serde_json::from_str(&read.body).unwrap();
        assert_eq!(
            value["receipt_ledger"]["bound"],
            crate::session::state_receipts::LEDGER_CAPACITY
        );
        assert!(
            value["receipt_ledger"]["evicted"].as_u64().unwrap() >= 1,
            "eviction is reported, not silent: {}",
            read.body
        );
        assert!(value["receipt_ledger"]["eviction"].is_string());
    }
}
