//! Daily coding workflows. Git arguments and patches never pass through a shell.
use crate::{
    agent::{Agent, Progress, Stop},
    auth, config, ext,
    input::KeyReader,
    modal, onboarding, profiles, provider,
    session::{Record, Session},
    tools,
};
use glass_mod_api::Message;
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

#[derive(Default, Serialize, Deserialize)]
struct Plan {
    task: String,
    proposal: String,
}
fn plan_path(session: &Session) -> PathBuf {
    session.path.with_extension("plan.json")
}
pub fn plan(
    keys: &KeyReader,
    cfg: &config::Config,
    current: &mut Session,
    state: &mut tools::State,
    factory: crate::warmer::ProviderFactory,
    task: &str,
) -> Result<Option<String>, String> {
    let mut saved: Plan = match std::fs::read(plan_path(current)) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|_| "Saved plan is invalid; not overwritten")?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Plan::default(),
        Err(e) => return Err(e.to_string()),
    };
    let task = if !task.trim().is_empty() {
        Some(task.trim().to_string())
    } else if saved.proposal.is_empty() {
        modal::validated_field(keys, "Plan a task", "Describe the task", "", false, |s| {
            if s.trim().is_empty() {
                Err("Describe a task".into())
            } else {
                Ok(())
            }
        })
    } else {
        None
    };
    if let Some(task) = task {
        let workspace = cfg.workspace.clone();
        let tokens = cfg.max_tokens;
        let reasoning = cfg.reasoning_budget;
        let instructions = crate::trust::gated(cfg, &crate::agents_md::load(&cfg.workspace), false);
        let planning_prompt = cfg.system_prompt.clone();
        let request = format!(
            "{task}\n\nReturn a plan grouped under Markdown phase headings, with each actionable step as - [ ] task. Do not execute the plan."
        );
        let factory_for_worker = factory.clone();
        let result = modal::task(keys, "Inspecting and planning", move |cancel, status| {
            let p = factory_for_worker()?;
            let mut session =
                Session::create(&config::data_dir().join("plans")).map_err(|e| e.to_string())?;
            session
                .append_checked(&[Record::User(request)])
                .map_err(|e| format!("cannot record planning prompt: {e}"))?;
            let mut state = tools::State::new();
            let mods = Vec::new();
            let mut extensions = ext::Registry::empty();
            let mut agent = Agent::new(
                p.as_ref(),
                &mut session,
                &mut state,
                &mods,
                &mut extensions,
                &workspace,
            )
            .with_plan_mode(true)
            .with_prompt(planning_prompt, None, instructions)
            .with_limits(tokens, reasoning, 12)
            .with_stream_factory(factory_for_worker)
            .with_progress(move |progress| {
                let text = match progress {
                    Progress::ToolEnd { summary, .. } => {
                        format!("Inspected: {}", glass_tui::text::clip(&summary, 100))
                    }
                    Progress::Text(_) => "Writing the proposed plan…".into(),
                    Progress::Retrying { .. } => "Retrying provider…".into(),
                    _ => "Read-only inspection · no shell or extensions".into(),
                };
                if let Ok(mut line) = status.lock() {
                    *line = text;
                }
            });
            let (out, stop) = agent
                .run_turn(|slot| {
                    if cancel.load(Ordering::Acquire) {
                        *slot = Some(String::new());
                    }
                })
                .map_err(|e| e.to_string())?;
            if !matches!(stop, Stop::Finished) {
                return Err(format!("Planning did not finish: {stop:?}"));
            }
            if out.text.trim().is_empty() {
                return Err("Provider returned an empty plan".into());
            }
            Ok(out.text)
        });
        match result {
            Some(Ok(proposal)) => {
                saved = Plan { task, proposal };
                auth::secure_write(
                    &plan_path(current),
                    &serde_json::to_vec_pretty(&saved).map_err(|e| e.to_string())?,
                )?;
            }
            Some(Err(e)) => return Err(e),
            None => return Ok(None),
        }
    }
    if saved.proposal.is_empty() {
        return Ok(None);
    }
    loop {
        let actions = [
            "Read proposed plan".into(),
            "Execute approved plan".into(),
            "Revise the task and regenerate".into(),
        ];
        match modal::choose(keys, "Plan review", &saved.task, &actions) {
            Some(0) => modal::info(keys, "Proposed plan", &saved.proposal),
            Some(1) => {
                crate::todo::apply_user(
                    state,
                    current,
                    serde_json::json!([{"op":"init","items":crate::todo::plan_items(&saved.proposal,&saved.task)}]),
                )?;
                return Ok(Some(format!(
                    "Implement this approved plan for the task: {}\n\n{}\n\nThe approved plan is already in the native checklist. Update state.todo by returned task IDs as work progresses. Verify the implementation and report the actual checks performed.",
                    saved.task, saved.proposal
                )));
            }
            Some(2) => {
                let Some(task) =
                    modal::field(keys, "Revise plan", "Task description", &saved.task, false)
                else {
                    continue;
                };
                if task.trim().is_empty() {
                    continue;
                }
                return plan(keys, cfg, current, state, factory, &task);
            }
            _ => return Ok(None),
        }
    }
}

