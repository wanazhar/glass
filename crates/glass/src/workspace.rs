//! On-demand local workflow state. Bounded, atomic, locked; never injected wholesale.
use crate::{auth, config, input::KeyReader, modal};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};
const CAP: usize = 2 * 1024 * 1024;
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Board {
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub hypotheses: Vec<Hypothesis>,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    #[serde(default)]
    pub roles: BTreeMap<String, Role>,
    #[serde(default)]
    pub handoffs: Vec<Handoff>,
    #[serde(default)]
    pub reviews: BTreeMap<String, Review>,
    #[serde(default)]
    pub findings: Vec<Finding>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: u32,
    pub title: String,
    pub acceptance: String,
    pub depends: Vec<u32>,
    pub owner: Option<String>,
    pub status: String,
    pub paths: Vec<String>,
    pub checks: Vec<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hypothesis {
    pub id: u32,
    pub claim: String,
    pub evidence: Vec<String>,
    pub instrumentation: Vec<String>,
    pub resolved: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub id: u32,
    pub label: String,
    pub path: PathBuf,
    pub fingerprint: String,
    pub bytes: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Role {
    pub model: String,
    pub tokens: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handoff {
    pub agent: String,
    pub task: u32,
    pub changed_files: Vec<String>,
    pub checks: Vec<String>,
    pub findings: Vec<String>,
    pub questions: Vec<String>,
    pub artifacts: Vec<u32>,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub reviewed: bool,
    pub comments: Vec<String>,
}
/// A review finding remains attributed and actionable after the prose turn ends.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    #[serde(default)]
    pub id: u32,
    pub severity: String,
    pub path: String,
    pub line: u32,
    pub title: String,
    pub evidence: String,
    #[serde(default = "open_disposition")]
    pub disposition: String,
}
fn open_disposition() -> String { "open".into() }
fn validate_finding(finding: &Finding) -> Result<(), String> {
    valid_path(&finding.path)?;
    short(&finding.title)?;
    if finding.line == 0 || finding.evidence.trim().is_empty() || finding.evidence.len() > 16 * 1024
        || !matches!(finding.severity.as_str(), "critical" | "high" | "medium" | "low" | "info")
        || !matches!(finding.disposition.as_str(), "open" | "accepted" | "fixed" | "dismissed")
    { return Err("Invalid finding severity, location, evidence or disposition".into()); }
    Ok(())
}

pub fn hash(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn directory(root: &Path) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    Ok(config::data_dir()
        .join("workspaces")
        .join(hash(root.to_string_lossy().as_bytes())))
}
pub fn load(root: &Path) -> Result<Board, String> {
    let path = directory(root)?.join("workflow.json");
    if !path.try_exists().map_err(|e| e.to_string())? {
        return Ok(Board::default());
    }
    let board = serde_json::from_slice(&crate::requests::bounded(&path, CAP)?)
        .map_err(|_| "Workflow state is invalid; it has not been overwritten")?;
    validate(&board)?;
    Ok(board)
}
pub fn edit<T>(
    root: &Path,
    update: impl FnOnce(&mut Board) -> Result<T, String>,
) -> Result<T, String> {
    let dir = directory(root)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(dir.join("workflow.lock"))
        .map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Invalid workflow lock".into());
    }
    fs2::FileExt::try_lock_exclusive(&file)
        .map_err(|_| "Workflow is being updated; retry".to_string())?;
    let mut board = load(root)?;
    let result = update(&mut board)?;
    validate(&board)?;
    let bytes = serde_json::to_vec_pretty(&board).map_err(|e| e.to_string())?;
    if bytes.len() > CAP {
        return Err("Workflow state exceeds 2 MiB; archive old entries first".into());
    }
    auth::secure_write(&dir.join("workflow.json"), &bytes)?;
    Ok(result)
}
fn short(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 16 * 1024 || s.contains('\0') {
        Err("Text must be nonempty and at most 16 KiB".into())
    } else {
        Ok(())
    }
}
fn identity(value: &str) -> Result<(), String> {
    short(value)?;
    if value.len() > 96 || value.chars().any(char::is_control) {
        return Err(
            "Owner and agent labels must be at most 96 bytes without control characters".into(),
        );
    }
    Ok(())
}
fn strings(values: &[String]) -> Result<(), String> {
    if values.len() > 128 {
        return Err("At most 128 entries".into());
    }
    for s in values {
        short(s)?;
    }
    Ok(())
}
fn valid_path(s: &str) -> Result<(), String> {
    short(s)?;
    if Path::new(s).is_absolute()
        || s.split('/').any(|p| p.is_empty() || p == "." || p == "..")
        || s.contains('\\')
    {
        return Err(
            "Ownership paths must be normalized relative paths without dot segments".into(),
        );
    }
    Ok(())
}
fn validate(board: &Board) -> Result<(), String> {
    use std::collections::HashSet;
    let mut finding_ids = HashSet::new();
    for finding in &board.findings {
        validate_finding(finding)?;
        if finding.id == 0 || !finding_ids.insert(finding.id) { return Err("Duplicate finding ID".into()); }
    }
    if [
        board.tasks.len(),
        board.hypotheses.len(),
        board.artifacts.len(),
        board.handoffs.len(),
        board.reviews.len(),
        board.findings.len(),
    ]
    .iter()
    .any(|n| *n > 256)
    {
        return Err("At most 256 entries per collection".into());
    }
    let mut ids = HashSet::new();
    for task in &board.tasks {
        if task.id == 0 || !ids.insert(task.id) {
            return Err("Duplicate or zero task ID".into());
        }
        short(&task.title)?;
        short(&task.acceptance)?;
        strings(&task.paths)?;
        strings(&task.checks)?;
        for p in &task.paths {
            valid_path(p)?;
        }
        if !matches!(task.status.as_str(), "pending" | "claimed" | "done")
            || ((task.status == "pending") != task.owner.is_none())
        {
            return Err("Invalid task ownership/status".into());
        }
        if let Some(owner) = &task.owner {
            identity(owner)?;
        }
        if task.status == "done" && task.checks.is_empty() {
            return Err("Completed task requires checks".into());
        }
        if task.depends.len() > 128
            || task
                .depends
                .iter()
                .any(|id| *id >= task.id || !board.tasks.iter().any(|t| t.id == *id))
        {
            return Err("Dependencies must refer to earlier existing tasks".into());
        }
    }
    ids.clear();
    for h in &board.hypotheses {
        if h.id == 0 || !ids.insert(h.id) {
            return Err("Invalid hypothesis ID".into());
        }
        short(&h.claim)?;
        strings(&h.evidence)?;
        strings(&h.instrumentation)?;
        if h.resolved && h.evidence.is_empty() {
            return Err("Resolved hypothesis requires evidence".into());
        }
    }
    ids.clear();
    for a in &board.artifacts {
        if a.id == 0
            || !ids.insert(a.id)
            || a.bytes > 8 * 1024 * 1024
            || !a.path.is_absolute()
            || a.fingerprint.len() != 64
            || !a.fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("Invalid artifact".into());
        }
        short(&a.label)?;
    }
    for (fingerprint, review) in &board.reviews {
        if fingerprint.len() != 64 || !fingerprint.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid review fingerprint".into());
        }
        strings(&review.comments)?;
    }
    for handoff in &board.handoffs {
        identity(&handoff.agent)?;
        strings(&handoff.changed_files)?;
        for p in &handoff.changed_files {
            valid_path(p)?;
        }
        strings(&handoff.checks)?;
        strings(&handoff.findings)?;
        strings(&handoff.questions)?;
        if !board.tasks.iter().any(|t| t.id == handoff.task) {
            return Err("Handoff references unknown task".into());
        }
        if handoff.artifacts.len() > 128
            || handoff
                .artifacts
                .iter()
                .any(|id| !board.artifacts.iter().any(|a| a.id == *id))
        {
            return Err("Handoff references unknown artifact".into());
        }
    }
    for (name, role) in &board.roles {
        if !matches!(name.as_str(), "executor" | "reviewer" | "research") {
            return Err("Unknown role".into());
        }
        short(&role.model)?;
        if role.model.len() > 256
            || role.model.chars().any(char::is_control)
            || !(1024..=1_000_000).contains(&role.tokens)
        {
            return Err("Role needs a model ID and budget of 1024–1000000 tokens".into());
        }
    }
    Ok(())
}
fn next(ids: impl Iterator<Item = u32>) -> Result<u32, String> {
    ids.max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| "ID space exhausted".into())
}
pub fn add_task(
    board: &mut Board,
    title: &str,
    acceptance: &str,
    depends: Vec<u32>,
    paths: Vec<String>,
) -> Result<u32, String> {
    short(title)?;
    short(acceptance)?;
    for p in &paths {
        valid_path(p)?;
    }
    if depends
        .iter()
        .any(|id| !board.tasks.iter().any(|t| t.id == *id))
    {
        return Err("Dependency must refer to an existing task".into());
    }
    let id = next(board.tasks.iter().map(|t| t.id))?;
    board.tasks.push(Task {
        id,
        title: title.into(),
        acceptance: acceptance.into(),
        depends,
        owner: None,
        status: "pending".into(),
        paths,
        checks: vec![],
    });
    Ok(id)
}
pub fn claim(board: &mut Board, id: u32, owner: &str) -> Result<(), String> {
    identity(owner)?;
    let i = board
        .tasks
        .iter()
        .position(|t| t.id == id)
        .ok_or("Unknown task")?;
    if board.tasks[i].status != "pending" || board.tasks[i].owner.is_some() {
        return Err("Task already claimed or completed".into());
    }
    if board.tasks[i].depends.iter().any(|id| {
        board
            .tasks
            .iter()
            .any(|t| t.id == *id && t.status != "done")
    }) {
        return Err("Task has unfinished dependencies".into());
    }
    let paths = &board.tasks[i].paths;
    for t in &board.tasks {
        if t.status == "claimed"
            && t.owner.as_deref() != Some(owner)
            && t.paths.iter().any(|p| paths.iter().any(|q| overlap(p, q)))
        {
            return Err(format!(
                "Ownership conflict with task {} ({})",
                t.id,
                t.owner.as_deref().unwrap_or("unknown")
            ));
        }
    }
    board.tasks[i].owner = Some(owner.into());
    board.tasks[i].status = "claimed".into();
    Ok(())
}
fn overlap(a: &str, b: &str) -> bool {
    Path::new(a).starts_with(b) || Path::new(b).starts_with(a)
}
fn ownership_path(root: &Path, text: &str) -> Result<String, String> {
    valid_path(text)?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let mut path = root.join(text);
    let mut suffix = Vec::new();
    while !path.exists() {
        suffix.push(
            path.file_name()
                .ok_or("Invalid ownership path")?
                .to_os_string(),
        );
        if !path.pop() {
            return Err("Invalid ownership path".into());
        }
    }
    let mut resolved = path.canonicalize().map_err(|e| e.to_string())?;
    for s in suffix.iter().rev() {
        resolved.push(s);
    }
    let relative = resolved
        .strip_prefix(&root)
        .map_err(|_| "Ownership path escapes workspace")?
        .to_str()
        .ok_or("Ownership path must be UTF-8")?
        .to_string();
    valid_path(&relative)?;
    Ok(relative)
}
fn scoped(root: &Path, path: &str) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let path = root.join(path).canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(root) {
        return Err("Path escapes the workspace".into());
    }
    Ok(path)
}
pub fn artifact(root: &Path, label: &str, path: &str) -> Result<Artifact, String> {
    short(label)?;
    let path = scoped(root, path)?;
    let bytes = crate::requests::bounded(&path, 8 * 1024 * 1024)?;
    Ok(Artifact {
        id: 0,
        label: label.into(),
        path,
        fingerprint: hash(&bytes),
        bytes: bytes.len() as u64,
    })
}
pub fn preview(root: &Path, id: u32) -> Result<String, String> {
    let board = load(root)?;
    let item = board
        .artifacts
        .iter()
        .find(|a| a.id == id)
        .ok_or("Unknown artifact")?;
    let path = scoped(root, item.path.to_str().ok_or("Non UTF-8 path")?)?;
    let bytes = crate::requests::bounded(&path, 8 * 1024 * 1024)?;
    if hash(&bytes) != item.fingerprint {
        return Err("Artifact changed since registration; register the new version".into());
    }
    if let Ok(text) = std::str::from_utf8(&bytes) {
        let mut end = text.len().min(32 * 1024);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        Ok(format!(
            "{} · {} bytes\n{}{}",
            item.label,
            item.bytes,
            &text[..end],
            if end < text.len() {
                "\n[preview capped at 32 KiB]"
            } else {
                ""
            }
        ))
    } else {
        Ok(format!(
            "{} · binary artifact · {} bytes\n{}",
            item.label,
            item.bytes,
            path.display()
        ))
    }
}
pub fn artifact_range(root: &Path, id: u32, offset: usize, limit: usize) -> Result<serde_json::Value, String> {
    if limit == 0 || limit > 32 * 1024 { return Err("Read limit must be 1..32768 bytes".into()); }
    let board = load(root)?;
    let item = board.artifacts.iter().find(|a| a.id == id).ok_or("Unknown artifact")?;
    let path = scoped(root, item.path.to_str().ok_or("Non UTF-8 path")?)?;
    let bytes = crate::requests::bounded(&path, 8 * 1024 * 1024)?;
    if hash(&bytes) != item.fingerprint { return Err("Artifact changed since registration".into()); }
    let text = std::str::from_utf8(&bytes).map_err(|_| "Artifact is binary; textual range unavailable")?;
    if offset > text.len() || !text.is_char_boundary(offset) { return Err("Byte offset must be an in-range UTF-8 boundary".into()); }
    let mut end = offset.saturating_add(limit).min(text.len());
    while !text.is_char_boundary(end) { end -= 1; }
    if end == offset && offset < text.len() { return Err("Limit cannot contain the next Unicode character".into()); }
    Ok(json!({"artifact":id,"revision":item.fingerprint,"byte_offset":offset,"end_byte_offset":end,"total_bytes":bytes.len(),"text":&text[offset..end],"next_byte_offset":(end < bytes.len()).then_some(end),"preview_truncated":end < bytes.len(),"source_truncated":false}))
}

