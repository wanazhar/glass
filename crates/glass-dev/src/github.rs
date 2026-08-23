//! GitHub integration through the user's authenticated `gh` CLI.
//!
//! Read-only status remains cheap and cached for the TUI. Review and ship
//! operations are explicit, bounded subprocess calls used by the command
//! palette, task loop, and private cockpit; no shell is involved.

use crate::development::{DevelopmentError, DevelopmentResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};
const PROBE_INTERVAL: Duration = Duration::from_secs(15);
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const REVIEW_TIMEOUT: Duration = Duration::from_secs(8);
const SHIP_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_REMOTE_BYTES: usize = 4 * 1024;
const MAX_REVIEW_BYTES: usize = 64 * 1024;
const MAX_SHIP_OUTPUT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubRepository {
    pub name_with_owner: String,
    pub web_url: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GitHubAvailability {
    #[default]
    NoRemote,
    RemoteUnavailable,
    GhUnavailable,
    NotAuthenticated,
    Authenticated,
    TimedOut,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubStatus {
    pub repository: Option<GitHubRepository>,
    pub availability: GitHubAvailability,
}

impl GitHubStatus {
    pub fn summary(&self) -> String {
        match (&self.repository, self.availability) {
            (None, GitHubAvailability::NoRemote) => "no GitHub origin".into(),
            (None, GitHubAvailability::RemoteUnavailable) => "origin unavailable".into(),
            (Some(repository), GitHubAvailability::GhUnavailable) => format!(
                "{} · gh unavailable · install github.com/cli/cli",
                repository.name_with_owner
            ),
            (Some(repository), GitHubAvailability::NotAuthenticated) => format!(
                "{} · gh not authenticated · run `gh auth login`",
                repository.name_with_owner
            ),
            (Some(repository), GitHubAvailability::Authenticated) => {
                format!("{} · gh authenticated", repository.name_with_owner)
            }
            (Some(repository), GitHubAvailability::TimedOut) => format!(
                "{} · gh auth check timed out · retrying automatically",
                repository.name_with_owner
            ),
            (Some(repository), _) => {
                format!("{} · GitHub status unavailable", repository.name_with_owner)
            }
            (None, _) => "GitHub status unavailable".into(),
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.availability == GitHubAvailability::Authenticated
    }
}

#[derive(Debug, Default)]
pub struct GitHubProbeCache {
    root: Option<PathBuf>,
    checked_at: Option<Instant>,
    status: GitHubStatus,
}

impl GitHubProbeCache {
    pub fn probe(&mut self, root: &Path) -> GitHubStatus {
        if self.root.as_deref() == Some(root)
            && self
                .checked_at
                .is_some_and(|checked_at| checked_at.elapsed() < PROBE_INTERVAL)
        {
            return self.status.clone();
        }

        let status = probe_uncached(root);
        self.root = Some(root.to_path_buf());
        self.checked_at = Some(Instant::now());
        self.status = status.clone();
        status
    }
}

#[derive(Debug, Default)]
pub struct GitHubReviewCache {
    root: Option<PathBuf>,
    checked_at: Option<Instant>,
    review: Option<GitHubReview>,
}

impl GitHubReviewCache {
    pub fn review(&mut self, root: &Path) -> GitHubReview {
        if self.root.as_deref() == Some(root)
            && self
                .checked_at
                .is_some_and(|checked_at| checked_at.elapsed() < PROBE_INTERVAL)
            && let Some(review) = &self.review
        {
            return review.clone();
        }
        let review = review(root).unwrap_or_else(|error| GitHubReview {
            repository: None,
            branch: current_branch(root),
            availability: GitHubAvailability::RemoteUnavailable,
            pull_request: None,
            checks: Vec::new(),
            summary: "GitHub review unavailable".into(),
            error: Some(error.to_string()),
        });
        self.root = Some(root.to_path_buf());
        self.checked_at = Some(Instant::now());
        self.review = Some(review.clone());
        review
    }
}

fn probe_uncached(root: &Path) -> GitHubStatus {
    let remote = match read_origin(root) {
        RemoteProbe::Missing => {
            return GitHubStatus::default();
        }
        RemoteProbe::Unavailable => {
            return GitHubStatus {
                repository: None,
                availability: GitHubAvailability::RemoteUnavailable,
            };
        }
        RemoteProbe::Value(remote) => remote,
    };
    let Some(repository) = parse_github_remote(&remote) else {
        return GitHubStatus::default();
    };

    let availability = match run_gh_auth_status(root) {
        GhProbe::Missing => GitHubAvailability::GhUnavailable,
        GhProbe::Authenticated => GitHubAvailability::Authenticated,
        GhProbe::NotAuthenticated => GitHubAvailability::NotAuthenticated,
        GhProbe::TimedOut => GitHubAvailability::TimedOut,
    };
    GitHubStatus {
        repository: Some(repository),
        availability,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum RemoteProbe {
    Missing,
    Unavailable,
    Value(String),
}

fn read_origin(root: &Path) -> RemoteProbe {
    let mut child = match Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return RemoteProbe::Unavailable,
    };
    let Some(status) = wait_for_exit(&mut child, PROBE_TIMEOUT) else {
        let _ = child.kill();
        let _ = child.wait();
        return RemoteProbe::Unavailable;
    };
    if !status.success() {
        return RemoteProbe::Missing;
    }
    let output = match child.wait_with_output() {
        Ok(output) if output.stdout.len() <= MAX_REMOTE_BYTES => output.stdout,
        _ => return RemoteProbe::Unavailable,
    };
    let remote = String::from_utf8(output)
        .ok()
        .map(|remote| remote.trim().to_string())
        .filter(|remote| !remote.is_empty());
    remote.map_or(RemoteProbe::Missing, RemoteProbe::Value)
}

#[derive(Debug, PartialEq, Eq)]
enum GhProbe {
    Missing,
    Authenticated,
    NotAuthenticated,
    TimedOut,
}

fn run_gh_auth_status(root: &Path) -> GhProbe {
    let mut child = match Command::new("gh")
        .args(["auth", "status", "--hostname", "github.com"])
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return GhProbe::Missing,
        Err(_) => return GhProbe::NotAuthenticated,
    };
    let Some(status) = wait_for_exit(&mut child, PROBE_TIMEOUT) else {
        let _ = child.kill();
        let _ = child.wait();
        return GhProbe::TimedOut;
    };
    let _ = child.wait();
    if status.success() {
        GhProbe::Authenticated
    } else {
        GhProbe::NotAuthenticated
    }
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) | Err(_) => return None,
        }
    }
}