/// The prompt that produces an AGENTS.md draft.
fn init_request(notes: Option<&str>) -> String {
    let mut p = String::from(
        "Draft an `AGENTS.md` for this repository: the instructions an AI coding agent \
         reads before working here.\n\n\
         Study the repository with the read-only inspection tools — do not guess. \
         Cover, in this order:\n\
         1. What the project is, in two or three sentences.\n\
         2. Build, test, and lint commands: the exact commands a contributor should run.\n\
         3. Layout: the directories and files that matter, and what lives where.\n\
         4. Conventions: style rules, review expectations, and anything non-obvious a \
         change must respect.\n\n\
         Keep it under about 100 lines, use Markdown headings and lists, and output only \
         the file content — no preamble, no code fence around the whole document.",
    );
    if let Some(notes) = notes.map(str::trim).filter(|n| !n.is_empty()) {
        p.push_str(&format!("\n\nThe user asked to emphasize: {notes}"));
    }
    p
}

/// Strip an outer Markdown fence if the model wrapped the whole draft in one.
fn strip_fence(text: &str) -> String {
    let t = text.trim();
    let Some(rest) = t.strip_prefix("```") else {
        return t.to_string();
    };
    let Some((_, body)) = rest.split_once('\n') else {
        return t.to_string();
    };
    let Some(end) = body.rfind("```") else {
        return t.to_string();
    };
    body[..end].trim().to_string()
}

/// Build an audited provider factory for a one-off workflow run.
fn ephemeral_factory(cfg: &config::Config) -> Result<crate::warmer::ProviderFactory, String> {
    let cfg = cfg.clone();
    Ok(Arc::new(move || {
        let inner = provider::build(
            &cfg.provider,
            &cfg.base_url,
            auth::resolve(&cfg)?,
            &cfg.model,
            cfg.adaptive_pacing,
        )
        .map_err(|e| e.to_string())?;
        let audit = Session::create(&config::data_dir().join("init")).map_err(|e| e.to_string())?;
        let path = audit.path.with_extension("cost.json");
        Ok(Box::new(crate::budget::Metered {
            inner,
            ledger: crate::budget::load(&path, cfg.cost_limit),
            card: cfg.rate_card(&cfg.model),
            path: Arc::new(std::sync::Mutex::new(path)),
        }) as Box<dyn provider::Provider>)
    }))
}

/// Generate an AGENTS.md draft in a read-only run.
pub fn init_draft(
    cfg: &config::Config,
    factory: crate::warmer::ProviderFactory,
    notes: Option<&str>,
    cancel: Arc<AtomicBool>,
    status: impl Fn(String),
) -> Result<String, String> {
    let p = factory()?;
    let workspace = cfg.workspace.clone();
    let tokens = cfg.max_tokens;
    let reasoning = cfg.reasoning_budget;
    let instructions = crate::trust::gated(cfg, &crate::agents_md::load(&cfg.workspace), false);
    let base = cfg.system_prompt.clone();
    let request = init_request(notes);
    let mut session =
        Session::create(&config::data_dir().join("init")).map_err(|e| e.to_string())?;
    session
        .append_checked(&[Record::User(request)])
        .map_err(|e| format!("cannot record setup prompt: {e}"))?;
    let mut state = tools::State::new();
    let mods = Vec::new();
    let mut extensions = ext::Registry::empty();
    let factory_for_worker = factory.clone();
    let mut agent = Agent::new(
        p.as_ref(),
        &mut session,
        &mut state,
        &mods,
        &mut extensions,
        &workspace,
    )
    .with_plan_mode(true)
    .with_prompt(base, None, instructions)
    .with_limits(tokens, reasoning, 12)
    .with_stream_factory(factory_for_worker)
    .with_progress(move |progress| {
        let text = match progress {
            Progress::ToolEnd { summary, .. } => {
                format!("Read: {}", glass_tui::text::clip(&summary, 100))
            }
            Progress::Text(_) => "Drafting AGENTS.md…".into(),
            Progress::Retrying { .. } => "Retrying provider…".into(),
            _ => "Read-only inspection · no shell or extensions".into(),
        };
        status(text);
    });
    let (out, stop) = agent
        .run_turn(|slot| {
            if cancel.load(Ordering::Acquire) {
                *slot = Some(String::new());
            }
        })
        .map_err(|e| e.to_string())?;
    if !matches!(stop, Stop::Finished) {
        return Err(format!("init did not finish: {stop:?}"));
    }
    let draft = strip_fence(&out.text);
    if draft.trim().is_empty() {
        return Err("Provider returned an empty draft".into());
    }
    Ok(draft)
}

