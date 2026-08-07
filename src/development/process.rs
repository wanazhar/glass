use super::{DevelopmentError, DevelopmentResult, MAX_PROCESS_OUTPUT_BYTES};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::{SystemTime, UNIX_EPOCH},
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
pub struct ProcessSnapshot {
    pub name: String,
    pub command: String,
    pub pid: Option<u32>,
    pub state: ProcessState,
    pub started_at_ms: u64,
    pub output: String,
    pub pty: bool,
}

struct RunningProcess {
    snapshot: ProcessSnapshot,
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    writer: Option<Box<dyn Write + Send>>,
    output: Arc<Mutex<VecDeque<u8>>>,
    _reader: Option<thread::JoinHandle<()>>,
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
        let pid = child.process_id();
        let reader = pty
            .master
            .try_clone_reader()
            .map_err(|error| DevelopmentError::Process(error.to_string()))?;
        let writer = pty
            .master
            .take_writer()
            .map_err(|error| DevelopmentError::Process(error.to_string()))?;
        let output = Arc::new(Mutex::new(VecDeque::new()));
        let reader_output = Arc::clone(&output);
        let reader_handle = thread::Builder::new()
            .name(format!("glass-process-{name}"))
            .spawn(move || read_output(reader, reader_output))
            .map_err(DevelopmentError::Io)?;
        let snapshot = ProcessSnapshot {
            name: name.into(),
            command: command.into(),
            pid,
            state: ProcessState::Running,
            started_at_ms: now_ms(),
            output: String::new(),
            pty: true,
        };
        self.processes.insert(
            name.into(),
            RunningProcess {
                snapshot: snapshot.clone(),
                child,
                master: pty.master,
                writer: Some(writer),
                output,
                _reader: Some(reader_handle),
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
        process
            .child
            .kill()
            .map_err(|error| DevelopmentError::Process(error.to_string()))?;
        process.snapshot.state = ProcessState::Stopped;
        Ok(snapshot(process))
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
            }
            process.snapshot.output = output_string(&process.output);
            updated.push(snapshot(process));
        }
        Ok(updated)
    }

    pub fn list(&mut self) -> Vec<ProcessSnapshot> {
        let _ = self.poll();
        self.processes.values().map(snapshot).collect()
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
                let _ = process.child.kill();
            }
        }
    }
}

fn snapshot(process: &RunningProcess) -> ProcessSnapshot {
    let mut snapshot = process.snapshot.clone();
    snapshot.output = output_string(&process.output);
    snapshot
}

fn read_output(mut reader: Box<dyn Read + Send>, output: Arc<Mutex<VecDeque<u8>>>) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn pty_process_emits_output_and_exits() {
        let mut manager = ProcessManager::new(std::env::temp_dir());
        manager
            .start("echo", "printf 'glass-process-ok\\n'")
            .unwrap();
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
}
