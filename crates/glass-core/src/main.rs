//! glass — a minimal, fast coding harness.
//!
//! Two tools, one loop, native terminal. Everything a mod can touch is in
//! `glass-mod-api`; everything the model can call is in `provider::builtin_tools`.

mod acp;
mod actions;
mod addons;
mod advisor;
mod agent;
mod agents_md;
mod attempt_log;
mod attention;
mod auth;
mod blobs;
mod btw;
mod budget;
mod capture;
mod checkpoint;
mod checkpoint_tools;
mod cli;
mod compact;
mod completions;
mod concurrency;
mod config;
mod desktop;
mod connect;
mod context_audit;
mod contracts;
mod diagnostic_bundle;
mod diagnostics;
mod doctor;
mod durable;
mod egress;
mod executable;
mod export;
mod ext;
mod files;
mod fs_scan;
mod guidance;
mod hardening;
mod hashing;
mod help;
mod history;
mod history_inspector;
mod hooks_shell;
mod images;
mod inbox;
mod input;
mod inspect;
mod jobs;
mod live;
mod loop_detect;
mod loops;
mod mermaid;
mod modal;
mod mods;
mod navigation;
mod neuralnets;
mod notify;
#[allow(dead_code)]
mod onboarding;
mod osc8;
mod output_view;
mod partial;
mod permissions;
mod plugin_dev;
mod plugins;
mod policy;
mod polish;
mod pricing;
mod profiles;
mod project_memory;
mod provider;
mod questions;
mod queue;
mod reader;
mod readguard;
mod reasoning;
mod recap;
mod recovery;
mod reminders;
mod requests;
mod requirements;
mod resume_picker;
mod rewind;
mod runtime;
mod sandbox;
mod serve;
mod session;
mod session_diagnostics;
mod settings;
mod signals;
mod skills;
mod source_control;
mod sse;
mod status;
mod structured;
mod subagent;
mod surface;
mod team;
mod todo;
mod tool_repair;
mod tools;
mod transactions;
mod transport;
mod trust;
mod verify;
mod warmer;
mod web_commands;
mod web_tools;
mod workbench;
mod workflows;
mod workspace;
mod worktree;

use std::io::Write;
use std::path::Path;

use glass_tui::{Edit, Editor, term};

use crate::agent::{Agent, Progress, Stop};
use crate::session::{Record, Session};

// These macros are reserved for interactive presentation, never headless source/logs.
macro_rules! ui_println {
    () => {{ let _ = surface::emit("\n"); }};
    ($($argument:tt)*) => {{ let _ = surface::emit(&format!("{}\n", glass_tui::theme::neutralize(&format!($($argument)*)))); }};
}
macro_rules! ui_print {
    ($($argument:tt)*) => {{ let _ = surface::emit(&glass_tui::theme::neutralize(&format!($($argument)*))); }};
}

static JSON_EMITTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn emit_json(value: serde_json::Value) {
    JSON_EMITTED.store(true, std::sync::atomic::Ordering::Relaxed);
    println!("{value}");
}

fn main() {
    cli::initialize_frontend();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let _job_cleanup = jobs::Cleanup;

    // Depth before anything else runs: no core dumps, no injected libraries
    // in child environments. Best-effort; a refused limit degrades to a
    // startup line, never to a crash. Opt-in visibility via GLASS_HARDENING
    // so the banner stays byte-identical in the common case.
    let hardened = hardening::harden();
    let hardening_line = hardening::line(&hardened);
    if !hardening_line.is_empty() && std::env::var("GLASS_HARDENING").is_ok() {
        eprintln!("glass-core: {hardening_line}");
    }

    match cli::command_help(&args) {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            cli::error("gc help", &error);
            std::process::exit(2);
        }
    }

    // Config parser errors can include source excerpts. Keep doctor JSON and
    // stderr useful without echoing potentially credential-bearing TOML.
    if args.first().is_some_and(|a| a == "doctor") && config::load(&config::default_path()).is_err()
    {
        let message = "Configuration could not be read or parsed; details are omitted to avoid exposing configured values.";
        if args.iter().any(|a| a == "--json") {
            println!(
                "{}",
                serde_json::json!({"status":"error","error":message,"text":""})
            );
        } else {
            cli::error("glass-core doctor", message);
        }
        std::process::exit(2);
    }
    // Headless prewalk gate (task six-wave-c-001): `gc prewalk --check SESSION`
    // reports whether the newest (or named) session ends with a pending plan
    // and no failing tool state — the precondition `/prewalk` runs under
    // interactively. Purely a read; it never routes.
    if args.first().is_some_and(|a| a == "prewalk") {
        let rest = &args[1..];
        if !rest.iter().any(|a| a.as_str() == "--check") {
            crate::cli::error(
                "Prewalk",
                "gc prewalk --check SESSION\ngc prewalk --check        (newest session)",
            );
            std::process::exit(2);
        }
        let result = (|| -> Result<(), String> {
            let cfg = config::load(&config::default_path()).map_err(|e| e.to_string())?;
            let dir = config::data_dir();
            let id = match rest.iter().position(|a| a.as_str() == "--check") {
                Some(p) if p + 1 < rest.len() => rest[p + 1].clone(),
                _ => {
                    let mut rows: Vec<(std::time::SystemTime, String)> = std::fs::read_dir(&dir)
                        .map_err(|e| format!("{}: {e}", dir.display()))?
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
                        .filter_map(|e| {
                            let m = e.metadata().ok()?;
                            Some((
                                m.modified().ok()?,
                                e.path().file_stem()?.to_string_lossy().into_owned(),
                            ))
                        })
                        .collect();
                    rows.sort_by_key(|r| std::cmp::Reverse(r.0));
                    rows.into_iter()
                        .next()
                        .map(|(_, id)| id)
                        .ok_or_else(|| format!("no sessions in {}", dir.display()))?
                }
            };
            let session = Session::open_readonly(&dir, &id).map_err(|e| e.to_string())?;
            let gate = crate::checkpoint_tools::prewalk_gate(&session, &cfg.prewalk);
            if rest.iter().any(|a| a.as_str() == "--json") {
                emit_json(gate);
            } else {
                crate::cli::report("Prewalk", &gate);
            }
            Ok(())
        })();
        return match result {
            Ok(()) => (),
            Err(error) => {
                cli::error("Prewalk", &error);
                std::process::exit(2);
            }
        };
    }
    match args.first().map(String::as_str) {
        Some("--help") | Some("-h") | Some("help") => {
            if let Err(error) = help::show(&args[1..]) {
                cli::error("gc help", &error);
                std::process::exit(2);
            }
            return;
        }
        Some("--version") | Some("-V") => {
            println!("glass-core {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        Some("skill") => {
            if let Err(e) = capture::command(&args[1..]) {
                cli::error("Skill", &e);
                std::process::exit(2);
            }
            return;
        }
        Some("inspect") => {
            if let Err(e) = structured::command(&args[1..]) {
                cli::error("Inspect", &e);
                std::process::exit(2);
            }
            return;
        }
        Some("policy") => {
            if let Err(e) = policy::command(&args[1..]) {
                cli::error("Policy", &e);
                std::process::exit(2);
            }
            return;
        }
        Some("workspace") => {
            if let Err(e) = workspace::command(&args[1..]) {
                cli::error("Workspace", &e);
                std::process::exit(2);
            }
            return;
        }
        Some("structure") => {
            let status = std::process::Command::new("python3")
                .arg("-c")
                .arg(include_str!(
                    "../../../products/glass-core/extensions/structure/structure.py"
                ))
                .args(&args[1..])
                .env(
                    "GC_BIN",
                    crate::executable::path().unwrap_or_else(|_| "glass-core".into()),
                )
                .status();
            match status {
                Ok(status) => std::process::exit(status.code().unwrap_or(1)),
                Err(error) => {
                    cli::error("Structural edit", &error.to_string());
                    std::process::exit(2);
                }
            }
        }
        Some("companion") => {
            let status = std::process::Command::new("python3")
                .arg("-c")
                .arg(include_str!(
                    "../../../products/glass-core/extensions/companion/companion.py"
                ))
                .args(&args[1..])
                .status();
            match status {
                Ok(status) => std::process::exit(status.code().unwrap_or(1)),
                Err(error) => {
                    cli::error("Companion", &error.to_string());
                    std::process::exit(2);
                }
            }
        }
        Some("advisor") => {
            if let Err(error) = advisor::command(&args[1..]) {
                cli::error("Advisor", &error);
                std::process::exit(2);
            }
            return;
        }
        Some("pr") => {
            if let Err(error) = source_control::command(&args[1..]) {
                cli::error("Pull request", &error);
                std::process::exit(2);
            }
            return;
        }
        Some("hub") => {
            if let Err(error) = navigation::command(&args[1..]) {
                cli::error("Workspace hub", &error);
                std::process::exit(2);
            }
            return;
        }
        Some("transaction") => {
            if let Err(error) = transactions::command(&args[1..]) {
                cli::error("File transaction", &error);
                std::process::exit(2);
            }
            return;
        }
        Some("contract") => {
            if let Err(error) = contracts::command(&args[1..]) {
                cli::error("Result contract", &error);
                std::process::exit(2);
            }
            return;
        }
        Some("neuralnets") | Some("guidance") => {
            let result = if args[0] == "neuralnets" {
                neuralnets::command(&args[1..])
            } else {
                guidance::command(&args[1..])
            };
            if let Err(error) = result {
                cli::error("Knowledge", &error);
                std::process::exit(2);
            }
            return;
        }
        Some("memory") => {
            if let Err(error) = project_memory::command(&args[1..]) {
                cli::error("Project memory", &error);
                std::process::exit(2);
            }
            return;
        }
        Some("settings") => {
            if let Err(error) = settings::command(&args[1..]) {
                cli::error("Settings", &error);
                std::process::exit(2);
            }
            return;
        }
        Some("usage") => {
            let result = (|| -> Result<(), String> {
                if args.len() < 2 || args.len() > 3 || args.get(2).is_some_and(|s| s != "--json") {
                    return Err("gc usage SESSION [--json]".into());
                }
                let session = Session::open_readonly(&config::data_dir(), &args[1])
                    .map_err(|e| e.to_string())?;
                let value = requests::usage_summary(&session)?;
                if args.iter().any(|s| s == "--json") {
                    emit_json(value);
                } else {
                    cli::report("Usage and cache", &value);
                }
                Ok(())
            })();
            if let Err(error) = result {
                cli::error("Usage", &error);
                std::process::exit(2);
            }
            return;
        }
        Some("blobs") => {
            let rest: Vec<&str> = args[1..].iter().map(String::as_str).collect();
            let action = rest.first().copied().unwrap_or("");
            let json = rest.contains(&"--json");
            if action != "sweep" || rest.iter().filter(|a| !a.starts_with("--")).count() > 1 {
                crate::cli::error("Blobs", "gc blobs sweep [--json]");
                std::process::exit(2);
            }
            let result = blobs::sweep(&config::data_dir());
            match result {
                Ok(report) => {
                    if json {
                        emit_json(serde_json::json!({
                            "status": "ok",
                            "scanned": report.scanned,
                            "kept_referenced": report.kept_referenced,
                            "kept_recent": report.kept_recent,
                            "removed": report.removed,
                            "errors": report.errors,
                        }));
                    } else {
                        let value = serde_json::json!({
                            "text": format!(
                                "scanned {} · kept (referenced) {} · kept (recent) {} · removed {} · errors {}",
                                report.scanned, report.kept_referenced, report.kept_recent,
                                report.removed, report.errors
                            ),
                            "scanned": report.scanned,
                            "kept_referenced": report.kept_referenced,
                            "kept_recent": report.kept_recent,
                            "removed": report.removed,
                            "errors": report.errors,
                        });
                        crate::cli::report("Blob sweep", &value);
                    }
                }
                Err(error) => {
                    crate::cli::error("Blobs", &error);
                    std::process::exit(2);
                }
            }
            return;
        }
        Some("attention") => {
            if let Err(error) = attention::command(&args[1..]) {
                cli::error("Attention", &error);
                std::process::exit(2);
            }
            return;
        }
        Some("history" | "handoff") => {
            if let Err(error) = history_inspector::command(&args[1..], args[0] == "handoff") {
                cli::error("History", &error);
                std::process::exit(2);
            }
            return;
        }
        Some("diagnostics") => {
            if let Err(error) = diagnostic_bundle::command(&args[1..]) {
                cli::error("Diagnostics", &error);
                std::process::exit(2);
            }
            return;
        }
        Some("requests") => {
            let result = (|| -> Result<(), String> {
                let id = args
                    .get(1)
                    .ok_or("Usage: gc requests SESSION_ID [--json]")?;
                if id.is_empty()
                    || !id
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                {
                    return Err("Invalid session ID".into());
                }
                let value = serde_json::json!({"requests":requests::reports(&config::data_dir().join(format!("{id}.jsonl")))?});
                if args.iter().any(|a| a == "--json") {
                    emit_json(value);
                } else {
                    cli::report("Requests", &value);
                }
                Ok(())
            })();
            if let Err(e) = result {
                cli::error("Requests", &e);
                std::process::exit(2);
            }
            return;
        }
        Some("completions") => {
            if let Err(e) = completions::command(&args[1..]) {
                cli::error("Completions", &e);
                std::process::exit(2);
            }
            return;
        }
        Some("agent") => {
            if let Err(e) = team::command(&args[1..]) {
                cli::error("agent", &glass_tui::markdown::sanitize(&e));
                std::process::exit(1);
            }
            return;
        }
        Some("file") => {
            if let Err(e) = files::command(&args[1..]) {
                if args.iter().any(|a| a == "--json")
                    && !JSON_EMITTED.load(std::sync::atomic::Ordering::Relaxed)
                {
                    emit_json(serde_json::json!({"status":"error","error":e}));
                }
                cli::error("file", &glass_tui::markdown::sanitize(&e));
                std::process::exit(1);
            }
            return;
        }
        Some("list") => {
            list_sessions();
            return;
        }
        Some("search") => {
            match args.get(1).map(String::as_str) {
                Some(pattern) => search_sessions(pattern),
                None => cli::line(&format!(
                    "usage: {} search <pattern>",
                    cli::command_prefix()
                )),
            }
            return;
        }
        _ => {}
    }

    if let Err(e) = run(&args) {
        if args.iter().any(|a| a == "--json")
            && !JSON_EMITTED.load(std::sync::atomic::Ordering::Relaxed)
        {
            emit_json(serde_json::json!({"status":"error","error":e,"text":""}));
        }
        cli::error(cli::product_name(), &glass_tui::markdown::sanitize(&e));
        jobs::finish_all();
        std::process::exit(if let Some(code) = signals::exit_code() {
            code
        } else if e.contains("session cost limit reached") {
            3
        } else if e.starts_with("incomplete:") {
            2
        } else {
            1
        });
    }
}

fn list_sessions() {
    let dir = config::data_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        crate::cli_println!("no sessions in {}", dir.display());
        return;
    };
    let mut rows: Vec<(String, u64, Option<std::time::SystemTime>, Option<String>)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|e| {
            let stem = e.path().file_stem()?.to_string_lossy().into_owned();
            let bytes = e.metadata().ok()?.len();
            let modified = e.metadata().ok().and_then(|m| m.modified().ok());
            let title = session_title(&e.path());
            Some((stem, bytes, modified, title))
        })
        .collect();
    // Newest first: the session a user wants is usually the last one.
    rows.sort_by_key(|r| std::cmp::Reverse(r.2));

    if rows.is_empty() {
        crate::cli_println!("no sessions in {}", dir.display());
        return;
    }
    if cli::styled() {
        let items=rows.iter().map(|(id,bytes,modified,title)|serde_json::json!({"thread":id,"title":title.as_deref().unwrap_or("Untitled conversation"),"modified":modified.map(format_time),"bytes":bytes})).collect::<Vec<_>>();
        cli::report(
            "Saved conversations",
            &serde_json::json!({"conversations":items}),
        );
        return;
    }
    crate::cli_println!(
        "{:<28} {:<16} {:>10}  title / first message",
        "session",
        "modified (UTC)",
        "bytes"
    );
    for (id, bytes, modified, title) in rows {
        let when = modified.map(format_time).unwrap_or_else(|| "?".into());
        let preview = title
            .map(|f| first_line(&f, 56))
            .unwrap_or_else(|| "(empty)".into());
        crate::cli_println!("{id:<28} {when:<16} {bytes:>10}  {preview}");
    }
}

/// The display title of a session file: its saved name, else
/// the first user message — the same fallback `polish::title` uses.
fn session_title(path: &std::path::Path) -> Option<String> {
    let meta = path.with_extension("meta.json");
    if let Ok(bytes) = std::fs::read(&meta)
        && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes)
        && let Some(title) = v.get("title").and_then(|t| t.as_str())
        && !title.trim().is_empty()
    {
        return Some(title.to_string());
    }
    first_user_message(path)
}

/// Per-skill context accounting: what the catalogue always pays,
/// and what loading each body would cost. `/skill-doctor`.
///
/// Glass does not track which skills the model actually read — it
/// reads them with `bash` — so the honest report is size and age,
/// not use. A large body costs nothing until the model reads it.
fn skill_doctor(cfg: &config::Config) -> String {
    let Some(dir) = cfg.skills_dir.as_deref() else {
        return "No skills directory configured (skills_dir in config.toml).".into();
    };
    let skills = skills::load_dir(dir);
    if skills.is_empty() {
        return format!("No skills in {}.", dir.display());
    }
    let mut rows = vec![format!(
        "{:<28} {:>9} {:>9}  {}",
        "skill", "body", "desc", "modified"
    )];
    let mut bodies = 0u64;
    let mut descs = 0u64;
    for s in &skills {
        let body = std::fs::metadata(&s.path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let desc = s.name.len().saturating_add(s.description.len());
        bodies = bodies.saturating_add(body);
        descs = descs.saturating_add(u64::try_from(desc).unwrap_or(u64::MAX));
        let modified = std::fs::metadata(&s.path)
            .and_then(|m| m.modified())
            .ok()
            .map(crate::format_time)
            .unwrap_or_else(|| "?".into());
        rows.push(format!(
            "{:<28} {:>8}B {:>8}B  {}",
            s.name, body, desc, modified
        ));
    }
    rows.push(String::new());
    rows.push(format!(
        "{} skills · the catalogue always costs ~{}B (names and descriptions); \
         bodies cost {}B in total, only when the model reads them",
        skills.len(),
        descs,
        bodies
    ));
    rows.join("\n")
}

/// Review the working diff through a read-only delegated agent.
/// `/code-review [max-findings]`.
///
/// The reviewer cannot mutate the tree it is judging: the task
/// runs with the inspection tools only, the same guarantee the
/// oracle relies on. The report is returned to the caller, which
/// posts it into the conversation.
fn code_review(
    workspace: &Path,
    scratch: &Path,
    factory: &crate::warmer::ProviderFactory,
    max_findings: usize,
    parent_prompt: Option<String>,
) -> Result<String, String> {
    let diff = std::process::Command::new("git")
        .current_dir(workspace)
        .args(["diff", "HEAD"])
        .output()
        .map_err(|e| format!("git diff failed: {e}"))?;
    if !diff.status.success() {
        return Err("git diff failed — is this a Git repository?".into());
    }
    let diff = String::from_utf8_lossy(&diff.stdout);
    let diff = diff.trim();
    if diff.is_empty() {
        return Err("no uncommitted changes to review".into());
    }
    /// One review's worth of diff. Beyond this the findings
    /// degrade, and the cap keeps the delegated prompt bounded.
    const DIFF_CAP: usize = 96 * 1024;
    let diff = if diff.len() > DIFF_CAP {
        let mut end = DIFF_CAP;
        while !diff.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…\n[diff truncated at {DIFF_CAP} bytes]", &diff[..end])
    } else {
        diff.to_string()
    };
    let task = ext::protocol::SubagentTask {
        task: format!(
            "Review this diff for correctness bugs only — real defects, not style. \
             Report at most {max_findings} findings, most severe first. \
             For each: file:line, severity (critical/major/minor), the defect, \
             and a one-line fix. If the diff is clean, say so in one line.\n\n```diff\n{diff}\n```"
        ),
        read_only: true,
        ..Default::default()
    };
    let make = |_: Option<&str>| -> Result<Box<dyn provider::Provider>, String> { factory() };
    let reports = crate::subagent::run_batch(&[task], 1, |t| {
        crate::subagent::run_task(t, workspace, scratch, &make, parent_prompt.as_deref())
    });
    let report = reports
        .first()
        .map(|report| report.trim())
        .filter(|report| !report.is_empty())
        .ok_or("the reviewer produced no report")?;
    if let Some(error) = report.strip_prefix("subagent failed: ") {
        return Err(error.into());
    }
    Ok(report.to_owned())
}

/// The first user message of a session, for `glass-core list`.
fn first_user_message(path: &std::path::Path) -> Option<String> {
    use std::io::{BufRead, BufReader};
    let file = std::fs::File::open(path).ok()?;
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if v.get("k").and_then(|k| k.as_str()) == Some("user") {
            return v.get("t").and_then(|t| t.as_str()).map(str::to_string);
        }
    }
    None
}

/// Format a timestamp as `YYYY-MM-DD HH:MM` in UTC, without
/// pulling in a date crate for one column of one command.
pub(crate) fn format_time(t: std::time::SystemTime) -> String {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = i64::try_from(secs / 86_400).unwrap_or(i64::MAX);
    let sod = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        sod / 3_600,
        (sod % 3_600) / 60
    )
}

/// Howard Hinnant's days-to-civil algorithm: the date for a
/// count of days since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097).cast_unsigned(); // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe.cast_signed() + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or_default(); // [1, 31]
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or_default(); // [1, 12]
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