fn init_modal(
    keys: &KeyReader,
    cfg: config::Config,
    factory: crate::warmer::ProviderFactory,
    notes: Option<String>,
) -> Option<Result<String, String>> {
    modal::task(keys, "Reading the repository", move |cancel, status| {
        init_draft(&cfg, factory, notes.as_deref(), cancel, move |text| {
            if let Ok(mut line) = status.lock() {
                *line = text;
            }
        })
    })
}

/// `/init`: draft an AGENTS.md in a read-only run, then review and save it.
pub fn init(
    keys: &KeyReader,
    cfg: &config::Config,
    factory: crate::warmer::ProviderFactory,
    notes: Option<&str>,
) -> Result<Option<String>, String> {
    let notes = notes.map(str::to_string);
    let Some(result) = init_modal(keys, cfg.clone(), factory.clone(), notes.clone()) else {
        return Ok(None);
    };
    let mut draft = result?;
    loop {
        let path = cfg.workspace.join("AGENTS.md");
        let exists = path.exists();
        let actions = [
            if exists {
                "Overwrite AGENTS.md"
            } else {
                "Save AGENTS.md"
            }
            .to_string(),
            "Read the draft".to_string(),
            "Regenerate".to_string(),
            "Cancel".to_string(),
        ];
        match modal::choose(
            keys,
            "AGENTS.md draft",
            "Save the generated instructions, review them, or regenerate.",
            &actions,
        ) {
            Some(0) => {
                if exists {
                    let confirm = ["Overwrite".to_string(), "Cancel".to_string()];
                    if modal::choose(
                        keys,
                        "Overwrite existing AGENTS.md?",
                        "The file already exists and will be replaced.",
                        &confirm,
                    ) != Some(0)
                    {
                        continue;
                    }
                }
                std::fs::write(&path, format!("{draft}\n")).map_err(|e| e.to_string())?;
                return Ok(Some(path.display().to_string()));
            }
            Some(1) => modal::info(keys, "Proposed AGENTS.md", &draft),
            Some(2) => {
                if let Some(result) = init_modal(keys, cfg.clone(), factory.clone(), notes.clone())
                {
                    draft = result?;
                }
            }
            _ => return Ok(None),
        }
    }
}

