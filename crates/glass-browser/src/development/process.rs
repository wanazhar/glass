use super::{DevelopmentError, DevelopmentResult, MAX_PROCESS_OUTPUT_BYTES};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use std::{
    collections::{BTreeMap, VecDeque},
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProcessState {
    Running,
    Exited { code: Option<u32> },
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProcessHealth {
    Starting,
    Healthy,
    Exited,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSnapshot {
    pub name: String,
    pub command: String,
    pub pid: Option<u32>,
    pub state: ProcessState,
    pub started_at_ms: u64,
    pub output: String,
    pub pty: bool,
    pub cwd: PathBuf,
    pub health: ProcessHealth,
    #[serde(default)]
    pub detected_urls: Vec<String>,
}

struct RunningProcess {
    snapshot: ProcessSnapshot,
    child: Box<dyn Child + Send + Sync>,
    master: Option<Box<dyn MasterPty + Send>>,
    writer: Option<Box<dyn Write + Send>>,
    output: Arc<Mutex<VecDeque<u8>>>,
    reader: Option<thread::JoinHandle<()>>,
    reader_done: mpsc::Receiver<()>,
    #[cfg(windows)]
    job: WindowsJob,
}

pub struct ProcessManager {
    root: PathBuf,
    processes: BTreeMap<String, RunningProcess>,
}

impl std::fmt::Debug for ProcessManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProcessManager")
            .field("root", &self.root)
            .field("process_count", &self.processes.len())
            .finish()
    }
}

impl ProcessManager {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            processes: BTreeMap::new(),
        }
    }

    pub fn start(&mut self, name: &str, command: &str) -> DevelopmentResult<ProcessSnapshot> {
        validate_name(name)?;
        if command.trim().is_empty() || command.len() > 4 * 1024 {
            return Err(DevelopmentError::InvalidInput(
                "process command must be non-empty and at most 4096 bytes".into(),
            ));
        }
        self.poll()?;
        if self.processes.len() >= 32 {
            return Err(DevelopmentError::Process(
                "process manager is limited to 32 registered processes".into(),
            ));
        }
        if self.processes.contains_key(name) {
            return Err(DevelopmentError::Process(format!(
                "process {name} is already registered"
            )));
        }
        let pty = native_pty_system()
            .openpty(PtySize {
                rows: 32,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| DevelopmentError::Process(error.to_string()))?;
        let mut builder = shell_command(command);
        builder.cwd(self.root.as_os_str());
        let child = pty
            .slave
            .spawn_command(builder)
            .map_err(|error| DevelopmentError::Process(error.to_string()))?;
        #[cfg(windows)]
        let mut child = child;
        let pid = child.process_id();
        #[cfg(windows)]
        let job = match pid
            .ok_or_else(|| std::io::Error::other("PTY child PID is unavailable"))
            .and_then(WindowsJob::assign)
        {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill();
                return Err(DevelopmentError::Process(format!(
                    "failed to place PTY process in a kill-on-close Windows Job Object: {error}"
                )));
            }
        };
        let reader = pty
            .master
            .try_clone_reader()
            .map_err(|error| DevelopmentError::Process(error.to_string()))?;
        let writer = pty
            .master
            .take_writer()
            .map_err(|error| DevelopmentError::Process(error.to_string()))?;
        #[cfg(windows)]
        let writer = {
            let mut writer = writer;
            // ConPTY asks the terminal for its cursor position (`CSI 6 n`)
            // before dispatching some short-lived commands. Seed the canonical
            // response so the shell cannot stall before Glass' reader starts.
            writer.write_all(b"\x1b[1;1R")?;
            writer.flush()?;
            writer
        };
        let output = Arc::new(Mutex::new(VecDeque::new()));
        let reader_output = Arc::clone(&output);
        let (reader_done_tx, reader_done) = mpsc::channel();
        let reader_handle = thread::Builder::new()
            .name(format!("glass-process-{name}"))
            .spawn(move || {
                read_output(reader, reader_output);
                let _ = reader_done_tx.send(());
            })
            .map_err(DevelopmentError::Io)?;
        let snapshot = ProcessSnapshot {
            name: name.into(),
            command: command.into(),
            pid,
            state: ProcessState::Running,
            started_at_ms: now_ms(),
            output: String::new(),
            pty: true,
            cwd: self.root.clone(),
            health: ProcessHealth::Starting,
            detected_urls: Vec::new(),
        };
        self.processes.insert(
            name.into(),
            RunningProcess {
                snapshot: snapshot.clone(),
                child,
                master: Some(pty.master),
                writer: Some(writer),
                output,
                reader: Some(reader_handle),
                reader_done,
                #[cfg(windows)]
                job,
            },
        );
        Ok(snapshot)
    }

    pub fn send_input(&mut self, name: &str, input: &str) -> DevelopmentResult<()> {
        if input.len() > 16 * 1024 {
            return Err(DevelopmentError::InvalidInput(
                "process input exceeds 16384 bytes".into(),
            ));
        }
        let process = self
            .processes
            .get_mut(name)
            .ok_or_else(|| DevelopmentError::NotFound(format!("process {name}")))?;
        let writer = process
            .writer
            .as_mut()
            .ok_or_else(|| DevelopmentError::Process("process input is closed".into()))?;
        writer.write_all(input.as_bytes())?;
        writer.flush()?;
        Ok(())
    }

    /// Finalize process input while retaining output and lifecycle tracking.
    ///
    /// Bounded, non-interactive commands use this after spawning. Unix shells
    /// observe EOF. Windows ConPTY turns a closed input pipe into Control-C, so
    /// its owned console receives the platform EOF key (Control-Z then Enter)
    /// and retains the input handle until the child exits naturally.
    pub fn close_input(&mut self, name: &str) -> DevelopmentResult<()> {
        let process = self
            .processes
            .get_mut(name)
            .ok_or_else(|| DevelopmentError::NotFound(format!("process {name}")))?;

        #[cfg(windows)]
        if let Some(writer) = process.writer.as_mut() {
            writer.write_all(b"\x1a\r\n")?;
            writer.flush()?;
            return Ok(());
        }

        #[cfg(not(windows))]
        process.writer.take();
        Ok(())
    }

    /// Run a bounded non-interactive command without routing it through ConPTY.
    ///
    /// Windows pseudo consoles intentionally model an interactive terminal and
    /// do not provide portable pipe-EOF semantics. One-shot commands use normal
    /// pipes while retaining the same Job Object descendant ownership.
    #[cfg(windows)]
    pub fn run_bounded(
        &self,
        name: &str,
        command: &str,
        timeout: Duration,
    ) -> DevelopmentResult<ProcessSnapshot> {
        validate_name(name)?;
        if command.trim().is_empty() || command.len() > 4 * 1024 {
            return Err(DevelopmentError::InvalidInput(
                "process command must be non-empty and at most 4096 bytes".into(),
            ));
        }
        let started_at_ms = now_ms();
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/D", "/Q", "/C", command])
            .current_dir(&self.root)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        let job = WindowsJob::assign(child.id()).map_err(|error| {
            let _ = child.kill();
            DevelopmentError::Process(format!(
                "failed to own bounded Windows process tree: {error}"
            ))
        })?;
        let output = Arc::new(Mutex::new(VecDeque::new()));
        let mut readers = Vec::with_capacity(2);
        if let Some(reader) = child.stdout.take() {
            let reader_output = Arc::clone(&output);
            readers.push(thread::spawn(move || read_output(reader, reader_output)));
        }
        if let Some(reader) = child.stderr.take() {
            let reader_output = Arc::clone(&output);
            readers.push(thread::spawn(move || read_output(reader, reader_output)));
        }
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                // SAFETY: `job` exclusively owns this valid handle.
                unsafe {
                    windows_sys::Win32::System::JobObjects::TerminateJobObject(job.handle, 1)
                };
                let _ = child.wait();
                for reader in readers {
                    let _ = reader.join();
                }
                return Err(DevelopmentError::Process(format!(
                    "process {name} exceeded {} seconds",
                    timeout.as_secs()
                )));
            }
            thread::sleep(Duration::from_millis(20));
        };
        for reader in readers {
            reader
                .join()
                .map_err(|_| DevelopmentError::Process("process output reader panicked".into()))?;
        }
        let output = output_string(&output);
        Ok(ProcessSnapshot {
            name: name.into(),
            command: command.into(),
            pid: Some(child.id()),
            state: ProcessState::Exited {
                code: status.code().map(|code| code as u32),
            },
            started_at_ms,
            output: output.clone(),
            pty: false,
            cwd: self.root.clone(),
            health: if status.success() {
                ProcessHealth::Exited
            } else {
                ProcessHealth::Failed
            },
            detected_urls: detect_urls(&output),
        })
    }

    pub fn resize(&mut self, name: &str, cols: u16, rows: u16) -> DevelopmentResult<()> {
        if cols == 0 || rows == 0 {
            return Err(DevelopmentError::InvalidInput(
                "PTY dimensions must be non-zero".into(),
            ));
        }
        let process = self
            .processes
            .get_mut(name)
            .ok_or_else(|| DevelopmentError::NotFound(format!("process {name}")))?;
        process
            .master
            .as_ref()
            .ok_or_else(|| DevelopmentError::Process("process PTY is closed".into()))?
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| DevelopmentError::Process(error.to_string()))
    }

    pub fn stop(&mut self, name: &str) -> DevelopmentResult<ProcessSnapshot> {
        let process = self
            .processes
            .get_mut(name)
            .ok_or_else(|| DevelopmentError::NotFound(format!("process {name}")))?;
        terminate_process_tree(process)?;
        finish_reader(process)?;
        process.snapshot.state = ProcessState::Stopped;
        process.snapshot.health = ProcessHealth::Stopped;
        Ok(snapshot(process))
    }

    pub fn restart(&mut self, name: &str) -> DevelopmentResult<ProcessSnapshot> {
        let command = self
            .processes
            .get(name)
            .ok_or_else(|| DevelopmentError::NotFound(format!("process {name}")))?
            .snapshot
            .command
            .clone();
        if matches!(
            self.processes
                .get(name)
                .map(|process| &process.snapshot.state),
            Some(ProcessState::Running)
        ) {
            self.stop(name)?;
        }
        self.processes.remove(name);
        self.start(name, &command)
    }

    pub fn remove(&mut self, name: &str) -> DevelopmentResult<ProcessSnapshot> {
        if matches!(
            self.processes
                .get(name)
                .map(|process| &process.snapshot.state),
            Some(ProcessState::Running)
        ) {
            return Err(DevelopmentError::Conflict(format!(
                "stop process {name} before removing it"
            )));
        }
        self.processes
            .remove(name)
            .map(|process| snapshot(&process))
            .ok_or_else(|| DevelopmentError::NotFound(format!("process {name}")))
    }

    pub fn poll(&mut self) -> DevelopmentResult<Vec<ProcessSnapshot>> {
        let mut updated = Vec::new();
        for process in self.processes.values_mut() {
            if matches!(process.snapshot.state, ProcessState::Running)
                && let Some(status) = process
                    .child
                    .try_wait()
                    .map_err(|error| DevelopmentError::Process(error.to_string()))?
            {
                process.snapshot.state = ProcessState::Exited {
                    code: Some(status.exit_code()),
                };
                process.snapshot.health = if status.success() {
                    ProcessHealth::Exited
                } else {
                    ProcessHealth::Failed
                };
                finish_reader(process)?;
            }
            process.snapshot.output = output_string(&process.output);
            process.snapshot.detected_urls = detect_urls(&process.snapshot.output);
            if matches!(process.snapshot.state, ProcessState::Running)
                && (!process.snapshot.detected_urls.is_empty()
                    || process
                        .snapshot
                        .output
                        .to_ascii_lowercase()
                        .contains("ready"))
            {
                process.snapshot.health = ProcessHealth::Healthy;
            }
            updated.push(snapshot(process));
        }
        Ok(updated)
    }

    pub fn list(&mut self) -> Vec<ProcessSnapshot> {
        self.list_checked().unwrap_or_else(|error| {
            self.processes
                .values_mut()
                .map(|process| {
                    process.snapshot.state = ProcessState::Failed;
                    process.snapshot.health = ProcessHealth::Failed;
                    let mut value = snapshot(process);
                    value.output =
                        format!("{}\n[process state unavailable: {error}]", value.output);
                    value
                })
                .collect()
        })
    }

    /// Return current process state without discarding polling failures.
    pub fn list_checked(&mut self) -> DevelopmentResult<Vec<ProcessSnapshot>> {
        self.poll()
    }

    pub fn output(&self, name: &str) -> DevelopmentResult<String> {
        let process = self
            .processes
            .get(name)
            .ok_or_else(|| DevelopmentError::NotFound(format!("process {name}")))?;
        Ok(output_string(&process.output))
    }
}

