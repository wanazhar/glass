//! Session-owned process groups with bounded capture, incremental polling and stdin.
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
}
#[derive(Default)]
pub struct Jobs {
    jobs: BTreeMap<u32, Job>,
    next: u32,
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
                "sandbox = strict is not available ({reason}); refusing jobs unsandboxed"
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
            let mut c = Command::new("/bin/sh");
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
        if let Ok(exe) = std::env::current_exe() {
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
                loop {
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
            },
        );
        Ok(self.next)
    }
    fn refresh(&mut self) {
        for j in self.jobs.values_mut() {
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
            return Err("offset is inside a UTF-8 character; use the previous next_offset cursor".into());
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
        if j.closed {
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
pub fn attention(workspace:&Path,owner:&str)->Vec<Value> {
    let Ok(mut registry)=registry().lock() else{return Vec::new();};
    let Some(jobs)=registry.get_mut(&owner_key(workspace,owner)) else{return Vec::new();};
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
        Self { key: owner_key(workspace, owner), cleanup_on_drop: false }
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
    pub fn inspect(&mut self, keys:&crate::input::KeyReader,id:u32)->Result<(),String> {
        use glass_tui::crossterm::event::{Event,KeyCode,KeyModifiers};
        let screen=crate::modal::Screen::new();
        let poll=|offset|registry().lock().map_err(err)?.get_mut(&self.key).ok_or("unknown job")?.poll(id,offset,8192);
        let first=poll(0)?;
        let mut cursor=first["captured_bytes"].as_u64().unwrap_or(0).saturating_sub(24576);
        let mut text=String::new();let mut follow=true;let mut offset=0usize;
        let mut previous=(0,0);let mut lines=Vec::new();
        loop {
            let value=poll(cursor)?;
            cursor=value["next_offset"].as_u64().ok_or("Invalid output cursor")?;
            let delta=value["output"].as_str().unwrap_or("");
            text.push_str(delta);
            if text.len()>65536 {
                let mut start=text.len()-65536;while !text.is_char_boundary(start){start+=1;}
                text.drain(..start);
            }
            let dimensions=glass_tui::crossterm::terminal::size().unwrap_or((80,24));
            if !delta.is_empty()||dimensions!=previous||lines.is_empty() {
                let width=usize::from(dimensions.0).saturating_sub(if dimensions.0>=24&&dimensions.1>=12 {6}else{0}).clamp(1,100);
                lines=text.lines().flat_map(|row|glass_tui::text::wrap_words(&glass_tui::markdown::sanitize(row),width)).collect();
                previous=dimensions;
            }
            let page=usize::from(dimensions.1).saturating_sub(6).max(1);
            if follow{offset=lines.len().saturating_sub(page);}else{offset=offset.min(lines.len().saturating_sub(page));}
            let title=format!("Job {id} · {} · exit {} · {:.1}s",value["state"].as_str().unwrap_or("unknown"),value["exit_code"],value["elapsed_seconds"].as_f64().unwrap_or(0.0));
            screen.draw(&title,&lines,&format!("↑↓ scroll · End follow · I stdin · Ctrl-D EOF · Esc close · {} dropped bytes",value["dropped_bytes"]),offset);
            match keys.next_timeout(Duration::from_millis(100)) {
                Some(Event::Key(k))=>match k.code {
                    KeyCode::Esc|KeyCode::Char('q')=>return Ok(()),
                    KeyCode::Up|KeyCode::PageUp=>{follow=false;offset=offset.saturating_sub(if k.code==KeyCode::Up{1}else{page});},
                    KeyCode::Down|KeyCode::PageDown=>{offset=offset.saturating_add(if k.code==KeyCode::Down{1}else{page});},
                    KeyCode::End=>follow=true,
                    KeyCode::Char('i')=>if let Some(input)=crate::modal::field(keys,"Owned job stdin","Send a line (no shell interpretation by GC)","",false) {
                        let receipt=registry().lock().map_err(err)?.get_mut(&self.key).ok_or("unknown job")?.input(id,&format!("{input}\n"),false)?;
                        if receipt["remaining_bytes"].as_u64().unwrap_or(0)>0 {crate::modal::info(keys,"Partial input",&receipt.to_string());}
                    },
                    KeyCode::Char('d') if k.modifiers.contains(KeyModifiers::CONTROL)=>{
                        registry().lock().map_err(err)?.get_mut(&self.key).ok_or("unknown job")?.input(id,"",true)?;
                    },_=>{},
                },Some(Event::Resize(_, _))=>{},_=>{},
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
        if !self.cleanup_on_drop { return; }
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
            Ok(json!({"job":id,"state":"running","next_offset":0}))
        }
        "poll" => jobs.poll(
            id()?,
            job.get("offset").and_then(Value::as_u64).unwrap_or(0),
            job.get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(8192)
                .min(24576) as usize,
        ),
        "input" => jobs.input(
            id()?,
            job.get("text").and_then(Value::as_str).unwrap_or(""),
            job.get("eof").and_then(Value::as_bool).unwrap_or(false),
        ),
        "stop" => Ok(json!({"job":id()?,"message":jobs.stop(id()?)?})),
        "list" => Ok(json!({"jobs":jobs.list()})),
        _ => Err("job.action must be start, poll, input, stop or list".into()),
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
        while !owner.list().contains("1: exit 0") || !owner.output(1).unwrap().contains("COMPLETED") {
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
        assert!(!registry().lock().unwrap().contains_key(&owner_key(&dir, "session")));
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
            if chunk["state"] == "exited" && chunk["draining"] == false
                && cursor == chunk["captured_bytes"].as_u64().unwrap() { break; }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(output, "A€😀Z");
        drop(jobs);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn poll_rejects_cursors_inside_unicode_scalars() {
        let dir = scratch("unicode-offset");
        let mut jobs = Jobs::default();
        jobs.start("printf '😀Z'", &dir, &dir).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while jobs.poll(1, 0, 100).unwrap()["captured_bytes"].as_u64().unwrap() < 5 {
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