pub fn role(root: &Path, name: &str) -> Option<Role> {
    load(root).ok()?.roles.get(name).cloned()
}
pub fn annotate(
    root: &Path,
    patch: &str,
    comment: Option<&str>,
    reviewed: bool,
) -> Result<(), String> {
    edit(root, |board| {
        let review = board.reviews.entry(hash(patch.as_bytes())).or_default();
        review.reviewed |= reviewed;
        if let Some(comment) = comment {
            short(comment)?;
            review.comments.push(comment.into());
        }
        Ok(())
    })
}
pub fn receive_handoff(root: &Path, value: serde_json::Value) -> Result<String, String> {
    let handoff: Handoff = serde_json::from_value(value).map_err(|_| "Invalid handoff schema")?;
    let summary = format!(
        "Handoff from {} · T{} · {} changed files · {} checks · {} open questions. Review /workspace → Handoffs.",
        handoff.agent,
        handoff.task,
        handoff.changed_files.len(),
        handoff.checks.len(),
        handoff.questions.len()
    );
    edit(root, |b| {
        b.handoffs.push(handoff);
        Ok(())
    })?;
    Ok(summary)
}
const HELP: &str = "gc workspace [--json]\n  task add TITLE --accept CRITERIA [--depends 1,2] [--paths src/a,src/b]\n  task claim ID OWNER | finish ID OWNER CHECK | release ID OWNER\n  debug add HYPOTHESIS | evidence ID TEXT | instrument ID PATH | resolve ID\n  artifact add LABEL PATH | preview ID | read ID BYTE_OFFSET LIMIT\n  findings import FILE.json | disposition ID open|accepted|fixed|dismissed\n  role set executor|reviewer|research MODEL TOKENS\n  handoff FILE.json\nState is local to this directory, works outside Git, and is never automatically added to context.\nClaims are atomic. Existing-task dependencies are acyclic by construction. Finish requires an attributed check.\nHandoff JSON: agent, task, changed_files, checks, findings, questions, artifacts.\n";
fn number(s: Option<&String>) -> Result<u32, String> {
    s.ok_or(HELP)?
        .parse()
        .map_err(|_| "Expected numeric ID".into())
}
fn option(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}
pub fn command(args: &[String]) -> Result<(), String> {
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    if args.iter().any(|a| matches!(a.as_str(), "--help" | "-h")) {
        crate::cli::text("Workspace", HELP);
        return Ok(());
    }
    let args = args
        .iter()
        .filter(|a| a.as_str() != "--json")
        .cloned()
        .collect::<Vec<_>>();
    let arg = |i| args.get(i).ok_or(HELP).map(String::as_str);
    let arity = match (
        args.first().map(String::as_str),
        args.get(1).map(String::as_str),
    ) {
        (None, _) => 0,
        (Some("list"), _) => 1,
        (Some("task"), Some("add")) => {
            if args.len() < 5 || (args.len() - 3) % 2 != 0 {
                return Err(HELP.into());
            }
            let mut seen = std::collections::BTreeSet::new();
            for pair in args[3..].chunks(2) {
                if !matches!(pair[0].as_str(), "--accept" | "--depends" | "--paths")
                    || !seen.insert(&pair[0])
                {
                    return Err("Unknown or repeated task option".into());
                }
            }
            args.len()
        }
        (Some("task"), Some("claim" | "release")) => 4,
        (Some("task"), Some("finish")) => 5,
        (Some("debug"), Some("add" | "resolve")) => 3,
        (Some("debug"), Some("evidence" | "instrument")) => 4,
        (Some("artifact"), Some("add")) => 4,
        (Some("artifact"), Some("preview")) => 3,
        (Some("artifact"), Some("read")) => 5,
        (Some("findings"), Some("import")) => 3,
        (Some("findings"), Some("disposition")) => 4,
        (Some("role"), Some("set")) => 5,
        (Some("handoff"), _) => 2,
        _ => return Err(HELP.into()),
    };
    if args.len() != arity {
        return Err(HELP.into());
    }
    let value = match args.first().map(String::as_str) {
        None | Some("list") => serde_json::to_value(load(&root)?).map_err(|e| e.to_string())?,
        Some("task") => match arg(1)? {
            "add" => {
                let title = arg(2)?;
                let accept = option(&args, "--accept").ok_or("--accept CRITERIA is required")?;
                let depends = option(&args, "--depends")
                    .map(|s| {
                        s.split(',')
                            .map(str::parse::<u32>)
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()
                    .map_err(|_| "Invalid dependencies")?
                    .unwrap_or_default();
                let paths = option(&args, "--paths")
                    .map(|s| {
                        s.split(',')
                            .map(|p| ownership_path(&root, p))
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?
                    .unwrap_or_default();
                let id = edit(&root, |b| add_task(b, title, &accept, depends, paths))?;
                json!({"task":id,"status":"pending"})
            }
            "claim" => {
                let id = number(args.get(2))?;
                edit(&root, |b| {
                    for task in &mut b.tasks {
                        task.paths = task
                            .paths
                            .iter()
                            .map(|p| ownership_path(&root, p))
                            .collect::<Result<Vec<_>, _>>()?;
                    }
                    claim(b, id, arg(3)?)
                })?;
                json!({"task":id,"status":"claimed"})
            }
            "finish" | "release" => {
                let id = number(args.get(2))?;
                let owner = arg(3)?;
                let finish = arg(1)? == "finish";
                edit(&root, |b| {
                    let task = b
                        .tasks
                        .iter_mut()
                        .find(|t| t.id == id)
                        .ok_or("Unknown task")?;
                    if task.owner.as_deref() != Some(owner) || task.status != "claimed" {
                        return Err("Only the claiming owner can finish or release".into());
                    }
                    if finish {
                        let check = arg(4)?;
                        short(check)?;
                        task.checks.push(check.into());
                        task.status = "done".into();
                    } else {
                        task.status = "pending".into();
                        task.owner = None;
                    }
                    Ok(())
                })?;
                json!({"task":id,"status":if finish{"done"}else{"pending"}})
            }
            _ => return Err(HELP.into()),
        },
        Some("debug") => {
            let action = arg(1)?;
            let id = edit(&root, |b| {
                if action == "add" {
                    let claim = arg(2)?;
                    short(claim)?;
                    let id = next(b.hypotheses.iter().map(|h| h.id))?;
                    b.hypotheses.push(Hypothesis {
                        id,
                        claim: claim.into(),
                        evidence: vec![],
                        instrumentation: vec![],
                        resolved: false,
                    });
                    return Ok(id);
                }
                let id = number(args.get(2))?;
                let h = b
                    .hypotheses
                    .iter_mut()
                    .find(|h| h.id == id)
                    .ok_or("Unknown hypothesis")?;
                match action {
                    "evidence" => {
                        short(arg(3)?)?;
                        h.evidence.push(arg(3)?.into());
                    }
                    "instrument" => {
                        let path = scoped(&root, arg(3)?)?;
                        h.instrumentation.push(path.display().to_string());
                    }
                    "resolve" => {
                        if h.evidence.is_empty() {
                            return Err("Add runtime evidence before resolving a hypothesis".into());
                        }
                        h.resolved = true;
                    }
                    _ => return Err(HELP.into()),
                }
                Ok(id)
            })?;
            json!({"hypothesis":id,"action":action})
        }
        Some("findings") => match arg(1)? {
            "import" => {
                let bytes = crate::requests::bounded(Path::new(arg(2)?), 256 * 1024)?;
                let mut findings: Vec<Finding> = serde_json::from_slice(&bytes).map_err(|e| format!("Invalid findings: {e}"))?;
                if findings.len() > 128 { return Err("Import at most 128 findings".into()); }
                for finding in &findings { validate_finding(finding)?; }
                let ids = edit(&root, |board| {
                    let mut ids = Vec::new();
                    for finding in &mut findings {
                        finding.id = next(board.findings.iter().map(|f| f.id))?;
                        ids.push(finding.id);
                        board.findings.push(finding.clone());
                    }
                    Ok(ids)
                })?;
                json!({"findings":ids})
            }
            "disposition" => {
                let id = number(args.get(2))?;
                let disposition = arg(3)?;
                edit(&root, |board| {
                    let finding = board.findings.iter_mut().find(|f| f.id == id).ok_or("Unknown finding")?;
                    finding.disposition = disposition.into();
                    validate_finding(finding)
                })?;
                json!({"finding":id,"disposition":disposition})
            }
            _ => return Err(HELP.into()),
        },
        Some("artifact") => match arg(1)? {
            "add" => {
                let mut item = artifact(&root, arg(2)?, arg(3)?)?;
                let id = edit(&root, |b| {
                    item.id = next(b.artifacts.iter().map(|a| a.id))?;
                    let id = item.id;
                    b.artifacts.push(item);
                    Ok(id)
                })?;
                json!({"artifact":id})
            }
            "preview" => json!({"text":preview(&root,number(args.get(2))?)?}),
            "read" => artifact_range(&root, number(args.get(2))?, arg(3)?.parse().map_err(|_| "Invalid byte offset")?, arg(4)?.parse().map_err(|_| "Invalid limit")?)?,
            _ => return Err(HELP.into()),
        },
        Some("role") if arg(1)? == "set" => {
            let name = arg(2)?;
            if !matches!(name, "executor" | "reviewer" | "research") {
                return Err("Role: executor, reviewer or research".into());
            }
            let model = arg(3)?;
            let tokens = arg(4)?.parse::<u64>().map_err(|_| "Invalid token budget")?;
            edit(&root, |b| {
                b.roles.insert(
                    name.into(),
                    Role {
                        model: model.into(),
                        tokens,
                    },
                );
                Ok(())
            })?;
            json!({"role":name,"model":model,"tokens":tokens})
        }
        Some("handoff") => {
            let bytes = crate::requests::bounded(&scoped(&root, arg(1)?)?, 64 * 1024)?;
            let value = serde_json::from_slice(&bytes).map_err(|_| "Invalid handoff schema")?;
            receive_handoff(&root, value)?;
            json!({"status":"handoff saved"})
        }
        _ => return Err(HELP.into()),
    };
    if std::env::args().any(|a| a == "--json") {
        crate::emit_json(value);
    } else {
        crate::cli::report("Workspace", &value);
    }
    Ok(())
}
fn tasks_view(keys: &KeyReader, root: &Path, board: &Board) -> Result<(), String> {
    let mut rows = vec!["＋ Add task".to_owned()];
    rows.extend(board.tasks.iter().map(|task| {
        format!(
            "T{} · {} · {}{}",
            task.id,
            task.status,
            task.title,
            task.owner
                .as_ref()
                .map(|o| format!(" · {o}"))
                .unwrap_or_default()
        )
    }));
    let Some(index) = modal::choose(
        keys,
        "Tasks",
        "Atomic claims · explicit acceptance · Esc back",
        &rows,
    ) else {
        return Ok(());
    };
    if index == 0 {
        let Some(title) = modal::field(keys, "New task", "Title", "", false) else {
            return Ok(());
        };
        let Some(acceptance) = modal::field(
            keys,
            "Acceptance",
            "How will completion be checked?",
            "",
            false,
        ) else {
            return Ok(());
        };
        edit(root, |b| add_task(b, &title, &acceptance, vec![], vec![]))?;
        return Ok(());
    }
    let task = &board.tasks[index - 1];
    let detail = format!(
        "# T{} · {}\n\n**Status:** {}\n\n**Acceptance:** {}\n\n**Dependencies:** {:?}\n\n**Owned paths:** {}\n\n**Checks:** {}",
        task.id,
        task.title,
        task.status,
        task.acceptance,
        task.depends,
        task.paths.join(", "),
        task.checks.join("; ")
    );
    let actions = if task.status == "pending" {
        vec!["Read details".into(), "Claim task…".into()]
    } else if task.status == "claimed" {
        vec![
            "Read details".into(),
            "Finish with check…".into(),
            "Release claim…".into(),
        ]
    } else {
        vec!["Read details".into()]
    };
    let Some(action) = modal::choose(keys, "Selected task", &task.title, &actions) else {
        return Ok(());
    };
    if action == 0 {
        modal::rich_info(keys, "Task details", &detail);
        return Ok(());
    }
    let Some(owner) = modal::field(
        keys,
        "Task owner",
        "Claim owner (cooperative attribution)",
        task.owner.as_deref().unwrap_or("human"),
        false,
    ) else {
        return Ok(());
    };
    if task.status == "pending" {
        edit(root, |b| {
            for t in &mut b.tasks {
                t.paths = t
                    .paths
                    .iter()
                    .map(|p| ownership_path(root, p))
                    .collect::<Result<_, _>>()?;
            }
            claim(b, task.id, &owner)
        })?;
    } else {
        let check = if action == 1 {
            let Some(check) = modal::field(
                keys,
                "Completion check",
                "Describe a check actually performed",
                "",
                false,
            ) else {
                return Ok(());
            };
            Some(check)
        } else {
            None
        };
        edit(root, |b| {
            let current = b
                .tasks
                .iter_mut()
                .find(|t| t.id == task.id)
                .ok_or("Task no longer exists")?;
            if current.status != "claimed" || current.owner.as_deref() != Some(&owner) {
                return Err("Claim changed; reopen the task".into());
            }
            if let Some(check) = check {
                short(&check)?;
                current.checks.push(check);
                current.status = "done".into();
            } else {
                current.owner = None;
                current.status = "pending".into();
            }
            Ok(())
        })?;
    }
    Ok(())
}
fn debug_view(keys: &KeyReader, root: &Path, board: &Board) -> Result<(), String> {
    let mut rows = vec!["＋ Add hypothesis".into()];
    rows.extend(board.hypotheses.iter().map(|h| {
        format!(
            "H{} · {} · {}",
            h.id,
            if h.resolved {
                "resolved"
            } else {
                "investigating"
            },
            h.claim
        )
    }));
    let Some(index) = modal::choose(
        keys,
        "Debug evidence",
        "Hypothesis → instrument → observe → verify → remove instrumentation",
        &rows,
    ) else {
        return Ok(());
    };
    if index == 0 {
        if let Some(claim) =
            modal::field(keys, "Hypothesis", "A falsifiable explanation", "", false)
        {
            edit(root, |b| {
                short(&claim)?;
                let id = next(b.hypotheses.iter().map(|h| h.id))?;
                b.hypotheses.push(Hypothesis {
                    id,
                    claim,
                    evidence: vec![],
                    instrumentation: vec![],
                    resolved: false,
                });
                Ok(())
            })?;
        }
        return Ok(());
    }
    let h = &board.hypotheses[index - 1];
    let Some(action) = modal::choose(
        keys,
        "Selected hypothesis",
        &h.claim,
        &[
            "Read evidence".into(),
            "Add observed evidence…".into(),
            "Register instrumentation file…".into(),
            "Resolve hypothesis".into(),
        ],
    ) else {
        return Ok(());
    };
    if action == 0 {
        crate::structured::view(
            keys,
            "Hypothesis",
            &serde_json::to_string_pretty(h).map_err(|e| e.to_string())?,
        );
        return Ok(());
    }
    let value = if action == 3 {
        None
    } else {
        let Some(value) = modal::field(
            keys,
            if action == 1 {
                "Observed evidence"
            } else {
                "Instrumentation"
            },
            if action == 1 {
                "Actual results; avoid secrets"
            } else {
                "Workspace-relative path; remove temporary instrumentation before completion"
            },
            "",
            false,
        ) else {
            return Ok(());
        };
        Some(value)
    };
    let instrument = if action == 2 {
        Some(scoped(root, value.as_deref().unwrap())?)
    } else {
        None
    };
    let id = h.id;
    edit(root, |b| {
        let h = b
            .hypotheses
            .iter_mut()
            .find(|h| h.id == id)
            .ok_or("Hypothesis missing")?;
        match action {
            1 => {
                let value = value.unwrap();
                short(&value)?;
                h.evidence.push(value);
            }
            2 => h
                .instrumentation
                .push(instrument.unwrap().display().to_string()),
            _ => {
                if h.evidence.is_empty() {
                    return Err("Record observed evidence before resolving".into());
                }
                h.resolved = true;
            }
        }
        Ok(())
    })?;
    Ok(())
}
pub fn view(keys: &KeyReader, root: &Path) {
    loop {
        let options = [
            "Tasks · dependencies and ownership",
            "Debug · hypotheses and evidence",
            "Artifacts · local outputs",
            "Roles · model and budget presets",
            "Handoffs · checks and open questions",
            "Review · hunk comments",
            "Findings · severity, evidence and disposition",
        ]
        .map(String::from);
        let Some(index) = modal::choose(
            keys,
            "Workspace",
            "Local metadata · loaded on demand · Esc closes",
            &options,
        ) else {
            return;
        };
        let board = match load(root) {
            Ok(b) => b,
            Err(e) => {
                modal::info(keys, "Workspace error", &e);
                return;
            }
        };
        if index == 0 || index == 1 {
            let result = if index == 0 {
                tasks_view(keys, root, &board)
            } else {
                debug_view(keys, root, &board)
            };
            if let Err(e) = result {
                modal::info(keys, "Workspace action", &e);
            }
            continue;
        }
        if index == 6 {
            let rows = board.findings.iter().map(|f| format!("{} · {}:{} · {} · {}", f.severity, f.path, f.line, f.title, f.disposition)).collect::<Vec<_>>();
            let Some(selected) = modal::choose_search(keys, "Review findings", "Select to inspect evidence and disposition", &rows, &rows) else { continue };
            let finding = &board.findings[selected];
            modal::markdown(keys, &finding.title, &format!("**{}** · {}:{}\n\n{}\n\nDisposition: {}", finding.severity, finding.path, finding.line, finding.evidence, finding.disposition));
            let choices = ["open", "accepted", "fixed", "dismissed"].map(String::from);
            if let Some(choice) = modal::choose(keys, "Finding disposition", &finding.title, &choices) {
                if let Err(error) = edit(root, |b| {
                    let item = b.findings.iter_mut().find(|f| f.id == finding.id).ok_or("Finding no longer exists")?;
                    item.disposition = choices[choice].clone();
                    Ok(())
                }) { modal::info(keys, "Finding was not changed", &error); }
            }
            continue;
        }
        if index == 3 {
            let names = ["executor", "reviewer", "research"];
            let roles = names
                .iter()
                .map(|name| {
                    format!(
                        "{} · {}",
                        name,
                        board
                            .roles
                            .get(*name)
                            .map(|r| format!("{} · {} tokens", r.model, r.tokens))
                            .unwrap_or_else(|| "not configured".into())
                    )
                })
                .collect::<Vec<_>>();
            if let Some(i) = modal::choose(
                keys,
                "Role presets",
                "Used only when explicitly selected by an agent command",
                &roles,
            ) {
                let role = board.roles.get(names[i]);
                if let Some(model) = modal::field(
                    keys,
                    "Role model",
                    "Exact model ID",
                    role.map_or("", |r| r.model.as_str()),
                    false,
                ) && let Some(tokens) = modal::validated_field(
                    keys,
                    "Role budget",
                    "Cumulative token budget",
                    &role.map_or(10000, |r| r.tokens).to_string(),
                    false,
                    |s| {
                        s.parse::<u64>()
                            .ok()
                            .filter(|n| (1024..=1_000_000).contains(n))
                            .map(|_| ())
                            .ok_or("Choose 1024–1000000 tokens".into())
                    },
                ) && let Err(e) = edit(root, |b| {
                    b.roles.insert(
                        names[i].into(),
                        Role {
                            model,
                            tokens: tokens.parse().unwrap(),
                        },
                    );
                    Ok(())
                }) {
                    modal::info(keys, "Role preset", &e);
                }
            }
            continue;
        }
        if index == 4 {
            let labels = board
                .handoffs
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    format!(
                        "{} · T{} · {} · {} checks · {} questions",
                        i + 1,
                        h.task,
                        h.agent,
                        h.checks.len(),
                        h.questions.len()
                    )
                })
                .collect::<Vec<_>>();
            if labels.is_empty() {
                modal::info(
                    keys,
                    "Handoffs",
                    "No handoffs yet. gc agent handoff FILE.json validates a report and notifies the parent. Reports are attribution, not proof of correctness.",
                );
            } else if let Some(i) = modal::choose(
                keys,
                "Handoffs",
                "Select findings, checks and unresolved questions",
                &labels,
            ) {
                crate::structured::view(
                    keys,
                    "Handoff",
                    &serde_json::to_string_pretty(&board.handoffs[i]).unwrap_or_default(),
                );
            }
            continue;
        }
        if index == 2 {
            let labels = board
                .artifacts
                .iter()
                .map(|a| format!("{} · {} · {} bytes", a.id, a.label, a.bytes))
                .collect::<Vec<_>>();
            if labels.is_empty() {
                modal::info(
                    keys,
                    "Artifacts",
                    "No artifacts. Register one with gc workspace artifact add LABEL PATH. Nothing enters model context automatically.",
                );
            } else if let Some(i) = modal::choose(
                keys,
                "Artifacts",
                "Select a bounded preview; binary files show their path",
                &labels,
            ) {
                modal::info(
                    keys,
                    "Artifact preview",
                    &preview(root, board.artifacts[i].id).unwrap_or_else(|e| e),
                );
            }
            continue;
        }
        let value = match index {
            0 => json!(board.tasks),
            1 => json!(board.hypotheses),
            3 => json!(board.roles),
            4 => json!(board.handoffs),
            _ => json!(board.reviews),
        };
        let text = if value == json!([]) || value == json!({}) {
            "Nothing recorded yet. Use gc workspace --help to add local entries. No model calls are made by this view.".into()
        } else {
            serde_json::to_string_pretty(&value).unwrap_or_default()
        };
        crate::structured::view(keys, &options[index], &text);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn review_findings_require_evidence_location_and_known_disposition() {
        let mut finding = Finding { id:1, severity:"high".into(),path:"src/main.rs".into(),line:5,title:"Incorrect result".into(),evidence:"The branch returns before saving".into(),disposition:"open".into() };
        assert!(validate_finding(&finding).is_ok());
        finding.line = 0; assert!(validate_finding(&finding).is_err());
        finding.line = 5; finding.path = "../outside".into(); assert!(validate_finding(&finding).is_err());
        finding.path = "src/main.rs".into(); finding.disposition = "pretend-fixed".into(); assert!(validate_finding(&finding).is_err());
    }

    #[test]
    fn dependencies_claims_and_ownership() {
        let mut b = Board::default();
        let a = add_task(
            &mut b,
            "parser",
            "tests pass",
            vec![],
            vec!["src/parser".into()],
        )
        .unwrap();
        let c = add_task(&mut b, "tests", "tests pass", vec![a], vec![]).unwrap();
        assert!(claim(&mut b, c, "test").is_err());
        claim(&mut b, a, "main").unwrap();
        assert!(claim(&mut b, a, "other").is_err());
        let d = add_task(
            &mut b,
            "overlap",
            "pass",
            vec![],
            vec!["src/parser/mod.rs".into()],
        )
        .unwrap();
        assert!(claim(&mut b, d, "other").is_err());
        b.tasks[0].status = "done".into();
        b.tasks[0].checks.push("verified".into());
        claim(&mut b, c, "test").unwrap();
    }
    #[test]
    fn invalid_handoffs_and_budgets_are_refused() {
        let mut b = Board::default();
        b.roles.insert(
            "reviewer".into(),
            Role {
                model: "m".into(),
                tokens: 0,
            },
        );
        assert!(validate(&b).is_err());
        assert!(serde_json::from_str::<Handoff>(r#"{"agent":"a","surprise":1}"#).is_err());
    }
}