impl Drop for ProcessManager {
    fn drop(&mut self) {
        for process in self.processes.values_mut() {
            if matches!(process.snapshot.state, ProcessState::Running) {
                let _ = terminate_process_tree(process);
                let _ = finish_reader(process);
            }
        }
    }
}

fn finish_reader(process: &mut RunningProcess) -> DevelopmentResult<()> {
    process.writer.take();
    #[cfg(windows)]
    process.master.take();
    if process.reader.is_none() {
        return Ok(());
    }
    match process.reader_done.recv_timeout(Duration::from_secs(2)) {
        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {}
        Err(mpsc::RecvTimeoutError::Timeout) => {
            return Err(DevelopmentError::Process(
                "PTY output did not close within 2 seconds after process exit".into(),
            ));
        }
    }
    let reader = process
        .reader
        .take()
        .expect("reader existence checked above");
    reader
        .join()
        .map_err(|_| DevelopmentError::Process("PTY output reader panicked".into()))
}

fn terminate_process_tree(process: &mut RunningProcess) -> DevelopmentResult<()> {
    #[cfg(unix)]
    {
        let pid = process.snapshot.pid.ok_or_else(|| {
            DevelopmentError::Process("owned PTY process has no process-group ID".into())
        })?;
        // portable-pty creates a new session for the spawned command on Unix;
        // its PID is therefore also the process-group ID. Signal the group so
        // grandchildren cannot outlive the owning Glass workspace.
        for (signal, grace) in [
            (libc::SIGINT, Duration::from_millis(250)),
            (libc::SIGTERM, Duration::from_millis(500)),
        ] {
            let result = unsafe { libc::kill(-(pid as i32), signal) };
            if result != 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(DevelopmentError::Process(format!(
                        "failed to signal process group {pid}: {error}"
                    )));
                }
            }
            let deadline = Instant::now() + grace;
            while Instant::now() < deadline {
                if process
                    .child
                    .try_wait()
                    .map_err(|error| DevelopmentError::Process(error.to_string()))?
                    .is_some()
                {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        let result = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
        if result != 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
            return Err(DevelopmentError::Process(format!(
                "failed to kill process group {pid}: {}",
                std::io::Error::last_os_error()
            )));
        }
        let _ = process.child.wait();
        Ok(())
    }

    #[cfg(windows)]
    {
        // SAFETY: the handle is created and exclusively owned by WindowsJob
        // until this process record is dropped.
        let terminated = unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(process.job.handle, 1)
        };
        if terminated == 0 {
            return Err(DevelopmentError::Process(format!(
                "failed to terminate Windows process job: {}",
                std::io::Error::last_os_error()
            )));
        }
        let _ = process.child.wait();
        return Ok(());
    }

    #[cfg(not(any(unix, windows)))]
    process
        .child
        .kill()
        .map_err(|error| DevelopmentError::Process(error.to_string()))?;
    #[cfg(not(any(unix, windows)))]
    let _ = process.child.wait();
    #[cfg(not(any(unix, windows)))]
    Ok(())
}