/// Search every session's conversation for a pattern.
///
/// Matches user and assistant messages, so a search finds both
/// what was asked and what was answered. Case-insensitive: a
/// search pattern is a memory aid, not a query language.
fn search_sessions(pattern: &str) {
    let dir = config::data_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        crate::cli_println!("no sessions in {}", dir.display());
        return;
    };
    let needle = pattern.to_ascii_lowercase();
    const MAX_HITS: usize = 100;
    let mut hits = 0;
    let mut rows: Vec<std::path::PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .map(|e| e.path())
        .collect();
    // Newest first, so the most recent match is the first one read.
    rows.sort_by(|a, b| {
        let m = |p: &std::path::Path| {
            p.metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::UNIX_EPOCH)
        };
        m(b).cmp(&m(a))
    });
    for path in rows {
        let id = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(file) = std::fs::File::open(&path).ok() else {
            continue;
        };
        use std::io::{BufRead, BufReader};
        let mut turn = 0usize;
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let kind = v.get("k").and_then(|k| k.as_str()).unwrap_or_default();
            if kind == "user" {
                turn += 1;
            }
            if kind != "user" && kind != "assistant" {
                continue;
            }
            let Some(text) = v.get("t").and_then(|t| t.as_str()) else {
                continue;
            };
            if !text.to_ascii_lowercase().contains(&needle) {
                continue;
            }
            crate::cli_println!("{id} t{turn:<4} {}", first_line(text, 100));
            hits += 1;
            if hits >= MAX_HITS {
                crate::cli_println!("… stopping at {MAX_HITS} matches; narrow the pattern");
                return;
            }
        }
    }
    if hits == 0 {
        crate::cli_println!("no sessions match {pattern:?}");
    }
}

fn run_resume(args: &[String]) -> Result<Option<&str>, String> {
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--resume" => {
                return args
                    .get(i + 1)
                    .map(|s| Some(s.as_str()))
                    .ok_or("--resume requires a session ID".into());
            }
            "--attach" | "--budget" | "--model" | "--connection" | "--output-schema" => i += 2,
            "--json" | "--retry" | "--plan" | "--trust" => i += 1,
            _ => break,
        }
    }
    Ok(None)
}

fn run_plan(args: &[String]) -> bool {
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--plan" => return true,
            "--json" | "--retry" | "--trust" => i += 1,
            "--resume" | "--model" | "--connection" | "--attach" | "--budget" => i += 2,
            _ => return false,
        }
    }
    false
}

fn run(args: &[String]) -> Result<(), String> {
    let mode = args
        .first()
        .filter(|arg| matches!(arg.as_str(), "--restricted" | "--yolo" | "--unrestricted"))
        .map(String::as_str);
    let args = if mode.is_some() { &args[1..] } else { args };
    if mode.is_some()
        && args
            .first()
            .is_some_and(|arg| !matches!(arg.as_str(), "run" | "resume" | "connect"))
    {
        return Err("Mode flags apply to interactive startup, run, resume or connect; select a saved permission profile for other subcommands".into());
    }
    let cfg_path = config::default_path();
    let mut cfg = config::load(&cfg_path).map_err(|e| e.to_string())?;
    // Admin requirements sit above every config layer: whatever the
    // requirements file forces wins over user, project, and runtime choices.
    // A corrupt file fails closed (it forces nothing) and is refused loudly
    // rather than silently running without the operator's constraints.
    let requirements = crate::requirements::load(&config::config_dir())?;
    let forced = crate::requirements::apply(&requirements, &mut cfg);
    if !forced.is_empty() {
        eprintln!("glass-core: admin requirements: {}", forced.join("; "));
    }
    if args.first().is_some_and(|a| a == "resume") {
        let all = args.iter().any(|a| a == "--all");
        let browse = args.iter().any(|a| a == "--browse");
        if all || browse || args.len() == 1 {
            if args[1..]
                .iter()
                .any(|a| !matches!(a.as_str(), "--all" | "--browse"))
            {
                return Err(format!(
                    "usage: {} resume [ID | --all | --browse]",
                    cli::command_prefix()
                ));
            }
            return match resume_picker::choose(&cfg.workspace, all, browse)? {
                Some(pick) => {
                    std::env::set_current_dir(&pick.workspace).map_err(|e| e.to_string())?;
                    let mut resume_args = vec!["resume".into(), pick.id];
                    if let Some(mode) = mode {
                        resume_args.insert(0, mode.into());
                    }
                    run(&resume_args)
                }
                None => Ok(()),
            };
        }
        if args.len() != 2 || args[1].starts_with('-') {
            return Err(format!(
                "usage: {} resume [ID | --all | --browse]",
                cli::command_prefix()
            ));
        }
    }
    if args.first().is_some_and(|a| a == "doctor") && !args.iter().any(|a| a == "--probe") {
        // Local checks use best-effort saved references only. Keep this path
        // before sandbox/status initialization and never let corrupt local
        // stores turn the offline doctor into a provider/network request.
        let _ = auth::apply(&mut cfg);
        let _ = addons::apply(&mut cfg);
        let _ = profiles::apply_active(&mut cfg);
        if workflows::headless_info(args, &cfg)? {
            return Ok(());
        }
    }
    auth::apply(&mut cfg)?;
    addons::apply(&mut cfg)?;
    profiles::apply_active(&mut cfg)?;
    match mode {
        Some("--restricted") => {
            cfg.permission_profile = sandbox::PermissionProfile::WorkspaceEdit;
            cfg.sandbox = sandbox::Policy::Workspace;
            cfg.sandbox_ports.clear();
        }
        Some("--yolo" | "--unrestricted") => {
            cfg.permission_profile = sandbox::PermissionProfile::Unrestricted;
            cfg.sandbox = sandbox::Policy::Off;
            cfg.sandbox_env = "inherit".into();
        }
        _ => {}
    }
    connect::inherit_security(&mut cfg, args)?;
    crate::requirements::apply(&requirements, &mut cfg);
    for warning in sandbox::init(&cfg) {
        eprintln!("glass-core: {warning}");
    }
    session::configure_fsync(cfg.log_fsync == "always");
    // OSC 8 hyperlinks (wave D): conservative capability gate, off by
    // default — unsupported terminals keep byte-identical output.
    crate::osc8::configure(&cfg.hyperlinks);
    // Native diagrams are enabled by default; an explicit false keeps source.
    glass_tui::markdown::enable_mermaid(cfg.mermaid);
    let mermaid_renderer: glass_tui::markdown::MermaidRendererResultFn = Box::new(|body, width| {
        let head = body
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with("%%"))
            .unwrap_or("");
        if width < 8 {
            return Err("viewport too narrow (minimum 8 cells)".into());
        }
        if head.starts_with("sequenceDiagram") || head == "sequence" {
            return crate::mermaid::parse_sequence_at(body, width)
                .ok_or_else(|| "unsupported sequence syntax or diagram limit".into());
        }
        if head.starts_with("stateDiagram") {
            return crate::mermaid::parse_state(body, width)
                .ok_or_else(|| "unsupported state syntax or diagram limit".into());
        }
        if head.starts_with("classDiagram") {
            return crate::mermaid::parse_class(body, width)
                .ok_or_else(|| "unsupported class syntax or diagram limit".into());
        }
        if head.starts_with("erDiagram") {
            return crate::mermaid::parse_er(body, width)
                .ok_or_else(|| "unsupported ER syntax or diagram limit".into());
        }
        if head.starts_with("flowchart") || head.starts_with("graph") {
            return crate::mermaid::parse(body)
                .and_then(|chart| crate::mermaid::render(&chart, width))
                .ok_or_else(|| "unsupported flowchart syntax or diagram limit".into());
        }
        if let Some(chart) = crate::mermaid::render_chart(body, width) {
            return chart;
        }
        Err(format!(
            "unsupported diagram type: {}",
            glass_tui::text::clip(head, 48)
        ))
    });
    glass_tui::markdown::set_mermaid_renderer_detailed(mermaid_renderer);
    status::configure(&cfg.program_status);
    if workflows::headless_info(args, &cfg)? {
        return Ok(());
    }
    if headless_info(args, &cfg)? {
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "export") {
        return headless_export(&cfg, &args[1..]);
    }
    if args.first().is_some_and(|a| a == "plugin") {
        return plugins::command(&args[1..]);
    }
    if args.first().is_some_and(|a| a == "worktree") {
        return worktree::command(&args[1..]);
    }
    if args.first().is_some_and(|a| a == "send") {
        return inbox::command(&cfg, &args[1..]);
    }
    if args
        .first()
        .is_some_and(|a| matches!(a.as_str(), "runtime" | "durable"))
    {
        if args.len() != 1 {
            return Err(format!(
                "usage: {} runtime (configuration is supplied through initialize and session/new)",
                cli::command_prefix()
            ));
        }
        return runtime::serve(cfg);
    }
    if args.first().is_some_and(|a| a == "acp") {
        return acp::serve(cfg);
    }
    if args.first().is_some_and(|a| a == "connect") {
        return connect::command(cfg, &args[1..]);
    }
    if args.first().is_some_and(|a| a == "resume")
        && let Some(id) = args.get(1)
        && !id.contains(['/', '\\'])
    {
        let socket = config::data_dir().join(format!("serve-{id}.sock"));
        if std::os::unix::net::UnixStream::connect(&socket).is_ok() {
            return serve::attach(socket.to_str());
        }
    }
    if args.first().is_some_and(|a| a == "serve") {
        let mut session_id: Option<&str> = None;
        let mut socket: Option<&str> = None;
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--socket" => {
                    i += 1;
                    socket = Some(args.get(i).ok_or("--socket requires a path")?);
                }
                a if a.starts_with("--") => return Err(format!("unknown option {a}")),
                a => session_id = Some(a),
            }
            i += 1;
        }
        return serve::serve(cfg, session_id, socket);
    }
    if args.first().is_some_and(|a| a == "attach") {
        let mut socket: Option<&str> = None;
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--socket" => {
                    i += 1;
                    socket = Some(args.get(i).ok_or("--socket requires a path")?);
                }
                a => return Err(format!("unknown option {a}")),
            }
            i += 1;
        }
        return serve::attach(socket);
    }
    if args.first().is_some_and(|a| a == "stop") {
        return serve::stop(args.get(1).map(String::as_str));
    }
    if args.first().is_some_and(|a| a == "respawn") {
        return serve::respawn(args.get(1).map(String::as_str));
    }
    if args.first().is_some_and(|a| a == "logs") {
        return serve::logs(args.get(1).map(String::as_str), &cfg.workspace);
    }
    if !term::stdin_is_tty() && !args.first().is_some_and(|a| a == "run") {
        return Err("No interactive terminal. Use glass-core run [--json] [--resume ID] <prompt>, or pipe a prompt to glass-core run.".into());
    }

    let loaded_mods = mods::load(&cfg.mods)?;

    // Extensions are opt-in: nothing is spawned unless config names it, so a
    // default install pays nothing for the mechanism.
    let extensions = if cfg.extensions.is_empty()
        || (args.first().is_some_and(|a| a == "run") && run_plan(&args[1..]))
    {
        ext::Registry::empty()
    } else {
        ext::Registry::load(&cfg.extensions, &cfg.workspace, &cfg.model)
    };

    let mut skills = cfg
        .skills_dir
        .as_deref()
        .map(skills::load_dir)
        .unwrap_or_default();
    // Agent Plugins contribute skills (and, via mcp-bridge, MCP servers).
    let mut plugin_report = Vec::new();
    for plugin in plugins::load_all(&mut plugin_report) {
        skills.extend(plugin.skills);
    }
    for line in plugin_report {
        eprintln!("glass-core: {line}");
    }
    let catalogue = skills::catalogue(&skills);
    // Project instructions are pure prompt text and cost no code path, but they
    // are the one thing that can be large, so they are capped hard.
    let instructions = agents_md::load(&cfg.workspace);
    for i in &instructions {
        if !i.truncated && !args.first().is_some_and(|a| a == "run") {
            println!("\x1b[90m  reading {}\x1b[0m", i.path.display());
        }
    }
    let mut project_instructions = agents_md::render_all(&instructions);
    if args.first().is_some_and(|a| a == "run") {
        // Headless runs cannot ask; the store, config, and --trust decide.
        let force = args.iter().any(|a| a == "--trust");
        project_instructions = trust::gated(&cfg, &instructions, force);
        if !instructions.is_empty() && project_instructions.is_none() {
            eprintln!(
                "glass-core: project instructions not loaded — this workspace is not trusted \
                 for its AGENTS.md; run interactively to allow it, or pass --trust"
            );
        }
    }
    if instructions.is_empty() {
        project_instructions = None;
    }

    // Resume or create the session before touching the terminal so a bad path
    // produces a clean error instead of a mangled screen.
    let dir = config::data_dir();
    let resume = args.get(1).map(String::as_str);
    let session = match args.first().map(String::as_str) {
        Some("resume") => match resume {
            Some(id) => Session::open(&dir, id).map_err(|e| format!("cannot resume {id}: {e}"))?,
            None => return Err("choose a conversation with gc resume, or supply an ID".into()),
        },
        Some("run") => match run_resume(&args[1..])? {
            Some(id) => Session::open(&dir, id).map_err(|e| e.to_string())?,
            None => Session::create(&dir).map_err(|e| e.to_string())?,
        },
        _ => Session::create(&dir).map_err(|e| e.to_string())?,
    };

    polish::bind(&session, &cfg.workspace)?;
    if args.first().is_some_and(|a| a == "run") {
        let mut extensions = extensions;
        let result = headless(
            &cfg,
            &loaded_mods,
            &mut extensions,
            catalogue,
            project_instructions,
            session,
            &args[1..],
        );
        extensions.shutdown();
        return result;
    }

    if !term::stdin_is_tty() {
        return Err("glass-core needs a terminal. use glass-core run for piped input.".into());
    }

    term::enter().map_err(|e| format!("cannot set up terminal: {e}"))?;
    let mut extensions = extensions;
    let result = repl(
        cfg,
        loaded_mods,
        &mut extensions,
        catalogue,
        &skills,
        instructions,
        session,
    );
    // Extensions are child processes: they must be reaped on every exit path,
    // or they outlive the session holding the workspace open.
    extensions.shutdown();
    term::leave().ok();
    result
}