pub fn parse_github_remote(remote: &str) -> Option<GitHubRepository> {
    let remote = remote.trim();
    let path = if let Some(path) = remote.strip_prefix("git@github.com:") {
        path
    } else if let Some((_, authority_and_path)) = remote.split_once("://") {
        let (authority, path) = authority_and_path.split_once('/')?;
        let host = authority.rsplit('@').next()?.to_ascii_lowercase();
        if host != "github.com" {
            return None;
        }
        path
    } else {
        return None;
    };

    let mut segments = path.trim_matches('/').split('/');
    let owner = segments.next()?.trim();
    let repository = segments.next()?.trim_end_matches(".git").trim();
    if segments.next().is_some()
        || !valid_segment(owner)
        || !valid_segment(repository)
        || repository.is_empty()
    {
        return None;
    }
    let name_with_owner = format!("{owner}/{repository}");
    Some(GitHubRepository {
        web_url: format!("https://github.com/{name_with_owner}"),
        name_with_owner,
    })
}

fn valid_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment.len() <= 128
        && segment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubPullRequest {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub url: String,
    pub head_branch: Option<String>,
    pub base_branch: Option<String>,
    pub review_decision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubCheck {
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubReview {
    pub repository: Option<GitHubRepository>,
    pub branch: Option<String>,
    pub availability: GitHubAvailability,
    pub pull_request: Option<GitHubPullRequest>,
    pub checks: Vec<GitHubCheck>,
    pub summary: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubShipRequest {
    pub title: Option<String>,
    pub body: Option<String>,
    pub base: Option<String>,
    pub draft: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubShipResult {
    pub repository: GitHubRepository,
    pub branch: String,
    pub url: Option<String>,
    pub output: String,
}

impl GitHubReview {
    pub fn display(&self) -> String {
        let checks = self
            .checks
            .iter()
            .take(8)
            .map(|check| {
                format!(
                    "{} {}{}",
                    check.name,
                    check.conclusion.as_deref().unwrap_or(check.status.as_str()),
                    check
                        .link
                        .as_deref()
                        .map(|link| format!(" · {link}"))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>();
        let mut lines = vec![self.summary.clone()];
        if let Some(branch) = &self.branch {
            lines.push(format!("branch {branch}"));
        }
        lines.extend(checks);
        if let Some(error) = &self.error {
            lines.push(format!("error {error}"));
        }
        lines.join("\n")
    }
}
/// normal GitHub workflow states as transport failures.
pub fn review(root: &Path) -> DevelopmentResult<GitHubReview> {
    let status = probe_uncached(root);
    let branch = current_branch(root);
    let repository = status.repository.clone();
    if status.availability != GitHubAvailability::Authenticated {
        return Ok(GitHubReview {
            repository,
            branch,
            availability: status.availability,
            pull_request: None,
            checks: Vec::new(),
            summary: status.summary(),
            error: None,
        });
    }

    let (exit, stdout, stderr) = run_gh_capture(
        root,
        &[
            "pr",
            "view",
            "--json",
            "number,title,state,url,headRefName,baseRefName,reviewDecision,statusCheckRollup",
        ],
        REVIEW_TIMEOUT,
        MAX_REVIEW_BYTES,
    )?;
    if !exit.success() {
        let detail = compact_process_output(&stderr, &stdout);
        if detail.to_ascii_lowercase().contains("no pull request") {
            return Ok(GitHubReview {
                repository,
                branch,
                availability: status.availability,
                pull_request: None,
                checks: Vec::new(),
                summary: "No pull request for the current branch".into(),
                error: None,
            });
        }
        return Ok(GitHubReview {
            repository,
            branch,
            availability: status.availability,
            pull_request: None,
            checks: Vec::new(),
            summary: "GitHub pull-request review unavailable".into(),
            error: Some(detail),
        });
    }
    let value: Value = serde_json::from_slice(&stdout)?;
    let pull_request = parse_pull_request(&value)?;
    let mut checks = value
        .get("statusCheckRollup")
        .map(parse_checks)
        .unwrap_or_default();
    if let Some(number) = pull_request
        .as_ref()
        .map(|pull_request| pull_request.number)
    {
        let (checks_exit, checks_stdout, checks_stderr) = run_gh_capture(
            root,
            &[
                "pr",
                "checks",
                &number.to_string(),
                "--json",
                "name,state,conclusion,bucket,link",
            ],
            REVIEW_TIMEOUT,
            MAX_REVIEW_BYTES,
        )?;
        if checks_exit.success() {
            if let Ok(value) = serde_json::from_slice::<Value>(&checks_stdout) {
                checks = parse_checks(&value);
            }
        } else if checks.is_empty() {
            return Ok(GitHubReview {
                repository,
                branch,
                availability: status.availability,
                pull_request,
                checks,
                summary: "Pull request found; checks unavailable".into(),
                error: Some(compact_process_output(&checks_stderr, &checks_stdout)),
            });
        }
    }
    let summary = review_summary(pull_request.as_ref(), &checks);
    Ok(GitHubReview {
        repository,
        branch,
        availability: status.availability,
        pull_request,
        checks,
        summary,
        error: None,
    })
}

/// Create a pull request from the current branch after explicit caller
/// confirmation. Arguments are passed directly to `gh`; no shell is used.
pub fn ship(root: &Path, request: &GitHubShipRequest) -> DevelopmentResult<GitHubShipResult> {
    let status = probe_uncached(root);
    let repository = status
        .repository
        .clone()
        .ok_or_else(|| DevelopmentError::NotFound("GitHub origin is not configured".into()))?;
    if status.availability != GitHubAvailability::Authenticated {
        return Err(DevelopmentError::Conflict(format!(
            "GitHub ship requires authenticated gh CLI ({})",
            status.summary()
        )));
    }
    let branch = current_branch(root).ok_or_else(|| {
        DevelopmentError::Conflict("GitHub ship requires a named current branch".into())
    })?;
    if matches!(branch.as_str(), "main" | "master") {
        return Err(DevelopmentError::Conflict(
            "GitHub ship requires a feature branch, not the default branch".into(),
        ));
    }
    let title = bounded_optional("title", request.title.as_deref(), 512)?;
    let body = bounded_optional("body", request.body.as_deref(), 16 * 1024)?;
    let base = bounded_optional("base", request.base.as_deref(), 256)?;
    let mut args = vec!["pr", "create"];
    if let Some(title) = title.as_deref() {
        args.extend(["--title", title]);
        args.extend(["--body", body.as_deref().unwrap_or("")]);
    } else {
        args.push("--fill");
        if let Some(body) = body.as_deref() {
            args.extend(["--body", body]);
        }
    }
    if let Some(base) = base.as_deref() {
        args.extend(["--base", base]);
    }
    if request.draft {
        args.push("--draft");
    }
    let (exit, stdout, stderr) = run_gh_capture(root, &args, SHIP_TIMEOUT, MAX_SHIP_OUTPUT_BYTES)?;
    let output = compact_process_output(&stdout, &stderr);
    if !exit.success() {
        return Err(DevelopmentError::Process(format!(
            "gh pr create failed: {output}"
        )));
    }
    let url = output
        .split_whitespace()
        .find(|part| part.starts_with("https://github.com/"))
        .map(str::to_string);
    Ok(GitHubShipResult {
        repository,
        branch,
        url,
        output,
    })
}

fn current_branch(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(root)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.len() > MAX_REMOTE_BYTES {
        return None;
    }
    let branch = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!branch.is_empty()
        && branch.len() <= 256
        && branch
            .bytes()
            .all(|byte| !byte.is_ascii_control() && byte != b' '))
    .then_some(branch)
}

fn run_gh_capture(
    root: &Path,
    args: &[&str],
    timeout: Duration,
    limit: usize,
) -> DevelopmentResult<(ExitStatus, Vec<u8>, Vec<u8>)> {
    let mut child = Command::new("gh")
        .args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                DevelopmentError::NotFound("gh CLI is not installed".into())
            } else {
                DevelopmentError::Process(format!("failed to start gh: {error}"))
            }
        })?;
    let Some(status) = wait_for_exit(&mut child, timeout) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(DevelopmentError::Process(format!(
            "gh command timed out after {} seconds",
            timeout.as_secs()
        )));
    };
    let output = child.wait_with_output()?;
    if output.stdout.len() > limit || output.stderr.len() > limit {
        return Err(DevelopmentError::InvalidInput(
            "gh output exceeded the bounded GitHub response limit".into(),
        ));
    }
    Ok((status, output.stdout, output.stderr))
}

fn compact_process_output(primary: &[u8], secondary: &[u8]) -> String {
    let bytes = if !primary.is_empty() {
        primary
    } else {
        secondary
    };
    String::from_utf8_lossy(bytes)
        .trim()
        .chars()
        .take(2_048)
        .collect()
}

fn parse_pull_request(value: &Value) -> DevelopmentResult<Option<GitHubPullRequest>> {
    if value.is_null() || value.as_object().is_none() {
        return Ok(None);
    }
    let number = value.get("number").and_then(Value::as_u64).ok_or_else(|| {
        DevelopmentError::Serialization("GitHub PR response has no number".into())
    })?;
    let title = bounded_json_text(value, "title", 512)?;
    let state = bounded_json_text(value, "state", 64)?;
    let url = bounded_json_text(value, "url", 2_048)?;
    Ok(Some(GitHubPullRequest {
        number,
        title,
        state,
        url,
        head_branch: optional_json_text(value, "headRefName", 256),
        base_branch: optional_json_text(value, "baseRefName", 256),
        review_decision: optional_json_text(value, "reviewDecision", 128),
    }))
}

fn parse_checks(value: &Value) -> Vec<GitHubCheck> {
    let values = value
        .as_array()
        .cloned()
        .or_else(|| value.get("contexts").and_then(Value::as_array).cloned())
        .unwrap_or_default();
    values
        .into_iter()
        .take(256)
        .filter_map(|value| {
            let object = value.as_object()?;
            let name = object
                .get("name")
                .or_else(|| object.get("context"))
                .and_then(Value::as_str)?
                .chars()
                .take(256)
                .collect::<String>();
            if name.is_empty() {
                return None;
            }
            let status = object
                .get("status")
                .or_else(|| object.get("state"))
                .or_else(|| object.get("bucket"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .chars()
                .take(64)
                .collect();
            let conclusion = object
                .get("conclusion")
                .and_then(Value::as_str)
                .map(|value| value.chars().take(64).collect());
            let link = object
                .get("link")
                .or_else(|| object.get("detailsUrl"))
                .and_then(Value::as_str)
                .map(|value| value.split(['?', '#']).next().unwrap_or(value))
                .map(|value| value.chars().take(2_048).collect());
            Some(GitHubCheck {
                name,
                status,
                conclusion,
                link,
            })
        })
        .collect()
}

fn review_summary(pull_request: Option<&GitHubPullRequest>, checks: &[GitHubCheck]) -> String {
    let Some(pull_request) = pull_request else {
        return "No pull request for the current branch".into();
    };
    let failing = checks
        .iter()
        .filter(|check| {
            matches!(
                check.conclusion.as_deref().or(Some(check.status.as_str())),
                Some("FAILURE" | "failure" | "ERROR" | "error" | "CANCELLED" | "cancelled")
            )
        })
        .count();
    if failing > 0 {
        format!(
            "PR #{} · {} failing check{}",
            pull_request.number,
            failing,
            if failing == 1 { "" } else { "s" }
        )
    } else if checks.is_empty() {
        format!("PR #{} · checks pending", pull_request.number)
    } else {
        format!(
            "PR #{} · {} checks observed",
            pull_request.number,
            checks.len()
        )
    }
}

fn bounded_optional(
    label: &str,
    value: Option<&str>,
    limit: usize,
) -> DevelopmentResult<Option<String>> {
    value
        .map(|value| {
            if value.is_empty() || value.len() > limit || value.contains('\0') {
                Err(DevelopmentError::InvalidInput(format!(
                    "GitHub {label} must be 1..={limit} bytes without NUL"
                )))
            } else {
                Ok(value.to_string())
            }
        })
        .transpose()
}

fn bounded_json_text(value: &Value, key: &str, limit: usize) -> DevelopmentResult<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= limit && !value.contains('\0'))
        .map(str::to_string)
        .ok_or_else(|| {
            DevelopmentError::Serialization(format!(
                "GitHub response field {key} is missing or exceeds {limit} bytes"
            ))
        })
}

fn optional_json_text(value: &Value, key: &str, limit: usize) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= limit && !value.contains('\0'))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_github_remote_forms() {
        for remote in [
            "https://github.com/wanazhar/glass.git",
            "ssh://git@github.com/wanazhar/glass.git",
            "git@github.com:wanazhar/glass.git",
        ] {
            let repository = parse_github_remote(remote).unwrap();
            assert_eq!(repository.name_with_owner, "wanazhar/glass");
            assert_eq!(repository.web_url, "https://github.com/wanazhar/glass");
        }
    }

    #[test]
    fn rejects_non_github_and_unsafe_remote_forms() {
        for remote in [
            "https://gitlab.com/owner/repo.git",
            "git@github.com:owner/repo/extra.git",
            "git@github.com:owner/../repo.git",
            "github.com/owner/repo.git",
        ] {
            assert!(parse_github_remote(remote).is_none(), "{remote}");
        }
    }

    #[test]
    fn status_summary_never_requires_auth_output() {
        let status = GitHubStatus {
            repository: Some(GitHubRepository {
                name_with_owner: "owner/repo".into(),
                web_url: "https://github.com/owner/repo".into(),
            }),
            availability: GitHubAvailability::NotAuthenticated,
        };
        assert_eq!(
            status.summary(),
            "owner/repo · gh not authenticated · run `gh auth login`"
        );
    }
}
