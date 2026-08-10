//! Resident test discovery and execution service.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_TEST_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
const MAX_TEST_SUITES: usize = 64;
const MAX_TEST_RUNS: usize = 256;
const DEFAULT_TEST_TIMEOUT: Duration = Duration::from_secs(600);

pub type TestResult<T> = Result<T, TestError>;

#[derive(Debug)]
pub enum TestError {
    Io(std::io::Error),
    InvalidInput(String),
    NotFound(String),
    Process(String),
}

impl fmt::Display for TestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "test runtime I/O error: {error}"),
            Self::InvalidInput(message) => write!(formatter, "invalid test input: {message}"),
            Self::NotFound(message) => write!(formatter, "test resource not found: {message}"),
            Self::Process(message) => write!(formatter, "test process error: {message}"),
        }
    }
}

impl std::error::Error for TestError {}

impl From<std::io::Error> for TestError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TestFramework {
    Cargo,
    Pytest,
    Npm,
    Pnpm,
    Yarn,
    Go,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TestSuite {
    pub id: String,
    pub name: String,
    pub framework: TestFramework,
    pub program: String,
    pub arguments: Vec<String>,
    pub source: PathBuf,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TestRunState {
    Running,
    Passed,
    Failed,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TestCaseState {
    Passed,
    Failed,
    Ignored,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TestCaseResult {
    pub name: String,
    pub state: TestCaseState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TestRun {
    pub id: String,
    pub suite_id: String,
    pub actor_id: String,
    pub workspace_revision: u64,
    pub started_at_ms: u64,
    pub duration_ms: Option<u64>,
    pub state: TestRunState,
    pub exit_code: Option<i32>,
    pub output: String,
    pub output_truncated: bool,
    pub cases: Vec<TestCaseResult>,
}

struct RunningTest {
    snapshot: TestRun,
    child: Child,
    stdout: Option<OutputReader>,
    stderr: Option<OutputReader>,
    started: Instant,
    timeout: Duration,
    requested_stop: Option<TestRunState>,
}

type OutputReader = JoinHandle<std::io::Result<(Vec<u8>, bool)>>;

pub struct TestService {
    root: PathBuf,
    suites: BTreeMap<String, TestSuite>,
    running: BTreeMap<String, RunningTest>,
    completed: Vec<TestRun>,
    watched: BTreeMap<String, u64>,
}

impl TestService {
    pub fn discover(root: impl AsRef<Path>) -> TestResult<Self> {
        let root = root.as_ref().canonicalize()?;
        let suites = discover_suites(&root)?
            .into_iter()
            .map(|suite| (suite.id.clone(), suite))
            .collect();
        Ok(Self {
            root,
            suites,
            running: BTreeMap::new(),
            completed: Vec::new(),
            watched: BTreeMap::new(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn suites(&self) -> impl Iterator<Item = &TestSuite> {
        self.suites.values()
    }

    pub fn register(&mut self, suite: TestSuite) -> TestResult<()> {
        validate_name(&suite.id, "test suite")?;
        if suite.program.is_empty() || suite.arguments.len() > 256 {
            return Err(TestError::InvalidInput(
                "test suite requires a program and at most 256 arguments".into(),
            ));
        }
        if self.suites.len() == MAX_TEST_SUITES && !self.suites.contains_key(&suite.id) {
            return Err(TestError::InvalidInput(format!(
                "test suite limit is {MAX_TEST_SUITES}"
            )));
        }
        self.suites.insert(suite.id.clone(), suite);
        Ok(())
    }

    pub fn start(
        &mut self,
        run_id: &str,
        suite_id: &str,
        actor_id: &str,
        workspace_revision: u64,
        timeout: Option<Duration>,
    ) -> TestResult<TestRun> {
        validate_name(run_id, "test run")?;
        validate_name(actor_id, "test actor")?;
        if self.running.contains_key(run_id)
            || self.completed.iter().any(|result| result.id == run_id)
        {
            return Err(TestError::InvalidInput(format!(
                "test run {run_id} already exists"
            )));
        }
        if self.running.len() + self.completed.len() >= MAX_TEST_RUNS {
            return Err(TestError::InvalidInput(format!(
                "test run history limit is {MAX_TEST_RUNS}"
            )));
        }
        let suite = self
            .suites
            .get(suite_id)
            .ok_or_else(|| TestError::NotFound(format!("test suite {suite_id}")))?;
        let timeout = timeout.unwrap_or(DEFAULT_TEST_TIMEOUT);
        if timeout.is_zero() || timeout > Duration::from_secs(3600) {
            return Err(TestError::InvalidInput(
                "test timeout must be between 1 ms and 1 hour".into(),
            ));
        }
        let mut child = Command::new(&suite.program)
            .args(&suite.arguments)
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| TestError::Process("test stdout was unavailable".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| TestError::Process("test stderr was unavailable".into()))?;
        let snapshot = TestRun {
            id: run_id.to_string(),
            suite_id: suite_id.to_string(),
            actor_id: actor_id.to_string(),
            workspace_revision,
            started_at_ms: unix_time_ms(),
            duration_ms: None,
            state: TestRunState::Running,
            exit_code: None,
            output: String::new(),
            output_truncated: false,
            cases: Vec::new(),
        };
        self.running.insert(
            run_id.to_string(),
            RunningTest {
                snapshot: snapshot.clone(),
                child,
                stdout: Some(read_output(stdout)),
                stderr: Some(read_output(stderr)),
                started: Instant::now(),
                timeout,
                requested_stop: None,
            },
        );
        Ok(snapshot)
    }

    pub fn cancel(&mut self, run_id: &str) -> TestResult<()> {
        let running = self
            .running
            .get_mut(run_id)
            .ok_or_else(|| TestError::NotFound(format!("running test {run_id}")))?;
        running.requested_stop = Some(TestRunState::Cancelled);
        if running.child.try_wait()?.is_none() {
            running.child.kill()?;
        }
        Ok(())
    }

    pub fn poll(&mut self) -> TestResult<Vec<TestRun>> {
        let ids = self.running.keys().cloned().collect::<Vec<_>>();
        let mut finished = Vec::new();
        for id in ids {
            let running = self.running.get_mut(&id).expect("run ID came from map");
            if running.started.elapsed() >= running.timeout && running.requested_stop.is_none() {
                running.requested_stop = Some(TestRunState::TimedOut);
                if running.child.try_wait()?.is_none() {
                    running.child.kill()?;
                }
            }
            if let Some(status) = running.child.try_wait()? {
                let running = self.running.remove(&id).expect("finished run exists");
                let result = finish_run(running, status)?;
                self.completed.push(result.clone());
                finished.push(result);
            }
        }
        Ok(finished)
    }

    pub fn running(&self) -> impl Iterator<Item = &TestRun> {
        self.running.values().map(|running| &running.snapshot)
    }

    pub fn results(&self) -> impl DoubleEndedIterator<Item = &TestRun> {
        self.completed.iter()
    }

    pub fn result(&self, run_id: &str) -> Option<&TestRun> {
        self.completed.iter().find(|result| result.id == run_id)
    }

    pub fn affected_suites<'a>(
        &'a self,
        changed_paths: impl IntoIterator<Item = &'a str>,
    ) -> Vec<&'a TestSuite> {
        let extensions = changed_paths
            .into_iter()
            .filter_map(|path| Path::new(path).extension().and_then(|value| value.to_str()))
            .collect::<BTreeSet<_>>();
        self.suites
            .values()
            .filter(|suite| match suite.framework {
                TestFramework::Cargo => extensions.contains("rs") || extensions.contains("toml"),
                TestFramework::Pytest => extensions.contains("py"),
                TestFramework::Npm | TestFramework::Pnpm | TestFramework::Yarn => {
                    ["js", "jsx", "ts", "tsx", "json"]
                        .iter()
                        .any(|extension| extensions.contains(extension))
                }
                TestFramework::Go => extensions.contains("go"),
                TestFramework::Custom => true,
            })
            .collect()
    }

    pub fn watch(&mut self, suite_id: &str, current_revision: u64) -> TestResult<()> {
        if !self.suites.contains_key(suite_id) {
            return Err(TestError::NotFound(format!("test suite {suite_id}")));
        }
        self.watched.insert(suite_id.to_string(), current_revision);
        Ok(())
    }

    pub fn unwatch(&mut self, suite_id: &str) -> bool {
        self.watched.remove(suite_id).is_some()
    }

    pub fn changed_watches(&mut self, current_revision: u64) -> Vec<String> {
        let mut changed = Vec::new();
        for (suite, revision) in &mut self.watched {
            if *revision != current_revision {
                *revision = current_revision;
                changed.push(suite.clone());
            }
        }
        changed
    }
}

impl Drop for TestService {
    fn drop(&mut self) {
        for running in self.running.values_mut() {
            if running.child.try_wait().ok().flatten().is_none() {
                let _ = running.child.kill();
                let _ = running.child.wait();
            }
            join_reader(running.stdout.take());
            join_reader(running.stderr.take());
        }
    }
}

fn discover_suites(root: &Path) -> TestResult<Vec<TestSuite>> {
    let mut suites = Vec::new();
    if root.join("Cargo.toml").is_file() {
        suites.push(TestSuite {
            id: "cargo".into(),
            name: "Cargo tests".into(),
            framework: TestFramework::Cargo,
            program: "cargo".into(),
            arguments: vec!["test".into()],
            source: root.join("Cargo.toml"),
        });
    }
    let python_source = ["pyproject.toml", "pytest.ini", "setup.cfg"]
        .into_iter()
        .map(|name| root.join(name))
        .find(|path| path.is_file());
    if let Some(source) = python_source {
        suites.push(TestSuite {
            id: "pytest".into(),
            name: "Python tests".into(),
            framework: TestFramework::Pytest,
            program: if cfg!(windows) { "python" } else { "python3" }.into(),
            arguments: vec!["-m".into(), "pytest".into()],
            source,
        });
    }
    let package_json = root.join("package.json");
    if package_json.is_file() {
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&package_json)?)
            .map_err(|error| TestError::InvalidInput(format!("invalid package.json: {error}")))?;
        if value
            .get("scripts")
            .and_then(|scripts| scripts.get("test"))
            .and_then(serde_json::Value::as_str)
            .is_some()
        {
            let (framework, program, arguments) = if root.join("pnpm-lock.yaml").is_file() {
                (TestFramework::Pnpm, "pnpm", vec!["test".into()])
            } else if root.join("yarn.lock").is_file() {
                (TestFramework::Yarn, "yarn", vec!["test".into()])
            } else {
                (TestFramework::Npm, "npm", vec!["test".into()])
            };
            suites.push(TestSuite {
                id: "node".into(),
                name: "Node tests".into(),
                framework,
                program: program.into(),
                arguments,
                source: package_json,
            });
        }
    }
    if root.join("go.mod").is_file() {
        suites.push(TestSuite {
            id: "go".into(),
            name: "Go tests".into(),
            framework: TestFramework::Go,
            program: "go".into(),
            arguments: vec!["test".into(), "./...".into()],
            source: root.join("go.mod"),
        });
    }
    suites.truncate(MAX_TEST_SUITES);
    Ok(suites)
}

fn finish_run(mut running: RunningTest, status: ExitStatus) -> TestResult<TestRun> {
    let stdout = join_output(running.stdout.take())?;
    let stderr = join_output(running.stderr.take())?;
    let mut combined = stdout.0;
    if !combined.is_empty() && !stderr.0.is_empty() {
        combined.push(b'\n');
    }
    let remaining = MAX_TEST_OUTPUT_BYTES.saturating_sub(combined.len());
    combined.extend_from_slice(&stderr.0[..stderr.0.len().min(remaining)]);
    let truncated = stdout.1 || stderr.1 || stderr.0.len() > remaining;
    let output = String::from_utf8_lossy(&combined).into_owned();
    running.snapshot.duration_ms = Some(running.started.elapsed().as_millis() as u64);
    running.snapshot.exit_code = status.code();
    running.snapshot.state = running.requested_stop.unwrap_or(if status.success() {
        TestRunState::Passed
    } else {
        TestRunState::Failed
    });
    running.snapshot.cases = parse_test_cases(&output);
    running.snapshot.output = output;
    running.snapshot.output_truncated = truncated;
    Ok(running.snapshot)
}

fn read_output(mut stream: impl Read + Send + 'static) -> OutputReader {
    std::thread::spawn(move || {
        let mut retained = Vec::new();
        let mut truncated = false;
        let mut chunk = [0_u8; 8192];
        loop {
            let read = stream.read(&mut chunk)?;
            if read == 0 {
                break;
            }
            let remaining = MAX_TEST_OUTPUT_BYTES.saturating_sub(retained.len());
            retained.extend_from_slice(&chunk[..read.min(remaining)]);
            truncated |= read > remaining;
        }
        Ok((retained, truncated))
    })
}

fn join_output(reader: Option<OutputReader>) -> TestResult<(Vec<u8>, bool)> {
    reader
        .ok_or_else(|| TestError::Process("test output reader was unavailable".into()))?
        .join()
        .map_err(|_| TestError::Process("test output reader panicked".into()))?
        .map_err(TestError::Io)
}

fn join_reader(reader: Option<OutputReader>) {
    if let Some(reader) = reader {
        let _ = reader.join();
    }
}

fn parse_test_cases(output: &str) -> Vec<TestCaseResult> {
    output
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("test ")?;
            let (name, state) = if let Some(name) = rest.strip_suffix(" ... ok") {
                (name, TestCaseState::Passed)
            } else if let Some(name) = rest.strip_suffix(" ... FAILED") {
                (name, TestCaseState::Failed)
            } else {
                (rest.strip_suffix(" ... ignored")?, TestCaseState::Ignored)
            };
            Some(TestCaseResult {
                name: name.to_string(),
                state,
            })
        })
        .take(100_000)
        .collect()
}

fn validate_name(name: &str, description: &str) -> TestResult<()> {
    if name.is_empty()
        || name.len() > 128
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
    {
        return Err(TestError::InvalidInput(format!(
            "{description} must be 1..=128 ASCII letters, digits, '-', '_' or '.'"
        )));
    }
    Ok(())
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn root() -> PathBuf {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "glass-test-service-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn discovers_supported_project_test_suites() {
        let root = root();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .unwrap();
        std::fs::write(root.join("pyproject.toml"), "[project]\nname='x'\n").unwrap();
        std::fs::write(
            root.join("package.json"),
            r#"{"scripts":{"test":"vitest"}}"#,
        )
        .unwrap();
        std::fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: 9\n").unwrap();
        std::fs::write(root.join("go.mod"), "module example.invalid/x\n").unwrap();
        let service = TestService::discover(&root).unwrap();
        let frameworks = service
            .suites()
            .map(|suite| suite.framework)
            .collect::<Vec<_>>();
        assert_eq!(
            frameworks,
            vec![
                TestFramework::Cargo,
                TestFramework::Go,
                TestFramework::Pnpm,
                TestFramework::Pytest
            ]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resident_test_run_records_actor_revision_output_and_result() {
        let root = root();
        let mut service = TestService::discover(&root).unwrap();
        service
            .register(TestSuite {
                id: "version".into(),
                name: "Rust compiler version".into(),
                framework: TestFramework::Custom,
                program: "rustc".into(),
                arguments: vec!["--version".into()],
                source: root.join("glass.toml"),
            })
            .unwrap();
        service
            .start(
                "run-1",
                "version",
                "agent-tester",
                7,
                Some(Duration::from_secs(10)),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let result = loop {
            if let Some(result) = service.poll().unwrap().into_iter().next() {
                break result;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(result.actor_id, "agent-tester");
        assert_eq!(result.workspace_revision, 7);
        assert_eq!(result.state, TestRunState::Passed);
        assert!(result.output.contains("rustc"));
        assert_eq!(service.result("run-1"), Some(&result));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn case_parser_and_watch_revisions_are_structured() {
        let cases = parse_test_cases(
            "test alpha ... ok\ntest beta ... FAILED\ntest gamma ... ignored\nnoise\n",
        );
        assert_eq!(cases.len(), 3);
        assert_eq!(cases[1].state, TestCaseState::Failed);

        let root = root();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .unwrap();
        let mut service = TestService::discover(&root).unwrap();
        service.watch("cargo", 1).unwrap();
        assert!(service.changed_watches(1).is_empty());
        assert_eq!(service.changed_watches(2), vec!["cargo"]);
        assert!(service.changed_watches(2).is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
