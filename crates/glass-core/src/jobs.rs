//! Session-owned process groups with bounded capture and host-driven completion notices.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Seek, SeekFrom, Write},
    os::fd::AsRawFd,
    os::unix::{fs::OpenOptionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
trait Pipe: Read + AsRawFd + Send {}
impl<T: Read + AsRawFd + Send> Pipe for T {}
const MAX_LOG: u64 = 4 * 1024 * 1024;
const MAX_JOBS: usize = 16;
// Match modal::Screen's frame, title, optional spacer and footer rows.
fn viewer_page((width, height): (u16, u16)) -> usize {
    let framed = width >= 24 && height >= 12;
    let capacity = usize::from(height).saturating_sub(if framed { 4 } else { 0 });
    capacity
        .saturating_sub(2 + usize::from(capacity > 10))
        .max(1)
}
/// Peek at most four bytes so a tail preview starts at a complete scalar.
fn aligned_tail_cursor(path: &Path, size: u64, window: u64) -> Result<u64, String> {
    let offset = size.saturating_sub(window);
    if offset == 0 {
        return Ok(0);
    }
    let start = offset.saturating_sub(3);
    let mut file = std::fs::File::open(path).map_err(err)?;
    file.seek(SeekFrom::Start(start)).map_err(err)?;
    let mut bytes = [0; 4];
    let mut index = (offset - start) as usize;
    file.read_exact(&mut bytes[..=index]).map_err(err)?;
    while index > 0 && bytes[index] & 0xc0 == 0x80 {
        index -= 1;
    }
    Ok(start + index as u64)
}

struct Capture {
    file: std::fs::File,
    bytes: u64,
}
pub struct Job {
    child: Child,
    input: Option<ChildStdin>,
    command: String,
    output: PathBuf,
    capture: Arc<Mutex<Capture>>,
    dropped: Arc<AtomicU64>,
    drains: Vec<std::thread::JoinHandle<()>>,
    started: Instant,
    exit: Option<i32>,
    closed: bool,
    stopping: Arc<AtomicBool>,
    finished: Option<Duration>,
    notify: bool,
    notified: bool,
    task_id: Option<String>,
    timeout: Option<Duration>,
    timed_out: bool,
}
#[derive(Default)]
pub struct Jobs {
    jobs: BTreeMap<u32, Job>,
    next: u32,
    auto_wakes: u8,
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
impl Jobs {
    pub fn start(&mut self, command: &str, workspace: &Path, dir: &Path) -> Result<String, String> {
        let approvals = crate::permissions::approvals_for_command(command)?;
        let id = self.start_owned(command, workspace, dir, &approvals)?;
        let j = &self.jobs[&id];
        Ok(format!(
            "job {id} pid {}: {}",
            j.child.id(),
            j.output.display()
        ))
    }
    fn start_owned(
        &mut self,
        command: &str,
        workspace: &Path,
        dir: &Path,
        approvals: &crate::sandbox::SessionApprovals,
    ) -> Result<u32, String> {
        if command.trim().is_empty() {
            return Err("background command is empty".into());
        }
        self.refresh();
        if self.jobs.values().filter(|j| j.exit.is_none()).count() >= MAX_JOBS {
            return Err("16 active jobs maximum; stop a job first".into());
        }
        if self.jobs.len() >= 128 {
            return Err("128 retained jobs maximum for a session".into());
        }
        let exception = approvals.permits_exact(command);
        if !exception && let Some(reason) = crate::sandbox::refusal() {
            return Err(format!(
                "the active sandbox or egress policy is unavailable ({reason}); refusing to run jobs without it"
            ));
        }
        std::fs::create_dir_all(dir).map_err(err)?;
        self.next = self.next.checked_add(1).ok_or("job ID overflow")?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(err)?
            .as_nanos();
        let output = dir.join(format!("job-{}-{stamp}.log", self.next));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&output)
            .map_err(err)?;
        let mut cmd = if exception {
            let mut c = Command::new("/bin/bash");
            c.arg("-c").arg(command);
            c
        } else {
            crate::sandbox::shell(command)
        };
        cmd.current_dir(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        crate::sandbox::harden_command(&mut cmd, command, approvals, exception, false)?;
        crate::sandbox::scrub_env(&mut cmd);
        if let Ok(exe) = crate::executable::path() {
            cmd.env("GC_BIN", exe);
        }
        let mut child = cmd.spawn().map_err(err)?;
        let input = child.stdin.take();
        if let Some(ref input) = input {
            let fd = input.as_raw_fd();
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.wait();
                return Err("cannot make job stdin nonblocking".into());
            }
        }
        let capture = Arc::new(Mutex::new(Capture { file, bytes: 0 }));
        let dropped = Arc::new(AtomicU64::new(0));
        let mut drains = Vec::new();
        let stopping = Arc::new(AtomicBool::new(false));
        for mut pipe in [
            Box::new(child.stdout.take().unwrap()) as Box<dyn Pipe>,
            Box::new(child.stderr.take().unwrap()) as Box<dyn Pipe>,
        ] {
            let capture = capture.clone();
            let dropped = dropped.clone();
            let stopping = stopping.clone();
            let fd = pipe.as_raw_fd();
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                stopping.store(true, Ordering::Relaxed);
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.wait();
                return Err("cannot make job capture nonblocking".into());
            }
            drains.push(std::thread::spawn(move || {
                let mut buffer = [0u8; 8192];
                let mut stop_deadline = None;
                loop {
                    // A detached writer can keep the pipe readable forever. Allow
                    // final buffered output, but bound capture even under continuous reads.
                    if stopping.load(Ordering::Relaxed) {
                        let deadline = stop_deadline
                            .get_or_insert_with(|| Instant::now() + Duration::from_millis(100));
                        if Instant::now() >= *deadline {
                            break;
                        }
                    }
                    let n = match pipe.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => n,
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            if stopping.load(Ordering::Relaxed) {
                                break;
                            }
                            let mut event = libc::pollfd {
                                fd,
                                events: libc::POLLIN,
                                revents: 0,
                            };
                            unsafe {
                                libc::poll(&mut event, 1, 25);
                            }
                            continue;
                        }
                        Err(_) => break,
                    };
                    let Ok(mut c) = capture.lock() else { break };
                    let keep = (MAX_LOG.saturating_sub(c.bytes) as usize).min(n);
                    if keep > 0 {
                        if c.file.write_all(&buffer[..keep]).is_err() {
                            dropped.fetch_add(n as u64, Ordering::Relaxed);
                            continue;
                        }
                        c.bytes += keep as u64;
                    }
                    dropped.fetch_add((n - keep) as u64, Ordering::Relaxed);
                }
            }));
        }
        self.jobs.insert(
            self.next,
            Job {
                child,
                input,
                command: command.into(),
                output,
                capture,
                dropped,
                drains,
                started: Instant::now(),
                exit: None,
                closed: false,
                stopping,
                finished: None,
                notify: false,
                notified: false,
                task_id: None,
                timeout: None,
                timed_out: false,
            },
        );
        Ok(self.next)
    }
    fn refresh(&mut self) {
        for j in self.jobs.values_mut() {
            if j.exit.is_none() && j.timeout.is_some_and(|limit| j.started.elapsed() >= limit) {
                j.timed_out = true;
                unsafe {
                    libc::kill(-(j.child.id() as i32), libc::SIGKILL);
                }
            }
            if j.exit.is_none()
                && let Ok(Some(status)) = j.child.try_wait()
            {
                j.exit = Some(status.code().unwrap_or(-1));
                j.finished = Some(j.started.elapsed());
                j.stopping.store(true, Ordering::Relaxed);
                j.input.take();
                // A completed shell must not leave descendants holding capture pipes open.
                unsafe {
                    libc::kill(-(j.child.id() as i32), libc::SIGKILL);
                }
                j.closed = true;
            }
        }
    }
    pub fn list(&mut self) -> String {
        self.refresh();
        self.jobs
            .iter()
            .map(|(id, j)| {
                format!(
                    "{id}: {} — {} — {}",
                    j.exit.map_or("running".into(), |n| format!("exit {n}")),
                    j.command,
                    j.output.display()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub fn output(&self, id: u32) -> Result<String, String> {
        let j = self.jobs.get(&id).ok_or("unknown job")?;
        let mut f = std::fs::File::open(&j.output).map_err(err)?;
        let size = f.metadata().map_err(err)?.len();
        f.seek(SeekFrom::Start(size.saturating_sub(60_000)))
            .map_err(err)?;
        let mut bytes = Vec::new();
        f.take(60_000).read_to_end(&mut bytes).map_err(err)?;
        Ok(format!(
            "{}{}",
            String::from_utf8_lossy(&bytes),
            if j.dropped.load(Ordering::Relaxed) > 0 {
                "\n[job log capped at 4 MiB; further bytes discarded]"
            } else {
                ""
            }
        ))
    }
    fn tail_cursor(&mut self, id: u32, window: u64) -> Result<u64, String> {
        self.refresh();
        let job = self.jobs.get(&id).ok_or("unknown job")?;
        let size = job.capture.lock().map_err(err)?.bytes;
        aligned_tail_cursor(&job.output, size, window)
    }

    fn poll(&mut self, id: u32, offset: u64, limit: usize) -> Result<Value, String> {
        self.refresh();
        let j = self.jobs.get(&id).ok_or("unknown job")?;
        let size = j.capture.lock().map_err(err)?.bytes;
        if offset > size {
            return Err("offset exceeds captured output".into());
        }
        let mut f = std::fs::File::open(&j.output).map_err(err)?;
        f.seek(SeekFrom::Start(offset)).map_err(err)?;
        let mut bytes = Vec::new();
        let limit = limit.clamp(1, 24 * 1024);
        // Peek up to three bytes beyond the requested boundary so a Unicode
        // scalar is delivered whole. Byte cursors still describe consumed bytes.
        f.take(((limit + 3) as u64).min(size - offset))
            .read_to_end(&mut bytes)
            .map_err(err)?;
        if offset > 0 && bytes.first().is_some_and(|byte| byte & 0xc0 == 0x80) {
            return Err(
                "offset is inside a UTF-8 character; use the previous next_offset cursor".into(),
            );
        }
        let mut end = limit.min(bytes.len());
        while end < bytes.len() && bytes[end] & 0xc0 == 0x80 {
            end += 1;
        }
        bytes.truncate(end);
        let draining = j.drains.iter().any(|h| !h.is_finished());
        if (j.exit.is_none() || draining)
            && let Err(error) = std::str::from_utf8(&bytes)
            && error.error_len().is_none()
        {
            // A writer may have emitted only the beginning of a scalar. Leave
            // its bytes at the same cursor until the remaining bytes arrive.
            bytes.truncate(error.valid_up_to());
        }
        Ok(
            json!({"job":id,"state":if j.exit.is_some(){"exited"}else{"running"},"exit_code":j.exit,"elapsed_seconds":j.finished.unwrap_or_else(||j.started.elapsed()).as_secs_f32(),"offset":offset,"next_offset":offset+bytes.len() as u64,"captured_bytes":size,"dropped_bytes":j.dropped.load(Ordering::Relaxed),"draining":draining,"output":String::from_utf8_lossy(&bytes)}),
        )
    }
    fn input(&mut self, id: u32, text: &str, eof: bool) -> Result<Value, String> {
        if text.len() > 8192 {
            return Err("job input limited to 8192 bytes per call".into());
        }
        self.refresh();
        let j = self.jobs.get_mut(&id).ok_or("unknown job")?;
        let input = j.input.as_mut().ok_or("job stdin is closed")?;
        let n = if text.is_empty() {
            0
        } else {
            // Nonblocking writes above PIPE_BUF may split a Unicode scalar.
            // Keep each write atomic so accepted_bytes is a reusable string boundary.
            let end = if text.is_ascii() {
                text.len()
            } else {
                let atomic = unsafe { libc::fpathconf(input.as_raw_fd(), libc::_PC_PIPE_BUF) };
                let atomic = usize::try_from(atomic)
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or("cannot determine atomic job stdin write size")?;
                let mut end = text.len().min(atomic);
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                end
            };
            match input.write(&text.as_bytes()[..end]) {
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => 0,
                Err(e) => return Err(err(e)),
            }
        };
        if eof && n == text.len() {
            j.input.take();
        }
        Ok(
            json!({"job":id,"accepted_bytes":n,"remaining_bytes":text.len()-n,"stdin_closed":j.input.is_none()}),
        )
    }
    pub fn stop(&mut self, id: u32) -> Result<String, String> {
        let j = self.jobs.get_mut(&id).ok_or("unknown job")?;
        j.notify = false;
        if j.closed {
            j.stopping.store(true, Ordering::Relaxed);
            for h in j.drains.drain(..) {
                let _ = h.join();
            }
            return Ok(format!("job {id} already exited"));
        }
        j.input.take();
        unsafe {
            libc::kill(-(j.child.id() as i32), libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_millis(250);
        let mut exit = None;
        while Instant::now() < deadline {
            if let Ok(Some(s)) = j.child.try_wait() {
                exit = Some(s.code().unwrap_or(-1));
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        unsafe {
            libc::kill(-(j.child.id() as i32), libc::SIGKILL);
        }
        if exit.is_none() {
            exit = j.child.wait().ok().map(|s| s.code().unwrap_or(-1));
        }
        j.exit = exit;
        j.finished = Some(j.started.elapsed());
        j.closed = true;
        j.stopping.store(true, Ordering::Relaxed);
        for h in j.drains.drain(..) {
            let _ = h.join();
        }
        Ok(format!("stopped job {id}"))
    }
}
impl Drop for Jobs {
    fn drop(&mut self) {
        for id in self.jobs.keys().copied().collect::<Vec<_>>() {
            let _ = self.stop(id);
        }
    }
}
fn registry() -> &'static Mutex<BTreeMap<String, Jobs>> {
    static R: OnceLock<Mutex<BTreeMap<String, Jobs>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(BTreeMap::new()))
}
fn owner_key(workspace: &Path, owner: &str) -> String {
    format!("{}\0{owner}", crate::concurrency::canonical_key(workspace))
}
pub fn has_running(workspace: &Path, owner: &str) -> bool {
    let Ok(mut registry) = registry().lock() else {
        return true;
    };
    let Some(jobs) = registry.get_mut(&owner_key(workspace, owner)) else {
        return false;
    };
    jobs.refresh();
    jobs.jobs.values().any(|job| job.exit.is_none())
}
pub const FOLLOW_UP: &str = "[Process completion follow-up]";

pub fn validate_task(
    call: &glass_mod_api::ToolCall,
    board: &crate::todo::Board,
) -> Result<(), String> {
    if call.name != "bash" {
        return Ok(());
    }
    let Some(job) = call
        .arg_raw("job")
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
    else {
        return Ok(());
    };
    if let Some(id) = job["task_id"].as_str()
        && !board.tasks.iter().any(|task| {
            task.id == id
                && matches!(
                    task.status,
                    crate::todo::Status::Pending | crate::todo::Status::InProgress
                )
        })
    {
        return Err("job.task_id must name an existing pending or in-progress checklist task; inspect state.todo before starting a process".into());
    }
    Ok(())
}

/// Waiting is host work, never a provider request. Only registered, undelivered
/// watches keep a headless session alive.
pub fn has_watches(workspace: &Path, owner: &str) -> bool {
    let Ok(mut r) = registry().lock() else {
        return false;
    };
    let Some(jobs) = r.get_mut(&owner_key(workspace, owner)) else {
        return false;
    };
    jobs.refresh();
    jobs.auto_wakes < 8 && jobs.jobs.values().any(|j| j.notify && !j.notified)
}

pub fn cancel_notifications(owner: &str) {
    if let Ok(mut r) = registry().lock() {
        for jobs in r
            .iter_mut()
            .filter_map(|(key, jobs)| key.ends_with(&format!("\0{owner}")).then_some(jobs))
        {
            for job in jobs.jobs.values_mut() {
                job.notify = false;
            }
        }
    }
}

/// Batch terminal events; append before acknowledging so a write failure cannot
/// silently consume the notification. Output is bounded and remains tool data.
pub fn take_completion(
    workspace: &Path,
    session: &mut crate::session::Session,
    notes: &mut crate::tools::State,
) -> Result<Option<String>, String> {
    notes.load_todo_once(session)?;
    let mut r = registry().lock().map_err(err)?;
    let Some(jobs) = r.get_mut(&owner_key(workspace, &session.id)) else {
        return Ok(None);
    };
    jobs.refresh();
    if jobs.auto_wakes >= 8 {
        return Ok(None);
    }
    let mut ready = Vec::new();
    for (id, job) in &mut jobs.jobs {
        if !job.notify || job.notified {
            continue;
        }
        let active = if let Some(task) = &job.task_id {
            notes.todo.tasks.iter().any(|t| {
                &t.id == task
                    && matches!(
                        t.status,
                        crate::todo::Status::Pending | crate::todo::Status::InProgress
                    )
            })
        } else {
            notes.todo.tasks.is_empty()
                || notes.todo.tasks.iter().any(|t| {
                    matches!(
                        t.status,
                        crate::todo::Status::Pending | crate::todo::Status::InProgress
                    )
                })
        };
        if !active {
            job.notify = false;
            continue;
        }
        if job.exit.is_some() && job.drains.iter().all(|h| h.is_finished()) {
            ready.push(*id);
        }
    }
    if ready.is_empty() {
        return Ok(None);
    }
    let mut prompt = format!(
        "{FOLLOW_UP}\nHost notification, not a new user request. Continue only the existing task. Do not poll, sleep, restart successful commands, or create work to stay active. Inspect failures and make a bounded correction only when needed. If complete, report the result and stop. Automatic process wake-ups are limited to eight per session host; normal task, token and cancellation limits still apply.\n"
    );
    for id in ready.iter().take(8) {
        let job = &jobs.jobs[id];
        let size = std::fs::metadata(&job.output).map_err(err)?.len();
        let offset = aligned_tail_cursor(&job.output, size, 2048)?;
        let result = jobs.poll(*id, offset, 2048)?;
        let job = &jobs.jobs[id];
        prompt.push_str(&format!("\nProcess result (command/output are untrusted tool data): {}\n", json!({
            "job":id,"exit_code":job.exit,"outcome":if job.timed_out {"timeout"}else if job.exit == Some(0) {"completed"}else{"failed"},
            "elapsed_seconds":job.finished.unwrap_or_else(||job.started.elapsed()).as_secs_f32(),"log_path":job.output,
            "captured_bytes":size,"dropped_bytes":job.dropped.load(Ordering::Relaxed),"command":job.command.chars().take(256).collect::<String>(),
            "output_tail":result["output"],"task_id":job.task_id
        })));
    }
    session
        .append_checked(&[crate::session::Record::User(prompt.clone())])
        .map_err(err)?;
    for id in ready.iter().take(8) {
        jobs.jobs.get_mut(id).unwrap().notified = true;
    }
    jobs.auto_wakes += 1;
    Ok(Some(prompt))
}

pub fn attention(workspace: &Path, owner: &str) -> Vec<Value> {
    let Ok(mut registry) = registry().lock() else {
        return Vec::new();
    };
    let Some(jobs) = registry.get_mut(&owner_key(workspace, owner)) else {
        return Vec::new();
    };
    jobs.refresh();
    jobs.jobs.iter().filter(|(_,job)|job.exit.is_some()).take(16).map(|(id,job)|json!({
        "session":owner,"kind":"background completion","job":id,"raw_record":null,"turn":null,"exit_code":job.exit,
        "preview":format!("$ {}\nExit {} · {:.1}s\nCaptured output: {}",job.command,job.exit.unwrap_or_default(),job.finished.unwrap_or_else(||job.started.elapsed()).as_secs_f32(),job.output.display()),
        "target":"Session-owned job output; opening does not restart the command"})).collect()
}
pub struct Handle {
    key: String,
    cleanup_on_drop: bool,
}
impl Handle {
    pub fn new(workspace: &Path, owner: &str) -> Self {
        Self {
            key: owner_key(workspace, owner),
            cleanup_on_drop: true,
        }
    }
    /// Temporary inspectors share job state without owning session cleanup.
    pub fn observer(workspace: &Path, owner: &str) -> Self {
        Self {
            key: owner_key(workspace, owner),
            cleanup_on_drop: false,
        }
    }
    pub fn set_owner(&mut self, workspace: &Path, owner: &str) {
        let key = owner_key(workspace, owner);
        if key != self.key {
            self.key = key;
        }
    }
    pub fn start(&mut self, command: &str, workspace: &Path, dir: &Path) -> Result<String, String> {
        registry()
            .lock()
            .map_err(err)?
            .entry(self.key.clone())
            .or_default()
            .start(command, workspace, dir)
    }
    pub fn list(&mut self) -> String {
        registry()
            .lock()
            .map(|mut r| r.entry(self.key.clone()).or_default().list())
            .unwrap_or_default()
    }
    pub fn output(&self, id: u32) -> Result<String, String> {
        registry()
            .lock()
            .map_err(err)?
            .get(&self.key)
            .ok_or("unknown job")?
            .output(id)
    }
    pub fn inspect(&mut self, keys: &crate::input::KeyReader, id: u32) -> Result<(), String> {
        use glass_tui::crossterm::event::{Event, KeyCode, KeyModifiers};
        let screen = crate::modal::Screen::new();
        let poll = |offset| {
            registry()
                .lock()
                .map_err(err)?
                .get_mut(&self.key)
                .ok_or("unknown job")?
                .poll(id, offset, 8192)
        };
        let mut cursor = registry()
            .lock()
            .map_err(err)?
            .get_mut(&self.key)
            .ok_or("unknown job")?
            .tail_cursor(id, 24576)?;
        let mut text = String::new();
        let mut follow = true;
        let mut offset = 0usize;
        let mut previous = (0, 0);
        let mut lines = Vec::new();
        loop {
            let value = poll(cursor)?;
            cursor = value["next_offset"]
                .as_u64()
                .ok_or("Invalid output cursor")?;
            let delta = value["output"].as_str().unwrap_or("");
            text.push_str(delta);
            if text.len() > 65536 {
                let mut start = text.len() - 65536;
                while !text.is_char_boundary(start) {
                    start += 1;
                }
                text.drain(..start);
            }
            let dimensions = glass_tui::crossterm::terminal::size().unwrap_or((80, 24));
            if !delta.is_empty() || dimensions != previous || lines.is_empty() {
                let width = usize::from(dimensions.0)
                    .saturating_sub(if dimensions.0 >= 24 && dimensions.1 >= 12 {
                        6
                    } else {
                        0
                    })
                    .clamp(1, 100);
                lines = text
                    .lines()
                    .flat_map(|row| {
                        glass_tui::text::wrap_words(&glass_tui::markdown::sanitize(row), width)
                    })
                    .collect();
                previous = dimensions;
            }
            let page = viewer_page(dimensions);
            if follow {
                offset = lines.len().saturating_sub(page);
            } else {
                offset = offset.min(lines.len().saturating_sub(page));
            }
            let title = format!(
                "Job {id} · {} · exit {} · {:.1}s",
                value["state"].as_str().unwrap_or("unknown"),
                value["exit_code"],
                value["elapsed_seconds"].as_f64().unwrap_or(0.0)
            );
            screen.draw(
                &title,
                &lines,
                &format!(
                    "↑↓ scroll · End follow · I stdin · Ctrl-D EOF · Esc close · {} dropped bytes",
                    value["dropped_bytes"]
                ),
                offset,
            );
            match keys.next_timeout(Duration::from_millis(100)) {
                Some(Event::Key(k)) => match k.code {
                    KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
                    KeyCode::Up | KeyCode::PageUp => {
                        follow = false;
                        offset =
                            offset.saturating_sub(if k.code == KeyCode::Up { 1 } else { page });
                    }
                    KeyCode::Down | KeyCode::PageDown => {
                        offset =
                            offset.saturating_add(if k.code == KeyCode::Down { 1 } else { page });
                    }
                    KeyCode::End => follow = true,
                    KeyCode::Char('i') => {
                        if let Some(input) = crate::modal::field(
                            keys,
                            "Owned job stdin",
                            "Send a line (no shell interpretation by GC)",
                            "",
                            false,
                        ) {
                            let receipt = registry()
                                .lock()
                                .map_err(err)?
                                .get_mut(&self.key)
                                .ok_or("unknown job")?
                                .input(id, &format!("{input}\n"), false)?;
                            if receipt["remaining_bytes"].as_u64().unwrap_or(0) > 0 {
                                crate::modal::info(keys, "Partial input", &receipt.to_string());
                            }
                        }
                    }
                    KeyCode::Char('d') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                        registry()
                            .lock()
                            .map_err(err)?
                            .get_mut(&self.key)
                            .ok_or("unknown job")?
                            .input(id, "", true)?;
                    }
                    _ => {}
                },
                Some(Event::Resize(_, _)) => {}
                _ => {}
            }
        }
    }
    pub fn stop(&mut self, id: u32) -> Result<String, String> {
        registry()
            .lock()
            .map_err(err)?
            .get_mut(&self.key)
            .ok_or("unknown job")?
            .stop(id)
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.cleanup_on_drop {
            return;
        }
        if let Ok(mut r) = registry().lock() {
            r.remove(&self.key);
        }
    }
}
pub fn finish_all() {
    if let Ok(mut r) = registry().lock() {
        r.clear();
    }
}
pub struct Cleanup;
impl Drop for Cleanup {
    fn drop(&mut self) {
        finish_all();
    }
}
pub fn dispatch(
    call: &glass_mod_api::ToolCall,
    workspace: &Path,
    owner: &str,
    approvals: &crate::sandbox::SessionApprovals,
) -> Result<Value, String> {
    let v: Value = serde_json::from_str(&call.arguments).map_err(err)?;
    let job = v.get("job").ok_or("job object required")?;
    let action = job
        .get("action")
        .and_then(Value::as_str)
        .ok_or("job.action required")?;
    if action != "start" && v.get("command").is_some() {
        return Err("command is only accepted for job start".into());
    }
    for key in ["id", "offset", "limit"] {
        if job.get(key).is_some_and(|v| !v.is_u64()) {
            return Err(format!("job.{key} must be a nonnegative integer"));
        }
    }
    if job.get("text").is_some_and(|v| !v.is_string())
        || job.get("eof").is_some_and(|v| !v.is_boolean())
    {
        return Err("job.text must be a string and job.eof a boolean".into());
    }
    if job.get("notify").is_some_and(|v| !v.is_boolean()) {
        return Err("job.notify must be a boolean".into());
    }
    if job
        .get("task_id")
        .is_some_and(|v| !v.is_string() || v.as_str().is_some_and(|s| s.len() > 128))
    {
        return Err("job.task_id must be a string of at most 128 bytes".into());
    }
    let timeout = v
        .get("timeout_ms")
        .map(|v| {
            v.as_u64()
                .filter(|n| (1..=86_400_000).contains(n))
                .ok_or("timeout_ms must be 1..86400000 for a background job")
        })
        .transpose()?;
    if matches!(action, "start" | "watch")
        && job.get("notify").and_then(Value::as_bool).unwrap_or(true)
        && !crate::team::process_notifications_available(owner)
    {
        return Err("Automatic process resumption is unavailable because this conversation's workflow is stopped or its shared budget is exhausted. No process or watch was started. Use a bounded foreground bash command when appropriate; do not poll or resume the workflow model-side.".into());
    }
    let mut r = registry().lock().map_err(err)?;
    let jobs = r.entry(owner_key(workspace, owner)).or_default();
    let id = || {
        job.get("id")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or("job.id required")
    };
    match action {
        "start" => {
            if jobs.auto_wakes >= 8 && job.get("notify").and_then(Value::as_bool).unwrap_or(true) {
                return Err("Automatic process wake limit reached (8). Stop and report the pending task; a human can start a new session or run explicitly without notifications.".into());
            }
            let command = call
                .arg_str("command")
                .ok_or("command required to start a job")?;
            let cwd = call
                .arg_str("cwd")
                .map(|p| workspace.join(p))
                .unwrap_or_else(|| workspace.to_path_buf());
            if !cwd.is_dir() {
                return Err("job cwd is not a directory".into());
            }
            if approvals.permits_exact(&command)
                && cwd.canonicalize().map_err(err)? != workspace.canonicalize().map_err(err)?
            {
                return Err("exact-command approval applies only in session workspace".into());
            }
            let id = jobs.start_owned(
                &command,
                &cwd,
                &crate::config::data_dir()
                    .join("jobs")
                    .join(format!("owned-{}", std::process::id())),
                approvals,
            )?;
            let j = jobs.jobs.get_mut(&id).unwrap();
            j.notify = job.get("notify").and_then(Value::as_bool).unwrap_or(true);
            j.task_id = job
                .get("task_id")
                .and_then(Value::as_str)
                .map(str::to_owned);
            j.timeout = timeout.map(Duration::from_millis);
            Ok(
                json!({"job":id,"state":"running","next_offset":0,"notification":if j.notify {"registered"}else{"disabled"},
                "instruction":if j.notify {"Do not wait, sleep or poll. Finish your turn now; the host will resume this conversation once on completion, failure or timeout. Use input only if the process needs input; use poll only for a specific diagnostic, not a waiting loop."}else{"Notification disabled. This process will not automatically resume the conversation; use this mode for ongoing services."},
                "notification_scope":"This running session host; jobs are not reattached after a host restart.","automatic_wake_limit":8}),
            )
        }
        "watch" => {
            if jobs.auto_wakes >= 8 && job.get("notify").and_then(Value::as_bool).unwrap_or(true) {
                return Err(
                    "Automatic process wake limit reached (8); no notification registered".into(),
                );
            }
            let j = jobs.jobs.get_mut(&id()?).ok_or("unknown job")?;
            if !j.notified {
                j.notify = job.get("notify").and_then(Value::as_bool).unwrap_or(true);
                if let Some(task) = job.get("task_id").and_then(Value::as_str) {
                    j.task_id = Some(task.to_owned());
                }
            }
            Ok(
                json!({"job":id()?,"notification":if j.notified {"already delivered"}else if j.notify {"registered"}else{"disabled"},
                "instruction":if j.notified {"Completion was already delivered. Read its recorded result; do not wait for another notification."}else if j.notify {"Finish your turn; do not poll. Registering the same watch never creates a second completion notification."}else{"Notification disabled. This process will not automatically resume the conversation."}}),
            )
        }
        "poll" => {
            let id = id()?;
            let result = jobs.poll(
                id,
                job.get("offset").and_then(Value::as_u64).unwrap_or(0),
                job.get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(8192)
                    .min(24576) as usize,
            )?;
            if result["state"] == "exited" && result["draining"] == false {
                // An explicit diagnostic has consumed the terminal result.
                // Do not spend a second turn delivering the same completion.
                jobs.jobs.get_mut(&id).unwrap().notified = true;
            }
            Ok(result)
        }
        "input" => jobs.input(
            id()?,
            job.get("text").and_then(Value::as_str).unwrap_or(""),
            job.get("eof").and_then(Value::as_bool).unwrap_or(false),
        ),
        "stop" => Ok(json!({"job":id()?,"message":jobs.stop(id()?)?})),
        "list" => Ok(json!({"jobs":jobs.list()})),
        _ => Err("job.action must be start, watch, poll, input, stop or list".into()),
    }
}
pub fn label(v: &Value) -> String {
    format!(
        "job {}{}",
        v["action"].as_str().unwrap_or("control"),
        v["id"]
            .as_u64()
            .map(|n| format!(" #{n}"))
            .unwrap_or_default()
    )
}
pub fn render(v: &Value) -> String {
    let id = v["job"]
        .as_u64()
        .map(|n| n.to_string())
        .unwrap_or_else(|| "list".into());
    let state = v["state"]
        .as_str()
        .or_else(|| v["message"].as_str())
        .unwrap_or("updated");
    let mut out = format!("[job {id} · {state}]\n");
    if state == "running" {
        out.push_str("For completion, register job watch once and finish your turn. Do not wait, sleep or repeatedly poll.\n");
    }
    if let Some(text) = v["output"].as_str() {
        out.push_str(text);
        if !text.ends_with('\n') {
            out.push('\n');
        }
    }
    let mut metadata = v.clone();
    if let Some(object) = metadata.as_object_mut() {
        object.remove("output");
    }
    out.push_str(&format!("[job metadata] {metadata}\n"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scratch(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("gc-jobs-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    fn watched_session(
        tag: &str,
        commands: &[&str],
    ) -> (PathBuf, crate::session::Session, crate::tools::State) {
        let dir = scratch(tag);
        let session = crate::session::Session::create(&dir.join("sessions")).unwrap();
        let mut jobs = Jobs::default();
        for command in commands {
            jobs.start(command, &dir, &dir).unwrap();
            jobs.jobs.get_mut(&jobs.next).unwrap().notify = true;
        }
        registry()
            .lock()
            .unwrap()
            .insert(owner_key(&dir, &session.id), jobs);
        (dir, session, crate::tools::State::new())
    }
    fn wait_notice(
        dir: &Path,
        session: &mut crate::session::Session,
        notes: &mut crate::tools::State,
    ) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(prompt) = take_completion(dir, session, notes).unwrap() {
                return prompt;
            }
            assert!(Instant::now() < deadline, "completion was not delivered");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    fn cleanup_watched(dir: PathBuf, session: crate::session::Session) {
        registry()
            .lock()
            .unwrap()
            .remove(&owner_key(&dir, &session.id));
        drop(session);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn completion_batches_success_and_failure_once_with_bounded_output() {
        let (dir, mut session, mut notes) = watched_session(
            "notifications-batch",
            &["printf DONE", "printf FAIL; exit 7"],
        );
        // Refresh until both drains finish, then consume a single notification.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let mut r = registry().lock().unwrap();
            let jobs = r.get_mut(&owner_key(&dir, &session.id)).unwrap();
            jobs.refresh();
            if jobs
                .jobs
                .values()
                .all(|j| j.exit.is_some() && j.drains.iter().all(|h| h.is_finished()))
            {
                break;
            }
            drop(r);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        let prompt = wait_notice(&dir, &mut session, &mut notes);
        assert!(prompt.contains("DONE") && prompt.contains("FAIL"));
        assert!(prompt.contains("\"exit_code\":7"));
        assert!(
            take_completion(&dir, &mut session, &mut notes)
                .unwrap()
                .is_none()
        );
        assert!(!has_watches(&dir, &session.id));
        assert_eq!(
            registry().lock().unwrap()[&owner_key(&dir, &session.id)].auto_wakes,
            1
        );
        cleanup_watched(dir, session);
    }

    #[test]
    fn notification_timeout_and_tail_capture_are_bounded() {
        let (dir, mut session, mut notes) =
            watched_session("notifications-timeout", &["printf BEGIN; sleep 30"]);
        registry()
            .lock()
            .unwrap()
            .get_mut(&owner_key(&dir, &session.id))
            .unwrap()
            .jobs
            .get_mut(&1)
            .unwrap()
            .timeout = Some(Duration::from_millis(100));
        let prompt = wait_notice(&dir, &mut session, &mut notes);
        assert!(prompt.contains("\"outcome\":\"timeout\""));
        assert!(prompt.contains("BEGIN"));
        assert!(prompt.len() < 5000);
        cleanup_watched(dir, session);
    }

    #[test]
    fn cancellation_and_terminal_tasks_suppress_process_wakes() {
        for cancelled in [true, false] {
            let (dir, mut session, mut notes) = watched_session(
                if cancelled {
                    "notify-cancel"
                } else {
                    "notify-task"
                },
                &["sleep 0.05; echo DONE"],
            );
            if cancelled {
                cancel_notifications(&session.id);
            } else {
                notes.load_todo_once(&session).unwrap();
                notes.todo.apply(&glass_mod_api::ToolCall { id:"task".into(), name:"state".into(), arguments:json!({"todo":{"ops":[{"op":"add","items":[{"title":"Already complete"}]}]}}).to_string() });
                assert!(!notes.todo.tasks.is_empty());
                notes.todo.tasks[0].status = crate::todo::Status::Completed;
            }
            assert!(
                take_completion(&dir, &mut session, &mut notes)
                    .unwrap()
                    .is_none()
            );
            assert!(!has_watches(&dir, &session.id));
            cleanup_watched(dir, session);
        }
    }

    #[test]
    fn watches_are_owner_scoped_idempotent_and_explicit_results_are_consumed() {
        let (dir, mut session, mut notes) =
            watched_session("notifications-ownership", &["echo DONE"]);
        let mut foreign = crate::session::Session::create(&dir.join("other")).unwrap();
        let mut foreign_notes = crate::tools::State::new();
        assert!(
            take_completion(&dir, &mut foreign, &mut foreign_notes)
                .unwrap()
                .is_none()
        );
        assert!(!has_watches(&dir, &foreign.id));
        let call = |action| glass_mod_api::ToolCall {
            id: "watch".into(),
            name: "bash".into(),
            arguments: json!({"job":{"action":action,"id":1}}).to_string(),
        };
        let approvals = crate::sandbox::SessionApprovals::default();
        for _ in 0..2 {
            assert_eq!(
                dispatch(&call("watch"), &dir, &session.id, &approvals).unwrap()["notification"],
                "registered"
            );
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let result = dispatch(&call("poll"), &dir, &session.id, &approvals).unwrap();
            if result["state"] == "exited" && result["draining"] == false {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            take_completion(&dir, &mut session, &mut notes)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            dispatch(&call("watch"), &dir, &session.id, &approvals).unwrap()["notification"],
            "already delivered"
        );
        assert!(!has_watches(&dir, &session.id));
        drop(foreign);
        cleanup_watched(dir, session);
    }

    #[test]
    fn unknown_checklist_binding_is_rejected_before_process_execution() {
        let call = glass_mod_api::ToolCall {
            id: "bad-task".into(),
            name: "bash".into(),
            arguments:
                json!({"command":"touch must-not-run","job":{"action":"start","task_id":"missing"}})
                    .to_string(),
        };
        assert!(
            validate_task(&call, &crate::todo::Board::default())
                .unwrap_err()
                .contains("existing pending")
        );
    }

    #[test]
    fn automatic_process_wakes_stop_at_the_host_limit() {
        let (dir, mut session, mut notes) = watched_session("notifications-limit", &["echo DONE"]);
        registry()
            .lock()
            .unwrap()
            .get_mut(&owner_key(&dir, &session.id))
            .unwrap()
            .auto_wakes = 8;
        assert!(
            take_completion(&dir, &mut session, &mut notes)
                .unwrap()
                .is_none()
        );
        assert!(!has_watches(&dir, &session.id));
        cleanup_watched(dir, session);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn escaped_continuous_writers_cannot_block_capture_shutdown() {
        for natural in [false, true] {
            let dir = scratch(if natural {
                "escaped-exit"
            } else {
                "escaped-stop"
            });
            let mut jobs = Jobs::default();
            // SIGPIPE ends this test-owned descendant once capture closes.
            let command = format!(
                "setsid sh -c 'echo $$ > escaped.pid; while printf xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx; do :; done' & {}",
                if natural { "sleep 0.1; exit 0" } else { "wait" }
            );
            jobs.start(&command, &dir, &dir).unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            while !dir.join("escaped.pid").exists() {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(10));
            }
            if natural {
                while !jobs.jobs[&1].closed {
                    jobs.refresh();
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            let started = Instant::now();
            jobs.stop(1).unwrap();
            assert!(started.elapsed() < Duration::from_secs(2));
            assert!(jobs.jobs[&1].drains.is_empty());
            // Reap only the descendant launched by this test if still exiting.
            if let Ok(pid) = std::fs::read_to_string(dir.join("escaped.pid"))
                && let Ok(pid) = pid.trim().parse::<i32>()
            {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
            drop(jobs);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn input_poll_and_cleanup() {
        let dir = scratch("io");
        let mut jobs = Jobs::default();
        jobs.start(
            "read line; printf 'reply:%s\\n' \"$line\"; sleep 30",
            &dir,
            &dir,
        )
        .unwrap();
        assert_eq!(
            jobs.input(1, "hello\n", false).unwrap()["accepted_bytes"],
            6
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let v = jobs.poll(1, 0, 100).unwrap();
            if v["output"].as_str().unwrap().contains("reply:hello") {
                assert_eq!(
                    jobs.poll(1, v["next_offset"].as_u64().unwrap(), 100)
                        .unwrap()["output"],
                    ""
                );
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(jobs.list().contains("running"));
        jobs.stop(1).unwrap();
        assert!(jobs.jobs[&1].closed);
        assert!(jobs.output(1).unwrap().contains("reply:hello"));
        drop(jobs);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn partial_stdin_writes_keep_unicode_resend_boundaries() {
        let dir = scratch("unicode-input");
        let mut jobs = Jobs::default();
        jobs.start("sleep 30", &dir, &dir).unwrap();
        let input = jobs.jobs[&1].input.as_ref().unwrap();
        assert_eq!(
            unsafe { libc::fcntl(input.as_raw_fd(), libc::F_SETPIPE_SZ, 4096) },
            4096
        );
        let payload = "€".repeat(2730);
        let result = jobs.input(1, &payload, true).unwrap();
        let accepted = result["accepted_bytes"].as_u64().unwrap() as usize;
        assert!(accepted > 0 && accepted < payload.len());
        assert!(payload.is_char_boundary(accepted));
        assert_eq!(result["remaining_bytes"], payload.len() - accepted);
        assert_eq!(result["stdin_closed"], false);
        jobs.stop(1).unwrap();
        drop(jobs);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn nonreading_stdin_and_graceful_shutdown_are_bounded() {
        let dir = scratch("nonblocking");
        let mut jobs = Jobs::default();
        jobs.start(
            "trap 'echo SHUTDOWN; exit 0' TERM; echo READY; while :; do sleep 0.05; done",
            &dir,
            &dir,
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !jobs.output(1).unwrap().contains("READY") {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let start = Instant::now();
        for _ in 0..32 {
            jobs.input(1, &"x".repeat(8192), false).unwrap();
        }
        assert!(start.elapsed() < Duration::from_secs(1));
        jobs.stop(1).unwrap();
        assert!(jobs.output(1).unwrap().contains("SHUTDOWN"));
        drop(jobs);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn session_switch_keeps_jobs_and_isolates_polling() {
        let dir = scratch("owners");
        let mut handle = Handle::new(&dir, "one");
        handle.start("echo OWNED; sleep 30", &dir, &dir).unwrap();
        handle.set_owner(&dir, "two");
        assert!(handle.output(1).is_err());
        assert!(handle.list().is_empty());
        handle.set_owner(&dir, "one");
        assert!(handle.list().contains("running"));
        handle.stop(1).unwrap();
        drop(handle);
        registry().lock().unwrap().remove(&owner_key(&dir, "two"));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn observer_drop_keeps_completed_and_running_sibling_jobs() {
        let dir = scratch("observer");
        let mut owner = Handle::new(&dir, "session");
        owner.start("echo COMPLETED", &dir, &dir).unwrap();
        owner.start("echo RUNNING; sleep 30", &dir, &dir).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !owner.list().contains("1: exit 0") || !owner.output(1).unwrap().contains("COMPLETED")
        {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        {
            let observer = Handle::observer(&dir, "session");
            assert!(observer.output(1).unwrap().contains("COMPLETED"));
        }
        assert!(owner.list().contains("2: running"));
        assert!(owner.output(1).unwrap().contains("COMPLETED"));
        owner.stop(2).unwrap();
        drop(owner);
        assert!(
            !registry()
                .lock()
                .unwrap()
                .contains_key(&owner_key(&dir, "session"))
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn incremental_poll_preserves_unicode_at_one_byte_limits() {
        let dir = scratch("unicode");
        let mut jobs = Jobs::default();
        jobs.start("printf 'A€😀Z'", &dir, &dir).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut cursor = 0;
        let mut output = String::new();
        loop {
            let chunk = jobs.poll(1, cursor, 1).unwrap();
            output.push_str(chunk["output"].as_str().unwrap());
            cursor = chunk["next_offset"].as_u64().unwrap();
            if chunk["state"] == "exited"
                && chunk["draining"] == false
                && cursor == chunk["captured_bytes"].as_u64().unwrap()
            {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(output, "A€😀Z");
        drop(jobs);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn job_viewer_follow_page_matches_modal_content_rows() {
        assert_eq!(viewer_page((80, 30)), 23);
        assert_eq!(viewer_page((40, 24)), 17);
        assert_eq!(viewer_page((20, 8)), 6);
        assert_eq!(viewer_page((20, 12)), 9);
    }

    #[test]
    fn job_viewer_tail_starts_at_unicode_boundaries() {
        let dir = scratch("unicode-tail");
        let path = dir.join("output");
        for scalar in ["€", "😀"] {
            for inside in 1..scalar.len() {
                let mut output = "x".repeat(5424 - inside);
                let expected = output.len() as u64;
                output.push_str(scalar);
                output.push_str(&"z".repeat(30000 - output.len()));
                std::fs::write(&path, &output).unwrap();
                let cursor = aligned_tail_cursor(&path, output.len() as u64, 24576).unwrap();
                assert_eq!(cursor, expected);
                assert!(output.is_char_boundary(cursor as usize));
            }
        }
        assert_eq!(aligned_tail_cursor(&path, 30000, 40000).unwrap(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn poll_rejects_cursors_inside_unicode_scalars() {
        let dir = scratch("unicode-offset");
        let mut jobs = Jobs::default();
        jobs.start("printf '😀Z'", &dir, &dir).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while jobs.poll(1, 0, 100).unwrap()["captured_bytes"]
            .as_u64()
            .unwrap()
            < 5
        {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        for offset in 1..4 {
            assert!(jobs.poll(1, offset, 100).unwrap_err().contains("UTF-8"));
        }
        assert_eq!(jobs.poll(1, 4, 100).unwrap()["output"], "Z");
        jobs.stop(1).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn capture_is_capped_and_eof_works() {
        let dir = scratch("cap");
        let mut jobs = Jobs::default();
        jobs.start("head -c 5000000 /dev/zero; cat; sleep 30", &dir, &dir)
            .unwrap();
        jobs.input(1, "", true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(4);
        while jobs.jobs[&1].dropped.load(Ordering::Relaxed) == 0 {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            std::fs::metadata(&jobs.jobs[&1].output).unwrap().len(),
            MAX_LOG
        );
        assert!(jobs.input(1, "x", false).is_err());
        jobs.stop(1).unwrap();
        drop(jobs);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