#[cfg(windows)]
struct WindowsJob {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
// SAFETY: WindowsJob uniquely owns the kernel handle and only uses it through
// thread-safe kernel object operations; Drop closes it exactly once.
unsafe impl Send for WindowsJob {}

#[cfg(windows)]
impl WindowsJob {
    fn assign(pid: u32) -> std::io::Result<Self> {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::{
                JobObjects::{
                    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                    SetInformationJobObject,
                },
                Threading::{
                    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA,
                    PROCESS_TERMINATE,
                },
            },
        };
        // SAFETY: null security/name creates an unnamed job owned by this process.
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: the information pointer and size match the requested class.
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        };
        // SAFETY: requested rights are limited to job assignment and lifecycle.
        let process = unsafe {
            OpenProcess(
                PROCESS_SET_QUOTA | PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                pid,
            )
        };
        if configured == 0 || process.is_null() {
            // SAFETY: non-null job handle is owned here.
            unsafe { CloseHandle(handle) };
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: both handles are valid and owned for the duration of the call.
        let assigned = unsafe { AssignProcessToJobObject(handle, process) };
        // SAFETY: the temporary process handle is no longer needed.
        unsafe { CloseHandle(process) };
        if assigned == 0 {
            // SAFETY: assignment failed; closing the owned job is sufficient cleanup.
            unsafe { CloseHandle(handle) };
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self { handle })
    }
}

