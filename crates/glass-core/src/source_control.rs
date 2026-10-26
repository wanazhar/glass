//! Lazy source-control cockpit. Read requests never publish; all effects are explicit.
use serde_json::{Value, json};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const HELP: &str = "gc pr status|checks|reviews [--json]\ngc pr preview --title TEXT --body-file FILE [--base BRANCH] [--json]\ngc pr create --publish --expect-hash HASH --title TEXT --body-file FILE [--base BRANCH] [--json]\nPublication is explicit. Push the reviewed branch separately; no implicit push or merge.";
pub(crate) fn capture(
    root: &Path,
    program: &str,
    args: &[&str],
    cancel: &AtomicBool,
) -> Result<String, String> {
    use std::os::unix::process::CommandExt;
    fn drain(pipe: impl Read) -> Result<Vec<u8>, String> {
        let mut pipe = pipe;
        let mut bytes = Vec::new();
        pipe.by_ref()
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        let _ = std::io::copy(&mut pipe, &mut std::io::sink());
        Ok(bytes)
    }
    let mut child = Command::new(program)
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_PAGER", "cat")
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    let stdout = child.stdout.take().ok_or("Missing stdout")?;
    let stderr = child.stderr.take().ok_or("Missing stderr")?;
    let out = std::thread::spawn(move || drain(stdout));
    let err = std::thread::spawn(move || drain(stderr));
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Ok(s),
            Ok(None) => {}
            Err(e) => break Err(e.to_string()),
        }
        if cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
            break Err(
                "Request cancelled or timed out; inspect PR state before retrying publication"
                    .into(),
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.wait();
    let stdout = out.join().map_err(|_| "Output reader stopped")??;
    let stderr = err.join().map_err(|_| "Diagnostic reader stopped")??;
    let status = status?;
    if !status.success() {
        return Err(crate::live::sanitize_provider_detail(
            &String::from_utf8_lossy(&stderr),
        ));
    }
    if stdout.len() > 1024 * 1024 {
        return Err("PR response exceeds 1 MiB; narrow the query".into());
    }
    String::from_utf8(stdout).map_err(|_| "PR response is not UTF-8".into())
}
fn query(root: &Path, action: &str, cancel: &AtomicBool) -> Result<Value, String> {
    let fields = match action {
        "status" => {
            "number,title,url,state,isDraft,headRefName,baseRefName,reviewDecision,statusCheckRollup"
        }
        "checks" => "number,url,statusCheckRollup",
        "reviews" => "number,url,reviews,comments,reviewDecision",
        _ => return Err(HELP.into()),
    };
    let text = capture(root, "gh", &["pr", "view", "--json", fields], cancel)?;
    serde_json::from_str(&text).map_err(|e| format!("Invalid GitHub response: {e}"))
}
fn plan(root: &Path, args: &[String]) -> Result<Value, String> {
    let mut title = None;
    let mut body = None;
    let mut base = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" | "--publish" => {}
            "--expect-hash" => {
                i += 1;
                args.get(i).ok_or(HELP)?;
            }
            "--title" if title.is_none() => {
                i += 1;
                title = Some(args.get(i).ok_or(HELP)?.clone());
            }
            "--body-file" if body.is_none() => {
                i += 1;
                body = Some(args.get(i).ok_or(HELP)?.clone());
            }
            "--base" if base.is_none() => {
                i += 1;
                base = Some(args.get(i).ok_or(HELP)?.clone());
            }
            _ => return Err(HELP.into()),
        }
        i += 1;
    }
    let title = title
        .filter(|s| !s.trim().is_empty() && s.len() <= 256)
        .ok_or("Provide a title up to 256 bytes")?;
    let body_path = body.ok_or("Provide --body-file FILE")?;
    let bytes = crate::requests::bounded(&root.join(&body_path), 64 * 1024)?;
    let body = String::from_utf8(bytes).map_err(|_| "PR body must be UTF-8")?;
    let branch = capture(
        root,
        "git",
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        &AtomicBool::new(false),
    )?
    .trim()
    .to_owned();
    if base
        .as_ref()
        .is_some_and(|s| s.is_empty() || s.starts_with('-') || s.len() > 256)
    {
        return Err("Invalid base branch".into());
    }
    let revision = capture(root, "git", &["rev-parse", "HEAD"], &AtomicBool::new(false))?
        .trim()
        .to_owned();
    let mut preview = json!({"revision":revision,"title":title,"body":body,"body_source":body_path,"base":base,"head":branch,"draft":true,
        "published":false,"push":"No branch is pushed automatically.","body_fingerprint":crate::workspace::hash(body.as_bytes())});
    preview["preview_fingerprint"] = json!(crate::workspace::hash(preview.to_string().as_bytes()));
    Ok(preview)
}
fn publish(root: &Path, value: &Value, cancel: &AtomicBool) -> Result<Value, String> {
    // gh reads a private snapshot: changing the caller's source after preview cannot
    // replace the reviewed body between validation and publication.
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let revision = capture(root, "git", &["rev-parse", "HEAD"], cancel)?;
    let branch = capture(
        root,
        "git",
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        cancel,
    )?;
    if value["revision"].as_str() != Some(revision.trim())
        || value["head"].as_str() != Some(branch.trim())
    {
        return Err("Branch or HEAD changed since preview; prepare the PR again".into());
    }
    // Explicit --head prevents gh's interactive push/fork path. Require an
    // existing upstream and the exact reviewed commit there before publication.
    let upstream = capture(
        root,
        "git",
        &["rev-parse", "--verify", "@{upstream}"],
        cancel,
    )?;
    if upstream.trim() != revision.trim() {
        return Err("Push the reviewed HEAD explicitly before publishing a PR".into());
    }
    let temp = crate::config::data_dir().join(format!(
        "pr-body-{}-{}.md",
        std::process::id(),
        crate::loops::now()
    ));
    std::fs::create_dir_all(crate::config::data_dir()).map_err(|e| e.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    file.write_all(
        value["body"]
            .as_str()
            .ok_or("Missing reviewed body")?
            .as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    let mut args = vec![
        "pr",
        "create",
        "--draft",
        "--title",
        value["title"].as_str().ok_or("Missing title")?,
        "--body-file",
        temp.to_str().ok_or("Invalid temporary path")?,
        "--head",
        value["head"].as_str().ok_or("Missing branch")?,
    ];
    if let Some(base) = value["base"].as_str() {
        args.extend(["--base", base]);
    }
    let result = capture(root, "gh", &args, cancel);
    let _ = std::fs::remove_file(&temp);
    result.map(|url| json!({"published":true,"draft":true,"url":url.trim(),"head":value["head"]}))
}
pub fn request(root: &Path, args: &[String]) -> Result<Value, String> {
    if args.is_empty() {
        return query(root, "status", &AtomicBool::new(false));
    }
    let action = &args[0];
    let value = if matches!(action.as_str(), "preview" | "create") {
        let value = plan(root, &args[1..])?;
        if action == "create" {
            if !args.iter().any(|s| s == "--publish") {
                return Err(format!(
                    "Publication requires --publish; use {} pr preview first",
                    crate::cli::command_prefix()
                ));
            }
            let expected = args
                .iter()
                .position(|s| s == "--expect-hash")
                .and_then(|i| args.get(i + 1));
            if expected.map(String::as_str) != value["preview_fingerprint"].as_str() {
                return Err(format!(
                    "Run {} pr preview, review it, then pass --expect-hash PREVIEW_FINGERPRINT; nothing published",
                    crate::cli::command_prefix()
                ));
            }
            publish(root, &value, &AtomicBool::new(false))?
        } else {
            value
        }
    } else {
        if args[1..].iter().any(|s| s != "--json") {
            return Err(HELP.into());
        }
        query(root, action, &AtomicBool::new(false))?
    };
    Ok(value)
}
pub fn command(args: &[String]) -> Result<(), String> {
    if args.is_empty() || args.iter().any(|s| s == "--help") {
        crate::cli::text(
            "Pull requests",
            &crate::cli::present_command_references(HELP),
        );
        return Ok(());
    }
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let value = request(&root, args)?;
    if args.iter().any(|s| s == "--json") {
        crate::emit_json(value);
    } else {
        crate::cli::report("Pull request", &value);
    }
    Ok(())
}

pub fn view(keys: &crate::input::KeyReader, root: &Path) {
    loop {
        let actions = [
            "PR and branch status",
            "CI checks",
            "Review feedback",
            "Prepare draft PR…",
        ]
        .map(String::from);
        let Some(i) = crate::modal::choose(
            keys,
            "Pull request cockpit",
            "GitHub CLI · network only on selection · publication requires confirmation",
            &actions,
        ) else {
            return;
        };
        if i < 3 {
            let root = root.to_owned();
            let action = ["status", "checks", "reviews"][i];
            if let Some(result) = crate::modal::task(keys, "Reading GitHub", move |cancel, _| {
                query(&root, action, &cancel)
            }) {
                match result {
                    Ok(v) => crate::structured::view(keys, &actions[i], &v.to_string()),
                    Err(e) => crate::modal::info(keys, "PR unavailable", &e),
                }
            }
        } else {
            let Some(title) = crate::modal::field(keys, "Draft PR", "Title", "", false) else {
                continue;
            };
            let Some(body) = crate::modal::field(keys, "Draft PR", "Markdown body file", "", false)
            else {
                continue;
            };
            match plan(root, &["--title".into(), title, "--body-file".into(), body]) {
                Ok(value) => {
                    crate::structured::view(keys, "Publication preview", &value.to_string());
                    if crate::modal::choose(
                        keys,
                        "Publish draft PR?",
                        "This contacts GitHub and creates a draft PR for this head branch",
                        &["Cancel", "Publish reviewed draft"].map(String::from),
                    ) == Some(1)
                    {
                        let root = root.to_owned();
                        if let Some(result) =
                            crate::modal::task(keys, "Publishing draft", move |cancel, _| {
                                publish(&root, &value, &cancel)
                            })
                        {
                            match result {
                                Ok(v) => crate::structured::view(
                                    keys,
                                    "Draft PR created",
                                    &v.to_string(),
                                ),
                                Err(e) => crate::modal::info(keys, "Publication result", &e),
                            }
                        }
                    }
                }
                Err(e) => crate::modal::info(keys, "Preview unavailable", &e),
            }
        }
    }
}
