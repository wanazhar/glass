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

impl GitStatus {
    /// Conflicts first, then unstaged, staged, untracked.
    pub fn sorted_entries(&self) -> Vec<GitStatusEntry> {
        let mut entries = self.entries.clone();
        entries.sort_by(|left, right| {
            entry_rank(left, &self.conflicts)
                .cmp(&entry_rank(right, &self.conflicts))
                .then_with(|| left.path.cmp(&right.path))
        });
        entries
    }
}

fn entry_rank(entry: &GitStatusEntry, conflicts: &[String]) -> u8 {
    if conflicts.iter().any(|path| path == &entry.path)
        || entry.index_status == 'U'
        || entry.worktree_status == 'U'
    {
        0
    } else if entry.untracked {
        3
    } else if entry.worktree_status != ' ' {
        1
    } else {
        2
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct PushDestination {
    remote: String,
    url: String,
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
        let toplevel = PathBuf::from(probe.stdout.trim());
        if toplevel.as_os_str().is_empty() || toplevel.canonicalize().is_err() {
            return Err(GitError::NotRepository(path));
        }
        Ok(Self {
            root: path,
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
        let mut arguments = vec!["diff", "--no-ext-diff", "--no-textconv", "--binary"];
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

    pub(crate) fn stage(&self, paths: &[String]) -> GitResult<()> {
        let paths = validate_paths(paths)?;
        let mut arguments = vec!["add", "--"];
        arguments.extend(paths.iter().map(String::as_str));
        self.run(&arguments, "stage")?;
        Ok(())
    }

    pub(crate) fn unstage(&self, paths: &[String]) -> GitResult<()> {
        let paths = validate_paths(paths)?;
        let mut arguments = vec!["restore", "--staged", "--"];
        arguments.extend(paths.iter().map(String::as_str));
        self.run(&arguments, "unstage")?;
        Ok(())
    }

    pub(crate) fn discard(&self, paths: &[String]) -> GitResult<()> {
        let paths = validate_paths(paths)?;
        let mut arguments = vec!["restore", "--"];
        arguments.extend(paths.iter().map(String::as_str));
        self.run(&arguments, "discard working-tree changes")?;
        Ok(())
    }

    pub(crate) fn fetch(&self, remote: Option<&str>) -> GitResult<()> {
        let mut arguments = vec!["fetch"];
        if let Some(remote) = remote {
            validate_ref(remote)?;
            arguments.push(remote);
        }
        self.run(&arguments, "fetch")?;
        Ok(())
    }

    pub(crate) fn pull(&self, remote: Option<&str>, branch: Option<&str>) -> GitResult<()> {
        let mut arguments = vec!["pull"];
        if let Some(remote) = remote {
            validate_ref(remote)?;
            arguments.push(remote);
        }
        if let Some(branch) = branch {
            validate_ref(branch)?;
            arguments.push(branch);
        }
        self.run(&arguments, "pull")?;
        Ok(())
    }

    pub(crate) fn merge(&self, branch: &str) -> GitResult<()> {
        validate_ref(branch)?;
        self.run(&["merge", "--no-edit", branch], "merge")?;
        Ok(())
    }

    pub(crate) fn rebase(&self, onto: &str) -> GitResult<()> {
        validate_ref(onto)?;
        self.run(&["rebase", onto], "rebase")?;
        Ok(())
    }

    pub(crate) fn push(&self, remote: Option<&str>, branch: Option<&str>) -> GitResult<()> {
        if let Some(branch) = branch {
            push_branch_name(branch)?;
        }
        let current_branch = self
            .status()?
            .branch
            .ok_or_else(|| GitError::InvalidInput("push requires a named current branch".into()))?;
        self.push_with_target_default_branch(remote, branch, &current_branch, |push_url| {
            crate::github::default_branch_for_remote(&self.root, push_url)
                .map_err(|error| GitError::InvalidInput(error.to_string()))
        })
    }

    fn push_with_target_default_branch(
        &self,
        remote: Option<&str>,
        branch: Option<&str>,
        current_branch: &str,
        default_branch_for_target: impl FnOnce(&str) -> GitResult<String>,
    ) -> GitResult<()> {
        let destination = self.resolve_push_destination(remote, current_branch)?;
        let default_branch = default_branch_for_target(&destination.url)?;
        self.push_with_branch_policy(
            Some(&destination.remote),
            branch,
            current_branch,
            &default_branch,
        )
    }

    fn resolve_push_destination(
        &self,
        remote: Option<&str>,
        current_branch: &str,
    ) -> GitResult<PushDestination> {
        let current_branch = push_branch_name(current_branch)?;
        let remote = match remote {
            Some(remote) => {
                validate_ref(remote)?;
                remote.to_string()
            }
            None => self.configured_push_remote(&current_branch)?,
        };
        let output = self.run(
            &["remote", "get-url", "--push", "--all", &remote],
            "resolve push destination",
        )?;
        let urls = output
            .stdout
            .lines()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();
        if urls.len() != 1
            || urls[0]
                .bytes()
                .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
        {
            return Err(GitError::InvalidInput(
                "push requires one unambiguous remote push URL".into(),
            ));
        }
        Ok(PushDestination {
            remote,
            url: urls[0].to_string(),
        })
    }

    fn configured_push_remote(&self, current_branch: &str) -> GitResult<String> {
        let config = self
            .run(
                &["config", "--null", "--local", "--list"],
                "read push configuration",
            )?
            .stdout;
        let remote = if let Some(remote) = config_value(&config, |key| {
            branch_config_key_matches(key, current_branch, "pushremote")
        })? {
            remote
        } else if let Some(remote) = config_value(&config, |key| {
            key.eq_ignore_ascii_case("remote.pushdefault")
        })? {
            remote
        } else if let Some(remote) = config_value(&config, |key| {
            branch_config_key_matches(key, current_branch, "remote")
        })? {
            remote
        } else {
            "origin".into()
        };
        validate_ref(&remote)?;
        Ok(remote)
    }

    fn push_with_branch_policy(
        &self,
        remote: Option<&str>,
        branch: Option<&str>,
        current_branch: &str,
        default_branch: &str,
    ) -> GitResult<()> {
        let source_branch = push_branch_name(current_branch)?;
        let destination_branch = branch
            .map(push_branch_name)
            .transpose()?
            .unwrap_or_else(|| source_branch.clone());
        crate::github::require_push_branches(&source_branch, &destination_branch, default_branch)
            .map_err(|error| GitError::InvalidInput(error.to_string()))?;
        let refspec = explicit_push_refspec(&source_branch, &destination_branch)?;
        let mut arguments = vec!["push"];
        if let Some(remote) = remote {
            validate_ref(remote)?;
            arguments.push(remote);
        }
        arguments.push(&refspec);
        self.run(&arguments, "push")?;
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

    pub(crate) fn create_branch(&self, name: &str, start_point: Option<&str>) -> GitResult<()> {
        validate_ref(name)?;
        let mut arguments = vec!["branch", name];
        if let Some(start_point) = start_point {
            validate_ref(start_point)?;
            arguments.push(start_point);
        }
        self.run(&arguments, "create branch")?;
        Ok(())
    }

    pub(crate) fn switch_branch(&self, name: &str, create: bool) -> GitResult<()> {
        validate_ref(name)?;
        let mut arguments = vec!["switch"];
        if create {
            arguments.push("-c");
        }
        arguments.push(name);
        self.run(&arguments, "switch branch")?;
        Ok(())
    }

    pub(crate) fn commit(&self, message: &str) -> GitResult<GitCommit> {
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

    pub(crate) fn stash_push(&self, message: &str, include_untracked: bool) -> GitResult<()> {
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

    pub(crate) fn stash_pop(&self, reference: &str) -> GitResult<()> {
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

    pub(crate) fn create_worktree(
        &self,
        path: &Path,
        branch: &str,
        create_branch: bool,
    ) -> GitResult<()> {
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

    pub(crate) fn remove_worktree(&self, path: &Path, force: bool) -> GitResult<()> {
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

pub(crate) fn git_command(root: &Path) -> Command {
    let mut command = Command::new("git");
    apply_untrusted_git_isolation(&mut command);
    command.current_dir(root);
    command
}

fn apply_untrusted_git_isolation(command: &mut Command) {
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", git_null_device());
}

fn git_null_device() -> &'static str {
    if cfg!(windows) { "NUL" } else { "/dev/null" }
}

fn git_safety_overrides(verb: &str) -> Vec<String> {
    vec![
        "-c".into(),
        format!("alias.{verb}="),
        "-c".into(),
        format!("core.hooksPath={}", git_null_device()),
        "-c".into(),
        "core.fsmonitor=".into(),
        "-c".into(),
        "core.fsmonitorHookVersion=".into(),
        "-c".into(),
        "diff.external=".into(),
        "-c".into(),
        "filter.lfs.smudge=".into(),
        "-c".into(),
        "filter.lfs.process=".into(),
        "-c".into(),
        "filter.lfs.required=false".into(),
    ]
}

fn run_git_at(
    root: &Path,
    arguments: &[&str],
    timeout: Duration,
    operation: &str,
) -> GitResult<GitCommandResult> {
    let mut command = git_command(root);
    if let Some(verb) = arguments.first() {
        command.args(git_safety_overrides(verb));
    }
    let mut child = command
        .args(arguments)
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
        } else if record.starts_with("1 ") {
            let fields = record.splitn(9, ' ').collect::<Vec<_>>();
            if fields.len() < 9 {
                return Err(GitError::Command {
                    operation: "parse status".into(),
                    detail: "Git returned a malformed status record".into(),
                });
            }
            let xy = fields[1].as_bytes();
            status.entries.push(GitStatusEntry {
                path: fields[8].to_string(),
                original_path: None,
                index_status: porcelain_flag(xy.first().copied()),
                worktree_status: porcelain_flag(xy.get(1).copied()),
                untracked: false,
            });
        } else if record.starts_with("u ") {
            let fields = record.splitn(11, ' ').collect::<Vec<_>>();
            if fields.len() < 11 {
                return Err(GitError::Command {
                    operation: "parse status".into(),
                    detail: "Git returned a malformed unmerged status record".into(),
                });
            }
            let xy = fields[1].as_bytes();
            let path = fields[10].to_string();
            status.conflicts.push(path.clone());
            status.entries.push(GitStatusEntry {
                path,
                original_path: None,
                index_status: xy.first().copied().unwrap_or(b'.') as char,
                worktree_status: xy.get(1).copied().unwrap_or(b'.') as char,
                untracked: false,
            });
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
                index_status: porcelain_flag(xy.first().copied()),
                worktree_status: porcelain_flag(xy.get(1).copied()),
                untracked: false,
            });
        }
    }
    Ok(status)
}

fn porcelain_flag(flag: Option<u8>) -> char {
    match flag.unwrap_or(b'.') {
        b'.' => ' ',
        other => other as char,
    }
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
    let rooted = candidate.components().any(|part| {
        matches!(
            part,
            std::path::Component::Prefix(_) | std::path::Component::RootDir
        )
    });
    if path.is_empty()
        || path.len() > 4096
        || rooted
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

fn push_branch_name(branch: &str) -> GitResult<String> {
    validate_ref(branch)?;
    if branch.starts_with('+') || branch.contains(':') {
        return Err(GitError::InvalidInput(
            "Git push branch must be a branch name, not a refspec".into(),
        ));
    }
    let branch = branch.strip_prefix("refs/heads/").unwrap_or(branch);
    if branch.starts_with("refs/") {
        return Err(GitError::InvalidInput(
            "Git push target must name a branch under refs/heads".into(),
        ));
    }
    Ok(branch.to_string())
}

fn explicit_push_refspec(source: &str, destination: &str) -> GitResult<String> {
    let source = push_branch_name(source)?;
    let destination = push_branch_name(destination)?;
    Ok(format!("refs/heads/{source}:refs/heads/{destination}"))
}

fn config_value(config: &str, matches_key: impl Fn(&str) -> bool) -> GitResult<Option<String>> {
    let mut values = Vec::new();
    for entry in config.split('\0').filter(|entry| !entry.is_empty()) {
        let Some((key, value)) = entry.split_once('\n') else {
            return Err(GitError::InvalidInput(
                "Git returned malformed local configuration".into(),
            ));
        };
        if matches_key(key) {
            values.push(value.to_string());
        }
    }
    if values.len() > 1 {
        return Err(GitError::InvalidInput(
            "push remote configuration is ambiguous".into(),
        ));
    }
    match values.pop() {
        Some(value) if value.is_empty() => Err(GitError::InvalidInput(
            "push remote configuration must not be empty".into(),
        )),
        Some(value) => Ok(Some(value)),
        None => Ok(None),
    }
}

fn branch_config_key_matches(key: &str, branch: &str, setting: &str) -> bool {
    const PREFIX: &str = "branch.";
    if key
        .get(..PREFIX.len())
        .is_none_or(|prefix| !prefix.eq_ignore_ascii_case(PREFIX))
    {
        return false;
    }
    let Some(rest) = key.get(PREFIX.len()..) else {
        return false;
    };
    let suffix = format!(".{setting}");
    if rest.len() < suffix.len() {
        return false;
    }
    let split = rest.len() - suffix.len();
    rest.get(split..)
        .is_some_and(|actual| actual.eq_ignore_ascii_case(&suffix))
        && rest.get(..split) == Some(branch)
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

    #[test]
    fn porcelain_v2_dot_means_unstaged_not_staged() {
        let status = parse_status(
            "# branch.head main\x001 .M N... 100644 100644 100644 0 0 notes.txt\x00? extra.rs\x00",
        )
        .unwrap();
        assert_eq!(status.entries[0].index_status, ' ');
        assert_eq!(status.entries[0].worktree_status, 'M');
        assert!(!status.entries[0].untracked);
    }

    #[test]
    fn sorted_entries_rank_conflicts_before_unstaged_and_untracked() {
        let status = GitStatus {
            branch: Some("main".into()),
            upstream: None,
            ahead: 0,
            behind: 0,
            entries: vec![
                GitStatusEntry {
                    path: "new.rs".into(),
                    original_path: None,
                    index_status: '?',
                    worktree_status: '?',
                    untracked: true,
                },
                GitStatusEntry {
                    path: "staged.rs".into(),
                    original_path: None,
                    index_status: 'M',
                    worktree_status: ' ',
                    untracked: false,
                },
                GitStatusEntry {
                    path: "conflict.rs".into(),
                    original_path: None,
                    index_status: 'U',
                    worktree_status: 'U',
                    untracked: false,
                },
                GitStatusEntry {
                    path: "unstaged.rs".into(),
                    original_path: None,
                    index_status: ' ',
                    worktree_status: 'M',
                    untracked: false,
                },
            ],
            conflicts: vec!["conflict.rs".into()],
        };
        let order = status
            .sorted_entries()
            .into_iter()
            .map(|entry| entry.path)
            .collect::<Vec<_>>();
        assert_eq!(
            order,
            vec!["conflict.rs", "unstaged.rs", "staged.rs", "new.rs"]
        );
    }

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

    fn push_repository() -> (PathBuf, GitService, PathBuf, PathBuf) {
        let root = repository();
        let service = GitService::open(&root).unwrap();
        service
            .run(&["branch", "-M", "main"], "name default branch")
            .unwrap();
        std::fs::write(root.join("main.txt"), "main\n").unwrap();
        service.stage(&["main.txt".into()]).unwrap();
        service
            .commit("test: initialize push policy fixture")
            .unwrap();
        service.create_branch("feature/push", None).unwrap();
        service.switch_branch("feature/push", false).unwrap();

        let origin = root.with_extension("origin.git");
        let backup = root.with_extension("backup.git");
        for (name, path) in [("origin", &origin), ("backup", &backup)] {
            let path = path.to_string_lossy().into_owned();
            let output = Command::new("git")
                .args(["init", "--bare", "-q", &path])
                .current_dir(&root)
                .output()
                .unwrap();
            assert!(output.status.success());
            service
                .run(&["remote", "add", name, &path], "add push test remote")
                .unwrap();
        }
        (root, service, origin, backup)
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
        service.discard(&["main.rs".into()]).unwrap();
        assert!(
            !service
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
        let root = repository();
        let service = GitService::open(&root).unwrap();
        assert!(service.push(Some("--upload-pack=bad"), None).is_err());
        assert!(absolute_worktree_path(Path::new("relative")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn git_service_push_policy_rejects_protected_source_and_destination() {
        let root = repository();
        let service = GitService::open(&root).unwrap();

        let protected_source = service
            .push_with_branch_policy(None, Some("feature/fix"), "develop", "develop")
            .unwrap_err();
        assert!(
            protected_source
                .to_string()
                .contains("protected branch policy")
        );

        let protected_destination = service
            .push_with_branch_policy(None, Some("refs/heads/develop"), "feature/fix", "develop")
            .unwrap_err();
        assert!(
            protected_destination
                .to_string()
                .contains("protected branch policy")
        );
        assert!(push_branch_name("feature/fix:develop").is_err());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_non_origin_push_uses_the_backup_default_branch() {
        let (root, service, origin, backup) = push_repository();
        let origin_url = origin.to_string_lossy().into_owned();
        let backup_url = backup.to_string_lossy().into_owned();

        let error = service
            .push_with_target_default_branch(Some("backup"), None, "feature/push", |push_url| {
                assert_eq!(push_url, backup_url);
                assert_ne!(push_url, origin_url);
                Ok("feature/push".into())
            })
            .unwrap_err();
        assert!(error.to_string().contains("protected branch policy"));

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(origin).unwrap();
        std::fs::remove_dir_all(backup).unwrap();
    }

    #[test]
    fn omitted_push_remote_honors_branch_push_remote_before_origin_defaults() {
        let (root, service, origin, backup) = push_repository();
        let origin_url = origin.to_string_lossy().into_owned();
        let backup_url = backup.to_string_lossy().into_owned();
        service
            .run(
                &["config", "branch.feature/push.remote", "origin"],
                "configure branch fetch remote",
            )
            .unwrap();
        service
            .run(
                &["config", "remote.pushDefault", "origin"],
                "configure default push remote",
            )
            .unwrap();
        service
            .run(
                &["config", "branch.feature/push.pushRemote", "backup"],
                "configure branch push remote",
            )
            .unwrap();

        let error = service
            .push_with_target_default_branch(None, None, "feature/push", |push_url| {
                assert_eq!(push_url, backup_url);
                assert_ne!(push_url, origin_url);
                Ok("feature/push".into())
            })
            .unwrap_err();
        assert!(error.to_string().contains("protected branch policy"));

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(origin).unwrap();
        std::fs::remove_dir_all(backup).unwrap();
    }

    #[test]
    fn push_rejects_a_remote_with_multiple_push_urls() {
        let (root, service, origin, backup) = push_repository();
        let backup_url = backup.to_string_lossy().into_owned();
        service
            .run(
                &["config", "--add", "remote.backup.pushurl", &backup_url],
                "configure first backup push URL",
            )
            .unwrap();
        service
            .run(
                &["config", "--add", "remote.backup.pushurl", &backup_url],
                "configure second backup push URL",
            )
            .unwrap();

        let error = service
            .resolve_push_destination(Some("backup"), "feature/push")
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("one unambiguous remote push URL")
        );

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(origin).unwrap();
        std::fs::remove_dir_all(backup).unwrap();
    }

    #[test]
    fn push_without_destination_uses_one_explicit_refspec_despite_remote_push_config() {
        let root = repository();
        let service = GitService::open(&root).unwrap();
        let run_git = |directory: &Path, arguments: &[&str]| {
            let output = Command::new("git")
                .args(arguments)
                .current_dir(directory)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {arguments:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        };

        run_git(&root, &["branch", "-M", "main"]);
        std::fs::write(root.join("main.txt"), "main\n").unwrap();
        service.stage(&["main.txt".into()]).unwrap();
        service.commit("test: initialize protected main").unwrap();
        service.create_branch("feature/push", None).unwrap();
        service.switch_branch("feature/push", false).unwrap();
        std::fs::write(root.join("feature.txt"), "feature\n").unwrap();
        service.stage(&["feature.txt".into()]).unwrap();
        service.commit("test: add feature commit").unwrap();

        let remote = root.with_extension("push-remote.git");
        let remote_path = remote.to_string_lossy().into_owned();
        let output = Command::new("git")
            .args(["init", "--bare", "-q", &remote_path])
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(output.status.success());
        service
            .run(
                &["remote", "add", "origin", &remote_path],
                "add test remote",
            )
            .unwrap();
        service
            .run(
                &[
                    "config",
                    "--add",
                    "remote.origin.push",
                    "refs/heads/main:refs/heads/main",
                ],
                "configure protected push ref",
            )
            .unwrap();
        service
            .run(
                &[
                    "config",
                    "--add",
                    "remote.origin.push",
                    "refs/heads/feature/push:refs/heads/feature/push",
                ],
                "configure feature push ref",
            )
            .unwrap();
        service
            .run(
                &["config", "push.default", "matching"],
                "configure push default",
            )
            .unwrap();

        assert_eq!(
            explicit_push_refspec("feature/push", "feature/push").unwrap(),
            "refs/heads/feature/push:refs/heads/feature/push"
        );
        service
            .push_with_branch_policy(Some("origin"), None, "feature/push", "main")
            .unwrap();

        let refs = Command::new("git")
            .args(["for-each-ref", "--format=%(refname:short)"])
            .current_dir(&remote)
            .output()
            .unwrap();
        assert!(refs.status.success());
        let refs = String::from_utf8(refs.stdout).unwrap();
        assert_eq!(refs.trim(), "feature/push");

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(remote).unwrap();
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

    #[test]
    fn status_parser_keeps_unmerged_conflict_paths() {
        let parsed = parse_status(concat!(
            "# branch.head main\0",
            "u UU N... 100644 100644 100644 100644 a b c conflicted.txt\0",
        ))
        .unwrap();
        assert_eq!(parsed.conflicts, vec!["conflicted.txt"]);
        assert_eq!(parsed.entries[0].path, "conflicted.txt");
        assert_eq!(parsed.entries[0].index_status, 'U');
        assert_eq!(parsed.entries[0].worktree_status, 'U');
    }

    #[test]
    fn git_service_stays_in_the_opened_workspace_directory() {
        let root = repository();
        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::write(root.join("root.rs"), "fn root() {}\n").unwrap();
        std::fs::write(root.join("nested/lib.rs"), "fn nested() {}\n").unwrap();
        let service = GitService::open(root.join("nested")).unwrap();
        assert_eq!(service.root(), root.join("nested").canonicalize().unwrap());
        service.stage(&["lib.rs".into()]).unwrap();
        let status = service.status().unwrap();
        assert!(
            status.entries.iter().any(|entry| {
                entry.path.contains("lib.rs") && entry.index_status != '?' && !entry.untracked
            }),
            "Git mutations must apply inside the opened workspace, got {status:?}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn git_status_does_not_execute_repository_aliases() {
        let root = repository();
        std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
        assert!(
            Command::new("git")
                .args(["config", "alias.status", "!touch pwned"])
                .current_dir(&root)
                .status()
                .unwrap()
                .success()
        );
        let service = GitService::open(&root).unwrap();
        service.status().unwrap();
        assert!(
            !root.join("pwned").exists(),
            "repository Git aliases must not run"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