#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        // SAFETY: this type exclusively owns the job handle.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.handle) };
    }
}

fn snapshot(process: &RunningProcess) -> ProcessSnapshot {
    let mut snapshot = process.snapshot.clone();
    snapshot.output = output_string(&process.output);
    snapshot
}

fn read_output(mut reader: impl Read, output: Arc<Mutex<VecDeque<u8>>>) {
    let mut buffer = [0_u8; 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                if let Ok(mut output) = output.lock() {
                    for byte in &buffer[..count] {
                        if output.len() == MAX_PROCESS_OUTPUT_BYTES {
                            output.pop_front();
                        }
                        output.push_back(*byte);
                    }
                }
            }
        }
    }
}

fn output_string(output: &Mutex<VecDeque<u8>>) -> String {
    output
        .lock()
        .map(|value| {
            let bytes: Vec<u8> = value.iter().copied().collect();
            String::from_utf8_lossy(&bytes).into_owned()
        })
        .unwrap_or_else(|_| "process output unavailable".into())
}

fn shell_command(command: &str) -> CommandBuilder {
    #[cfg(windows)]
    {
        let mut builder = CommandBuilder::new("cmd.exe");
        builder.args(["/C", command]);
        builder
    }
    #[cfg(not(windows))]
    {
        let mut builder = CommandBuilder::new("sh");
        builder.args(["-lc", command]);
        builder
    }
}

