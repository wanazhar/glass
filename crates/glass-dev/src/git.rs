//! Native bounded Git and worktree service for Glass Dev.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const MAX_GIT_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const DEFAULT_GIT_TIMEOUT: Duration = Duration::from_secs(30);

pub type GitResult<T> = Result<T, GitError>;

#[derive(Debug)]
pub enum GitError {
    Io(std::io::Error),
    InvalidInput(String),
    NotRepository(PathBuf),
    Timeout(String),
    Command { operation: String, detail: String },
    OutputLimit(String),
}

impl fmt::Display for GitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "Git I/O error: {error}"),
            Self::InvalidInput(message) => write!(formatter, "invalid Git input: {message}"),
            Self::NotRepository(path) => {
                write!(formatter, "not a Git repository: {}", path.display())
            }
            Self::Timeout(operation) => write!(formatter, "Git operation timed out: {operation}"),
            Self::Command { operation, detail } => {
                write!(formatter, "Git operation {operation} failed: {detail}")
            }
            Self::OutputLimit(operation) => {
                write!(
                    formatter,
                    "Git operation {operation} exceeded the output limit"
                )
            }
        }
    }
}

impl std::error::Error for GitError {}

impl From<std::io::Error> for GitError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u64,
    pub behind: u64,
    pub entries: Vec<GitStatusEntry>,
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusEntry {
    pub path: String,
    pub original_path: Option<String>,
    pub index_status: char,
    pub worktree_status: char,
    pub untracked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitBranch {
    pub name: String,
    pub current: bool,
    pub upstream: Option<String>,
    pub commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitCommit {
    pub id: String,
    pub author: String,
    pub timestamp: i64,
    pub subject: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitWorktree {
    pub path: PathBuf,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitCommandResult {
    pub stdout: String,
    pub stderr: String,
}

pub struct GitService {
    root: PathBuf,
    timeout: Duration,
}

impl GitService {
    pub fn open(path: impl AsRef<Path>) -> GitResult<Self> {
        let path = path.as_ref().canonicalize()?;
        let probe = run_git_at(
            &path,
            &["rev-parse", "--show-toplevel"],
            DEFAULT_GIT_TIMEOUT,
            "discover repository",
        )?;
        let root = PathBuf::from(probe.stdout.trim()).canonicalize()?;
        Ok(Self {
            root,
            timeout: DEFAULT_GIT_TIMEOUT,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn set_timeout(&mut self, timeout: Duration) -> GitResult<()> {
        if timeout.is_zero() || timeout > Duration::from_secs(600) {
            return Err(GitError::InvalidInput(
                "Git timeout must be between 1 ms and 600 seconds".into(),
            ));
        }
        self.timeout = timeout;
        Ok(())
    }

    pub fn status(&self) -> GitResult<GitStatus> {
        let result = self.run(&["status", "--porcelain=v2", "--branch", "-z"], "status")?;
        parse_status(&result.stdout)
    }

    pub fn diff(&self, staged: bool, path: Option<&str>) -> GitResult<String> {
        let mut arguments = vec!["diff", "--no-ext-diff", "--binary"];
        if staged {
            arguments.push("--cached");
        }
        let validated;
        if let Some(path) = path {
            validated = validate_relative_path(path)?;
            arguments.extend(["--", validated.as_str()]);
        }
        Ok(self.run(&arguments, "diff")?.stdout)
    }

    pub fn stage(&self, paths: &[String]) -> GitResult<()> {
        let paths = validate_paths(paths)?;
        let mut arguments = vec!["add", "--"];
        arguments.extend(paths.iter().map(String::as_str));
        self.run(&arguments, "stage")?;
        Ok(())
    }

    pub fn unstage(&self, paths: &[String]) -> GitResult<()> {
        let paths = validate_paths(paths)?;
        let mut arguments = vec!["restore", "--staged", "--"];
        arguments.extend(paths.iter().map(String::as_str));
        self.run(&arguments, "unstage")?;
        Ok(())
    }

    pub fn branches(&self) -> GitResult<Vec<GitBranch>> {
        let format = "%(HEAD)%00%(refname:short)%00%(upstream:short)%00%(objectname)%00";
        let format_argument = format!("--format={format}");
        let output = self
            .run(
                &["for-each-ref", &format_argument, "refs/heads"],
                "list branches",
            )?
            .stdout;
        let fields = output.split('\0').collect::<Vec<_>>();
        let mut branches = Vec::new();
        for row in fields.chunks(4) {
            if row.len() < 4 || row[1].is_empty() {
                continue;
            }
            branches.push(GitBranch {
                current: row[0].trim() == "*",
                name: row[1].to_string(),
                upstream: (!row[2].is_empty()).then(|| row[2].to_string()),
                commit: row[3].trim().to_string(),
            });
        }
        Ok(branches)
    }

    pub fn create_branch(&self, name: &str, start_point: Option<&str>) -> GitResult<()> {
        validate_ref(name)?;
        let mut arguments = vec!["branch", name];
        if let Some(start_point) = start_point {
            validate_ref(start_point)?;
            arguments.push(start_point);
        }
        self.run(&arguments, "create branch")?;
        Ok(())
    }

    pub fn switch_branch(&self, name: &str, create: bool) -> GitResult<()> {
        validate_ref(name)?;
        let mut arguments = vec!["switch"];
        if create {
            arguments.push("-c");
        }
        arguments.push(name);
        self.run(&arguments, "switch branch")?;
        Ok(())
    }

    pub fn commit(&self, message: &str) -> GitResult<GitCommit> {
        if message.trim().is_empty() || message.len() > 16 * 1024 {
            return Err(GitError::InvalidInput(
                "commit message must contain 1..=16384 bytes".into(),
            ));
        }
        self.run(&["commit", "-m", message], "commit")?;
        self.commit_info("HEAD")
    }

    pub fn commit_info(&self, revision: &str) -> GitResult<GitCommit> {
        validate_ref(revision)?;
        let format = "%H%x00%an <%ae>%x00%ct%x00%s";
        let format_argument = format!("--format={format}");
        let output = self
            .run(
                &["show", "-s", &format_argument, revision],
                "inspect commit",
            )?
            .stdout;
        let fields = output.trim_end().splitn(4, '\0').collect::<Vec<_>>();
        if fields.len() != 4 {
            return Err(GitError::Command {
                operation: "inspect commit".into(),
                detail: "Git returned an invalid commit record".into(),
            });
        }
        Ok(GitCommit {
            id: fields[0].to_string(),
            author: fields[1].to_string(),
            timestamp: fields[2].parse().map_err(|_| GitError::Command {
                operation: "inspect commit".into(),
                detail: "Git returned an invalid commit timestamp".into(),
            })?,
            subject: fields[3].to_string(),
        })
    }

    pub fn blame(&self, path: &str, start_line: u64, end_line: u64) -> GitResult<String> {
        let path = validate_relative_path(path)?;
        if start_line == 0 || end_line < start_line || end_line - start_line > 10_000 {
            return Err(GitError::InvalidInput(
                "blame lines must be positive, ordered, and span at most 10001 lines".into(),
            ));
        }
        let range = format!("{start_line},{end_line}");
        Ok(self
            .run(
                &["blame", "--line-porcelain", "-L", &range, "--", &path],
                "blame",
            )?
            .stdout)
    }

    pub fn conflicts(&self) -> GitResult<Vec<String>> {
        Ok(self
            .run(
                &["diff", "--name-only", "--diff-filter=U", "-z"],
                "list conflicts",
            )?
            .stdout
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(str::to_string)
            .collect())
    }

    pub fn stash_push(&self, message: &str, include_untracked: bool) -> GitResult<()> {
        if message.len() > 1024 {
            return Err(GitError::InvalidInput(
                "stash message exceeds 1024 bytes".into(),
            ));
        }
        let mut arguments = vec!["stash", "push"];
        if include_untracked {
            arguments.push("--include-untracked");
        }
        if !message.is_empty() {
            arguments.extend(["-m", message]);
        }
        self.run(&arguments, "stash changes")?;
        Ok(())
    }

    pub fn stash_list(&self) -> GitResult<Vec<String>> {
        Ok(self
            .run(&["stash", "list", "--format=%gd%x00%s"], "list stashes")?
            .stdout
            .lines()
            .map(str::to_string)
            .collect())
    }

    pub fn stash_pop(&self, reference: &str) -> GitResult<()> {
        validate_ref(reference)?;
        self.run(&["stash", "pop", reference], "pop stash")?;
        Ok(())
    }

    pub fn worktrees(&self) -> GitResult<Vec<GitWorktree>> {
        let output = self
            .run(&["worktree", "list", "--porcelain", "-z"], "list worktrees")?
            .stdout;
        parse_worktrees(&output)
    }

    pub fn create_worktree(&self, path: &Path, branch: &str, create_branch: bool) -> GitResult<()> {
        validate_ref(branch)?;
        let path = absolute_worktree_path(path)?;
        let encoded = path
            .to_str()
            .ok_or_else(|| GitError::InvalidInput("worktree path is not UTF-8".into()))?;
        let mut arguments = vec!["worktree", "add"];
        if create_branch {
            arguments.extend(["-b", branch]);
            arguments.push(encoded);
        } else {
            arguments.extend([encoded, branch]);
        }
        self.run(&arguments, "create worktree")?;
        Ok(())
    }

    pub fn remove_worktree(&self, path: &Path, force: bool) -> GitResult<()> {
        let path = absolute_worktree_path(path)?;
        let path = path.canonicalize()?;
        if path == self.root {
            return Err(GitError::InvalidInput(
                "cannot remove the primary repository worktree".into(),
            ));
        }
        let known = self.worktrees()?.into_iter().any(|item| {
            item.path
                .canonicalize()
                .is_ok_and(|known_path| known_path == path)
        });
        if !known {
            return Err(GitError::InvalidInput(format!(
                "path is not an owned repository worktree: {}",
                path.display()
            )));
        }
        let encoded = path
            .to_str()
            .ok_or_else(|| GitError::InvalidInput("worktree path is not UTF-8".into()))?;
        let mut arguments = vec!["worktree", "remove"];
        if force {
            arguments.push("--force");
        }
        arguments.push(encoded);
        self.run(&arguments, "remove worktree")?;
        Ok(())
    }

    fn run(&self, arguments: &[&str], operation: &str) -> GitResult<GitCommandResult> {
        run_git_at(&self.root, arguments, self.timeout, operation)
    }
}

fn run_git_at(
    root: &Path,
    arguments: &[&str],
    timeout: Duration,
    operation: &str,
) -> GitResult<GitCommandResult> {
    let mut child = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().ok_or_else(|| GitError::Command {
        operation: operation.into(),
        detail: "Git stdout was unavailable".into(),
    })?;
    let stderr = child.stderr.take().ok_or_else(|| GitError::Command {
        operation: operation.into(),
        detail: "Git stderr was unavailable".into(),
    })?;
    let stdout_reader = read_output(stdout);
    let stderr_reader = read_output(stderr);
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            join_output(stdout_reader)?;
            join_output(stderr_reader)?;
            return Err(GitError::Timeout(operation.into()));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = join_output(stdout_reader)?;
    let stderr = join_output(stderr_reader)?;
    command_result(status, stdout, stderr, operation)
}

type OutputReader = JoinHandle<std::io::Result<(Vec<u8>, bool)>>;

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
            let remaining = MAX_GIT_OUTPUT_BYTES.saturating_sub(retained.len());
            retained.extend_from_slice(&chunk[..read.min(remaining)]);
            truncated |= read > remaining;
        }
        Ok((retained, truncated))
    })
}

fn join_output(reader: OutputReader) -> GitResult<(Vec<u8>, bool)> {
    let output = reader.join().map_err(|_| GitError::Command {
        operation: "read output".into(),
        detail: "Git output reader panicked".into(),
    })??;
    Ok(output)
}

fn command_result(
    status: ExitStatus,
    stdout: (Vec<u8>, bool),
    stderr: (Vec<u8>, bool),
    operation: &str,
) -> GitResult<GitCommandResult> {
    if stdout.1 || stderr.1 {
        return Err(GitError::OutputLimit(operation.into()));
    }
    let stdout = String::from_utf8_lossy(&stdout.0).into_owned();
    let stderr = String::from_utf8_lossy(&stderr.0).into_owned();
    if !status.success() {
        return Err(GitError::Command {
            operation: operation.into(),
            detail: stderr.trim().to_string(),
        });
    }
    Ok(GitCommandResult { stdout, stderr })
}

fn parse_status(output: &str) -> GitResult<GitStatus> {
    let mut status = GitStatus {
        branch: None,
        upstream: None,
        ahead: 0,
        behind: 0,
        entries: Vec::new(),
        conflicts: Vec::new(),
    };
    let records = output.split('\0').collect::<Vec<_>>();
    let mut index = 0;
    while index < records.len() {
        let record = records[index];
        index += 1;
        if let Some(branch) = record.strip_prefix("# branch.head ") {
            status.branch = (branch != "(detached)").then(|| branch.to_string());
        } else if let Some(upstream) = record.strip_prefix("# branch.upstream ") {
            status.upstream = Some(upstream.to_string());
        } else if let Some(ab) = record.strip_prefix("# branch.ab ") {
            for value in ab.split_whitespace() {
                if let Some(value) = value.strip_prefix('+') {
                    status.ahead = value.parse().unwrap_or(0);
                } else if let Some(value) = value.strip_prefix('-') {
                    status.behind = value.parse().unwrap_or(0);
                }
            }
        } else if let Some(path) = record.strip_prefix("? ") {
            status.entries.push(GitStatusEntry {
                path: path.to_string(),
                original_path: None,
                index_status: '?',
                worktree_status: '?',
                untracked: true,
            });
        } else if record.starts_with("1 ") || record.starts_with("u ") {
            let fields = record.splitn(9, ' ').collect::<Vec<_>>();
            if fields.len() < 9 {
                return Err(GitError::Command {
                    operation: "parse status".into(),
                    detail: "Git returned a malformed status record".into(),
                });
            }
            let xy = fields[1].as_bytes();
            let entry = GitStatusEntry {
                path: fields[8].to_string(),
                original_path: None,
                index_status: xy.first().copied().unwrap_or(b'.') as char,
                worktree_status: xy.get(1).copied().unwrap_or(b'.') as char,
                untracked: false,
            };
            if record.starts_with("u ") {
                status.conflicts.push(entry.path.clone());
            }
            status.entries.push(entry);
        } else if record.starts_with("2 ") {
            let fields = record.splitn(10, ' ').collect::<Vec<_>>();
            if fields.len() < 10 || index >= records.len() {
                return Err(GitError::Command {
                    operation: "parse status".into(),
                    detail: "Git returned a malformed rename record".into(),
                });
            }
            let xy = fields[1].as_bytes();
            let original_path = records[index].to_string();
            index += 1;
            status.entries.push(GitStatusEntry {
                path: fields[9].to_string(),
                original_path: Some(original_path),
                index_status: xy.first().copied().unwrap_or(b'.') as char,
                worktree_status: xy.get(1).copied().unwrap_or(b'.') as char,
                untracked: false,
            });
        }
    }
    Ok(status)
}

fn parse_worktrees(output: &str) -> GitResult<Vec<GitWorktree>> {
    let mut worktrees = Vec::new();
    let mut current: Option<GitWorktree> = None;
    for field in output.split('\0') {
        if field.is_empty() {
            if let Some(item) = current.take() {
                worktrees.push(item);
            }
        } else if let Some(path) = field.strip_prefix("worktree ") {
            if let Some(item) = current.take() {
                worktrees.push(item);
            }
            current = Some(GitWorktree {
                path: PathBuf::from(path),
                head: None,
                branch: None,
                bare: false,
                detached: false,
                locked: false,
                prunable: false,
            });
        } else if let Some(item) = current.as_mut() {
            if let Some(head) = field.strip_prefix("HEAD ") {
                item.head = Some(head.to_string());
            } else if let Some(branch) = field.strip_prefix("branch ") {
                item.branch = Some(branch.trim_start_matches("refs/heads/").to_string());
            } else {
                match field.split_whitespace().next().unwrap_or_default() {
                    "bare" => item.bare = true,
                    "detached" => item.detached = true,
                    "locked" => item.locked = true,
                    "prunable" => item.prunable = true,
                    _ => {}
                }
            }
        }
    }
    if let Some(item) = current {
        worktrees.push(item);
    }
    if worktrees
        .iter()
        .any(|item| item.path.as_os_str().is_empty())
    {
        return Err(GitError::Command {
            operation: "parse worktrees".into(),
            detail: "Git returned an empty worktree path".into(),
        });
    }
    Ok(worktrees)
}

fn validate_paths(paths: &[String]) -> GitResult<Vec<String>> {
    if paths.is_empty() || paths.len() > 1024 {
        return Err(GitError::InvalidInput(
            "Git path operations require 1..=1024 paths".into(),
        ));
    }
    paths
        .iter()
        .map(|path| validate_relative_path(path))
        .collect()
}

fn validate_relative_path(path: &str) -> GitResult<String> {
    let candidate = Path::new(path);
    if path.is_empty()
        || path.len() > 4096
        || candidate.is_absolute()
        || candidate
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(GitError::InvalidInput(format!(
            "Git path must be relative and remain in the repository: {path}"
        )));
    }
    Ok(path.to_string())
}

fn validate_ref(reference: &str) -> GitResult<()> {
    if reference.is_empty()
        || reference.len() > 1024
        || reference.starts_with('-')
        || reference.chars().any(char::is_whitespace)
    {
        return Err(GitError::InvalidInput(
            "Git reference must be non-empty, bounded, contain no whitespace, and not start with '-'"
                .into(),
        ));
    }
    Ok(())
}

fn absolute_worktree_path(path: &Path) -> GitResult<PathBuf> {
    if !path.is_absolute() || path == Path::new("/") {
        return Err(GitError::InvalidInput(
            "worktree path must be an explicit absolute path".into(),
        ));
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_REPOSITORY: AtomicU64 = AtomicU64::new(0);

    fn repository() -> PathBuf {
        let sequence = NEXT_REPOSITORY.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "glass-git-service-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        for arguments in [
            vec!["init", "-q"],
            vec!["config", "user.name", "Glass Test"],
            vec!["config", "user.email", "glass@example.invalid"],
        ] {
            assert!(
                Command::new("git")
                    .args(arguments)
                    .current_dir(&root)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        root
    }

    #[test]
    fn git_service_tracks_stage_commit_diff_branch_and_blame() {
        let root = repository();
        std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
        let service = GitService::open(&root).unwrap();
        let status = service.status().unwrap();
        assert!(status.entries.iter().any(|entry| entry.path == "main.rs"));

        service.stage(&["main.rs".into()]).unwrap();
        let commit = service.commit("test: initialize fixture").unwrap();
        assert_eq!(commit.subject, "test: initialize fixture");
        assert!(
            service
                .blame("main.rs", 1, 1)
                .unwrap()
                .contains("Glass Test")
        );

        std::fs::write(root.join("main.rs"), "fn main() { println!(\"ok\"); }\n").unwrap();
        assert!(
            service
                .diff(false, Some("main.rs"))
                .unwrap()
                .contains("println")
        );
        service.create_branch("fixture-branch", None).unwrap();
        assert!(
            service
                .branches()
                .unwrap()
                .iter()
                .any(|branch| branch.name == "fixture-branch")
        );
        assert!(service.conflicts().unwrap().is_empty());
        assert_eq!(service.worktrees().unwrap().len(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn git_paths_and_references_fail_closed() {
        assert!(validate_relative_path("src/main.rs").is_ok());
        assert!(validate_relative_path("../escape").is_err());
        assert!(validate_relative_path("/absolute").is_err());
        assert!(validate_ref("--upload-pack=bad").is_err());
        assert!(absolute_worktree_path(Path::new("relative")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn worktree_removal_accepts_a_canonical_path_alias() {
        use std::os::unix::fs::symlink;

        let root = repository();
        std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
        let service = GitService::open(&root).unwrap();
        service.stage(&["main.rs".into()]).unwrap();
        service.commit("test: initialize fixture").unwrap();

        let worktree = root.with_extension("worktree");
        let alias = root.with_extension("worktree-alias");
        service
            .create_worktree(&worktree, "canonical-alias", true)
            .unwrap();
        symlink(&worktree, &alias).unwrap();

        service.remove_worktree(&alias, true).unwrap();
        assert!(!worktree.exists());

        std::fs::remove_file(alias).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn status_parser_handles_branch_untracked_and_rename_records() {
        let parsed = parse_status(concat!(
            "# branch.head main\0",
            "# branch.upstream origin/main\0",
            "# branch.ab +2 -1\0",
            "? new.txt\0",
            "2 R. N... 100644 100644 100644 a b R100 renamed.txt\0old.txt\0",
        ))
        .unwrap();
        assert_eq!(parsed.branch.as_deref(), Some("main"));
        assert_eq!(parsed.upstream.as_deref(), Some("origin/main"));
        assert_eq!((parsed.ahead, parsed.behind), (2, 1));
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(parsed.entries[1].original_path.as_deref(), Some("old.txt"));
    }
}