pub fn diagnostics(
    cfg: &config::Config,
    probe: bool,
    cancel: &AtomicBool,
    factory: Option<crate::warmer::ProviderFactory>,
) -> Result<String, String> {
    provider::validate_endpoint(&cfg.base_url).map_err(|e| e.to_string())?;
    let start = Instant::now();
    let key = auth::resolve(cfg)?;
    let models = onboarding::fetch_models(cfg, key.clone())?;
    let mut report = format!(
        "Connection: {}\nEndpoint: {}\nModel: {}\nCatalog latency: {} ms\nAdvertised models: {}\nSelected model: {}\n",
        auth::status(cfg),
        cfg.base_url,
        cfg.model,
        start.elapsed().as_millis(),
        models.len(),
        if models.iter().any(|m| m.id == cfg.model) {
            "advertised"
        } else {
            "custom or not listed; this may be valid"
        }
    );
    if !probe {
        report.push_str("\nStreaming and tool compatibility: not probed. Choose the quota-using probe to test a real model request.");
        return Ok(report);
    }
    if cancel.load(Ordering::Acquire) {
        return Err("Cancelled".into());
    }
    let p = if let Some(factory) = factory {
        factory()?
    } else {
        let inner = provider::build(
            &cfg.provider,
            &cfg.base_url,
            key,
            &cfg.model,
            cfg.adaptive_pacing,
        )
        .map_err(|e| e.to_string())?;
        let audit =
            Session::create(&config::data_dir().join("diagnostics")).map_err(|e| e.to_string())?;
        let path = audit.path.with_extension("cost.json");
        Box::new(crate::budget::Metered {
            inner,
            ledger: crate::budget::load(&path, cfg.cost_limit),
            card: cfg.rate_card(&cfg.model),
            path: std::sync::Arc::new(std::sync::Mutex::new(path)),
        }) as Box<dyn provider::Provider>
    };
    let messages = vec![Message::User(
        "Call gc_probe with value=ok, or reply briefly if tools are unsupported.".into(),
    )];
    let tools = vec![("gc_probe".into(), "Diagnostic only; no action is executed.".into(), serde_json::json!({"type":"object","properties":{"value":{"type":"string"}},"required":["value"]}).to_string())];
    let request = provider::Request {
        model: &cfg.model,
        system: "This is a connectivity and tool-format probe. Never request another tool.",
        messages: &messages,
        tools: &tools,
        max_tokens: 128,
        reasoning: None,
    };
    let start = Instant::now();
    let mut first = None;
    let mut demonstrated = false;
    let mut complete = false;
    for event in p.stream(&request).map_err(|e| e.to_string())? {
        if cancel.load(Ordering::Acquire) {
            return Err("Cancelled".into());
        }
        if first.is_none() {
            first = Some(start.elapsed().as_millis());
        }
        match event {
            provider::Event::ToolCall {
                name, arguments, ..
            } if name == "gc_probe" => {
                demonstrated = serde_json::from_str::<serde_json::Value>(&arguments)
                    .is_ok_and(|v| v["value"] == "ok");
            }
            provider::Event::Error(error) => return Err(error),
            provider::Event::Done => complete = true,
            _ => {}
        }
    }
    if !complete {
        return Err("Probe stream ended without completion".into());
    }
    report.push_str(&format!("\nStreaming: completed\nFirst event: {} ms\nRequest duration: {} ms\nTool format: {}\nNo tool was executed.", first.unwrap_or(0), start.elapsed().as_millis(), if demonstrated { "demonstrated" } else { "not demonstrated; model returned no valid probe call" }));
    Ok(report)
}
pub fn doctor(keys: &KeyReader, cfg: &config::Config, factory: crate::warmer::ProviderFactory) {
    let actions = [
        "Local offline diagnostics".into(),
        "Check connection and model catalog · network request".into(),
        "Probe streaming and tool calls · uses model quota".into(),
    ];
    if let Some(i) = modal::choose(
        keys,
        "Diagnostics",
        "Local checks are offline. Provider checks contact the configured endpoint.",
        &actions,
    ) {
        if i == 0 {
            let report = crate::doctor::inspect(cfg, &config::default_path(), &config::data_dir());
            modal::info(keys, "Local diagnostics", &report.text());
        } else {
            let cfg = cfg.clone();
            if let Some(result) = modal::task(keys, "Checking provider", move |cancel, _| {
                diagnostics(&cfg, i == 2, &cancel, Some(factory))
            }) {
                modal::info(keys, "Diagnostic results", &result.unwrap_or_else(|e| format!("Check failed: {e}\n\nTry /model to choose a model, /login to reconnect, or check that your SSH host can reach the endpoint.")));
            }
        }
    }
}