fn latest_task_checkpoint(dir: &std::path::Path) -> Option<checkpoint::Checkpoint> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| {
            e.path().extension().is_some_and(|x| x == "json")
                && e.file_name().to_string_lossy().starts_with("task-")
        })
        .max_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()))
        .and_then(|e| std::fs::read(e.path()).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
}
fn headless_info(args: &[String], cfg: &config::Config) -> Result<bool, String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Ok(false);
    };
    if !matches!(
        command,
        "sessions" | "status" | "context" | "review" | "details"
    ) {
        return Ok(false);
    }
    let json = args.iter().any(|s| s == "--json");
    let value = if command == "status" {
        let store = auth::load()?;
        serde_json::json!({"status":auth::status(cfg),"provider":cfg.provider,"model":cfg.model,"endpoint":cfg.base_url,"connections":store.connections.iter().map(|c|serde_json::json!({"id":c.id,"label":c.label,"model":c.model})).collect::<Vec<_>>()})
    } else if command == "sessions" {
        serde_json::json!({"sessions":polish::sessions(&cfg.workspace).iter().map(|s|serde_json::json!({"id":s.id,"title":polish::title(s),"turns":s.turn,"workspace":cfg.workspace})).collect::<Vec<_>>()})
    } else {
        let id = args
            .iter()
            .skip(1)
            .find(|a| !a.starts_with('-'))
            .ok_or("requires a session ID; use glass-core sessions --json")?;
        let session = Session::open_readonly(&config::data_dir(), id).map_err(|e| e.to_string())?;
        polish::bind(&session, &cfg.workspace)?;
        let text = match command {
            "context" => polish::context_report(
                &session,
                cfg,
                &agents_md::render_all(&agents_md::load(&cfg.workspace)),
                &skills::catalogue(
                    &cfg.skills_dir
                        .as_deref()
                        .map(skills::load_dir)
                        .unwrap_or_default(),
                ),
            ),
            "details" => session
                .records
                .iter()
                .filter_map(|r| {
                    if let Record::Bash {
                        command,
                        output,
                        exit_code,
                        ..
                    } = r
                    {
                        Some(format!("$ {command} (exit {exit_code:?})\n{output}"))
                    } else if let Record::ToolResult { name, body, .. } = r {
                        if name != "bash" {
                            Some(format!("{name}\n{body}"))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
            _ => polish::review_report(
                &session,
                latest_task_checkpoint(&config::data_dir().join("checkpoints").join(&session.id))
                    .as_ref(),
            ),
        };
        serde_json::json!({"session":session.id,"text":text})
    };
    if json {
        emit_json(value)
    } else {
        cli::report(command, &value);
    }
    Ok(true)
}

fn headless_export(cfg: &config::Config, args: &[String]) -> Result<(), String> {
    let mut json = false;
    let mut thinking = false;
    let mut latest = false;
    let mut out: Option<std::path::PathBuf> = None;
    let mut id: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => json = true,
            "--thinking" => thinking = true,
            "--latest" => latest = true,
            "--out" => {
                i += 1;
                out = Some(std::path::PathBuf::from(
                    args.get(i).ok_or("--out requires a path")?,
                ));
            }
            a if a.starts_with("--") => return Err(format!("unknown option {a}")),
            a => {
                if id.is_some() {
                    return Err("export takes one session ID".into());
                }
                id = Some(a.to_string());
            }
        }
        i += 1;
    }
    let session = match id {
        Some(id) => Session::open_readonly(&config::data_dir(), &id).map_err(|e| e.to_string())?,
        None if latest => polish::sessions(&cfg.workspace)
            .into_iter()
            .next()
            .ok_or("no sessions in this workspace")?,
        None => return Err("give a session ID or --latest; see glass-core sessions --json".into()),
    };
    let md = export::markdown(&session, thinking, &export::raw_records(&session));
    let written = match &out {
        Some(path) => {
            std::fs::write(path, &md).map_err(|e| e.to_string())?;
            true
        }
        None => false,
    };
    if json {
        emit_json(serde_json::json!({
            "session": session.id,
            "path": out.as_ref().map(|p| p.display().to_string()),
            "bytes": md.len(),
            "written": written
        }));
    } else if let Some(path) = &out {
        cli::line(&format!("wrote {}", path.display()));
    } else {
        print!("{md}");
    }
    Ok(())
}

fn headless(
    cfg: &config::Config,
    mods: &[Box<dyn glass_mod_api::Mod_>],
    extensions: &mut ext::Registry,
    catalogue: Option<String>,
    instructions: Option<String>,
    mut session: Session,
    args: &[String],
) -> Result<(), String> {
    use std::sync::{Arc, Mutex};
    let _signals = signals::Guard::install();
    let mut cfg = cfg.clone();
    let mut json = false;
    let mut recovering = false;
    let mut planning = false;
    let mut output_schema = None;
    let mut limit = cfg.cost_limit;
    let mut prompt = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" if prompt.is_empty() => json = true,
            "--retry" if prompt.is_empty() => recovering = true,
            "--plan" if prompt.is_empty() => planning = true,
            "--output-schema" if prompt.is_empty() => {
                i += 1;
                output_schema = Some(contracts::load(std::path::Path::new(
                    args.get(i)
                        .ok_or("--output-schema requires a schema file")?,
                ))?);
            }
            "--trust" if prompt.is_empty() => {} // consumed by run(); accepted here
            "--budget" if prompt.is_empty() => {
                i += 1;
                let n = args
                    .get(i)
                    .ok_or("--budget requires dollars")?
                    .parse::<f64>()
                    .map_err(|_| "invalid budget")?;
                if !n.is_finite() || n <= 0.0 {
                    return Err("budget must be positive and finite".into());
                }
                limit = Some(n);
            }
            "--resume" if prompt.is_empty() => {
                i += 1;
                let id = args.get(i).ok_or("--resume requires a session ID")?;
                if session.id != *id {
                    drop(session);
                    session = Session::open(&config::data_dir(), id).map_err(|e| e.to_string())?;
                }
                polish::bind(&session, &cfg.workspace)?;
            }
            "--attach" if prompt.is_empty() => {
                i += 1;
                polish::attach(
                    &mut session,
                    &cfg.workspace,
                    args.get(i).ok_or("--attach requires a file path")?,
                )?;
            }
            "--model" if prompt.is_empty() => {
                i += 1;
                cfg.model = args.get(i).ok_or("--model requires a model ID")?.clone();
            }
            "--connection" if prompt.is_empty() => {
                i += 1;
                let store = auth::load()?;
                let c = store
                    .connections
                    .iter()
                    .find(|c| &c.id == args.get(i).unwrap_or(&String::new()))
                    .ok_or("unknown connection ID; use glass-core status --json")?;
                auth::configure(&mut cfg, c);
            }
            "--" if prompt.is_empty() => {
                prompt.extend_from_slice(&args[i + 1..]);
                break;
            }
            a if a.starts_with("--") && prompt.is_empty() => {
                return Err(format!("unknown option {a}"));
            }
            _ => prompt.push(args[i].clone()),
        }
        i += 1;
    }
    let mut prompt = prompt.join(" ");
    if recovering {
        if run_resume(args)?.is_none() {
            return Err("--retry requires --resume ID".into());
        }
        if !prompt.is_empty() {
            return Err("--retry resumes existing context; omit the prompt".into());
        }
        if !session
            .records
            .last()
            .is_some_and(|r| matches!(r,Record::Meta(t) if t.starts_with("error:")))
        {
            return Err("No failed response to retry in this session".into());
        }
    }
    if prompt.is_empty() && !recovering {
        use std::io::Read;
        if term::stdin_is_tty() {
            return Err("usage: glass-core run [--json] [--budget USD] <prompt>".into());
        }
        std::io::stdin()
            .read_to_string(&mut prompt)
            .map_err(|e| e.to_string())?;
    }
    if prompt.trim().is_empty() && !recovering {
        return Err("prompt is empty".into());
    }
    if let Some(schema) = &output_schema {
        prompt.push_str(&format!("\n\nFinal answer contract: return only JSON satisfying this schema. Complete the task normally first. Unsupported constraints were rejected locally before this request.\n{schema}"));
    }
    let path = session.path.with_extension("cost.json");
    let ledger = budget::load(&path, limit);
    let path = Arc::new(Mutex::new(path));
    let factory: crate::warmer::ProviderFactory = {
        let cfg = cfg.clone();
        let ledger = Arc::clone(&ledger);
        let path = Arc::clone(&path);
        Arc::new(move || make_provider(&cfg, Arc::clone(&ledger), Arc::clone(&path), None))
    };
    // A delegated task may name its own model; that needs the same
    // construction with a different model id, plus its own rate card.
    let factory_for_model: crate::warmer::ProviderForModel = {
        let cfg = cfg.clone();
        let ledger = Arc::clone(&ledger);
        let path = Arc::clone(&path);
        Arc::new(move |model: &str| {
            let cfg = cfg.clone();
            let ledger = Arc::clone(&ledger);
            let path = Arc::clone(&path);
            let model = model.to_string();
            let f: crate::warmer::ProviderFactory = Arc::new(move || {
                make_provider(&cfg, Arc::clone(&ledger), Arc::clone(&path), Some(&model))
            });
            f
        })
    };
    let _team = team::start(
        &session.id,
        &cfg.workspace,
        Arc::clone(&factory),
        Arc::clone(&factory_for_model),
        &cfg.model,
    )?;
    let workspace = cfg.workspace.clone();
    // Nested sessions and worktrees live under the data dir so `gc worktree`
    // can find them after the process that created them is long gone.
    let scratch = config::data_dir().join("subagents");
    let sub_factory = Arc::clone(&factory);
    let sub_factory_for = Arc::clone(&factory_for_model);
    let headless_parent_prompt = cfg.system_prompt.clone();
    extensions.set_subagent(move || {
        let factory = Arc::clone(&sub_factory);
        let factory_for = Arc::clone(&sub_factory_for);
        let workspace = workspace.clone();
        let scratch = scratch.clone();
        let parent_prompt = headless_parent_prompt.clone();
        Box::new(
            move |tasks: &[ext::protocol::SubagentTask]| -> Vec<String> {
                let make = |model: Option<&str>| -> Result<Box<dyn provider::Provider>, String> {
                    match model {
                        Some(m) => (factory_for(m))(),
                        None => factory(),
                    }
                };
                crate::subagent::run_batch(tasks, crate::subagent::MAX_CONCURRENT, |t| {
                    crate::subagent::run_task(t, &workspace, &scratch, &make, Some(&parent_prompt))
                })
            },
        )
    });
    // Extensions rebuild durable state from the log once the
    // session is open: the log is the store.
    if let Ok(records) = Session::raw_records(&session.path) {
        extensions.seed_custom(&records);
    }
    if !recovering {
        session
            .append_checked(&[Record::User(prompt)])
            .map_err(|e| {
                format!("cannot durably record prompt; no provider request was made: {e}")
            })?;
    }
    let p = match factory() {
        Ok(p) => p,
        Err(e) => {
            session.append(Record::Meta(format!("error: {e}")));
            if json {
                emit_json(
                    serde_json::json!({"session":session.id,"status":"error","text":"","error":e,"cost_usd":null,"checkpoint_error":null}),
                );
            }
            return Err(e);
        }
    };
    let dir = config::data_dir().join("checkpoints").join(&session.id);
    let (mut checkpoint, mut checkpoint_error) = if planning {
        (None, None)
    } else {
        match checkpoint::begin_if_repository(
            &cfg.workspace,
            &dir,
            &format!("task-{}", session.log_len()),
        ) {
            Ok(c) => (c, None),
            Err(e) => (None, Some(e)),
        }
    };
    let mut state = tools::State::new();
    state.restore_todo(&session.history_records())?;
    for r in &session.records {
        if let Record::State(items) = r {
            state.apply(&glass_mod_api::ToolCall {
                id: "restore".into(),
                name: "state".into(),
                arguments: serde_json::json!({"items":items}).to_string(),
            });
        }
    }
    let prompt_instructions = polish::prompt_context(&session, &instructions);
    let result = Agent::new(
        p.as_ref(),
        &mut session,
        &mut state,
        mods,
        extensions,
        &cfg.workspace,
    )
    .with_plan_mode(planning)
    .with_image_input(cfg.image_input)
    .prime_extensions()
    .with_prompt(cfg.system_prompt.clone(), catalogue, prompt_instructions)
    .with_limits(cfg.max_tokens, cfg.reasoning_budget, usize::MAX)
    .with_recovery(recovering)
    .with_checkpoint_tools(cfg.checkpoint_tools)
    .with_pre_tool_hook(if planning {
        // Never runs in read-only mode; skip the config parse entirely.
        None
    } else {
        crate::hooks_shell::load_config()
    })
    .with_live(cfg.live_flush_ms as u64)
    .with_stream_factory(Arc::clone(&factory))
    .run_turn(|slot| {
        if signals::interrupted() {
            *slot = Some(String::new());
        }
    });
    if let Some(c) = checkpoint.as_mut() {
        checkpoint_error = c.finish(&dir).err();
        if checkpoint_error.is_none() {
            checkpoint_error = checkpoint::record_turn(&mut session, c).err();
        }
    }
    let (mut text, mut status, mut error) = match result {
        Ok((out, Stop::Finished)) => (out.text, "finished", None),
        Ok((out, Stop::Cancelled)) => (
            out.text,
            "cancelled",
            Some("cancelled: tool execution stopped; partial response saved".into()),
        ),
        Ok((out, stop)) => (
            out.text,
            "incomplete",
            Some(format!("incomplete: {stop:?}")),
        ),
        Err(e) => {
            let start = session
                .records
                .iter()
                .rposition(|r| matches!(r, Record::User(_)))
                .unwrap_or(0);
            let partial = session.records[start..]
                .iter()
                .filter_map(|r| {
                    if let Record::Assistant(t) = r {
                        Some(t.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            (partial, "error", Some(e.to_string()))
        }
    };
    if error.is_none()
        && let Some(schema) = &output_schema
        && contracts::parse(schema, &text).is_err()
    {
        match contracts::repair(p.as_ref(), &mut session, schema, &text, cfg.max_tokens) {
            Ok(corrected) => text = corrected,
            Err(reason) => {
                status = "invalid_output";
                error = Some(format!(
                    "Final answer contract failed after one correction: {reason}"
                ));
            }
        }
    }
    if let Err(error) = navigation::record(&session, &cfg.workspace, &cfg.model, status) {
        eprintln!("navigation receipt: {error}");
    }
    let structured = output_schema
        .as_ref()
        .and_then(|schema| contracts::parse(schema, &text).ok());
    let l = budget::lock(&ledger);
    if let Some(e) = &error {
        session.append(Record::Meta(format!("error: {e}")));
    }
    if json {
        emit_json(
            serde_json::json!({"session": session.id, "status": status, "text": text, "error": error, "cost_usd": if l.unknown { None } else { Some(l.spent) }, "checkpoint_error": checkpoint_error,"structured":structured}),
        );
    } else {
        cli::markdown("Task result", &format!("{text}\n"));
        if let Some(e) = &checkpoint_error {
            eprintln!("checkpoint: {e}");
        }
    }
    match error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

fn command_catalogue(extensions: &ext::Registry) -> Vec<glass_tui::palette::Command> {
    actions::commands(extensions)
}

/// Build a metered provider for `cfg`, optionally overriding the model.
///
/// Delegated tasks and fallback entries need the same construction with a
/// different model id. The ledger and the rate card follow that model, so a
/// request is accounted at the price of the model that actually served it.
///
/// When `fallback` is configured, the result is a chain: the primary first,
/// then each fallback model in order. A fallback that cannot even be built
/// (a typo'd adapter, a missing credential) is skipped with a note rather
/// than taking the whole session down.
pub(crate) fn make_provider(
    cfg: &config::Config,
    ledger: std::sync::Arc<std::sync::Mutex<budget::Ledger>>,
    path: std::sync::Arc<std::sync::Mutex<std::path::PathBuf>>,
    model: Option<&str>,
) -> Result<Box<dyn provider::Provider>, String> {
    let model = model.unwrap_or(&cfg.model);
    let primary = make_metered(
        cfg,
        std::sync::Arc::clone(&ledger),
        std::sync::Arc::clone(&path),
        model,
    )?;
    let mut levels: Vec<Box<dyn provider::Provider>> = vec![primary];
    for fallback in &cfg.fallback {
        if levels.iter().any(|p| p.id() == fallback) {
            continue;
        }
        match make_metered(
            cfg,
            std::sync::Arc::clone(&ledger),
            std::sync::Arc::clone(&path),
            fallback,
        ) {
            Ok(p) => levels.push(p),
            Err(e) => eprintln!("glass-core: fallback `{fallback}` unusable: {e}"),
        }
    }
    if levels.len() == 1 {
        return Ok(levels.pop().expect("one level"));
    }
    Ok(Box::new(provider::Chain::new(model.to_string(), levels)))
}

/// One model's metered provider: adapter plus ledger and rate card.
fn make_metered(
    cfg: &config::Config,
    ledger: std::sync::Arc<std::sync::Mutex<budget::Ledger>>,
    path: std::sync::Arc<std::sync::Mutex<std::path::PathBuf>>,
    model: &str,
) -> Result<Box<dyn provider::Provider>, String> {
    let key = auth::resolve(cfg)?;
    let mut inner = provider::build(
        &cfg.provider,
        &cfg.base_url,
        key,
        model,
        cfg.adaptive_pacing,
    )
    .map_err(|e| e.to_string())?;
    inner.set_session_identity(cfg.provider_session.as_deref());
    Ok(Box::new(budget::Metered {
        inner,
        ledger,
        card: cfg.rate_card(model),
        path,
    }) as Box<dyn provider::Provider>)
}

fn budget_context_snapshot(
    context: &std::sync::Mutex<(
        budget::Shared,
        std::sync::Arc<std::sync::Mutex<std::path::PathBuf>>,
    )>,
) -> Result<
    (
        budget::Shared,
        std::sync::Arc<std::sync::Mutex<std::path::PathBuf>>,
    ),
    String,
> {
    let context = context
        .lock()
        .map_err(|_| "budget context lock poisoned".to_string())?;
    Ok((
        std::sync::Arc::clone(&context.0),
        std::sync::Arc::clone(&context.1),
    ))
}

fn repl(
    mut cfg: config::Config,
    mut loaded_mods: Vec<Box<dyn glass_mod_api::Mod_>>,
    extensions: &mut ext::Registry,
    catalogue: Option<String>,
    skills: &[crate::skills::Skill],
    instructions: Vec<crate::agents_md::Instruction>,
    mut session: Session,
) -> Result<(), String> {
    use std::sync::{Arc, Mutex};

    permissions::clear_session_approvals()?;

    let mut project_instructions = crate::agents_md::render_all(&instructions);

    // The model lives in one shared cell so `/model` can switch it
    // and both the live provider and the cache warmer pick the new
    // one up — neither holds a stale copy. The provider itself is not
    // `Send`, but it is cheap to construct from configuration, so it
    // is rebuilt on demand rather than shared with the warmer thread.
    let mut checkpoint_dir = config::data_dir().join("checkpoints").join(&session.id);
    let ledger_path = session.path.with_extension("cost.json");
    let mut ledger = budget::load(&ledger_path, cfg.cost_limit);
    let mut ledger_path = Arc::new(Mutex::new(ledger_path));
    let budget_context = Arc::new(Mutex::new((Arc::clone(&ledger), Arc::clone(&ledger_path))));
    let mut jobs = jobs::Handle::new(&cfg.workspace, &session.id);
    let mut latest_checkpoint: Option<checkpoint::Checkpoint> = std::fs::read_dir(&checkpoint_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| {
            e.path().extension().is_some_and(|x| x == "json")
                && e.file_name().to_string_lossy().starts_with("task-")
        })
        .max_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()))
        .and_then(|e| std::fs::read(e.path()).ok())
        .and_then(|b| serde_json::from_slice(&b).ok());
    let runtime_config = Arc::new(Mutex::new(cfg.clone()));
    let factory: crate::warmer::ProviderFactory = {
        let runtime_config = Arc::clone(&runtime_config);
        let budget_context = Arc::clone(&budget_context);
        Arc::new(move || {
            let cfg = runtime_config
                .lock()
                .map_err(|_| "configuration lock poisoned")?
                .clone();
            let (ledger, path) = budget_context_snapshot(&budget_context)?;
            make_provider(&cfg, ledger, path, None)
        })
    };
    // A delegated task may name its own model; that needs the same
    // construction with a different model id, plus its own rate card.
    let factory_for_model: crate::warmer::ProviderForModel = {
        let runtime_config = Arc::clone(&runtime_config);
        let budget_context = Arc::clone(&budget_context);
        Arc::new(move |model: &str| {
            let runtime_config = Arc::clone(&runtime_config);
            let budget_context = Arc::clone(&budget_context);
            let model = model.to_string();
            let f: crate::warmer::ProviderFactory = Arc::new(move || {
                let cfg = runtime_config
                    .lock()
                    .map_err(|_| "configuration lock poisoned")?
                    .clone();
                let (ledger, path) = budget_context_snapshot(&budget_context)?;
                make_provider(&cfg, ledger, path, Some(&model))
            });
            f
        })
    };
    // Start disconnected so /login remains available. Requests fail before network access.
    let mut provider: Box<dyn provider::Provider>;

    let mut state = tools::State::new();
    state.restore_todo(&session.history_records())?;
    for r in &session.records {
        if let Record::State(items) = r {
            state.apply(&glass_mod_api::ToolCall {
                id: "restore".into(),
                name: "state".into(),
                arguments: serde_json::json!({"items":items}).to_string(),
            });
        }
    }
    let mut editor = Editor::new();
    // Recall survives the process: the prompts of every
    // session are the ones most likely to be repeated.
    editor.set_history(crate::history::load(&config::data_dir()));
    let mut back = glass_tui::term::BackBuffer::new();

    let _surface = surface::enter().map_err(|e| e.to_string())?;
    todo::publish(&state.todo);
    let narrow = term_size().0 < 60;
    print_banner(&cfg, &loaded_mods, extensions, &session, narrow);
    ui_println!(
        "\x1b[90m{}\x1b[0m",
        glass_tui::text::clip(&auth::status(&cfg), usize::from(term_size().0))
    );

    let mut keys = input::KeyReader::spawn();
    let mut palette = glass_tui::palette::Palette::new(command_catalogue(extensions));

    // Workspace trust: repository-controlled instruction text is a decision,
    // not a surprise. Asked once per workspace and content hash; changed
    // instructions ask again.
    if project_instructions.is_some() {
        let digest = trust::digest(&instructions);
        match trust::remembered(&cfg.workspace, &digest) {
            Some(trust::Decision::Allow) => {}
            Some(trust::Decision::Deny) => {
                project_instructions = None;
            }
            None if cfg.trust == config::Trust::Always => {}
            None => {
                let files = instructions
                    .iter()
                    .map(|i| i.path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                let actions = [
                    "Allow for this project".to_string(),
                    "Deny".to_string(),
                    "Not now".to_string(),
                ];
                crate::status::blocked("permission", "Approve project instructions?");
                let decision = modal::choose(
                    &keys,
                    "Project instructions",
                    &format!(
                        "{files} will be added to the system prompt on every turn in this project."
                    ),
                    &actions,
                );
                crate::status::set("idle", "");
                match decision {
                    Some(0) => {
                        if let Err(e) =
                            trust::remember(&cfg.workspace, &digest, trust::Decision::Allow)
                        {
                            ui_println!("\x1b[33mtrust not saved: {e}\x1b[0m");
                        }
                    }
                    Some(1) => {
                        project_instructions = None;
                        let _ = trust::remember(&cfg.workspace, &digest, trust::Decision::Deny);
                    }
                    _ => project_instructions = None,
                }
            }
        }
    }
    let mut pending = queue::Queue::open(&session)?;
    if !pending.is_empty() {
        ui_println!("Saved follow-ups available · /queue to review · /queue-run to run");
    }

    let warmer = crate::warmer::Warmer::new(Arc::clone(&factory));

    // Subagents run as a batch of nested agents, each with its own
    // isolated context. The runner is a plain closure: it builds a
    // fresh provider and a throwaway session per task, so a delegated
    // task never sees the parent's conversation. Only the core tools
    // come along — the runner cannot borrow the live `mods` or the
    // live extension registry, both of which the parent turn owns.
    // `/code-review` drives the same machinery directly, so it keeps
    // its own handles to the factory and the scratch dir.
    let review_factory = Arc::clone(&factory);
    let review_workspace = cfg.workspace.clone();
    let subagent_factory = Arc::clone(&factory);
    let subagent_factory_for = Arc::clone(&factory_for_model);
    let subagent_workspace = cfg.workspace.clone();
    // Same scratch rule as the headless path: discoverable under the data dir.
    let subagent_scratch = config::data_dir().join("subagents");
    let review_scratch = subagent_scratch.clone();
    let subagent_parent_prompt = cfg.system_prompt.clone();
    let make_subagent = move || -> ext::host::SubagentFn {
        let factory = Arc::clone(&subagent_factory);
        let factory_for = Arc::clone(&subagent_factory_for);
        let workspace = subagent_workspace.clone();
        let scratch = subagent_scratch.clone();
        let parent_prompt = subagent_parent_prompt.clone();
        Box::new(
            move |tasks: &[ext::protocol::SubagentTask]| -> Vec<String> {
                let make = |model: Option<&str>| -> Result<Box<dyn provider::Provider>, String> {
                    match model {
                        Some(m) => (factory_for(m))(),
                        None => factory(),
                    }
                };
                crate::subagent::run_batch(tasks, crate::subagent::MAX_CONCURRENT, |t| {
                    crate::subagent::run_task(t, &workspace, &scratch, &make, Some(&parent_prompt))
                })
            },
        )
    };
    extensions.set_subagent(make_subagent);
    // Extensions rebuild durable state from the log once the
    // session is open: the log is the store.
    if let Ok(records) = Session::raw_records(&session.path) {
        extensions.seed_custom(&records);
    }

    let mut team_owner = session.id.clone();
    let mut team_guard = Some(team::start(
        &session.id,
        &cfg.workspace,
        Arc::clone(&factory),
        Arc::clone(&factory_for_model),
        &cfg.model,
    )?);
    let mut team_dock = String::new();
    let mut totals = Totals::new();
    let painter = Painter::new();
    painter.seed_live(session.history_entries(), session.reasoning_history());
    painter.welcome_back(&session);

    let mut retry_available = session
        .records
        .last()
        .is_some_and(|r| matches!(r,Record::Meta(t) if t.starts_with("error:")));
    // Messages from other sessions arrive through this mailbox while idle.
    let mut inbox_mailbox = crate::inbox::Mailbox::open(&session);
    let mut accepted_background: Option<(String, String)> = None;
    let mut menu_dispatched = false;
    let mut menu_parents: Vec<(Option<glass_tui::palette::Bookmark>, String)> = Vec::new();
    crate::status::set("idle", "");
    loop {
        if EXIT_REQUESTED.swap(false, std::sync::atomic::Ordering::AcqRel) {
            return Ok(());
        }
        if team_owner != session.id {
            team_guard.take();
            team_guard = Some(team::start(
                &session.id,
                &cfg.workspace,
                Arc::clone(&factory),
                Arc::clone(&factory_for_model),
                &cfg.model,
            )?);
            team_owner.clone_from(&session.id);
        }
        painter.team_owner.replace(session.id.clone());
        team::record_notices(&mut session)?;
        team::dock(&session.id, &mut team_dock);
        jobs.set_owner(&cfg.workspace, &session.id);
        let parent = if menu_dispatched {
            menu_dispatched = false;
            if modal::take_back() {
                menu_parents.pop()
            } else {
                menu_parents.clear();
                None
            }
        } else {
            None
        };
        let reopen = match parent {
            Some((Some(bookmark), _)) => {
                palette
                    .restore_managed(&mut editor, bookmark)
                    .map_err(|e| e.to_string())?;
                None
            }
            Some((None, command)) => Some(command),
            None => None,
        };
        let revisiting = reopen.is_some();
        let (text, prompt_row, prompt_rows, background_recorded) = if let Some(command) = reopen {
            (command, term_size().1.saturating_sub(2).max(1), 1, false)
        } else {
            let _ = painter.refresh_geometry(!palette.active());
            let (w, nh) = term_size();
            let prompt_row = nh.saturating_sub(2).max(1);
            painter.set_width(w);
            let restore_reader = painter.reader_needs_redraw(palette.active());
            if !palette.active()
                && painter.reader_active()
                && restore_reader
                && let Some(reader) = painter.reader.borrow_mut().as_mut()
            {
                reader.restore_after_overlay().map_err(|e| e.to_string())?;
            }
            let prompt_rows = if palette.active() {
                palette.render(editor.text(), w, term_size().1);
                1
            } else {
                surface::composer(&editor, true, false)
            };

            let mut background_recorded = accepted_background
                .as_ref()
                .is_some_and(|(id, text)| id == &session.id && text == editor.text());
            let ev = loop {
                match keys.try_next() {
                    Some(event) => break Some(event),
                    None => match keys.wait_timeout(std::time::Duration::from_millis(120)) {
                        Ok(Some(event)) => break Some(event),
                        Ok(None) => {
                            team::dock(&session.id, &mut team_dock);
                            if painter.refresh_geometry(!palette.active()) {
                                break None;
                            }
                            // A due loop only fires into an empty composer: the
                            // user mid-sentence outranks a schedule, and typed
                            // text must never be clobbered by a timer. Inbox
                            // messages take the same path, ahead of loops.
                            if editor.text().is_empty()
                                && let Some((from, message)) =
                                    inbox_mailbox.deliver(&mut session)?
                            {
                                background_recorded = true;
                                accepted_background = Some((session.id.clone(), message.clone()));
                                ui_println!("\x1b[90minbox ({from}):\x1b[0m {message}");
                                editor.insert_str(&message);
                                break Some(glass_tui::crossterm::event::Event::Key(
                                    glass_tui::crossterm::event::KeyEvent::new(
                                        glass_tui::crossterm::event::KeyCode::Enter,
                                        glass_tui::crossterm::event::KeyModifiers::NONE,
                                    ),
                                ));
                            }
                            if editor.text().is_empty()
                                && let Some(prompt) = crate::loops::due_for(&mut session, &cfg)?
                            {
                                background_recorded = true;
                                accepted_background = Some((session.id.clone(), prompt.clone()));
                                ui_println!("\x1b[90mloop:\x1b[0m {prompt}");
                                editor.insert_str(&prompt);
                                break Some(glass_tui::crossterm::event::Event::Key(
                                    glass_tui::crossterm::event::KeyEvent::new(
                                        glass_tui::crossterm::event::KeyCode::Enter,
                                        glass_tui::crossterm::event::KeyModifiers::NONE,
                                    ),
                                ));
                            }
                        }
                        Err(_) => {
                            term::leave().ok();
                            return Err("input stream closed".into());
                        }
                    },
                }
            };
            let Some(ev) = ev else { continue };
            let _ = painter.refresh_geometry(!palette.active());

            if painter.reader_active()
                && !palette.active()
                && painter.reader_event(&ev, !editor.text().is_empty())?
            {
                continue;
            }

            if should_open_reader_for_wheel(&ev, painter.reader_active(), palette.active()) {
                painter.open_reader(session.history_entries(), false)?;
                let _ = painter.reader_event(&ev, !editor.text().is_empty())?;
                continue;
            }

            // Global bindings are handled here and never reach the editor, so a
            // ctrl-key can never leak into the prompt as a literal character.
            let key = match ev {
                glass_tui::crossterm::event::Event::Key(k) => k,
                glass_tui::crossterm::event::Event::Resize(nw, nh) => {
                    painter.set_width(nw);
                    let _ = nh;
                    continue;
                }
                // Bracketed paste delivers a whole run at once,
                // newlines included, as a single event. Dropping
                // it — which is what a bare `continue` did —
                // meant pasted text silently vanished, the one
                // input a coding harness uses most.
                glass_tui::crossterm::event::Event::Paste(text) => {
                    editor.insert_str(&text);
                    if palette.active() {
                        palette.changed(&editor);
                    }
                    continue;
                }
                _ => continue,
            };

            if key.code == glass_tui::crossterm::event::KeyCode::Char('a')
                && key
                    .modifiers
                    .contains(glass_tui::crossterm::event::KeyModifiers::ALT)
            {
                palette.cancel(&mut editor);
                team::view(&keys, &session.id);
                continue;
            }
            if key.code == glass_tui::crossterm::event::KeyCode::PageUp {
                palette.cancel(&mut editor);
                painter.open_reader(session.history_entries(), true)?;
                continue;
            }
            let ctrl = key
                .modifiers
                .contains(glass_tui::crossterm::event::KeyModifiers::CONTROL);
            if ctrl
                && matches!(
                    key.code,
                    glass_tui::crossterm::event::KeyCode::Char('b' | 'f')
                )
            {
                palette.cancel(&mut editor);
                if key.code == glass_tui::crossterm::event::KeyCode::Char('f') {
                    painter.open_reader_search(session.history_entries())?;
                } else {
                    painter.open_reader(session.history_entries(), true)?;
                }
                continue;
            }
            if ctrl {
                match key.code {
                    glass_tui::crossterm::event::KeyCode::Char('r') => {
                        let prompts = crate::history::load(&config::data_dir())
                            .into_iter()
                            .rev()
                            .collect::<Vec<_>>();
                        if let Some(i) = modal::choose(
                            &keys,
                            "Prompt history",
                            "Search; Enter restores a prompt for editing",
                            &prompts,
                        ) {
                            editor.set_text(&prompts[i]);
                        }
                        continue;
                    }
                    glass_tui::crossterm::event::KeyCode::Char('x') => {
                        keys.stop();
                        if painter.reader_active() {
                            painter.close_reader()?;
                        }
                        let result = polish::external_editor(editor.text());
                        keys = input::KeyReader::spawn();
                        match result {
                            Ok(t) => editor.set_text(&t),
                            Err(e) => modal::info(&keys, "Editor", &e),
                        }
                        continue;
                    }
                    glass_tui::crossterm::event::KeyCode::Char('p') => {
                        if palette.active() {
                            palette.cancel(&mut editor);
                        } else {
                            palette
                                .open_managed(&mut editor)
                                .map_err(|e| e.to_string())?;
                        }
                        continue;
                    }
                    glass_tui::crossterm::event::KeyCode::Char('d') => {
                        if modal::confirm_exit(&keys, &editor, false) {
                            palette.close();
                            ui_println!("\nbye");
                            return Ok(());
                        }
                        surface::overlay(palette.active()).ok();
                        continue;
                    }
                    glass_tui::crossterm::event::KeyCode::Char('t') => {
                        painter
                            .live
                            .borrow_mut()
                            .toggle(true)
                            .map_err(|e| e.to_string())?;
                        while painter.live.borrow().active() {
                            painter.live.borrow_mut().draw().ok();
                            if let Some(ev) = keys.next_blocking() {
                                painter.live.borrow_mut().event(&ev);
                            } else {
                                painter.live.borrow_mut().close().ok();
                            }
                        }
                        continue;
                    }
                    glass_tui::crossterm::event::KeyCode::Char('g') => {
                        palette.cancel(&mut editor);
                        view_transcript(&session, &loaded_mods, &keys, false)?;
                        continue;
                    }
                    glass_tui::crossterm::event::KeyCode::Char('c') => {
                        palette.close();
                        editor.clear();
                        ui_println!("^C");
                        continue;
                    }
                    _ => {}
                }
            }

            if !palette.active() && key.code == glass_tui::crossterm::event::KeyCode::Tab {
                if let Some(path) = polish::path_picker(&keys, &cfg.workspace) {
                    editor.insert_str(&path);
                }
                continue;
            }
            if palette.active() && key.code == glass_tui::crossterm::event::KeyCode::F(1) {
                if let Some(details) = palette.selected_details(editor.text()) {
                    modal::info(&keys, "Command details", &details);
                    surface::overlay(true).ok();
                }
                continue;
            }
            let bookmark = palette.active().then(|| palette.bookmark(&editor));
            let text = if palette.active() {
                match palette.key(&mut editor, key) {
                    Some(t) => t,
                    None => continue,
                }
            } else {
                let open_palette = key.code == glass_tui::crossterm::event::KeyCode::Char('/')
                    && editor.text().is_empty();
                match editor.key(key) {
                    Edit::Continue => {
                        if open_palette {
                            palette
                                .open_managed(&mut editor)
                                .map_err(|e| e.to_string())?;
                        }
                        continue;
                    }
                    Edit::Submit(t) => t,
                }
            };

            if let Some(draft) = palette.take_draft() {
                editor.set_text(&draft);
            }
            if let Some(bookmark) = bookmark {
                menu_parents.push((Some(bookmark), String::new()));
            }
            (text, prompt_row, prompt_rows, background_recorded)
        };
        let mut text = text.trim().to_string();
        menu_dispatched = true;
        let run_saved_queue = text == "/queue-run";
        if text.is_empty() {
            continue;
        }

        // Every submitted prompt enters the history, commands
        // included: a shell records what was typed, and so does
        // the prompt.
        if !revisiting {
            crate::history::record(&config::data_dir(), &text);
        }

        if matches!(
            text.as_str(),
            "/work" | "/dashboard" | "/hub" | "/workspace"
        ) {
            match workbench::view(&keys, &session, &mut jobs, workbench::Scope::All) {
                Some(action) => {
                    menu_parents.push((None, text.clone()));
                    text = action;
                }
                None => continue,
            }
        }
        if text == "/settings" {
            match workbench::choose(&keys, actions::Group::Settings) {
                Some(action) => {
                    menu_parents.push((None, text.clone()));
                    text = action;
                }
                None => continue,
            }
        }
        if text == "/conversations" {
            text = "/sessions".into();
        }
        if text == "/profiles" {
            match profiles::manage(&keys, &cfg) {
                Ok(Some(candidate)) => {
                    warmer.begin_turn();
                    cfg = candidate;
                    loaded_mods = mods::load(&cfg.mods)?;
                    extensions.reload(&cfg.extensions, &cfg.workspace, &cfg.model);
                    *runtime_config.lock().unwrap() = cfg.clone();
                    palette = glass_tui::palette::Palette::new(command_catalogue(extensions));
                    modal::info(
                        &keys,
                        "Profile applied",
                        "Provider, model, instructions, mods, and extensions are active in this workspace.",
                    );
                }
                Ok(None) => {}
                Err(e) => modal::info(&keys, "Profile error", &e),
            }
            continue;
        }
        if text == "/doctor" {
            workflows::doctor(&keys, &cfg, Arc::clone(&factory));
            continue;
        }
        if text == "/search" {
            if let Err(error) = painter.open_reader_search(session.history_entries()) {
                modal::info(&keys, "Transcript search", &error);
            }
            continue;
        }
        if text == "/git" {
            match workflows::git_review(&keys, &cfg.workspace) {
                Ok(Some(prompt)) => text = prompt,
                Ok(None) => continue,
                Err(e) => {
                    modal::info(&keys, "Git review error", &e);
                    continue;
                }
            }
        }
        if text == "/responses" {
            if let Err(e) = workflows::responses(&keys, &session) {
                modal::info(&keys, "Response error", &e);
            }
            continue;
        }
        if text == "/plan" || text.starts_with("/plan ") {
            warmer.begin_turn();
            match workflows::plan(
                &keys,
                &cfg,
                &mut session,
                &mut state,
                Arc::clone(&factory),
                text.strip_prefix("/plan").unwrap_or("").trim(),
            ) {
                Ok(Some(task)) => text = task,
                Ok(None) => continue,
                Err(e) => {
                    modal::info(&keys, "Planning error", &e);
                    continue;
                }
            }
        }
        if text == "/init" || text.starts_with("/init ") {
            warmer.begin_turn();
            let notes = text.strip_prefix("/init").unwrap_or("").trim();
            match workflows::init(&keys, &cfg, Arc::clone(&factory), Some(notes)) {
                Ok(Some(path)) => ui_println!("\x1b[32mwrote\x1b[0m {path}"),
                Ok(None) => {}
                Err(e) => modal::info(&keys, "Init error", &e),
            }
            continue;
        }
        if text == "/constraints" || text.starts_with("/constraints ") {
            match reminders::command(&session, text.strip_prefix("/constraints").unwrap_or("")) {
                Ok(value) => structured::view(&keys, "Constraint reminders", &value),
                Err(error) => modal::info(&keys, "Constraints unavailable", &error),
            }
            continue;
        }
        if text == "/settings-info" {
            match settings::snapshot(&cfg) {
                Ok(value) => structured::view(&keys, "Effective settings", &value.to_string()),
                Err(error) => modal::info(&keys, "Settings unavailable", &error),
            }
            continue;
        }
        if text == "/usage" {
            match requests::usage_summary(&session) {
                Ok(value) => structured::view(&keys, "Usage and cache", &value.to_string()),
                Err(error) => modal::info(&keys, "Usage unavailable", &error),
            }
            continue;
        }
        if text == "/quote" {
            if let Err(e) = polish::quote_picker(&keys, &session) {
                modal::info(&keys, "Quote unavailable", &e);
            }
            continue;
        }
        if matches!(text.as_str(), "/neuralnets" | "/memory") {
            neuralnets::view(&keys, &cfg.workspace);
            continue;
        }
        if text == "/guidance" {
            match guidance::request(&serde_json::json!({"action":"catalogue"})) {
                Ok(value) => structured::view(&keys, "Model guidance", &value.to_string()),
                Err(error) => modal::info(&keys, "Knowledge", &error),
            }
            continue;
        }
        if text == "/advisor" {
            advisor::view(&keys, &session);
            continue;
        }
        if text == "/pr" {
            source_control::view(&keys, &cfg.workspace);
            continue;
        }
        if text == "/attention" {
            attention::view(&keys, &session);
            continue;
        }
        let mut hub_resume = None;
        if text == "/handoff" {
            hub_resume = history_inspector::view(&keys, &session, &cfg.workspace);
            if hub_resume.is_none() {
                continue;
            }
            text = "/sessions".into();
        }
        if text == "/branches" {
            hub_resume = navigation::view(&keys, &session, &cfg.workspace, true);
            if hub_resume.is_none() {
                continue;
            }
            text = "/sessions".into();
        }
        if text == "/history" {
            structured::view(
                &keys,
                "History boundaries",
                &history_inspector::receipts(&session).to_string(),
            );
            continue;
        }
        // `/recovery` is the navigable failure inspector; `/diagnostics` stays
        // the shareable redacted bundle, and the inspector reaches it too.
        if text == "/recovery" {
            recovery::view(&keys, &mut session, &factory);
            continue;
        }
        if text == "/diagnostics" {
            match diagnostic_bundle::snapshot(&session) {
                Ok(value) => structured::view(&keys, "Diagnostic bundle", &value.to_string()),
                Err(error) => modal::info(&keys, "Diagnostics unavailable", &error),
            }
            continue;
        }
        if text == "/queue" {
            pending.manage(&keys);
            continue;
        }
        if text == "/sessions" {
            warmer.begin_turn();
            let selected = match hub_resume.take() {
                Some(id) if id == session.id => Ok(None),
                Some(id) => Session::open(&config::data_dir(), &id)
                    .map(Some)
                    .map_err(|e| e.to_string()),
                None => polish::browser(&keys, &session, &cfg.workspace),
            };
            match selected {
                Ok(Some(s)) => {
                    permissions::clear_session_approvals()?;
                    session = s;
                    *painter.live.borrow_mut() = live::Live::new();
                    painter.seed_live(session.history_entries(), session.reasoning_history());
                    painter.welcome_back(&session);
                    navigation::viewed(&session)?;
                    inbox_mailbox = crate::inbox::Mailbox::open(&session);
                    pending = queue::Queue::open(&session)?;
                    state = tools::State::new();
                    state.restore_todo(&session.history_records())?;
                    totals = Totals::new();
                    for r in &session.records {
                        if let Record::State(items) = r {
                            state.apply(&glass_mod_api::ToolCall {
                                id: "restore".into(),
                                name: "state".into(),
                                arguments: serde_json::json!({"items":items}).to_string(),
                            });
                        }
                    }
                    checkpoint_dir = config::data_dir().join("checkpoints").join(&session.id);
                    latest_checkpoint = latest_task_checkpoint(&checkpoint_dir);
                    let limit = cfg.cost_limit;
                    let path = session.path.with_extension("cost.json");
                    ledger = budget::load(&path, limit);
                    ledger_path = Arc::new(Mutex::new(path));
                    *budget_context
                        .lock()
                        .map_err(|_| "budget context lock poisoned".to_string())? =
                        (Arc::clone(&ledger), Arc::clone(&ledger_path));
                    retry_available = false;
                    ui_println!("Resumed {} · {}", polish::title(&session), session.id);
                }
                Ok(None) => {}
                Err(e) => modal::info(&keys, "Session error", &e),
            }
            continue;
        }
        if text == "/name" || text.starts_with("/name ") {
            let args = text.strip_prefix("/name").unwrap_or("").trim();
            let title = if args.is_empty() {
                modal::field(
                    &keys,
                    "Name session",
                    "Title (empty falls back to the first message)",
                    &polish::title(&session),
                    false,
                )
            } else {
                Some(args.to_string())
            };
            if let Some(t) = title {
                let mut m = polish::metadata(&session);
                m.title = t.trim().to_string();
                match polish::save(&session, &m) {
                    Ok(()) => {
                        let shown = if m.title.is_empty() {
                            "first message".into()
                        } else {
                            m.title.clone()
                        };
                        ui_println!("\x1b[90msession title: {shown}\x1b[0m");
                    }
                    Err(e) => modal::info(&keys, "Name session", &e),
                }
            }
            continue;
        }
        if text == "/capture" {
            capture::view(&keys, &session, &cfg);
            continue;
        }
        if text == "/outputs" {
            structured::outputs(&keys, &session);
            continue;
        }
        if text == "/tasks" {
            workspace::view(&keys, &cfg.workspace);
            continue;
        }
        if text == "/requests" {
            requests::view(&keys, &session);
            continue;
        }
        if text == "/context" {
            if let Err(e) =
                polish::context(&keys, &mut session, &cfg, &project_instructions, &catalogue)
            {
                modal::info(&keys, "Context error", &e);
            }
            warmer.begin_turn();
            continue;
        }
        if text == "/attach" || text.starts_with("/attach ") {
            let path = if let Some(p) = text.strip_prefix("/attach ") {
                Some(p.trim().to_string())
            } else {
                polish::path_picker(&keys, &cfg.workspace)
            };
            if let Some(p) = path {
                match polish::attach_picker(&keys, &mut session, &cfg.workspace, &p) {
                    Ok(true) => ui_println!("Attached {p} · /context to inspect or remove"),
                    Ok(false) => {}
                    Err(e) => modal::info(&keys, "Attachment error", &e),
                }
            }
            warmer.begin_turn();
            continue;
        }
        if text == "/keys" {
            polish::keys(&keys);
            continue;
        }
        if text == "/details" {
            polish::details(&keys, &session);
            continue;
        }
        if text == "/agents" {
            team::view(&keys, &session.id);
            continue;
        }
        if text == "/activity" {
            workbench::view(&keys, &session, &mut jobs, workbench::Scope::Activity);
            continue;
        }
        if text == "/review" {
            modal::info(
                &keys,
                "Task review",
                &polish::review_report(&session, latest_checkpoint.as_ref()),
            );
            continue;
        }
        if text == "/skill-doctor" {
            modal::info(&keys, "Skill doctor", &skill_doctor(&cfg));
            continue;
        }
        if text == "/code-review" || text.starts_with("/code-review ") {
            let args = text.strip_prefix("/code-review").unwrap_or("").trim();
            let max_findings: usize = if args.is_empty() {
                10
            } else {
                args.parse().unwrap_or(10)
            }
            .clamp(1, 50);
            ui_println!("\x1b[90mcode review: reading the diff…\x1b[0m");
            match code_review(
                &review_workspace,
                &review_scratch,
                &review_factory,
                max_findings,
                Some(cfg.system_prompt.clone()),
            ) {
                Ok(report) => {
                    // Posted as a system record so the findings
                    // are in the model's context for the fix, the
                    // same way Claude Code posts review output.
                    session.append(Record::System(format!("code review findings:\n{report}")));
                    modal::info(&keys, "Code review", &report);
                }
                Err(e) => modal::info(&keys, "Code review", &e),
            }
            continue;
        }
        if text == "/prompts" {
            let prompts = crate::history::load(&config::data_dir())
                .into_iter()
                .rev()
                .filter(|p| p != "/prompts")
                .collect::<Vec<_>>();
            if let Some(i) =
                modal::choose(&keys, "Prompt history", "Choose a prompt to edit", &prompts)
            {
                editor.set_text(&prompts[i]);
            }
            continue;
        }
        if text == "/editor" {
            keys.stop();
            let result = polish::external_editor(editor.text());
            keys = input::KeyReader::spawn();
            match result {
                Ok(t) => editor.set_text(&t),
                Err(e) => modal::info(&keys, "Editor", &e),
            }
            continue;
        }
        if text == "/redraw" {
            surface::overlay(false).ok();
            continue;
        }
        let recovering = text == "/retry";
        if recovering {
            if !retry_available {
                modal::info(
                    &keys,
                    "Response recovery",
                    "No failed response to retry in this session.",
                );
                continue;
            }
            match modal::choose(
                &keys,
                "Recover response",
                "Keeps completed tool results and partial output; resumes from current context",
                &[
                    "Retry response".into(),
                    "Switch provider and model, then retry".into(),
                    "Sign in, then retry".into(),
                ],
            ) {
                Some(0) => {}
                Some(i) => {
                    let result = if i == 1 {
                        onboarding::model(&keys, &mut cfg)
                    } else {
                        onboarding::login(&keys, &mut cfg)
                    };
                    match result {
                        Ok(true) => {
                            *runtime_config.lock().unwrap() = cfg.clone();
                            if let Err(e) = profiles::update_active(&cfg) {
                                modal::info(&keys, "Profile save error", &e);
                            }
                        }
                        Ok(false) => continue,
                        Err(e) => {
                            modal::info(&keys, "Connection error", &e);
                            continue;
                        }
                    }
                }
                None => continue,
            }
        }
        if matches!(text.as_str(), "/login" | "/logout" | "/model") {
            warmer.begin_turn();
            if text == "/login" {
                crate::status::blocked("auth", "Connect a provider");
            }
            let result = match text.as_str() {
                "/login" => onboarding::login(&keys, &mut cfg),
                "/logout" => onboarding::logout(&keys, &mut cfg),
                _ => onboarding::model(&keys, &mut cfg),
            };
            crate::status::set("idle", "");
            match result {
                Ok(true) => {
                    *runtime_config.lock().unwrap() = cfg.clone();
                    if let Err(e) = profiles::update_active(&cfg) {
                        modal::info(&keys, "Profile save error", &e);
                    }
                    ui_println!("\x1b[90m{} · {}\x1b[0m", auth::status(&cfg), cfg.model);
                }
                Ok(false) => {}
                Err(e) => modal::info(&keys, "Connection error", &e),
            }
            continue;
        }
        if text == "/status" {
            let l = budget::lock(&ledger);
            let detail = format!(
                "{}\n\nProvider: {}\nEndpoint: {}\nModel: {}\nSession: {}\nSpent: {}\n\n/login connect · /model choose provider and model · /logout sign out",
                auth::status(&cfg),
                cfg.provider,
                cfg.base_url,
                cfg.model,
                session.id,
                budget::spent_label(&l)
            );
            drop(l);
            modal::info(&keys, "Connection and session", &detail);
            continue;
        }
        if text == "/tools" {
            let labels = vec![
                "Expanded · command and output preview".into(),
                "Compact · completion status".into(),
            ];
            if let Some(choice) = modal::choose(
                &keys,
                "Tool cards",
                "Full commands and recorded output stay in /details",
                &labels,
            ) {
                painter.compact_tools.set(choice == 1);
            }
            continue;
        }
        if text == "/thinking" {
            painter
                .live
                .borrow_mut()
                .toggle(true)
                .map_err(|e| e.to_string())?;
            while painter.live.borrow().active() {
                painter.live.borrow_mut().draw().ok();
                if let Some(ev) = keys.next_blocking() {
                    painter.live.borrow_mut().event(&ev);
                } else {
                    painter.live.borrow_mut().close().ok();
                }
            }
            continue;
        }
        if text == "/recap" {
            let text = recap::cached(&session).unwrap_or_else(|| {
                "Your recap will appear after your first message. No extra tokens are used.".into()
            });
            modal::info(&keys, "Welcome back", &text);
            continue;
        }
        if text == "/help" {
            let items = command_catalogue(extensions)
                .into_iter()
                .map(|c| format!("/{} {} — {}", c.name, c.hint, c.description))
                .collect::<Vec<_>>();
            if let Some(i) = modal::choose(
                &keys,
                "Help",
                "Choose a command to learn how to use it",
                &items,
            ) {
                let c = &command_catalogue(extensions)[i];
                modal::info(
                    &keys,
                    &format!("/{}", c.name),
                    &format!(
                        "{}\n\n/{} {}\n\nSelect it from / or Ctrl+P to start its guided flow.\nEscape closes a modal. Ctrl+G opens the transcript.\nCtrl+C clears input; Ctrl+D exits.",
                        c.description, c.name, c.hint
                    ),
                );
            }
            continue;
        }
        // Guided fields stay in a modal, including commands typed directly.
        if text == "/loop" || text.starts_with("/loop ") {
            let args = text.strip_prefix("/loop").unwrap_or("").trim();
            match crate::loops::command_for(&session, &cfg, args) {
                Ok(out) => ui_println!("{out}"),
                Err(e) => ui_println!("\x1b[31m{e}\x1b[0m"),
            }
            continue;
        }
        if text == "/send" || text.starts_with("/send ") {
            let args = text.strip_prefix("/send").unwrap_or("").trim();
            match crate::inbox::from_session(&session, args) {
                Ok(out) => ui_println!("{out}"),
                Err(e) => ui_println!("\x1b[31m{e}\x1b[0m"),
            }
            continue;
        }
        {
            let mut context_command: Option<(&str, String)> = None;
            for cmd in ["/forget", "/unforget", "/rewrite"] {
                if text == cmd {
                    context_command = Some((cmd, String::new()));
                    break;
                }
                if let Some(args) = text.strip_prefix(&format!("{cmd} ")) {
                    context_command = Some((cmd, args.trim().to_string()));
                    break;
                }
            }
            if let Some((cmd, args)) = context_command {
                let name = cmd.trim_start_matches('/');
                match session.context_command(name, &args) {
                    Ok(out) => ui_println!("{out}"),
                    Err(e) => ui_println!("\x1b[31m{e}\x1b[0m"),
                }
                continue;
            }
        }
        if text == "/budget" {
            let l = budget::lock(&ledger);
            let note = format!(
                "Spent {} · limit {}",
                budget::spent_label(&l),
                l.limit.map(pricing::money).unwrap_or_else(|| "off".into())
            );
            drop(l);
            match modal::choose(
                &keys,
                "Budget",
                &note,
                &["Set spending limit…".into(), "Turn limit off".into()],
            ) {
                Some(0) => {
                    match modal::validated_field(
                        &keys,
                        "Budget",
                        "Maximum session spend in USD",
                        "",
                        false,
                        |s| {
                            if s.parse::<f64>().is_ok_and(|n| n.is_finite() && n > 0.0) {
                                Ok(())
                            } else {
                                Err("Enter a positive, finite dollar amount.".into())
                            }
                        },
                    ) {
                        Some(v) => text = format!("/budget {v}"),
                        None => continue,
                    }
                }
                Some(1) => text = "/budget off".into(),
                _ => continue,
            }
        }
        if matches!(text.as_str(), "/bg" | "/btw") {
            let label = if text == "/bg" {
                "Shell command to run in background"
            } else {
                "Side question"
            };
            match modal::field(&keys, &text, label, "", false) {
                Some(v) => text = format!("{text} {v}"),
                None => continue,
            }
        }
        if text == "/branch" {
            let turns = session.raw_turns().unwrap_or(0);
            let choices = (0..=turns)
                .rev()
                .map(|t| {
                    format!(
                        "Through turn {t}{}",
                        if t == turns {
                            " (current conversation)"
                        } else {
                            ""
                        }
                    )
                })
                .collect::<Vec<_>>();
            match modal::choose(
                &keys,
                "Branch conversation",
                "Choose where to fork",
                &choices,
            ) {
                Some(i) => text = format!("/branch {}", turns - i),
                None => continue,
            }
        }
        if matches!(text.as_str(), "/restore" | "/checkpoints") {
            let mut labels = std::fs::read_dir(&checkpoint_dir)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
                .map(|e| {
                    e.path()
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect::<Vec<_>>();
            labels.sort();
            labels.reverse();
            if labels.is_empty() {
                modal::info(
                    &keys,
                    "Checkpoints",
                    "No checkpoints yet. Use /checkpoint to save a baseline.",
                );
                continue;
            }
            let Some(i) = modal::choose(
                &keys,
                "Checkpoints",
                "Choose a saved workspace checkpoint",
                &labels,
            ) else {
                continue;
            };
            let action = if text == "/restore" {
                Some(2)
            } else {
                modal::choose(
                    &keys,
                    "Checkpoint actions",
                    &labels[i],
                    &[
                        "Review changes".into(),
                        "Undo task changes".into(),
                        "Restore baseline…".into(),
                    ],
                )
            };
            text = match action {
                Some(0) => format!("/diff {}", labels[i]),
                Some(1) => format!("/undo {}", labels[i]),
                Some(2) => {
                    if modal::choose(
                        &keys,
                        "Restore baseline?",
                        "Current files will be replaced. A backup checkpoint will be saved.",
                        &["Cancel".into(), "Restore baseline".into()],
                    ) != Some(1)
                    {
                        continue;
                    }
                    format!("/restore {}", labels[i])
                }
                _ => continue,
            };
        }
        if matches!(text.as_str(), "/jobs" | "/job") {
            let list = jobs.list();
            if list.is_empty() {
                modal::info(
                    &keys,
                    "Background jobs",
                    "No jobs yet. Use /bg to start a command.",
                );
                continue;
            }
            let rows = list.lines().map(str::to_string).collect::<Vec<_>>();
            let Some(i) = modal::choose(&keys, "Background jobs", "Choose a job", &rows) else {
                continue;
            };
            let id = rows[i].split(':').next().unwrap_or("");
            match modal::choose(
                &keys,
                "Job actions",
                &rows[i],
                &["Read output".into(), "Stop job…".into()],
            ) {
                Some(0) => text = format!("/job {id}"),
                Some(1) => {
                    if modal::choose(
                        &keys,
                        "Stop background job?",
                        &rows[i],
                        &["Cancel".into(), "Stop owned job".into()],
                    ) != Some(1)
                    {
                        continue;
                    }
                    text = format!("/job {id} stop");
                }
                _ => continue,
            }
        }
        if let Some(args) = text
            .strip_prefix("/budget")
            .filter(|s| s.is_empty() || s.starts_with(' '))
        {
            let mut l = budget::lock(&ledger);
            let args = args.trim();
            if !args.is_empty() {
                if args == "off" {
                    l.limit = None;
                } else if let Ok(n) = args.parse::<f64>() {
                    if n.is_finite() && n > 0.0 {
                        l.limit = Some(n);
                    } else {
                        ui_println!("budget must be positive and finite");
                        continue;
                    }
                } else {
                    ui_println!("usage: /budget [dollars|off]");
                    continue;
                }
                let saved =
                    budget::path_snapshot(&ledger_path).and_then(|path| budget::save(&path, &l));
                if let Err(e) = saved {
                    ui_println!("budget persistence failed: {e}");
                    l.unknown = true;
                }
            }
            ui_println!(
                "spent {} · limit {}{}",
                budget::spent_label(&l),
                l.limit.map(pricing::money).unwrap_or_else(|| "off".into()),
                if l.unknown {
                    " · incomplete accounting"
                } else {
                    ""
                }
            );
            continue;
        }
        if let Some(label) = text
            .strip_prefix("/diff")
            .filter(|s| s.is_empty() || s.starts_with(' '))
        {
            let selected = if label.trim().is_empty() {
                latest_checkpoint
                    .as_ref()
                    .ok_or("no task checkpoint".to_string())
                    .cloned()
            } else {
                checkpoint::Checkpoint::load(&checkpoint_dir, label.trim())
            };
            match selected.and_then(|c| c.diff()) {
                Ok(d) => modal::diff(&keys, "Workspace diff", &d),
                Err(e) => modal::info(&keys, "Workspace diff", &e),
            }
            continue;
        }
        if let Some(label) = text
            .strip_prefix("/undo")
            .filter(|s| s.is_empty() || s.starts_with(' '))
        {
            let selected = if label.trim().is_empty() {
                latest_checkpoint
                    .as_ref()
                    .ok_or("no task checkpoint".to_string())
                    .cloned()
            } else {
                checkpoint::Checkpoint::load(&checkpoint_dir, label.trim())
            };
            match selected.and_then(|c| c.undo(&checkpoint_dir)) {
                Ok(d) => {
                    ui_println!("{d}");
                    session.append(Record::System("User restored files to the last task's starting checkpoint. Re-read files before continuing.".into()));
                }
                Err(e) => ui_println!("undo: {e}"),
            }
            continue;
        }
        if let Some(label) = text.strip_prefix("/restore ") {
            let result = checkpoint::Checkpoint::load(&checkpoint_dir, label.trim())
                .and_then(|c| c.restore(&checkpoint_dir));
            match result {
                Ok(s) => {
                    ui_println!("{s}");
                    session.append(Record::System("User restored a saved workspace checkpoint. Re-read files before continuing.".into()));
                }
                Err(e) => ui_println!("restore: {e}"),
            }
            continue;
        }
        if text == "/checkpoints" {
            let mut rows: Vec<_> = std::fs::read_dir(&checkpoint_dir)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
                .collect();
            rows.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));
            for row in rows {
                ui_println!(
                    "{}",
                    row.path().file_stem().unwrap_or_default().to_string_lossy()
                );
            }
            continue;
        }
        if let Some(args) = text
            .strip_prefix("/checkpoint")
            .filter(|s| s.is_empty() || s.starts_with(' '))
        {
            let label = format!("manual-{}", session.log_len());
            let result =
                checkpoint::begin(&cfg.workspace, &checkpoint_dir, &label).and_then(|mut c| {
                    c.finish(&checkpoint_dir)?;
                    Ok(c)
                });
            match result {
                Ok(c) => {
                    ui_println!("checkpoint {} (saved baseline)", c.label);
                }
                Err(e) => ui_println!("checkpoint: {e}"),
            }
            let _ = args;
            continue;
        }
        if text == "/turns" {
            rewind::review(&keys, &session);
            continue;
        }
        let mut prepared_branch = None;
        if text == "/rewind" {
            match rewind::choose(&keys, &session, &cfg.workspace) {
                Ok(Some((fork, through))) => {
                    prepared_branch = Some(fork);
                    text = format!("/branch {through}");
                }
                Ok(None) => continue,
                Err(error) => {
                    modal::info(&keys, "Rewind unavailable", &error);
                    continue;
                }
            }
        }
        if let Some(args) = text
            .strip_prefix("/branch")
            .filter(|s| s.is_empty() || s.starts_with(' '))
        {
            let turns = session.raw_turns().unwrap_or(0);
            let through = if args.trim().is_empty() {
                Ok(turns)
            } else {
                args.trim().parse::<usize>()
            };
            match through
                .map_err(|e| e.to_string())
                .and_then(|t| match prepared_branch.take() {
                    Some(fork) => Ok(fork),
                    None => session
                        .branch(&config::data_dir(), t)
                        .map_err(|e| e.to_string()),
                }) {
                Ok(s) => {
                    permissions::clear_session_approvals()?;
                    ui_println!("branch {} — conversation alternative retained", s.id);
                    warmer.begin_turn();
                    let parent_metadata = polish::metadata(&session);
                    polish::save(&s, &parent_metadata)?;
                    session = s;
                    *painter.live.borrow_mut() = live::Live::new();
                    painter.seed_live(session.history_entries(), session.reasoning_history());
                    painter.welcome_back(&session);
                    navigation::viewed(&session)?;
                    inbox_mailbox = crate::inbox::Mailbox::open(&session);
                    pending = queue::Queue::open(&session)?;
                    polish::bind(&session, &cfg.workspace)?;
                    state = tools::State::new();
                    state.restore_todo(&session.history_records())?;
                    checkpoint_dir = config::data_dir().join("checkpoints").join(&session.id);
                    latest_checkpoint = None;
                    let limit = budget::lock(&ledger).limit;
                    let path = session.path.with_extension("cost.json");
                    ledger = budget::load(&path, limit);
                    ledger_path = Arc::new(Mutex::new(path));
                    *budget_context
                        .lock()
                        .map_err(|_| "budget context lock poisoned".to_string())? =
                        (Arc::clone(&ledger), Arc::clone(&ledger_path));
                    // Refresh credentials before the next request.
                    for r in &session.records {
                        if let Record::State(items) = r {
                            state.apply(&glass_mod_api::ToolCall {
                                id: "restore".into(),
                                name: "state".into(),
                                arguments: serde_json::json!({"items": items}).to_string(),
                            });
                        }
                    }
                }
                Err(e) => ui_println!("branch: {e}"),
            }
            continue;
        }
        if let Some(command) = text.strip_prefix("/bg ") {
            match jobs.start(
                command.trim(),
                &cfg.workspace,
                &config::data_dir().join("jobs").join(&session.id),
            ) {
                Ok(s) => {
                    ui_println!("{s}");
                    session.append(Record::System(format!("User started background command {command}: {s}. Read its log with bash when needed.")));
                }
                Err(e) => ui_println!("background: {e}"),
            }
            continue;
        }
        if text == "/jobs" {
            ui_println!("{}", jobs.list());
            continue;
        }
        if let Some(args) = text.strip_prefix("/job ") {
            let mut parts = args.split_whitespace();
            let result = parts
                .next()
                .and_then(|n| n.parse::<u32>().ok())
                .ok_or("usage: /job <id> [stop]".to_string())
                .and_then(|id| match parts.next() {
                    Some("stop") => jobs.stop(id),
                    None => jobs.output(id),
                    _ => Err("usage: /job <id> [stop]".into()),
                });
            match result {
                Ok(s) => modal::info(&keys, "Job output", &s),
                Err(e) => modal::info(&keys, "Job error", &e),
            }
            continue;
        }

        // `/btw <question>` asks the model a side question against the current
        // context and prints the answer without recording either. The main
        // conversation is untouched, so the next turn costs what it would have
        // anyway.
        if let Some(question) = text.strip_prefix("/btw ") {
            provider = match factory() {
                Ok(p) => p,
                Err(e) => {
                    modal::info(&keys, "Provider is not ready", &e);
                    continue;
                }
            };
            let question = question.trim();
            if question.is_empty() {
                ui_println!("\x1b[33musage: /btw <question>\x1b[0m");
                continue;
            }
            ui_print!("\x1b[90masking...\x1b[0m\r");
            let _ = std::io::stdout().flush();
            match btw::ask(
                provider.as_ref(),
                &cfg.system_prompt,
                &session.messages(),
                question,
                provider.id(),
            ) {
                Ok(answer) if answer.is_empty() => ui_println!("\x1b[33mbtw: no answer\x1b[0m"),
                Ok(answer) => ui_print!(
                    "{}",
                    btw::render(question, &answer, cfg.rate_card(&cfg.model).as_ref())
                ),
                Err(e) => painter.notice(
                    &mut surface::BodyWriter,
                    "\x1b[31m",
                    &format!("btw failed: {e}"),
                ),
            }
            continue;
        }

        // `/compact [focus]` compacts now, below the automatic threshold. A
        // manual compaction is a statement about what the user wants, not a
        // measurement; the optional focus steers what the summary keeps.
        if text == "/compact"
            || text.starts_with("/compact ")
            || text == "/compaction"
            || text.starts_with("/compaction ")
        {
            provider = match factory() {
                Ok(p) => p,
                Err(e) => {
                    modal::info(&keys, "Provider is not ready", &e);
                    continue;
                }
            };
            let focus = text
                .strip_prefix("/compact ")
                .or_else(|| text.strip_prefix("/compaction "))
                .map(str::trim);
            ui_print!("\x1b[90mcompacting…\x1b[0m\r");
            let _ = std::io::stdout().flush();
            match compact::run_manual(provider.as_ref(), &mut session, &state, focus) {
                Ok((before, after)) => ui_println!(
                    "\x1b[32mcompacted\x1b[0m · ~{} → ~{} tokens (estimate) · summary saved to the session",
                    compact(before as u64),
                    compact(after as u64)
                ),
                Err(e) => painter.notice(
                    &mut surface::BodyWriter,
                    "\x1b[31m",
                    &format!("compact failed: {e}"),
                ),
            }
            continue;
        }

        // `/export [path]` writes the raw conversation as Markdown. The log is
        // the source, so the export includes turns compaction dropped.
        if text == "/export" || text.starts_with("/export ") {
            let arg = text.strip_prefix("/export").unwrap_or("").trim();
            let default = format!("glass-session-{}.md", session.id);
            let path = if arg.is_empty() {
                match modal::field(
                    &keys,
                    "Export session",
                    "Write Markdown to",
                    &default,
                    false,
                ) {
                    Some(p) => p,
                    None => continue,
                }
            } else {
                arg.to_string()
            };
            let path = std::path::PathBuf::from(path);
            if path.exists() {
                let confirm = ["Overwrite".to_string(), "Cancel".to_string()];
                if modal::choose(
                    &keys,
                    "Overwrite existing file?",
                    &path.display().to_string(),
                    &confirm,
                ) != Some(0)
                {
                    continue;
                }
            }
            let md = export::markdown(&session, false, &export::raw_records(&session));
            match std::fs::write(&path, md) {
                Ok(()) => ui_println!("\x1b[32mwrote\x1b[0m {}", path.display()),
                Err(e) => painter.notice(
                    &mut surface::BodyWriter,
                    "\x1b[31m",
                    &format!("export failed: {e}"),
                ),
            }
            continue;
        }

        if let Some(args) = text.strip_prefix("/prewalk") {
            let args = args.trim();
            if args.eq_ignore_ascii_case("off") {
                cfg.prewalk.clear();
                *runtime_config.lock().unwrap() = cfg.clone();
                ui_println!("\x1b[90mprewalk off: the primary model answers again\x1b[0m");
            } else if args.is_empty() {
                ui_println!(
                    "\x1b[90mprewalk: {} — set a target with `prewalk = \"MODEL\"` in config, arm it with /prewalk, disarm with /prewalk off\x1b[0m",
                    if cfg.prewalk.is_empty() {
                        "not configured"
                    } else {
                        &cfg.prewalk
                    }
                );
            } else if cfg.prewalk.is_empty() {
                ui_println!(
                    "\x1b[31mprewalk is not configured: set `prewalk = \"MODEL\"` in config.toml first\x1b[0m"
                );
            } else {
                // The handoff is one durable note: the planning model wrote
                // the plan and state; the target continues from them. The
                // model id recorded in the note is who answers next.
                let from = cfg.model.clone();
                let target = cfg.prewalk.clone();
                match crate::checkpoint_tools::prewalk_note(&mut session, &from, &target) {
                    Ok(note) => {
                        // Route: the runtime config is what the factory reads
                        // when the next turn rebuilds the provider.
                        cfg.model = target.clone();
                        *runtime_config.lock().unwrap() = cfg.clone();
                        ui_println!("\x1b[90mprewalk: {note}\x1b[0m");
                        warmer.begin_turn();
                    }
                    Err(e) => ui_println!("\x1b[31mprewalk failed: {e}\x1b[0m"),
                }
            }
            continue;
        }

        if let Some(name) = text.strip_prefix("/model ") {
            let name = name.trim();
            if !name.is_empty() {
                cfg.model = name.into();
                *runtime_config.lock().unwrap() = cfg.clone();
                if let Some(id) = &cfg.connection
                    && let Err(e) = auth::edit(|store| {
                        if let Some(c) = store.connections.iter_mut().find(|c| &c.id == id) {
                            c.model = name.into();
                        }
                        Ok(())
                    })
                {
                    ui_println!("model preference persistence failed: {e}");
                }
                warmer.begin_turn();
                ui_println!("\x1b[90mmodel: {name}\x1b[0m");
            }
            continue;
        }

        if text == "/pacing" {
            if let Some(index) = modal::choose(
                &keys,
                "Provider pacing",
                "Adaptive spacing is optional. Server Retry-After and reset deadlines always apply. Changes apply to subsequent requests; saved with an active workspace profile. Otherwise set adaptive_pacing in config.toml to persist.",
                &[
                    format!(
                        "{}Adaptive · learn spacing from provider pressure",
                        if cfg.adaptive_pacing { "✓ " } else { "" }
                    ),
                    format!(
                        "{}Maximum frequency · disable learned and burst spacing",
                        if !cfg.adaptive_pacing { "✓ " } else { "" }
                    ),
                ],
            ) {
                cfg.adaptive_pacing = index == 0;
                *runtime_config.lock().unwrap() = cfg.clone();
                if let Err(error) = profiles::update_active(&cfg) {
                    modal::info(&keys, "Profile save error", &error);
                }
                warmer.begin_turn();
            }
            continue;
        }

        if text == "/todo" {
            todo::view(&keys, &mut state, &mut session, &cfg.workspace);
            continue;
        }

        if text == "/animation" {
            if let Some(index) = modal::animation(&keys) {
                glass_tui::theme::select_animation(index);
                if let Err(error) = profiles::update_active(&cfg) {
                    modal::info(&keys, "Profile save error", &error);
                }
            }
            continue;
        }

        if text == "/theme" {
            let names = glass_tui::theme::NAMES.map(String::from);
            if let Some(i) = modal::choose(
                &keys,
                "Appearance",
                "Aurora · cool blue and lilac. Amber · warm gold. Monochrome · neutral. Orchid · violet. Ocean · teal. Active work animates with coordinated colors. NO_COLOR uses neutral presentation.",
                &names,
            ) {
                glass_tui::theme::select(i);
                if let Err(e) = profiles::update_active(&cfg) {
                    modal::info(&keys, "Profile save error", &e);
                }
            }
            continue;
        }
        if matches!(text.as_str(), "/security" | "/permissions") {
            match permissions::review(&keys, &cfg, &cfg.extensions) {
                permissions::ReviewAction::ApproveExactCommand(command) => {
                    match permissions::approve_session_command(&command) {
                        Ok(()) => modal::info(
                            &keys,
                            "Session exception approved",
                            "Only this exact command text is exempt from the configured shell confinement for this session.",
                        ),
                        Err(error) => modal::info(&keys, "Approval not saved", &error),
                    }
                }
                permissions::ReviewAction::SelectProfile(profile) => {
                    match permissions::persist_profile(&config::default_path(), profile) {
                        Ok(()) => {
                            cfg.permission_profile = profile;
                            modal::info(
                                &keys,
                                "Profile saved",
                                "The selected permission profile is saved in config.toml and takes effect after restarting glass-core. The current process keeps its startup policy.",
                            );
                        }
                        Err(error) => modal::info(&keys, "Profile not saved", &error),
                    }
                }
                permissions::ReviewAction::Close => {}
            }
            continue;
        }
        if text == "/mods" {
            let names = mods::available();
            let rows = names
                .iter()
                .map(|name| {
                    format!(
                        "{} · {}",
                        name,
                        if loaded_mods.iter().any(|m| m.name() == name) {
                            "enabled · disable"
                        } else {
                            "disabled · enable"
                        }
                    )
                })
                .collect::<Vec<_>>();
            if let Some(i) = modal::choose(
                &keys,
                "Built-in mods",
                "Changes apply to this session. Save mods in config.toml for future sessions. Redact masks credential-marker lines in tool results; noisy is a demo.",
                &rows,
            ) {
                let name = &names[i];
                if let Some(index) = loaded_mods.iter().position(|m| m.name() == name) {
                    loaded_mods.remove(index);
                } else {
                    loaded_mods.extend(mods::load(std::slice::from_ref(name))?);
                }
            }
            cfg.mods = loaded_mods.iter().map(|m| m.name().to_string()).collect();
            if let Err(e) = profiles::update_active(&cfg) {
                modal::info(&keys, "Profile save error", &e);
            }
            continue;
        }
        if text == "/extensions" {
            warmer.begin_turn();
            if let Err(e) = addons::manage(&keys, &mut cfg, extensions) {
                modal::info(&keys, "Extension manager error", &e);
            }
            if let Err(e) = profiles::update_active(&cfg) {
                modal::info(&keys, "Profile save error", &e);
            }
            *runtime_config.lock().unwrap() = cfg.clone();
            palette = glass_tui::palette::Palette::new(command_catalogue(extensions));
            continue;
        }
        if text == "/skills" {
            if skills.is_empty() {
                modal::info(
                    &keys,
                    "Skills",
                    &format!(
                        "No skills loaded. Add Markdown files in {}",
                        config::config_dir().join("skills").display()
                    ),
                );
            } else {
                let rows = skills
                    .iter()
                    .map(|s| format!("{} — {}", s.name, s.description))
                    .collect::<Vec<_>>();
                if let Some(i) = modal::choose(&keys, "Skills", "Choose a skill to read", &rows) {
                    let content =
                        std::fs::read_to_string(&skills[i].path).unwrap_or_else(|e| e.to_string());
                    modal::info(&keys, &skills[i].name, &content);
                }
            }
            continue;
        }

        // `/clear` starts a fresh conversation in this terminal.
        // The old session stays on disk — the log is the resume
        // mechanism — so clearing is a new conversation, not a
        // deletion.
        if text == "/clear" {
            match Session::create(&config::data_dir()) {
                Ok(s) => {
                    permissions::clear_session_approvals()?;
                    // A pending cache refresh would warm the old
                    // conversation; a new session supersedes it.
                    warmer.begin_turn();
                    session = s;
                    *painter.live.borrow_mut() = live::Live::new();
                    painter.seed_live(session.history_entries(), session.reasoning_history());
                    painter.welcome_back(&session);
                    navigation::viewed(&session)?;
                    inbox_mailbox = crate::inbox::Mailbox::open(&session);
                    pending = queue::Queue::open(&session)?;
                    polish::bind(&session, &cfg.workspace)?;
                    checkpoint_dir = config::data_dir().join("checkpoints").join(&session.id);
                    let limit = budget::lock(&ledger).limit;
                    let path = session.path.with_extension("cost.json");
                    ledger = budget::load(&path, limit);
                    ledger_path = Arc::new(Mutex::new(path));
                    *budget_context
                        .lock()
                        .map_err(|_| "budget context lock poisoned".to_string())? =
                        (Arc::clone(&ledger), Arc::clone(&ledger_path));
                    // Refresh credentials before the next request.
                    state = tools::State::new();
                    state.restore_todo(&session.history_records())?;
                    totals = Totals::new();
                    latest_checkpoint = None;
                    editor.clear();
                    ui_println!("\x1b[90mcleared — new session {}\x1b[0m", session.id);
                }
                Err(e) => ui_println!("\x1b[31m/clear failed: {e}\x1b[0m"),
            }
            continue;
        }

        // Anything else beginning with `/` may be an extension command. If no
        // extension claims it, say so rather than sending it to the model as a
        // prompt, where it would read as confused text.
        if let Some(rest) = text.strip_prefix('/')
            && !recovering
            && !run_saved_queue
        {
            let (cmd, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            if extensions.has_command(cmd) {
                match extensions.run_command(cmd, args.trim()) {
                    Some(out) => modal::rich_info(&keys, &format!("/{cmd}"), out.trim_end()),
                    None => ui_println!("\x1b[33m/{cmd} did not return a result\x1b[0m"),
                }
                continue;
            }
            ui_println!("\x1b[31munknown command /{cmd}\x1b[0m — try /help");
            continue;
        }

        if run_saved_queue {
            if let Some(prompt) = pending.next_prompt() {
                text = prompt.to_owned();
            } else {
                modal::info(&keys, "Follow-up queue", "No saved follow-ups to run.");
                continue;
            }
        }

        // A new turn supersedes any cache refresh the previous
        // one scheduled: the user is back, so the next request
        // rewrites the cache anyway.
        warmer.begin_turn();
        if auth::needs_refresh(&cfg) {
            let refresh_cfg = cfg.clone();
            match modal::work(&keys, "Refreshing ChatGPT session", move || {
                auth::resolve(&refresh_cfg)
            }) {
                Some(Ok(_)) => {}
                Some(Err(e)) => {
                    modal::info(&keys, "Sign-in needs attention", &e);
                    editor.set_text(&text);
                    continue;
                }
                None => {
                    editor.set_text(&text);
                    continue;
                }
            }
        }
        provider = match factory() {
            Ok(p) => p,
            Err(e) => {
                modal::info(&keys, "Provider is not ready", &e);
                editor.set_text(&text);
                continue;
            }
        };
        if run_saved_queue {
            match pending.deliver(&mut session) {
                Ok(Some(prompt)) => text = prompt,
                Ok(None) => continue,
                Err(error) => {
                    modal::info(&keys, "Follow-up remains queued", &error);
                    continue;
                }
            }
        }
        accepted_background = None;
        menu_parents.clear();
        if !recovering && !run_saved_queue && !background_recorded {
            // In-message citations (task six-wave-e-001): every range
            // attachment cited since the last turn becomes a durable
            // `@path#L..#hash8` line of this very message, so the quote
            // survives its source changing or vanishing. Dedupe happens at
            // projection; a hand-typed copy of the same marker collapses
            // there instead of double-citing.
            text = polish::with_citation_markers(&session, &text);
            if let Err(error) = session.append_checked(&[Record::User(text.clone())]) {
                modal::info(
                    &keys,
                    "Session log write failed",
                    &format!("Prompt was not sent: {error}"),
                );
                editor.set_text(&text);
                continue;
            }
        }
        // The typed prompt is still on the prompt row. Clear those
        // rows and print the prompt once, whole: a prompt must be
        // recorded once, and a multi-line one must land intact.
        echo_prompt(&mut std::io::stdout(), &text, prompt_row, prompt_rows);
        navigation::record(&session, &cfg.workspace, &cfg.model, "running")?;
        // Submission transfers ownership of the prompt to conversation
        // history. Remove its visible draft before provider work can begin.
        surface::clear_composer();

        // Dispatch this message, then anything the user queued while it ran.
        // The queue is drained here so a follow-up is a single Enter away from
        // being asked, which is the entire point of queuing it.
        let mut next = Some(text);
        while next.is_some() {
            let label = format!("task-{}", session.log_len());
            let mut task_checkpoint =
                match checkpoint::begin_if_repository(&cfg.workspace, &checkpoint_dir, &label) {
                    Ok(c) => c,
                    Err(e) => {
                        ui_println!("checkpoint unavailable: {e}");
                        None
                    }
                };
            let mut last_error: Option<String> = None;
            // Read before the agent borrows the session.
            let status_title = crate::polish::title(&session);
            crate::status::set("working", &status_title);
            let turn_instructions = polish::prompt_context(&session, &project_instructions);
            painter.seed_live(session.history_entries(), session.reasoning_history());
            let turn_started = std::time::Instant::now();
            let mut busy_commands = command_catalogue(extensions);
            for command in &mut busy_commands {
                if !actions::available_during_turn(&command.name) {
                    command.description = format!("After this turn · {}", command.description);
                }
            }
            let mut busy_palette = glass_tui::palette::Palette::new(busy_commands);
            let outcome = {
                let mut agent = Agent::new(
                    provider.as_ref(),
                    &mut session,
                    &mut state,
                    &loaded_mods,
                    extensions,
                    &cfg.workspace,
                )
                .with_image_input(cfg.image_input)
                .prime_extensions()
                .with_prompt(
                    cfg.system_prompt.clone(),
                    catalogue.clone(),
                    turn_instructions,
                )
                .with_limits(cfg.max_tokens, cfg.reasoning_budget, usize::MAX)
                .with_recovery(recovering)
                .with_checkpoint_tools(cfg.checkpoint_tools)
                .with_live(cfg.live_flush_ms as u64)
                .with_stream_factory(Arc::clone(&factory))
                .with_input_keys(&keys)
                .with_progress(|p| painter.handle(p))
                .with_warmer(&warmer);

                painter.reset();
                let mut draft = glass_tui::Editor::new();
                let result = agent.run_turn(&mut |steer_slot: &mut Option<String>| {
                    if painter.refresh_geometry(!busy_palette.active()) && !busy_palette.active() {
                        surface::composer(&draft, true, true);
                    }
                    if !busy_palette.active() {
                        painter.tick();
                    }
                    if let Some(s) = drain_input(
                        &keys,
                        &mut draft,
                        &mut pending,
                        &mut busy_palette,
                        &painter,
                        &team_owner,
                    ) {
                        if let Err(error) = pending.push_checked(queue::Intent::Steer(s.clone())) {
                            draft.set_text(&s);
                            ui_println!("Cannot save correction: {error}");
                        } else {
                            *steer_slot = Some(s);
                        }
                    }
                });
                if busy_palette.active() {
                    busy_palette.cancel(&mut draft);
                    surface::overlay(false).ok();
                }
                painter.flush(&mut surface::BodyWriter);
                editor.set_text(draft.text());
                painter.live.borrow_mut().finish();
                while painter.live.borrow().active() {
                    painter.live.borrow_mut().draw().ok();
                    if let Some(ev) = keys.next_blocking() {
                        painter.live.borrow_mut().event(&ev);
                    } else {
                        painter.live.borrow_mut().close().ok();
                    }
                }
                result
            };

            if let Some(c) = task_checkpoint.as_mut() {
                match c.finish(&checkpoint_dir) {
                    Ok(()) => {
                        if let Err(error) = checkpoint::record_turn(&mut session, c) {
                            ui_println!("checkpoint receipt: {error}");
                        }
                        latest_checkpoint = task_checkpoint;
                    }
                    Err(e) => ui_println!("checkpoint: {e}"),
                }
            }
            if matches!(&outcome, Ok((_, Stop::Finished)))
                && let Err(error) = advisor::milestone(&session)
            {
                ui_println!("Advisor: {error}");
            }
            if let Err(error) = navigation::record(
                &session,
                &cfg.workspace,
                &cfg.model,
                match &outcome {
                    Err(_) => "blocked",
                    Ok((_, Stop::Cancelled)) => "cancelled",
                    Ok((_, Stop::Steered)) => "steered",
                    Ok((_, Stop::ToolBudget)) => "incomplete",
                    Ok((_, Stop::Finished)) => "completed",
                },
            ) {
                ui_println!("navigation receipt: {error}");
            }
            let mut turn_failed = outcome.is_err();
            // The tab status reflects the turn's verdict: finished, failed
            // (first line of the reason), or back to idle after a cancel.
            crate::status::set(
                match &outcome {
                    Ok((_, Stop::Cancelled)) => "idle",
                    Ok(_) => "done",
                    Err(_) => "error",
                },
                &match &outcome {
                    Ok(_) => crate::polish::title(&session),
                    // One line only: the protocol forbids control characters
                    // in decoded text.
                    Err(e) => e.to_string().lines().next().unwrap_or("").to_string(),
                },
            );
            let cancelled = matches!(&outcome, Ok((_, Stop::Cancelled)));
            let mut turn_finished = false;
            let mut turn_summary = Vec::new();
            match outcome {
                Ok((out, stop)) => {
                    retry_available = false;
                    turn_finished = matches!(stop, Stop::Finished | Stop::ToolBudget);
                    match stop {
                        Stop::ToolBudget => ui_println!(
                            "\x1b[33m⚠ stopped: reached the tool-call budget for this turn\x1b[0m"
                        ),
                        Stop::Steered => {
                            ui_println!("\x1b[33m⚡ stopped to apply your correction\x1b[0m")
                        }
                        Stop::Cancelled => {
                            // Queued follow-ups survive cancellation and restart.
                            ui_println!(
                                "Cancelled · partial output saved · owned shell process groups stopped"
                            );
                        }
                        Stop::Finished => {}
                    }
                    for w in &out.warnings {
                        ui_println!("\x1b[33m⚠ {w}\x1b[0m");
                    }
                    let context_line = format!(
                        "Context: request {} · /context",
                        context_audit::reports(&session).len()
                    );
                    ui_println!("\x1b[90m{context_line}\x1b[0m");
                    turn_summary.push(context_line);
                    if let Some(u) = out.usage {
                        totals.add(&u);
                        totals.last_input = out.last_input.map(u64::from);
                        let card = cfg.rate_card(&cfg.model);
                        let width = term_size().0 as usize;
                        let l = budget::lock(&ledger);
                        let cost = if l.unknown {
                            "cost unavailable".into()
                        } else {
                            format!("session {}", pricing::money(l.spent))
                        };
                        if width < 80 {
                            let mut rows = vec![format!(
                                "Turn: {} input · {} output",
                                compact(u.prompt_total() as u64),
                                compact(u.output as u64)
                            )];
                            let window = provider.context_window();
                            rows.push(format!(
                                "{} session total tokens",
                                compact(totals.prompt_total() + totals.output)
                            ));
                            if let Some(input) = totals.last_input.filter(|_| window > 0) {
                                rows.push(format!(
                                    "{}% context",
                                    (input * 100 / window as u64).min(999)
                                ));
                            }
                            rows.push(cost);
                            turn_summary.extend(rows.iter().cloned());
                            for row in rows {
                                for line in
                                    glass_tui::text::wrap_words(&row, width.saturating_sub(1))
                                {
                                    ui_println!("\x1b[90m{line}\x1b[0m");
                                }
                            }
                        } else {
                            let report = format!(
                                "{}{} · {}",
                                usage_line(&u, card.as_ref(), false),
                                totals.suffix(provider.context_window(), None, false),
                                cost
                            );
                            ui_println!("{report}");
                            turn_summary.push(glass_tui::markdown::sanitize(&report));
                        }
                    }
                }
                Err(e) => {
                    painter.notice(
                        &mut surface::BodyWriter,
                        &glass_tui::theme::ansi(glass_tui::theme::Tone::Error),
                        &format!("error: {e}"),
                    );
                    last_error = Some(e.to_string());
                }
            }

            if !turn_summary.is_empty() {
                let summary = turn_summary.join("\n");
                session.append(Record::Meta(summary.clone()));
                painter.append_live_entry(glass_tui::Role::Meta, summary);
            }

            if turn_failed {
                // A failed turn always deserves attention, however fast it
                // failed: the connection is probably gone, and the user may
                // already be away from the keyboard.
                crate::notify::publish(
                    cfg.notify,
                    &crate::notify::TurnEvent::Failed {
                        seconds: turn_started.elapsed().as_secs(),
                    },
                );
            }

            if let Some(msg) = last_error {
                session.append(Record::Meta(format!("error: {msg}")));
                painter.append_live_entry(glass_tui::Role::Meta, format!("error: {msg}"));
                retry_available = true;
                ui_println!(
                    "/retry: Retry · Switch model · Sign in. Partial output is saved; completed tools stay in context."
                );
                // A failed turn means the connection is probably gone; draining
                // the queue would just burn the same failure repeatedly.
                break;
            }

            if cancelled {
                // A steered turn never alerts: the user is at the keyboard.
                crate::notify::publish(cfg.notify, &crate::notify::TurnEvent::Steered);
                break;
            }
            next = match pending.deliver(&mut session) {
                Ok(next) => next,
                Err(error) => {
                    ui_println!("Cannot record follow-up; it remains queued: {error}");
                    turn_failed = true;
                    break;
                }
            };
            if let Some(q) = &next {
                // Work continues: a subscriber sees Waiting, the bell stays
                // silent (an alert mid-series would be premature).
                crate::notify::publish(cfg.notify, &crate::notify::TurnEvent::Waiting);
                ui_println!("\n\x1b[38;5;39m❯\x1b[0m {q}");
            } else if turn_finished {
                // Only when the turn series is fully done: a queued follow-up
                // means work continues and an alert would be premature.
                let cost = {
                    let l = budget::lock(&ledger);
                    (!l.unknown).then(|| pricing::money(l.spent))
                };
                crate::notify::publish(
                    cfg.notify,
                    &crate::notify::TurnEvent::Finished {
                        seconds: turn_started.elapsed().as_secs(),
                        elapsed: turn_started.elapsed(),
                        cost,
                    },
                );
            }
        }
        let completion =
            polish::completion(&session, latest_checkpoint.as_ref(), term_size().0 as usize);
        ui_println!("\x1b[90m{completion}\x1b[0m");
        if !session
            .records
            .last()
            .is_some_and(|r| matches!(r, Record::Meta(t) if t.starts_with("error:")))
        {
            session.append(Record::Meta(completion.clone()));
            painter.append_live_entry(glass_tui::Role::Meta, completion);
        }
        ui_println!();

        // Scroll the prompt back to just under the last output line.
        let (nw, nh) = term_size();
        painter.set_width(nw);
        let _ = nh;
        back.push(String::new());
    }
}

/// Consume key events while the agent is busy.
///
/// Mid-turn input needs its own line editor: the REPL's editor has already been
/// consumed by the turn that is running, and replaying into it would show the
/// user characters in a buffer they are no longer looking at. So the drain path
/// keeps its own editor and renders it below the streaming output.
///
/// Only the two submission intents are honoured. Navigation and editing keys go
/// to this editor, so a message can be typed and corrected normally while the
/// model works.
static EXIT_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn drain_input(
    keys: &input::KeyReader,
    editor: &mut glass_tui::Editor,
    pending: &mut queue::Queue,
    palette: &mut glass_tui::palette::Palette,
    painter: &Painter,
    team_owner: &str,
) -> Option<String> {
    use glass_tui::crossterm::event::{Event, KeyCode, KeyModifiers};

    let mut dirty = false;

    while let Some(ev) = keys.try_next() {
        if matches!(&ev, Event::Key(k) if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('d'))
        {
            if modal::confirm_exit(keys, editor, true) {
                EXIT_REQUESTED.store(true, std::sync::atomic::Ordering::Release);
                palette.close();
                surface::overlay(false).ok();
                return Some(String::new());
            }
            surface::overlay(palette.active()).ok();
            dirty = true;
            continue;
        }
        // Interrupt must work before overlays consume editing keys.
        if matches!(&ev, Event::Key(k) if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c'))
        {
            painter.live.borrow_mut().close().ok();
            palette.close();
            surface::overlay(false).ok();
            editor.clear();
            painter.append_live_entry(
                glass_tui::Role::System,
                "Cancelled the current turn; partial output is saved.",
            );
            ui_println!("\r\x1b[2K\x1b[33m⚡ interrupt\x1b[0m");
            return Some(String::new());
        }
        if palette.active() {
            let selected = match ev {
                Event::Key(key) if key.code == KeyCode::F(1) => {
                    if let Some(details) = palette.selected_details(editor.text()) {
                        modal::info(keys, "Command details", &details);
                        surface::overlay(true).ok();
                    }
                    dirty = true;
                    None
                }
                Event::Key(key) => palette.key(editor, key),
                Event::Paste(text) => {
                    editor.insert_str(&text);
                    palette.changed(editor);
                    None
                }
                _ => None,
            };
            if !palette.active() {
                surface::overlay(false).ok();
                if let Some(draft) = palette.take_draft() {
                    editor.set_text(&draft);
                }
                dirty = true;
            }
            if let Some(command) = selected {
                match command.as_str() {
                    "/queue" => {
                        let old = pending.correction().map(str::to_owned);
                        pending.manage(keys);
                        if pending.correction() != old.as_deref()
                            && let Some(correction) = pending.correction()
                        {
                            return Some(correction.to_string());
                        }
                    }
                    "/agents" => team::view(keys, team_owner),
                    "/todo" => todo::view_current(keys),
                    "/details" | "/outputs" | "/responses" | "/work" | "/dashboard" | "/hub"
                    | "/workspace" | "/activity" => {
                        let entries = painter.live.borrow().snapshot();
                        if let Err(error) = painter.open_reader(entries, false) {
                            ui_println!("Reader unavailable: {error}");
                        }
                    }
                    "/thinking" => {
                        if let Err(error) = painter.live.borrow_mut().toggle(true) {
                            ui_println!("Reasoning view unavailable: {error}");
                        }
                    }
                    "/help" => modal::info(
                        keys,
                        "Commands during a turn",
                        "Queue, agents, checklist, thinking and details are available now. Other commands are marked After this turn. Enter sends a correction; Alt+Enter saves a follow-up. Esc closes the palette.",
                    ),
                    _ => modal::info(
                        keys,
                        "Available after this turn",
                        "This command changes session or provider state. Finish or cancel the current turn, then invoke it again. Your draft is preserved.",
                    ),
                }
            }
            if !palette.active()
                && let Some(reader) = painter.reader.borrow_mut().as_mut()
            {
                let _ = reader.restore_after_overlay();
            }
            continue;
        }
        if let Event::Key(key) = &ev
            && key.code == KeyCode::Char('/')
            && editor.text().is_empty()
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            editor.insert_str("/");
            surface::overlay(true).ok();
            if palette.open_managed(editor).is_err() {
                surface::overlay(false).ok();
            }
            continue;
        }
        let resized = painter.refresh_geometry(true);
        if resized {
            surface::composer(editor, true, true);
        }
        if painter.live.borrow_mut().event(&ev) {
            dirty = true;
            continue;
        }
        if painter.reader_active() {
            match painter.reader_event(&ev, !editor.text().is_empty()) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => {
                    let _ = painter.close_reader();
                    ui_println!("inline reader: {error}");
                }
            }
        }
        if should_open_reader_for_wheel(
            &ev,
            painter.reader_active(),
            painter.live.borrow().active(),
        ) {
            let entries = painter.live.borrow().snapshot();
            if let Err(error) = painter.open_reader(entries, false) {
                ui_println!("inline reader: {error}");
            } else if let Err(error) = painter.reader_event(&ev, !editor.text().is_empty()) {
                let _ = painter.close_reader();
                ui_println!("inline reader: {error}");
            }
            continue;
        }
        if let Event::Key(k) = &ev {
            if k.code == KeyCode::Char('a') && k.modifiers.contains(KeyModifiers::ALT) {
                team::view(keys, team_owner);
                dirty = true;
                continue;
            }
            if k.code == KeyCode::PageUp && !painter.live.borrow().active() {
                let entries = painter.live.borrow().snapshot();
                if let Err(error) = painter.open_reader(entries, true) {
                    ui_println!("inline reader: {error}");
                }
                continue;
            }
            if k.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(k.code, KeyCode::Char('b' | 'f'))
            {
                let entries = painter.live.borrow().snapshot();
                let result = if k.code == KeyCode::Char('f') {
                    painter.open_reader_search(entries)
                } else {
                    painter.open_reader(entries, true)
                };
                if let Err(error) = result {
                    ui_println!("inline reader: {error}");
                }
                continue;
            }
            if k.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(k.code, KeyCode::Char('g') | KeyCode::Char('t'))
            {
                painter
                    .live
                    .borrow_mut()
                    .toggle(k.code == KeyCode::Char('t'))
                    .ok();
                dirty = true;
                continue;
            }
        }
        if painter.live.borrow_mut().event(&ev) {
            dirty = true;
            continue;
        }
        let k = match ev {
            Event::Paste(text) => {
                editor.insert_str(&text);
                dirty = true;
                continue;
            }
            Event::Resize(_, _) => {
                dirty = true;
                continue;
            }
            Event::Key(k) => k,
            _ => continue,
        };

        let alt = k.modifiers.contains(KeyModifiers::ALT);
        let edit_key = if alt && k.code == KeyCode::Enter {
            glass_tui::crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
        } else {
            k
        };
        match editor.key(edit_key) {
            glass_tui::Edit::Continue => dirty = true,
            glass_tui::Edit::Submit(text) => {
                if text == "/queue" {
                    let old_correction = pending.correction().map(str::to_owned);
                    pending.manage(keys);
                    dirty = true;
                    if pending.correction() != old_correction.as_deref()
                        && let Some(correction) = pending.correction()
                    {
                        return Some(correction.to_string());
                    }
                    continue;
                }
                let Some(intent) = queue::classify(&text, alt, true) else {
                    continue;
                };
                match intent {
                    queue::Intent::Steer(s) => {
                        ui_println!(
                            "\x1b[33m⚡ steering: {}\x1b[0m",
                            glass_tui::markdown::sanitize(&s)
                        );
                        // Submit clears the editor buffer. Paint that cleared
                        // state before returning early to hand off the steer.
                        surface::composer(editor, true, true);
                        // Returned so the caller owns the slot; taking it here
                        // would borrow the same state the loop is holding.
                        return Some(s);
                    }
                    queue::Intent::FollowUp(s) => {
                        if let Err(error) = pending.push_checked(queue::Intent::FollowUp(s.clone()))
                        {
                            editor.set_text(&s);
                            ui_println!("Cannot save follow-up: {error}");
                            continue;
                        }
                        painter.append_live_entry(glass_tui::Role::User, format!("Queued: {s}"));
                        ui_println!("\x1b[90m· queued\x1b[0m");
                        dirty = true;
                    }
                }
            }
        }
    }

    if palette.active() {
        let (width, height) = term_size();
        palette.render(editor.text(), width, height);
        return None;
    }
    painter.live.borrow_mut().draw().ok();
    if dirty {
        surface::composer(editor, true, true);
    }
    None
}

/// Print the submitted prompt into scrollback.
///
/// The typed prompt is still on the prompt row, so those rows are
/// cleared and the prompt is printed once, whole: a prompt must be
/// recorded once, and a multi-line one must land intact.
fn render_prompt(io: &mut impl Write, text: &str, width: u16) {
    let clean = glass_tui::markdown::sanitize(text).replace('\t', "    ");
    for (index, line) in
        glass_tui::text::wrap_words(&clean, width.saturating_sub(3).max(1) as usize)
            .into_iter()
            .enumerate()
    {
        let _ = write!(
            io,
            "{}{} {line}\x1b[0m\r\n",
            glass_tui::theme::accent(),
            if index == 0 { "❯" } else { " " }
        );
    }
}

fn echo_prompt(_io: &mut impl Write, text: &str, _row: u16, _rows: u16) {
    let mut io = glass_tui::theme::TerminalWriter::new(surface::BodyWriter);
    render_prompt(&mut io, text, term_size().0);
    let _ = io.flush();
}

fn should_open_reader_for_wheel(
    event: &glass_tui::crossterm::event::Event,
    reader_active: bool,
    other_view_active: bool,
) -> bool {
    use glass_tui::crossterm::event::{Event, MouseEventKind};
    !reader_active
        && !other_view_active
        && matches!(event, Event::Mouse(mouse) if matches!(mouse.kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown))
}

fn usage_line(u: &provider::Usage, card: Option<&pricing::RateCard>, narrow: bool) -> String {
    // Compact counts: "1.8k in" keeps the line inside a
    // phone-width terminal, where "1,800 in" would wrap.
    let total = if narrow {
        compact(u.prompt_total() as u64)
    } else {
        thousands(u64::from(u.prompt_total()))
    };
    let pct = percentage(u64::from(u.cache_read), u64::from(u.prompt_total()), 100);
    let cached = if u.cache_read > 0 || u.cache_write > 0 {
        format!(" · cache {pct}%")
    } else {
        // Zero in both buckets means nothing was cached at all: either the
        // prefix is under the model's minimum, or there is no breakpoint.
        " · \x1b[33mno cache\x1b[90m".to_string()
    };
    let cost = card
        .map(|c| format!(" · {}", pricing::money(c.cost(u))))
        .unwrap_or_default();
    let output = if narrow {
        compact(u.output as u64)
    } else {
        thousands(u64::from(u.output))
    };
    format!("\x1b[90m  {total} input / {output} output{cached}{cost}\x1b[0m")
}

/// Running token totals for the whole session.
///
/// The per-turn line shows the rate; this shows how much
/// the session has cost so far. That cumulative number is
/// the one that decides whether a cache warmer, or a long
/// task, is worth what it is spending.
struct Totals {
    turns: u32,
    input: u64,
    cache_read: u64,
    cache_write: u64,
    output: u64,
    last_input: Option<u64>,
}

impl Totals {
    fn new() -> Self {
        Self {
            last_input: None,
            turns: 0,
            input: 0,
            cache_read: 0,
            cache_write: 0,
            output: 0,
        }
    }

    fn add(&mut self, u: &provider::Usage) {
        self.last_input = Some(u64::from(u.prompt_total()));
        self.turns = self.turns.saturating_add(1);
        self.input = self.input.saturating_add(u64::from(u.input));
        self.cache_read = self.cache_read.saturating_add(u64::from(u.cache_read));
        self.cache_write = self.cache_write.saturating_add(u64::from(u.cache_write));
        self.output = self.output.saturating_add(u64::from(u.output));
    }

    fn prompt_total(&self) -> u64 {
        self.input
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_write)
    }

    /// Compact tail for the usage line: cumulative tokens,
    /// how full the context window is, and what it all cost.
    ///
    /// The fill ratio is the number that matters most for
    /// judging a long session: it tells you how close the
    /// conversation is to compaction, which is the point
    /// where quality starts to drop. On a narrow terminal
    /// the tail takes its own row, so the line never
    /// wraps mid-number.
    fn suffix(&self, window: u32, card: Option<&pricing::RateCard>, narrow: bool) -> String {
        let tok = self.prompt_total().saturating_add(self.output);
        let fill = self
            .last_input
            .filter(|_| window > 0)
            .map(|input| percentage(input, u64::from(window), 999));
        let cost = card
            .map(|c| format!(" · {}", pricing::money(self.total_cost(c))))
            .unwrap_or_default();
        let lead = if narrow {
            "\n  session "
        } else {
            " · session "
        };
        match fill {
            // Near the window, compaction is close: flag it so
            // the fill is visible rather than discovered mid-task.
            Some(f) if f >= 80 => format!(
                "\x1b[90m{lead}{tok} total tokens · \x1b[33m{f}% ctx\x1b[90m{cost}\x1b[0m",
                tok = compact(tok),
            ),
            Some(f) => format!(
                "\x1b[90m{lead}{tok} total tokens · {f}% ctx{cost}\x1b[0m",
                tok = compact(tok),
            ),
            None => format!(
                "\x1b[90m{lead}{tok} total tokens{cost}\x1b[0m",
                tok = compact(tok),
            ),
        }
    }

    /// What the session has cost so far, in dollars.
    fn total_cost(&self, card: &pricing::RateCard) -> f64 {
        (count_as_f64(self.input) * card.input
            + count_as_f64(self.cache_read) * card.cache_read
            + count_as_f64(self.cache_write) * card.cache_write
            + count_as_f64(self.output) * card.output)
            / 1_000_000.0
    }
}

/// Abbreviate a large count so the session tail stays short.
fn compact(n: u64) -> String {
    if n >= 1_000_000 {
        compact_scaled(n, 1_000_000, "M")
    } else if n >= 10_000 {
        compact_scaled(n, 1_000, "k")
    } else {
        thousands(n)
    }
}

fn compact_scaled(value: u64, scale: u64, suffix: &str) -> String {
    let whole = value / scale;
    let tenths = ((value % scale) * 10 + scale / 2) / scale;
    if tenths == 10 {
        format!("{}.0{suffix}", whole.saturating_add(1))
    } else {
        format!("{whole}.{tenths}{suffix}")
    }
}

fn percentage(numerator: u64, denominator: u64, ceiling: u32) -> u32 {
    if denominator == 0 {
        return 0;
    }
    let threshold = denominator.saturating_mul(u64::from(ceiling)).div_ceil(100);
    if numerator >= threshold {
        return ceiling;
    }
    let whole = numerator / denominator;
    let remainder = numerator % denominator;
    let percent = whole
        .saturating_mul(100)
        .saturating_add((remainder * 100 + denominator / 2) / denominator);
    u32::try_from(percent).unwrap_or(ceiling).min(ceiling)
}

/// Token pricing uses `f64`; this split avoids a narrowing cast, while the
/// final floating arithmetic remains display-only and approximate.
fn count_as_f64(value: u64) -> f64 {
    let high = u32::try_from(value >> 32).unwrap_or(u32::MAX);
    let low = u32::try_from(value & u64::from(u32::MAX)).unwrap_or_default();
    f64::from(high) * 4_294_967_296.0 + f64::from(low)
}

/// Group digits so long token counts stay readable at a glance.
fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Paints progress events into the terminal's scrollback.
///
/// Streaming text is written bare so the turn reads as prose; tool
/// activity is prefixed and dimmed so it is visibly not prose.
/// Everything lands in the terminal's own scrollback, which is why
/// this needs no viewport bookkeeping at all.
///
/// The one piece of state is whether the cursor sits mid-line. A
/// tool chip must break a half-finished sentence but must not open
/// a blank row when the turn began with a tool call, and its
/// result belongs on the line below the chip, not appended to it.
struct Painter {
    mid_line: std::cell::Cell<bool>,
    markdown: std::cell::RefCell<glass_tui::markdown::Stream>,
    /// Terminal width: tool commands and results are
    /// truncated to it, because a wrapped command is
    /// noise and the full text lives in the transcript.
    width: std::cell::Cell<u16>,
    waiting: std::cell::Cell<bool>,
    tool_started: std::cell::Cell<Option<std::time::Instant>>,
    tool_name: std::cell::RefCell<String>,
    tool_runs:
        std::cell::RefCell<std::collections::HashMap<String, (String, String, std::time::Instant)>>,
    response_started: std::cell::Cell<bool>,
    response_prefix: std::cell::RefCell<String>,
    waiting_started: std::cell::Cell<Option<std::time::Instant>>,
    last_tick: std::cell::Cell<Option<std::time::Instant>>,
    activity_frame: std::cell::Cell<usize>,
    live: std::cell::RefCell<live::Live>,
    reader: std::cell::RefCell<Option<reader::Reader>>,
    reader_dirty: std::cell::Cell<bool>,
    reader_last_sync: std::cell::Cell<Option<std::time::Instant>>,
    last_size: std::cell::Cell<(u16, u16)>,
    palette_was_active: std::cell::Cell<bool>,
    compact_tools: std::cell::Cell<bool>,
    team_owner: std::cell::RefCell<String>,
}

impl Painter {
    fn new() -> Self {
        Self {
            mid_line: std::cell::Cell::new(false),
            markdown: std::cell::RefCell::new(glass_tui::markdown::Stream::default()),
            width: std::cell::Cell::new(80),
            waiting: std::cell::Cell::new(false),
            tool_started: std::cell::Cell::new(None),
            tool_name: std::cell::RefCell::new(String::new()),
            tool_runs: std::cell::RefCell::new(std::collections::HashMap::new()),
            response_started: std::cell::Cell::new(false),
            response_prefix: std::cell::RefCell::new(String::new()),
            waiting_started: std::cell::Cell::new(None),
            last_tick: std::cell::Cell::new(None),
            activity_frame: std::cell::Cell::new(0),
            live: std::cell::RefCell::new(live::Live::new()),
            reader: std::cell::RefCell::new(None),
            reader_dirty: std::cell::Cell::new(false),
            reader_last_sync: std::cell::Cell::new(None),
            last_size: std::cell::Cell::new(term_size()),
            palette_was_active: std::cell::Cell::new(false),
            compact_tools: std::cell::Cell::new(false),
            team_owner: std::cell::RefCell::new(String::new()),
        }
    }

    fn set_width(&self, width: u16) {
        // Reserve the same two columns as the inline reader and scrollbar.
        self.width.set(width.saturating_sub(2).max(1));
    }

    fn refresh_geometry(&self, reader_allowed: bool) -> bool {
        if self.live.borrow().active() {
            return false;
        }
        let current = term_size();
        if self.last_size.get() == current {
            return false;
        }
        self.set_width(current.0);
        let _ = surface::body_rect();
        if reader_allowed && let Some(reader) = self.reader.borrow_mut().as_mut() {
            let _ = reader.resize();
        }
        if reader_allowed || !self.reader_active() {
            self.last_size.set(current);
        }
        true
    }

    /// The repl writes around the painter — the usage line —
    /// and those end in a newline, so the cursor is never
    /// mid-line across one.
    fn reset(&self) {
        self.response_started.set(false);
        self.response_prefix.borrow_mut().clear();
        self.tool_started.set(None);
        self.tool_runs.borrow_mut().clear();
        self.waiting_started.set(None);
        self.last_tick.set(None);
        self.mid_line.set(false);
    }

    /// Close a span the model never closed, and land the cursor
    /// on a fresh row: the usage line that follows a turn must
    /// not append to half a sentence.
    fn flush(&self, io: &mut impl Write) {
        if self.waiting.replace(false) {
            let _ = surface::status("");
        }
        self.markdown
            .borrow_mut()
            .flush(io, self.width.get() as usize);
        self.mid_line.set(false);
        self.response_started.set(false);
        self.response_prefix.borrow_mut().clear();
    }

    fn handle(&self, p: Progress<'_>) {
        let before = self.live.borrow().content_revision();
        self.live.borrow_mut().progress(&p);
        if self.live.borrow().content_revision() != before && self.reader_active() {
            self.reader_dirty.set(true);
        }
        self.set_width(term_size().0);
        let mut io = glass_tui::theme::TerminalWriter::new(surface::BodyWriter);
        if self.waiting.replace(false) {
            let _ = surface::status("");
        }
        self.render_text_transition(&mut io, &p);
        match p {
            Progress::Waiting => {
                self.flush(&mut io);
                self.waiting_started.set(Some(std::time::Instant::now()));
                self.activity_frame.set(0);
                self.last_tick.set(Some(std::time::Instant::now()));
                self.waiting.set(true);
                self.draw_activity(&mut io);
            }
            Progress::Reasoning(_) => {
                self.waiting.set(true);
                self.draw_activity(&mut io);
            }
            Progress::Text(_) => {
                self.waiting.set(true);
                self.draw_activity(&mut io);
            }
            Progress::ToolStart {
                call_id,
                name,
                command,
            } => {
                self.tool_runs.borrow_mut().insert(
                    call_id.to_string(),
                    (
                        name.to_string(),
                        command.unwrap_or("").to_string(),
                        std::time::Instant::now(),
                    ),
                );
                self.tool_started.set(Some(std::time::Instant::now()));
                *self.tool_name.borrow_mut() = format!(
                    "{} running{}",
                    name,
                    command
                        .map(|c| format!(" · {}", glass_tui::markdown::sanitize(c)))
                        .unwrap_or_default()
                );
                self.waiting_started.set(Some(std::time::Instant::now()));
                self.activity_frame.set(0);
                self.last_tick.set(Some(std::time::Instant::now()));
                self.waiting.set(true);
                self.draw_activity(&mut io);
            }
            Progress::ToolEnd {
                call_id,
                summary,
                output,
            } => {
                let (name, command, started) = self
                    .tool_runs
                    .borrow_mut()
                    .remove(call_id)
                    .unwrap_or_else(|| ("tool".into(), String::new(), std::time::Instant::now()));
                let remaining = self.tool_runs.borrow().values().next().cloned();
                self.tool_started
                    .set(remaining.as_ref().map(|(_, _, start)| *start));
                if let Some((name, command, start)) = remaining {
                    *self.tool_name.borrow_mut() = format!("{name} running · {command}");
                    self.waiting_started.set(Some(start));
                }
                let elapsed = started.elapsed().as_secs_f32();
                let status = glass_tui::tool_output::status(&summary);
                let failed = !matches!(status.as_str(), "exit 0" | "completed");
                let margin = if self.width.get() >= 24 { 2 } else { 1 };
                let width = self.width.get().saturating_sub(margin + 1).max(1) as usize;
                for row in glass_tui::tool_output::lines(
                    &name,
                    &command,
                    failed,
                    elapsed,
                    if failed && output.is_empty() {
                        &summary
                    } else {
                        output
                    },
                    width,
                    self.compact_tools.get(),
                ) {
                    let _ = write!(io, "{}", " ".repeat(margin as usize));
                    glass_tui::markdown::write_ansi(&mut io, &row);
                    let _ = write!(io, "\r\n");
                }
                self.mid_line.set(false);
                if self.tool_started.get().is_some() {
                    self.waiting.set(true);
                    self.draw_activity(&mut io);
                }
            }
            Progress::BlockedByGuard(paths) => {
                self.flush(&mut io);
                self.notice(
                    &mut io,
                    &glass_tui::theme::ansi(glass_tui::theme::Tone::Error),
                    &format!("× refused to write unread file(s): {paths}"),
                );
            }
            Progress::Steered(text) => {
                self.flush(&mut io);
                self.notice(
                    &mut io,
                    &glass_tui::theme::ansi(glass_tui::theme::Tone::Warning),
                    &format!("⚡ applying your correction: {text}"),
                );
            }
            Progress::Retrying {
                attempt,
                delay_s,
                reason,
            } => {
                self.flush(&mut io);
                self.notice(
                    &mut io,
                    &glass_tui::theme::ansi(glass_tui::theme::Tone::Warning),
                    &format!(
                        "⟳ retry {attempt} in {delay_s}s — {} · /status /retry",
                        live::sanitize_provider_detail(&reason)
                    ),
                );
            }
            Progress::ProviderNotice(text) => {
                self.flush(&mut io);
                let cooldown = text.starts_with("Provider cooldown");
                if !cooldown {
                    self.waiting.set(false);
                }
                let detail = live::sanitize_provider_detail(text);
                let notice = if cooldown {
                    detail
                } else if text.starts_with("Tool repair") {
                    format!("⟳ {detail}")
                } else if text.starts_with("Reasoning-only") {
                    detail
                } else {
                    format!("Provider fallback — {detail} · /status /model")
                };
                self.notice(
                    &mut io,
                    &glass_tui::theme::ansi(glass_tui::theme::Tone::Warning),
                    &notice,
                );
            }
            Progress::ProviderFailure(_) => {
                // The turn result prints and persists one canonical error.
                self.flush(&mut io);
                self.waiting.set(false);
            }
            Progress::Compacted { kept, dropped } => {
                self.flush(&mut io);
                self.notice(
                    &mut io,
                    &glass_tui::theme::ansi(glass_tui::theme::Tone::Muted),
                    &format!("◆ context compacted · dropped {dropped}, kept {kept}"),
                );
            }
        }
        let _ = io.flush();
    }

    fn tick(&self) {
        if self.reader_dirty.get()
            && self
                .reader_last_sync
                .get()
                .is_none_or(|last| last.elapsed() >= std::time::Duration::from_millis(125))
        {
            self.sync_reader();
        }
        if !self.waiting.get() {
            return;
        }
        if self.last_tick.get().is_some_and(|last| {
            last.elapsed()
                < std::time::Duration::from_millis(glass_tui::theme::ACTIVITY_INTERVAL_MS)
        }) {
            return;
        }
        self.last_tick.set(Some(std::time::Instant::now()));
        self.activity_frame
            .set(self.activity_frame.get().wrapping_add(1));
        self.set_width(term_size().0);
        let mut io = glass_tui::theme::TerminalWriter::new(surface::BodyWriter);
        self.draw_activity(&mut io);
        let _ = io.flush();
    }

    fn sync_reader(&self) {
        // A fullscreen live view owns these cells until it closes.
        if self.live.borrow().active() {
            self.reader_dirty.set(true);
            return;
        }
        if surface::reader_invalidated()
            && let Some(reader) = self.reader.borrow_mut().as_mut()
        {
            reader.restore_after_overlay().ok();
        }
        let entries = self.live.borrow().snapshot();
        if let Some(reader) = self.reader.borrow_mut().as_mut()
            && reader.update(entries).is_ok()
        {
            self.reader_dirty.set(false);
            self.reader_last_sync.set(Some(std::time::Instant::now()));
        }
    }

    fn seed_live(&self, entries: Vec<glass_tui::Entry>, reasoning: String) {
        let mut live = self.live.borrow_mut();
        live.seed(entries);
        live.seed_reasoning(reasoning);
        drop(live);
        if self.reader_active() {
            self.reader_dirty.set(true);
            self.sync_reader();
        }
    }

    fn welcome_back(&self, session: &Session) {
        if let Some(text) = recap::cached(session) {
            self.append_live_entry(glass_tui::Role::System, text.clone());
            let mut io = surface::BodyWriter;
            for (index, section) in text.split("\n\n").enumerate() {
                let tone = if index == 0 {
                    glass_tui::theme::Tone::Accent
                } else if section.starts_with("Blocker:") {
                    glass_tui::theme::Tone::Warning
                } else if section.starts_with("Next:") {
                    glass_tui::theme::Tone::Secondary
                } else if section.starts_with("Saved conversation") {
                    glass_tui::theme::Tone::Muted
                } else {
                    glass_tui::theme::Tone::Body
                };
                if index > 0 {
                    let _ = writeln!(io);
                }
                self.notice(&mut io, &glass_tui::theme::ansi(tone), section);
            }
        }
    }

    fn append_live_entry(&self, role: glass_tui::Role, text: impl Into<String>) {
        self.live.borrow_mut().append_entry(role, text);
        if self.reader_active() {
            self.reader_dirty.set(true);
            self.sync_reader();
        }
    }

    fn open_reader(&self, entries: Vec<glass_tui::Entry>, page_up: bool) -> Result<(), String> {
        if self.reader.borrow().is_some() {
            return Ok(());
        }
        surface::reading(true).map_err(|e| e.to_string())?;
        match reader::Reader::new(entries, page_up) {
            Ok(reader) => {
                *self.reader.borrow_mut() = Some(reader);
                self.reader_dirty.set(false);
                self.reader_last_sync.set(Some(std::time::Instant::now()));
                Ok(())
            }
            Err(error) => {
                surface::reading(false).ok();
                Err(error.to_string())
            }
        }
    }

    fn open_reader_search(&self, entries: Vec<glass_tui::Entry>) -> Result<(), String> {
        use glass_tui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
        self.open_reader(entries, false)?;
        let event = Event::Key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        if self.reader_event(&event, false)? {
            Ok(())
        } else {
            Err("reader search did not accept its initial search key".into())
        }
    }

    fn close_reader(&self) -> Result<(), String> {
        self.reader.borrow_mut().take();
        self.reader_dirty.set(false);
        surface::reading(false).map_err(|e| e.to_string())
    }

    fn reader_active(&self) -> bool {
        self.reader.borrow().is_some()
    }

    fn reader_needs_redraw(&self, palette_active: bool) -> bool {
        let palette_closed = self.palette_was_active.replace(palette_active) && !palette_active;
        if palette_closed {
            // Managed palettes paint over the application screen. Restore the
            // body as well as the composer when any dismissal path closes one.
            let _ = surface::overlay(false);
        }
        palette_closed || surface::reader_invalidated()
    }

    /// Returns true when the event belongs exclusively to the inline reader.
    fn reader_event(
        &self,
        event: &glass_tui::crossterm::event::Event,
        composer_has_draft: bool,
    ) -> Result<bool, String> {
        use reader::Action;
        let action = {
            let mut slot = self.reader.borrow_mut();
            let Some(reader) = slot.as_mut() else {
                return Ok(false);
            };
            reader
                .event(event, composer_has_draft)
                .map_err(|e| e.to_string())?
        };
        match action {
            Action::Handled => Ok(true),
            Action::Close => {
                self.close_reader()?;
                Ok(true)
            }
            Action::CloseAndPass => {
                self.close_reader()?;
                Ok(false)
            }
            Action::PassThrough => Ok(false),
        }
    }
    fn draw_activity(&self, io: &mut impl Write) {
        let frame = self.activity_frame.get();
        let phase = self.live.borrow().phase().to_string();
        let label = if self.tool_started.get().is_some() {
            format!("{}", self.tool_name.borrow())
        } else if phase == "Answer streaming" {
            phase.clone()
        } else if phase == "Provider reasoning" {
            "Reasoning".into()
        } else if phase == "Provider cooldown" && self.width.get() < 32 {
            "Cooldown".into()
        } else if self.width.get() < 32 {
            glass_tui::text::clip(&phase, 22)
        } else {
            phase
        };
        let elapsed = self
            .waiting_started
            .get()
            .map(|start| start.elapsed().as_secs_f32())
            .unwrap_or(0.0);
        let provider_detail = self.live.borrow().provider_detail().map(str::to_string);
        let detail = if let Some(detail) = provider_detail {
            format!("{label} · {detail}")
        } else {
            format!("{label} · {elapsed:.1}s")
        };
        let content = first_line(
            &format!("{} {detail}", glass_tui::theme::frame(frame)),
            self.width.get().saturating_sub(3) as usize,
        );
        let _ = io;
        let _ = surface::status_colored(
            &format!(
                "  {content}{}",
                team::activity_suffix(&self.team_owner.borrow())
            ),
            &glass_tui::theme::activity_color(frame),
        );
    }

    fn notice(&self, io: &mut impl Write, color: &str, text: &str) {
        let io = &mut glass_tui::theme::TerminalWriter::new(io);
        let width = self.width.get().saturating_sub(3).max(1) as usize;
        for row in glass_tui::text::wrap_words(&glass_tui::markdown::sanitize(text), width) {
            let _ = writeln!(io, "  {color}{row}\x1b[0m");
        }
        self.mid_line.set(false);
    }

    fn write_text(&self, io: &mut impl Write, text: &str) {
        let text = if self.response_started.get() {
            text.to_string()
        } else {
            let mut prefix = self.response_prefix.borrow_mut();
            prefix.push_str(text);
            let Some(first) = prefix.find(|c: char| !c.is_whitespace()) else {
                if prefix.len() > 8192 {
                    let keep = prefix.rfind('\n').map(|i| i + 1).unwrap_or_else(|| {
                        let chars = prefix.chars().count().saturating_sub(256);
                        prefix.char_indices().nth(chars).map_or(0, |(i, _)| i)
                    });
                    prefix.drain(..keep);
                }
                return;
            };
            self.response_started.set(true);
            let start = prefix[..first].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let visible = prefix[start..].to_string();
            prefix.clear();
            let _ = io.write_all(b"\r\n");
            visible
        };
        self.markdown
            .borrow_mut()
            .write(io, &text, self.width.get() as usize);
        self.mid_line.set(!text.ends_with('\n'));
    }

    fn render_text_transition(&self, io: &mut impl Write, progress: &Progress<'_>) {
        match progress {
            Progress::Text(text) => self.write_text(io, text),
            Progress::ToolStart { .. } | Progress::ToolEnd { .. } => self.flush(io),
            _ => {}
        }
    }
}

fn first_line(s: &str, max: usize) -> String {
    let line = s.lines().next().unwrap_or("").trim();
    let mut out = glass_tui::text::clip(line, max);
    if glass_tui::text::width(line) > max {
        out.push('…');
    }
    out
}

/// Terminal size, with a sane fallback.
///
/// Some mobile terminals report 0x0 while a resize
/// is in flight; a zero dimension would wrap the
/// editor at 0 columns and park the prompt on row 0.
fn term_size() -> (u16, u16) {
    let (w, h) = glass_tui::crossterm::terminal::size().unwrap_or((80, 24));
    (if w == 0 { 80 } else { w }, if h == 0 { 24 } else { h })
}

fn print_banner(
    cfg: &config::Config,
    loaded: &[Box<dyn glass_mod_api::Mod_>],
    exts: &ext::Registry,
    session: &Session,
    narrow: bool,
) {
    let width = usize::from(term_size().0.max(1));
    let accent = glass_tui::theme::accent();
    ui_println!(
        "{accent}\x1b[1m{}\x1b[0m",
        glass_tui::text::clip("◆ glass-core", width)
    );
    let rows = if narrow {
        vec![cfg.model.clone(), " / commands · ctrl-g view".into()]
    } else {
        vec![
            format!("{} · {}", cfg.provider, cfg.model),
            format!(
                "{} mods · {} extensions · session {}",
                loaded.len(),
                exts.names().len(),
                session.id
            ),
            "/dashboard overview · / commands · ctrl-g transcript · ctrl-d quit".into(),
        ]
    };
    for row in rows {
        ui_println!("\x1b[90m{}\x1b[0m", glass_tui::text::clip(&row, width));
    }
    ui_println!();
}

/// Fullscreen transcript viewer.
fn view_transcript(
    session: &Session,
    loaded: &[Box<dyn glass_mod_api::Mod_>],
    keys: &input::KeyReader,
    page_up: bool,
) -> Result<(), String> {
    use glass_tui::crossterm::execute;
    use glass_tui::ratatui::Terminal;

    surface::overlay(true).map_err(|e| e.to_string())?;
    let mut transcript = glass_tui::Transcript::new(session.history_entries());
    if page_up {
        transcript.page(-1, term_size().1.saturating_sub(2));
    }
    let mut tui = Terminal::new(glass_tui::ratatui::backend::CrosstermBackend::new(
        glass_tui::theme::TerminalWriter::new(std::io::stdout()),
    ))
    .map_err(|e| format!("cannot start viewer: {e}"))?;

    // Alternate screen only here. The default view never touches it, which is
    // what keeps history in the terminal instead of the heap.
    execute!(
        std::io::stdout(),
        glass_tui::crossterm::terminal::Clear(glass_tui::crossterm::terminal::ClearType::All),
        glass_tui::crossterm::cursor::Hide
    )
    .map_err(|e| e.to_string())?;
    tui.hide_cursor().ok();

    let res = (|| -> Result<(), String> {
        use glass_tui::crossterm::event::{Event, KeyCode, KeyModifiers};
        loop {
            // Viewport height is needed to clamp paging against the real
            // window, which can change between frames.
            let height = tui
                .size()
                .map(|r| glass_tui::Transcript::content_rect(r.into()).height)
                .unwrap_or(22);
            tui.draw(|f| transcript.draw(f, loaded, &session.id))
                .map_err(|e| e.to_string())?;

            match keys.next_blocking().ok_or("input stream closed")? {
                Event::Key(k) => {
                    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
                    match k.code {
                        KeyCode::Esc => break,
                        KeyCode::Char('q') => break,
                        KeyCode::Char('c') if ctrl => break,
                        KeyCode::Char('g') if ctrl => break,
                        KeyCode::Up => transcript.step(-1, height),
                        KeyCode::Down => transcript.step(1, height),
                        KeyCode::PageUp => transcript.page(-1, height),
                        KeyCode::PageDown => transcript.page(1, height),
                        KeyCode::Home => transcript.top(),
                        KeyCode::End => transcript.follow_tail(height),
                        KeyCode::Enter => transcript.toggle_collapse(),
                        KeyCode::Char('f') => transcript.follow_tail(height),
                        KeyCode::Char('t') => transcript.jump_to_turn(1, height),
                        _ => {}
                    }
                }
                Event::Resize(_, _) => tui.autoresize().map_err(|e| e.to_string())?,
                _ => {}
            }
        }
        Ok(())
    })();

    tui.show_cursor().ok();
    execute!(
        std::io::stdout(),
        glass_tui::crossterm::cursor::Show,
        glass_tui::crossterm::terminal::Clear(glass_tui::crossterm::terminal::ClearType::All)
    )
    .map_err(|e| e.to_string())?;
    surface::overlay(false).map_err(|e| e.to_string())?;
    res
}

#[cfg(test)]
mod totals_tests {
    use super::{Totals, compact, percentage, thousands};
    use crate::provider::Usage;

    fn u(input: u32, read: u32, write: u32, output: u32) -> Usage {
        Usage {
            input,
            cache_read: read,
            cache_write: write,
            output,
        }
    }

    #[test]
    fn accumulates_across_turns() {
        let mut t = Totals::new();
        t.add(&u(100, 900, 100, 50));
        t.add(&u(0, 990, 0, 30));
        assert_eq!(t.turns, 2);
        assert_eq!(t.prompt_total(), 100 + 900 + 100 + 990);
        assert_eq!(t.output, 80);
    }

    #[test]
    fn context_fill_uses_last_request_not_session_spend() {
        let mut t = Totals::new();
        t.add(&u(900, 0, 0, 100));
        t.add(&u(100, 0, 0, 10));
        let line = t.suffix(1000, None, false);
        assert!(line.contains("10% ctx"), "{line}");
        assert!(line.contains("1,110 total tokens"), "{line}");
    }

    #[test]
    fn compact_abbreviates_large_counts() {
        assert_eq!(compact(9_999), "9,999");
        assert_eq!(compact(12_345), "12.3k");
        assert_eq!(compact(2_500_000), "2.5M");
        assert_eq!(compact(u64::MAX), "18446744073709.6M");
        assert_eq!(thousands(u64::MAX), "18,446,744,073,709,551,615");
    }

    #[test]
    fn percentage_rounds_and_caps_without_float_narrowing() {
        assert_eq!(percentage(1, 3, 100), 33);
        assert_eq!(percentage(2, 3, 100), 67);
        assert_eq!(percentage(u64::MAX, 1, 999), 999);
        assert_eq!(percentage(1, 0, 100), 0);
    }

    #[test]
    fn accumulated_counts_saturate_instead_of_wrapping() {
        let mut totals = Totals::new();
        totals.input = u64::MAX - u64::from(u32::MAX);
        totals.add(&u(u32::MAX, 0, 0, 0));
        assert_eq!(totals.input, u64::MAX);
    }

    #[test]
    fn the_session_tail_prices_the_session() {
        use crate::pricing::RateCard;
        let mut t = Totals::new();
        t.add(&u(1_000_000, 0, 0, 0));
        let card = RateCard {
            input: 5.0,
            cache_read: 0.5,
            cache_write: 6.25,
            output: 25.0,
        };
        assert!((t.total_cost(&card) - 5.0).abs() < 1e-9);
        let s = t.suffix(200_000, Some(&card), false);
        assert!(s.contains("$5.00"), "{s}");
        // Without a card the tail stays tokens-only.
        assert!(!t.suffix(200_000, None, false).contains('$'));
    }

    #[test]
    fn the_usage_line_prices_a_turn() {
        use crate::pricing::RateCard;
        let card = RateCard {
            input: 5.0,
            cache_read: 0.5,
            cache_write: 6.25,
            output: 25.0,
        };
        let line = super::usage_line(&u(1_000_000, 0, 0, 0), Some(&card), false);
        assert!(line.contains("$5.00"), "{line}");
        assert!(!super::usage_line(&u(1_000_000, 0, 0, 0), None, false).contains('$'));
        // Narrow counts are compact, so the line stays
        // inside a phone-width terminal.
        let narrow = super::usage_line(&u(1_000_000, 0, 0, 0), Some(&card), true);
        assert!(narrow.contains("1.0M input"), "{narrow}");
        assert!(narrow.chars().count() < 72, "{narrow}");
    }
}

#[cfg(test)]
mod session_list_tests {
    use super::{civil_from_days, first_user_message, format_time};

    #[test]
    fn civil_dates_are_correct() {
        // 1970-01-01 is day 0.
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2024-02-29 is a leap day.
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        // 2000-01-01.
        assert_eq!(civil_from_days(10_957), (2000, 1, 1));
        // 2026-10-06.
        assert_eq!(civil_from_days(20_732), (2026, 10, 6));
    }

    #[test]
    fn format_time_renders_utc() {
        use std::time::{Duration, SystemTime, UNIX_EPOCH};
        // 1970-01-01T00:00:00Z plus 3661 seconds.
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(3_661);
        assert_eq!(format_time(t), "1970-01-01 01:01");
        let _ = UNIX_EPOCH;
    }

    #[test]
    fn first_user_message_skips_other_records() {
        use std::io::Write;
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("glass-list-{}", n));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("s.jsonl");
        let mut f = std::fs::File::create(&p).unwrap();
        writeln!(f, r#"{{"k":"assistant","t":"answer"}}"#).unwrap();
        writeln!(f, r#"{{"k":"user","t":"the task"}}"#).unwrap();
        writeln!(f, r#"{{"k":"user","t":"second"}}"#).unwrap();
        drop(f);
        assert_eq!(first_user_message(&p).as_deref(), Some("the task"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn first_user_message_tolerates_a_torn_log() {
        use std::io::Write;
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("glass-list-torn-{}", n));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("s.jsonl");
        let mut f = std::fs::File::create(&p).unwrap();
        writeln!(f, "{{not json").unwrap();
        writeln!(f, r#"{{"k":"user","t":"real"}}"#).unwrap();
        drop(f);
        assert_eq!(first_user_message(&p).as_deref(), Some("real"));
        std::fs::remove_dir_all(&d).ok();
    }
}

#[cfg(test)]
mod painter_tests {
    use super::*;

    fn paint(p: &Painter, chunks: &[&str]) -> String {
        let mut out = Vec::new();
        for c in chunks {
            p.write_text(&mut out, c);
        }
        p.flush(&mut out);
        String::from_utf8(out).unwrap()
    }

    /// The text a terminal would show, with SGR color runs removed.
    ///
    /// Assertions about layout must not depend on the theme: once the
    /// gutter and the code have different colors, the raw bytes carry
    /// escapes between them and a literal match would fail on styling
    /// rather than on placement.
    fn plain(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut rest = s;
        while let Some(i) = rest.find('\x1b') {
            out.push_str(&rest[..i]);
            rest = &rest[i..];
            if rest.as_bytes().get(1) == Some(&b'[') {
                // CSI: ESC '[' params… final byte in @..~ (params are < '@').
                if let Some(end) = rest[2..].find(|c: char| ('@'..='~').contains(&c)) {
                    rest = &rest[end + 3..];
                } else {
                    rest = "";
                }
            } else {
                // A two-byte escape; nothing else is expected from the painter.
                rest = rest.get(2..).unwrap_or("");
            }
        }
        out.push_str(rest);
        out
    }

    #[test]
    fn prose_is_neutral_with_mobile_margins() {
        let out = paint(&Painter::new(), &["just prose"]);
        assert_eq!(
            out.contains("\x1b[38;5;252m"),
            !glass_tui::theme::no_color()
        );
        assert!(out.contains("just prose"));
        assert!(out.ends_with("\r\n"));
    }

    #[test]
    fn first_response_discards_only_leading_blank_rows_and_keeps_code_indent() {
        let p = Painter::new();
        let mut out = Vec::new();
        p.write_text(&mut out, "\n\n  ");
        p.write_text(&mut out, "```rust\n    fn main() {}\n```\n");
        p.flush(&mut out);
        let out = String::from_utf8(out).unwrap();
        assert!(!out.contains("Assistant"));
        assert!(!out.contains("\r\n\r\n"));
        assert!(plain(&out).contains("│     fn main() {}"), "{out:?}");
    }

    #[test]
    fn composed_prompt_and_first_text_have_one_blank_boundary() {
        let mut out = Vec::new();
        render_prompt(&mut out, "hello", 40);
        let p = Painter::new();
        p.render_text_transition(&mut out, &Progress::Text("\n\nReply."));
        p.flush(&mut out);
        let out = String::from_utf8(out).unwrap();
        let prompt = out.find("❯ hello").unwrap();
        let reply = out.find("Reply.").unwrap();
        let between = &out[prompt..reply];
        assert_eq!(
            between.matches("\r\n").count(),
            2,
            "one prompt end plus one blank row: {out:?}"
        );

        if let Some(dir) = std::env::var_os("GC_TEST_RENDER_CAPTURE_DIR") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            for (width, height) in [(65, 30), (40, 24), (24, 16), (20, 12), (20, 8)] {
                let painter = Painter::new();
                painter.set_width(width);
                let mut capture = Vec::new();
                render_prompt(&mut capture, "inspect the task", width);
                painter.render_text_transition(
                    &mut capture,
                    &Progress::Text("\n\nFirst reply with **Markdown**."),
                );
                painter.render_text_transition(
                    &mut capture,
                    &Progress::ToolStart {
                        call_id: "test-call",
                        name: "bash",
                        command: Some("pwd"),
                    },
                );
                painter.render_text_transition(
                    &mut capture,
                    &Progress::ToolEnd {
                        call_id: "test-call",
                        summary: "done".into(),
                        output: "workspace",
                    },
                );
                painter.render_text_transition(&mut capture, &Progress::Text("\n\nFinal reply."));
                painter.flush(&mut capture);
                std::fs::write(dir.join(format!("painter-{width}x{height}.ansi")), capture)
                    .unwrap();
            }
        }
    }

    #[test]
    fn streamed_tool_start_end_keep_one_compact_reply_boundary() {
        let p = Painter::new();
        let mut out = Vec::new();
        p.render_text_transition(&mut out, &Progress::Text("First response."));
        p.render_text_transition(
            &mut out,
            &Progress::ToolStart {
                call_id: "test-call",
                name: "bash",
                command: Some("pwd"),
            },
        );
        p.render_text_transition(
            &mut out,
            &Progress::ToolEnd {
                call_id: "test-call",
                summary: "done".into(),
                output: "workspace",
            },
        );
        p.render_text_transition(&mut out, &Progress::Text("\n\nFinal response."));
        p.flush(&mut out);
        let out = String::from_utf8(out).unwrap();
        assert!(!out.contains("Assistant"));
        assert_eq!(
            out.matches("\r\n\r\n").count(),
            1,
            "one ToolEnd/reply boundary: {out:?}"
        );
        assert!(out.contains("First response."));
        assert!(out.contains("Final response."));
    }

    #[test]
    fn oversized_unicode_whitespace_prefix_stays_bounded_and_safe() {
        let p = Painter::new();
        let mut out = Vec::new();
        p.write_text(&mut out, &"\u{2003}".repeat(9000));
        p.write_text(&mut out, "reply");
        p.flush(&mut out);
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("reply"));
        assert!(!out.contains("Assistant"));
    }

    #[test]
    fn first_wheel_opens_reader_only_when_no_other_view_owns_input() {
        use glass_tui::crossterm::event::{
            Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind,
        };
        let wheel = Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        let key = Event::Key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        assert!(should_open_reader_for_wheel(&wheel, false, false));
        assert!(!should_open_reader_for_wheel(&wheel, true, false));
        assert!(!should_open_reader_for_wheel(&wheel, false, true));
        assert!(!should_open_reader_for_wheel(&key, false, false));
    }
    #[test]
    fn chunk_split_styles_are_closed_and_markers_hidden() {
        let out = paint(&Painter::new(), &["run `l", "s` and **bo", "ld** now"]);
        assert_eq!(
            out.contains("\x1b[38;5;114m"),
            !glass_tui::theme::no_color()
        );
        assert!(out.contains("\x1b[1m"));
        assert!(!out.contains('`') && !out.contains("**"));
    }
    #[test]
    fn flush_preserves_unclosed_text_and_breaks_the_row() {
        let out = paint(&Painter::new(), &["run `ls"]);
        assert!(out.contains("`ls"));
        assert!(out.ends_with("\r\n"));
    }
    #[test]
    fn fenced_code_is_a_distinct_block() {
        let out = paint(&Painter::new(), &["```sh\nls\n```"]);
        assert!(out.contains("╭─ sh"));
        assert!(out.contains("│ "));
        assert!(!out.contains("```"));
    }
}

#[cfg(test)]
mod budget_context_tests {
    use super::*;

    #[test]
    fn poisoned_budget_context_returns_an_error_instead_of_panicking() {
        let context = std::sync::Mutex::new((
            std::sync::Arc::new(std::sync::Mutex::new(budget::Ledger::default())),
            std::sync::Arc::new(std::sync::Mutex::new(std::path::PathBuf::new())),
        ));
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = context.lock().unwrap();
            panic!("poison the test context");
        }));
        assert!(panic.is_err());
        assert!(matches!(
            budget_context_snapshot(&context),
            Err(message) if message == "budget context lock poisoned"
        ));
    }
}