fn validate_name(name: &str) -> DevelopmentResult<()> {
    if name.is_empty()
        || name.len() > 64
        || name
            .chars()
            .any(|character| !(character.is_ascii_alphanumeric() || "-_.".contains(character)))
    {
        return Err(DevelopmentError::InvalidInput(
            "process name must be 1-64 ASCII letters, digits, '.', '_' or '-'".into(),
        ));
    }
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

fn detect_urls(output: &str) -> Vec<String> {
    let mut urls = output
        .split_whitespace()
        .filter_map(|token| {
            let token = token.trim_matches(|character: char| {
                matches!(character, '(' | ')' | '[' | ']' | ',' | ';' | '\'' | '"')
            });
            let parsed = url::Url::parse(token).ok()?;
            if matches!(parsed.scheme(), "http" | "https")
                && matches!(
                    parsed.host_str(),
                    Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
                )
            {
                Some(parsed.to_string())
            } else {
                None
            }
        })
        .take(8)
        .collect::<Vec<_>>();
    urls.sort();
    urls.dedup();
    urls
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn pty_process_emits_output_and_exits() {
        let mut manager = ProcessManager::new(std::env::temp_dir());
        manager.start("echo", "echo glass-process-ok").unwrap();
        manager.close_input("echo").unwrap();
        for _ in 0..40 {
            let snapshots = manager.poll().unwrap();
            if snapshots[0].state != ProcessState::Running
                && !manager.output("echo").unwrap().is_empty()
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = manager.output("echo").unwrap();
        assert!(output.contains("glass-process-ok"), "output={output:?}");
        assert!(matches!(
            manager.list()[0].state,
            ProcessState::Exited { .. }
        ));
    }

    #[test]
    fn process_detects_local_dev_server_urls_and_can_restart() {
        let mut manager = ProcessManager::new(std::env::temp_dir());
        manager
            .start("server", "echo ready http://localhost:3000")
            .unwrap();
        for _ in 0..200 {
            let snapshots = manager.poll().unwrap();
            if !snapshots[0].detected_urls.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            manager.list()[0].detected_urls,
            vec!["http://localhost:3000/".to_string()]
        );
        manager.restart("server").unwrap();
        assert!(matches!(
            manager.list()[0].state,
            ProcessState::Running | ProcessState::Exited { .. }
        ));
    }

    #[cfg(unix)]
    #[test]
    fn stopping_owned_process_terminates_its_descendant_tree() {
        let mut manager = ProcessManager::new(std::env::temp_dir());
        manager
            .start("tree", "sleep 30 & printf 'child=%s\\n' \"$!\"; wait")
            .unwrap();
        let child_pid = (0..100).find_map(|_| {
            let _ = manager.poll();
            let pid = manager.output("tree").ok().and_then(|output| {
                output
                    .lines()
                    .find_map(|line| line.trim().strip_prefix("child="))?
                    .parse::<i32>()
                    .ok()
            });
            if pid.is_none() {
                std::thread::sleep(Duration::from_millis(20));
            }
            pid
        });
        let child_pid = child_pid.expect("descendant PID must be reported by the PTY process");

        manager.stop("tree").unwrap();
        let gone = (0..50).any(|_| {
            // SAFETY: signal zero performs a read-only existence probe.
            let result = unsafe { libc::kill(child_pid, 0) };
            if result != 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
                true
            } else {
                std::thread::sleep(Duration::from_millis(20));
                false
            }
        });
        assert!(
            gone,
            "descendant process {child_pid} survived group shutdown"
        );
    }
}