pub(crate) fn git(root: &Path, args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>, String> {
    git_cancellable(root, args, input, &AtomicBool::new(false))
}
fn git_cancellable(
    root: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    fn capture(mut stream: impl Read, limit: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        let _ = stream.by_ref().take(limit + 1).read_to_end(&mut bytes);
        let _ = std::io::copy(&mut stream, &mut std::io::sink());
        bytes
    }
    let mut command = Command::new("git");
    command
        .args([
            "--literal-pathspecs",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.pager=cat",
        ])
        .args(args)
        .current_dir(root)
        .process_group(0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("Missing Git stdout")?;
    let stderr = child.stderr.take().ok_or("Missing Git stderr")?;
    let out = std::thread::spawn(move || capture(stdout, 2_000_000));
    let err = std::thread::spawn(move || capture(stderr, 65_536));
    let writer = input.map(|bytes| {
        let bytes = bytes.to_vec();
        let pipe = child.stdin.take();
        std::thread::spawn(move || {
            if let Some(mut pipe) = pipe {
                let _ = pipe.write_all(&bytes);
            }
        })
    });
    let deadline = Instant::now() + std::time::Duration::from_secs(120);
    let mut cancelled = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
        if cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
            cancelled = true;
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            break child.wait().map_err(|e| e.to_string())?;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    // Git hooks must not leave descendants holding capture pipes open.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    let stdout = out.join().map_err(|_| "Git output reader stopped")?;
    let stderr = err.join().map_err(|_| "Git diagnostic reader stopped")?;
    if cancelled {
        return Err(
            "Git operation cancelled or timed out. Check Git status before retrying a commit."
                .into(),
        );
    }
    if !status.success() {
        return Err(String::from_utf8_lossy(&stderr).trim().to_string());
    }
    if stdout.len() > 2_000_000 {
        return Err("Git output exceeds 2 MB; narrow the review".into());
    }
    Ok(stdout)
}
#[derive(Debug)]
struct Change {
    status: String,
    path: String,
    original: Option<String>,
}
fn changes(root: &Path) -> Result<Vec<Change>, String> {
    let bytes = git(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        None,
    )?;
    let mut records = bytes.split(|b| *b == 0).filter(|b| !b.is_empty());
    let mut changes = Vec::new();
    while let Some(record) = records.next() {
        let record =
            std::str::from_utf8(record).map_err(|_| "Git review requires UTF-8 file paths")?;
        if record.len() < 4 {
            return Err("Invalid Git status record".into());
        }
        let status = record[..2].to_string();
        let path = record[3..].to_string();
        let original = if status.contains('R') || status.contains('C') {
            records
                .next()
                .map(|bytes| std::str::from_utf8(bytes).map(str::to_string))
                .transpose()
                .map_err(|_| "Original Git path is not UTF-8")?
        } else {
            None
        };
        changes.push(Change {
            status,
            path,
            original,
        });
    }
    Ok(changes)
}
pub(crate) fn diff(root: &Path, path: Option<&str>, staged: bool) -> Result<String, String> {
    let mut args = vec!["diff", "--no-ext-diff", "--no-textconv", "--unified=3"];
    if staged {
        args.push("--cached");
    }
    args.push("--");
    if let Some(path) = path {
        args.push(path);
    }
    String::from_utf8(git(root, &args, None)?).map_err(|_| "Diff is not UTF-8 text".into())
}
fn hunks(patch: &str) -> (String, Vec<String>) {
    let mut header = String::new();
    let mut hunks: Vec<String> = Vec::new();
    for line in patch.split_inclusive('\n') {
        if line.starts_with("@@ ") {
            hunks.push(line.to_string());
        } else if let Some(hunk) = hunks.last_mut() {
            hunk.push_str(line);
        } else {
            header.push_str(line);
        }
    }
    (header, hunks)
}
fn stage_hunks(keys: &KeyReader, root: &Path, path: &str) -> Result<Option<String>, String> {
    loop {
        let patch = diff(root, Some(path), false)?;
        let (header, parts) = hunks(&patch);
        if header.contains("old mode ")
            || header.contains("rename from ")
            || header.contains("copy from ")
        {
            modal::info(
                keys,
                "Hunk staging",
                "This change includes file metadata or a rename. Stage it as a whole file.",
            );
            return Ok(None);
        }
        if parts.is_empty() {
            modal::info(
                keys,
                "Hunk staging",
                "No textual unstaged hunks. Binary files, untracked files, renames, and mode changes can be staged as whole files.",
            );
            return Ok(None);
        }
        let board = crate::workspace::load(root)?;
        let rows = parts
            .iter()
            .map(|h| {
                let fingerprint = crate::workspace::hash(format!("{header}{h}").as_bytes());
                let review = board.reviews.get(&fingerprint);
                format!(
                    "{} {}{}",
                    if review.is_some_and(|r| r.reviewed) {
                        "✓"
                    } else {
                        "·"
                    },
                    h.lines().next().unwrap_or("Hunk"),
                    review
                        .filter(|r| !r.comments.is_empty())
                        .map(|r| format!(" · {} comments", r.comments.len()))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>();
        let Some(i) = modal::choose(keys, "Review a hunk", path, &rows) else {
            return Ok(None);
        };
        let actions = [
            "Read hunk".into(),
            "Stage this hunk".into(),
            "Comment and ask agent to revise…".into(),
            "Mark reviewed".into(),
            "Revert this unstaged hunk…".into(),
        ];
        match modal::choose(keys, "Selected hunk", &rows[i], &actions) {
            Some(0) => modal::info(keys, "Hunk patch", &parts[i]),
            Some(1) => {
                if diff(root, Some(path), false)? != patch {
                    return Err("File changed during review. Reopen hunk selection.".into());
                }
                git(
                    root,
                    &["apply", "--cached", "--check", "-"],
                    Some(format!("{header}{}", parts[i]).as_bytes()),
                )?;
                git(
                    root,
                    &["apply", "--cached", "-"],
                    Some(format!("{header}{}", parts[i]).as_bytes()),
                )?;
            }
            Some(2) => {
                if let Some(comment) = modal::field(
                    keys,
                    "Revise selected hunk",
                    "Describe the requested change",
                    "",
                    false,
                ) {
                    if comment.trim().is_empty() {
                        continue;
                    }
                    if diff(root, Some(path), false)? != patch {
                        return Err("File changed during review; select the hunk again".into());
                    }
                    crate::workspace::annotate(
                        root,
                        &format!("{header}{}", parts[i]),
                        Some(&comment),
                        false,
                    )?;
                    let excerpt = glass_tui::text::clip(&parts[i], 8000);
                    return Ok(Some(format!(
                        "Revise this reviewed hunk in {path}:\n{comment}\n\nPatch fingerprint: {}. Read the current file and use revision-checked edits; this preview may be stale.\n```diff\n{excerpt}\n```",
                        crate::workspace::hash(patch.as_bytes())
                    )));
                }
            }
            Some(3) => {
                if diff(root, Some(path), false)? != patch {
                    return Err("File changed during review; select the hunk again".into());
                }
                crate::workspace::annotate(root, &format!("{header}{}", parts[i]), None, true)?;
                modal::info(
                    keys,
                    "Hunk reviewed",
                    "Review saved against this exact patch. Later edits require a new review.",
                );
            }
            Some(4) => {
                modal::info(keys, "Revert preview", &parts[i]);
                if modal::choose(
                    keys,
                    "Revert selected hunk?",
                    "This removes only the selected unstaged change.",
                    &["Cancel".into(), "Revert this hunk".into()],
                ) == Some(1)
                {
                    if diff(root, Some(path), false)? != patch {
                        return Err("File changed during review; nothing reverted".into());
                    }
                    let selected = format!("{header}{}", parts[i]);
                    git(
                        root,
                        &["apply", "--reverse", "--check", "-"],
                        Some(selected.as_bytes()),
                    )?;
                    git(
                        root,
                        &["apply", "--reverse", "-"],
                        Some(selected.as_bytes()),
                    )?;
                }
            }
            _ => {}
        }
    }
}
pub fn git_review(keys: &KeyReader, workspace: &Path) -> Result<Option<String>, String> {
    let root_bytes = git(workspace, &["rev-parse", "--show-toplevel"], None)?;
    let root = PathBuf::from(
        String::from_utf8(root_bytes)
            .map_err(|_| "Git root is not UTF-8")?
            .trim(),
    );
    loop {
        let changes = changes(&root)?;
        let mut rows = vec![
            "Review all staged changes".into(),
            "Draft commit message and commit staged changes…".into(),
        ];
        rows.extend(changes.iter().map(|c| format!("{} {}", c.status, c.path)));
        let Some(i) = modal::choose(
            keys,
            "Git review",
            "Choose a file or hunk. Staging is explicit; commit requires review and confirmation.",
            &rows,
        ) else {
            return Ok(None);
        };
        if i == 0 {
            modal::info(keys, "Staged changes", &diff(&root, None, true)?);
        } else if i == 1 {
            let patch = diff(&root, None, true)?;
            if patch.is_empty() {
                modal::info(keys, "Nothing staged", "Stage files or hunks first.");
                continue;
            }
            let staged = String::from_utf8(git(
                &root,
                &["diff", "--cached", "--name-only", "--"],
                None,
            )?)
            .map_err(|_| "Invalid staged filenames")?;
            let count = staged.lines().count();
            let draft = if count == 1 {
                format!("Update {}", staged.lines().next().unwrap_or("file"))
            } else {
                format!("Update {count} files")
            };
            let Some(message) = modal::validated_field(
                keys,
                "Commit message",
                "Edit the generated draft",
                &draft,
                false,
                |s| {
                    if s.trim().is_empty() {
                        Err("A commit message is required".into())
                    } else {
                        Ok(())
                    }
                },
            ) else {
                continue;
            };
            let actions = [
                "Read staged diff".into(),
                "Commit these staged changes".into(),
                "Cancel".into(),
            ];
            loop {
                match modal::choose(
                    keys,
                    "Confirm commit",
                    &format!("{}\n{}", message, staged),
                    &actions,
                ) {
                    Some(0) => modal::info(keys, "Staged changes", &patch),
                    Some(1) => {
                        if diff(&root, None, true)? != patch {
                            return Err("Staged changes changed during review; review again before committing".into());
                        }
                        let root_for_commit = root.clone();
                        let message_for_commit = message.clone();
                        match modal::task(keys, "Creating commit", move |cancel, _| {
                            git_cancellable(
                                &root_for_commit,
                                &["commit", "-F", "-"],
                                Some(message_for_commit.as_bytes()),
                                &cancel,
                            )
                        }) {
                            Some(Ok(output)) => modal::info(
                                keys,
                                "Commit created",
                                &String::from_utf8_lossy(&output),
                            ),
                            Some(Err(error)) => return Err(error),
                            None => modal::info(
                                keys,
                                "Commit interrupted",
                                "Cancellation requested. A commit may already have completed; inspect Git status before retrying.",
                            ),
                        }
                        break;
                    }
                    _ => break,
                }
            }
        } else {
            let change = &changes[i - 2];
            let actions = [
                "Read unstaged changes".into(),
                "Read staged changes".into(),
                "Stage whole file".into(),
                "Review individual hunks".into(),
                "Unstage file".into(),
            ];
            match modal::choose(keys, &change.path, &change.status, &actions) {
                Some(0) => {
                    let patch = diff(&root, Some(&change.path), false)?;
                    let content = if change.status == "??" {
                        let path = root.join(&change.path);
                        if path.is_file() {
                            use std::io::Read;
                            if std::fs::symlink_metadata(&path)
                                .is_ok_and(|m| m.file_type().is_symlink())
                            {
                                format!("Symlink: {:?}", std::fs::read_link(&path))
                            } else {
                                let mut bytes = Vec::new();
                                std::fs::File::open(path)
                                    .map_err(|e| e.to_string())?
                                    .take(32_768)
                                    .read_to_end(&mut bytes)
                                    .map_err(|e| e.to_string())?;
                                String::from_utf8_lossy(&bytes).into_owned()
                            }
                        } else {
                            "Untracked directory; stage individual files from your shell".into()
                        }
                    } else {
                        patch
                    };
                    modal::info(keys, "File changes", &content);
                }
                Some(1) => {
                    modal::info(keys, "Staged file", &diff(&root, Some(&change.path), true)?)
                }
                Some(2) => {
                    let mut args = vec!["add", "--", &change.path];
                    if let Some(original) = &change.original {
                        args.push(original);
                    }
                    git(&root, &args, None)?;
                }
                Some(3) => {
                    if let Some(prompt) = stage_hunks(keys, &root, &change.path)? {
                        return Ok(Some(prompt));
                    }
                }
                Some(4) => {
                    let mut args = if git(&root, &["rev-parse", "--verify", "HEAD"], None).is_ok() {
                        vec!["reset", "-q", "HEAD", "--", &change.path]
                    } else {
                        vec!["rm", "--cached", "--ignore-unmatch", "--", &change.path]
                    };
                    if let Some(original) = &change.original {
                        args.push(original);
                    }
                    git(&root, &args, None)?;
                }
                _ => {}
            }
        }
    }
}

pub fn responses(keys: &KeyReader, session: &Session) -> Result<(), String> {
    crate::output_view::view(keys, session, crate::output_view::Filter::Replies);
    Ok(())
}

pub(crate) fn copy(text: &str) -> Result<(), String> {
    use base64::Engine;
    if text.len() > 1_000_000 {
        return Err("Copy is limited to 1 MB".into());
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    write!(std::io::stdout(), "\x1b]52;c;{encoded}\x07").map_err(|e| e.to_string())?;
    std::io::stdout().flush().map_err(|e| e.to_string())
}
pub fn headless_info(args: &[String], cfg: &config::Config) -> Result<bool, String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Ok(false);
    };
    if !matches!(command, "doctor" | "profiles" | "extensions" | "init") {
        return Ok(false);
    }
    if args.iter().skip(1).any(|a| {
        a != "--json"
            && !(command == "doctor" && a == "--probe")
            && !(command == "init" && a == "--write")
    }) {
        return Err(format!("Unknown {command} option"));
    }
    let value = match command {
        "profiles" => serde_json::to_value(profiles::load()?).map_err(|e| e.to_string())?,
        "extensions" => {
            serde_json::json!({"configured":cfg.extensions,"bundled":crate::addons::BUNDLES.iter().map(|b| serde_json::json!({"name":b.name,"description":b.description,"access":b.access})).collect::<Vec<_>>() })
        }
        "init" => {
            let write = args.iter().any(|a| a == "--write");
            let factory = ephemeral_factory(cfg)?;
            let text = init_draft(cfg, factory, None, Arc::new(AtomicBool::new(false)), |_| {})?;
            let path = cfg.workspace.join("AGENTS.md");
            if write {
                std::fs::write(&path, format!("{text}\n")).map_err(|e| e.to_string())?;
            }
            serde_json::json!({
                "status": "ok", "text": text, "written": write,
                "path": path.display().to_string()
            })
        }
        "doctor" if args.iter().any(|a| a == "--probe") => {
            let report = diagnostics(cfg, true, &AtomicBool::new(false), None);
            match report {
                Ok(text) => serde_json::json!({"status":"ok","text":text}),
                Err(error) => {
                    if args.iter().any(|a| a == "--json") {
                        crate::emit_json(serde_json::json!({"status":"error","error":error}));
                    }
                    return Err(error);
                }
            }
        }
        "doctor" => {
            let report = crate::doctor::inspect(cfg, &config::default_path(), &config::data_dir());
            serde_json::json!({"status":"ok", "text":report.text(), "report":report})
        }
        _ => unreachable!("command was validated above"),
    };
    if args.iter().any(|a| a == "--json") {
        crate::emit_json(value);
    } else {
        crate::cli::report(command, &value);
        if command == "init" {
            crate::cli::written(&value);
        }
    }
    Ok(true)
}

/// The browser's planning view uses the same read-only agent mode as the TUI.
pub fn portable_plan(
    cfg: &config::Config,
    current: &Session,
    factory: crate::warmer::ProviderFactory,
    task: &str,
    cancel: Arc<AtomicBool>,
) -> Result<serde_json::Value, String> {
    let path = plan_path(current);
    if task.trim().is_empty() {
        return match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(serde_json::json!({"task":"","proposal":""}))
            }
            Err(e) => Err(e.to_string()),
        };
    }
    let provider = factory()?;
    let mut session =
        Session::create(&config::data_dir().join("plans")).map_err(|e| e.to_string())?;
    session.append_checked(&[Record::User(format!("{task}\n\nReturn a plan grouped under Markdown phase headings, with each actionable step as - [ ] task. Do not execute the plan."))]).map_err(|e|e.to_string())?;
    let mut state = tools::State::new();
    let mods = Vec::new();
    let mut extensions = ext::Registry::empty();
    let mut agent = Agent::new(
        provider.as_ref(),
        &mut session,
        &mut state,
        &mods,
        &mut extensions,
        &cfg.workspace,
    )
    .with_plan_mode(true)
    .with_prompt(
        cfg.system_prompt.clone(),
        None,
        crate::trust::gated(cfg, &crate::agents_md::load(&cfg.workspace), false),
    )
    .with_limits(cfg.max_tokens, cfg.reasoning_budget, 12)
    .with_stream_factory(factory);
    let (out, stop) = agent
        .run_turn(|slot| {
            if cancel.load(Ordering::Acquire) {
                *slot = Some(String::new());
            }
        })
        .map_err(|e| e.to_string())?;
    if !matches!(stop, Stop::Finished) || out.text.trim().is_empty() {
        return Err("Planning did not finish; no plan saved".into());
    }
    if cancel.load(Ordering::Acquire) {
        return Err("Planning cancelled; no plan saved".into());
    }
    let saved = Plan {
        task: task.into(),
        proposal: out.text,
    };
    let value = serde_json::to_value(&saved).map_err(|e| e.to_string())?;
    auth::secure_write(
        &path,
        &serde_json::to_vec(&saved).map_err(|e| e.to_string())?,
    )?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hunk_parser_preserves_headers_and_newlines() {
        let (header, parts) = hunks(
            "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n@@ -9 +9 @@\n-c\n+d\n",
        );
        assert!(header.ends_with("+++ b/x\n"));
        assert_eq!(parts.len(), 2);
        assert!(parts[1].ends_with("+d\n"));
    }

    #[test]
    fn init_request_asks_for_file_content_only_and_carries_notes() {
        let plain = init_request(None);
        assert!(plain.contains("AGENTS.md"));
        assert!(plain.contains("no code fence"));
        assert!(!plain.contains("The user asked"));
        let noted = init_request(Some("  keep it short  "));
        assert!(noted.contains("The user asked to emphasize: keep it short"));
        assert!(!init_request(Some("   ")).contains("The user asked"));
    }

    #[test]
    fn strip_fence_unwraps_only_a_whole_document_fence() {
        let fenced = "```markdown\n# Title\n\nBody\n```";
        assert_eq!(strip_fence(fenced), "# Title\n\nBody");
        let plain = "# Title\n\n```rust\nfn main() {}\n```";
        assert_eq!(strip_fence(plain), plain, "inner fences must survive");
        assert_eq!(strip_fence("  # Title  "), "# Title");
    }
}
