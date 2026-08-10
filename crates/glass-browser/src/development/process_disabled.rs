use super::{DevelopmentError, DevelopmentResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

/// Browser-only builds retain the type contract but do not compile or install
/// the PTY implementation. `glass-dev` enables `development-runtime`.
#[derive(Debug)]
pub struct ProcessManager {
    root: PathBuf,
}

impl ProcessManager {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn unavailable<T>(&self) -> DevelopmentResult<T> {
        Err(DevelopmentError::Process(
            "PTY support belongs to glass-dev; enable development-runtime".into(),
        ))
    }

    pub fn start(&mut self, _name: &str, _command: &str) -> DevelopmentResult<ProcessSnapshot> {
        self.unavailable()
    }

    pub fn send_input(&mut self, _name: &str, _input: &str) -> DevelopmentResult<()> {
        self.unavailable()
    }

    pub fn close_input(&mut self, _name: &str) -> DevelopmentResult<()> {
        self.unavailable()
    }

    pub fn resize(&mut self, _name: &str, _cols: u16, _rows: u16) -> DevelopmentResult<()> {
        self.unavailable()
    }

    pub fn stop(&mut self, _name: &str) -> DevelopmentResult<ProcessSnapshot> {
        self.unavailable()
    }

    pub fn restart(&mut self, _name: &str) -> DevelopmentResult<ProcessSnapshot> {
        self.unavailable()
    }

    pub fn remove(&mut self, _name: &str) -> DevelopmentResult<ProcessSnapshot> {
        self.unavailable()
    }

    pub fn poll(&mut self) -> DevelopmentResult<Vec<ProcessSnapshot>> {
        Ok(Vec::new())
    }

    pub fn list(&mut self) -> Vec<ProcessSnapshot> {
        Vec::new()
    }

    pub fn list_checked(&mut self) -> DevelopmentResult<Vec<ProcessSnapshot>> {
        self.unavailable()
    }

    pub fn output(&self, _name: &str) -> DevelopmentResult<String> {
        self.unavailable()
    }

    pub fn root(&self) -> &std::path::Path {
        &self.root
    }
}
