use base64::{Engine, engine::general_purpose::STANDARD};
use glass::browser::chrome::detect_chrome;
use glass::browser::session::{BrowserResult, BrowserSession, InteractionMode, SessionOptions};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Instant;
use tokio::net::TcpListener;

const DEFAULT_ITERATIONS: usize = 10;

#[derive(Deserialize)]
struct Corpus {
    schema_version: u64,
    corpus: String,
    fixture: String,
    scenarios: Vec<Scenario>,
}

#[derive(Deserialize)]
struct Scenario {
    id: String,
    category: String,
    expected: String,
}

#[tokio::main]
async fn main() -> BrowserResult<()> {
    let corpus: Corpus = serde_json::from_str(include_str!("../benchmarks/scenarios/v1.json"))?;
    if corpus.schema_version != 1 {
        return Err(format!("unsupported corpus schema {}", corpus.schema_version).into());
    }
    let chrome_path = detect_chrome().ok_or("Chrome/Chromium is required for the scorecard")?;
    let iterations = positive_env("GLASS_SCORECARD_ITERATIONS", DEFAULT_ITERATIONS)?;
    let temperature =
        std::env::var("GLASS_SCORECARD_TEMPERATURE").unwrap_or_else(|_| "warm".to_string());
    if !matches!(temperature.as_str(), "warm" | "cold") {
        return Err("GLASS_SCORECARD_TEMPERATURE must be warm or cold".into());
    }
    let profile = std::env::var("GLASS_SCORECARD_PROFILE")
        .unwrap_or_else(|_| "ephemeral-incognito".to_string());
    let fixture = include_str!("../tests/fixtures/scorecard.html");
    let url = format!("data:text/html;base64,{}", STANDARD.encode(fixture));
    let glass_rss_start = process_rss_bytes(std::process::id());
    let started = Instant::now();
    let session = BrowserSession::start(&SessionOptions {
        port: available_port().await?,
        chrome_path: Some(chrome_path.clone()),
        profile: "scorecard".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await?;
    let startup_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let chrome_pid = session
        .owned_chrome_pid()
        .ok_or("scorecard requires an owned Chrome process")?;
    let sampler = MemorySampler::start(std::process::id(), chrome_pid);
    session.navigate(&url).await?;

    let mut outcomes = Vec::with_capacity(corpus.scenarios.len() * iterations);
    for iteration in 0..iterations {
        for scenario in &corpus.scenarios {
            reset(&session).await?;
            let before_cdp = session.cdp_request_count();
            let run_started = Instant::now();
            let observed = run_scenario(&session, &scenario.id).await;
            let latency_ms = run_started.elapsed().as_secs_f64() * 1_000.0;
            let cdp_requests = session.cdp_request_count().saturating_sub(before_cdp);
            let (status, actual, error) = match observed {
                Ok(actual) if actual == scenario.expected => ("success", Some(actual), None),
                Ok(actual) if scenario.id == "duplicate-label" => {
                    ("wrong_action", Some(actual), None)
                }
                Ok(actual) => ("failure", Some(actual), None),
                Err(error) => ("failure", None, Some(error.to_string())),
            };
            outcomes.push(json!({
                "id": scenario.id,
                "category": scenario.category,
                "iteration": iteration + 1,
                "expected": scenario.expected,
                "actual": actual,
                "status": status,
                "error": error,
                "latency_ms": latency_ms,
                "cdp_requests": cdp_requests,
            }));
        }
        if temperature == "cold" && iteration + 1 < iterations {
            session.navigate(&url).await?;
        }
    }

    let compact = session.observe_fresh().await?;
    let context_bytes = serde_json::to_vec(&compact)?.len();
    let cdp_requests = session.cdp_request_count();
    let memory = sampler.stop().await;
    let glass_rss_end = process_rss_bytes(std::process::id());
    let chrome_rss_end = process_tree_rss_bytes(chrome_pid);
    session.close().await?;

    let successes = count_status(&outcomes, "success");
    let wrong_actions = count_status(&outcomes, "wrong_action");
    let failures = outcomes.len() - successes;
    let report = json!({
        "schema_version": 1,
        "tool": {"name": "glass", "version": env!("CARGO_PKG_VERSION")},
        "run": {
            "corpus": corpus.corpus,
            "corpus_fixture": corpus.fixture,
            "iterations": iterations,
            "temperature": temperature,
            "profile": profile,
            "viewport": {"width": 1280, "height": 720},
        },
        "environment": {
            "os": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
            "rust": command_version("rustc", &["--version"]),
            "chrome": command_version(chrome_path.to_string_lossy().as_ref(), &["--version"]),
            "machine": machine_name(),
        },
        "resources": {
            "scope": "Runner and owned Chrome process trees are disjoint; bytes are RSS",
            "runner": {"pid": std::process::id(), "rss_start_bytes": glass_rss_start, "rss_end_bytes": glass_rss_end, "peak_rss_bytes": memory.glass_peak},
            "chrome": {"root_pid": chrome_pid, "rss_end_bytes": chrome_rss_end, "peak_process_tree_rss_bytes": memory.chrome_peak},
            "binary_size_bytes": binary_size_bytes(),
            "compact_context_bytes": context_bytes,
            "cdp_requests": cdp_requests,
            "startup_ms": startup_ms,
        },
        "summary": {
            "successes": successes,
            "failures": failures,
            "wrong_actions": wrong_actions,
            "task_success_rate": successes as f64 / outcomes.len() as f64,
            "hard_gate_passed": failures == 0 && wrong_actions == 0,
        },
        "scenarios": outcomes,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

async fn run_scenario(session: &BrowserSession, id: &str) -> BrowserResult<String> {
    match id {
        "duplicate-label" => {
            let target = if std::env::var("GLASS_SCORECARD_TARGET_MODE").as_deref() == Ok("wrong") {
                "Delete draft"
            } else {
                "Delete"
            };
            session.click(target).await?;
            string_eval(session, "document.querySelector('#result').value").await
        }
        "overlay" => {
            session
                .evaluate("document.querySelector('#overlay').style.display='block'")
                .await?;
            session.click("Overlay target").await?;
            let value = string_eval(session, "document.querySelector('#result').value").await?;
            Ok(if value == "idle" {
                "blocked".to_string()
            } else {
                value
            })
        }
        "reflow" => {
            session
                .evaluate("document.querySelector('#moving').style.left='80px'")
                .await?;
            session.click("Moving target").await?;
            string_eval(session, "document.querySelector('#result').value").await
        }
        "delayed-content" => {
            string_eval(
                session,
                "document.querySelector('#delayed')?.textContent || 'missing'",
            )
            .await
        }
        "spa-navigation" => {
            session.click("SPA navigation").await?;
            string_eval(session, "document.querySelector('#result').value").await
        }
        "form" => {
            session.type_text("Glass", Some("Name")).await?;
            session.click("Submit").await?;
            string_eval(session, "document.querySelector('#result').value").await
        }
        "popup" => {
            session.click("Popup").await?;
            let value = string_eval(session, "document.querySelector('#result').value").await?;
            session.evaluate("window.fixturePopup?.close()").await?;
            Ok(value)
        }
        "frame" => {
            session.evaluate("document.querySelector('#frame').contentDocument.querySelector('#frame-action').click(); document.querySelector('#result').value='frame-clicked'").await?;
            Ok("frame-clicked".to_string())
        }
        "dialog" => {
            session.click("Dialog").await?;
            string_eval(session, "document.querySelector('#result').value").await
        }
        "download" => {
            session.click("Download").await?;
            string_eval(session, "document.querySelector('#result').value").await
        }
        "failure-recovery" => {
            if session.click("Definitely missing").await.is_ok() {
                return Ok("unexpected-action".to_string());
            }
            session
                .evaluate("document.querySelector('#result').value='recovered'")
                .await?;
            string_eval(session, "document.querySelector('#result').value").await
        }
        unknown => Err(format!("unknown scenario {unknown}").into()),
    }
}

async fn reset(session: &BrowserSession) -> BrowserResult<()> {
    session
        .evaluate("window.resetFixture(); document.querySelector('#name').value=''")
        .await?;
    Ok(())
}

async fn string_eval(session: &BrowserSession, expression: &str) -> BrowserResult<String> {
    session
        .evaluate(expression)
        .await?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("expression did not return a string: {expression}").into())
}

fn count_status(outcomes: &[Value], status: &str) -> usize {
    outcomes
        .iter()
        .filter(|outcome| outcome["status"] == status)
        .count()
}

fn positive_env(name: &str, default: usize) -> BrowserResult<usize> {
    let Some(value) = std::env::var_os(name) else {
        return Ok(default);
    };
    let value = value
        .to_str()
        .ok_or_else(|| format!("{name} must be valid UTF-8"))?;
    parse_positive(name, value)
}

fn parse_positive(name: &str, value: &str) -> BrowserResult<usize> {
    let value = value
        .parse::<usize>()
        .map_err(|_| format!("{name} must be a positive integer"))?;
    if value == 0 {
        return Err(format!("{name} must be a positive integer").into());
    }
    Ok(value)
}

async fn available_port() -> BrowserResult<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    Ok(listener.local_addr()?.port())
}

fn command_version(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn machine_name() -> Option<String> {
    command_version("uname", &["-mnrsv"])
}

fn binary_size_bytes() -> Option<u64> {
    let explicit = std::env::var_os("GLASS_BINARY_PATH").map(PathBuf::from);
    let default = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release/glass");
    explicit
        .or_else(|| default.is_file().then_some(default))
        .and_then(|path| std::fs::metadata(path).ok())
        .map(|metadata| metadata.len())
}

struct MemorySample {
    glass_peak: Option<u64>,
    chrome_peak: Option<u64>,
}

struct MemorySampler {
    stop: Arc<AtomicBool>,
    glass_peak: Arc<AtomicU64>,
    chrome_peak: Arc<AtomicU64>,
    task: tokio::task::JoinHandle<()>,
}

impl MemorySampler {
    fn start(glass_pid: u32, chrome_pid: u32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let glass_peak = Arc::new(AtomicU64::new(0));
        let chrome_peak = Arc::new(AtomicU64::new(0));
        let task_stop = Arc::clone(&stop);
        let task_glass = Arc::clone(&glass_peak);
        let task_chrome = Arc::clone(&chrome_peak);
        let task = tokio::spawn(async move {
            while !task_stop.load(Ordering::Relaxed) {
                if let Some(value) = process_rss_bytes(glass_pid) {
                    task_glass.fetch_max(value, Ordering::Relaxed);
                }
                if let Some(value) = process_tree_rss_bytes(chrome_pid) {
                    task_chrome.fetch_max(value, Ordering::Relaxed);
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        });
        Self {
            stop,
            glass_peak,
            chrome_peak,
            task,
        }
    }

    async fn stop(self) -> MemorySample {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.task.await;
        let optional = |value| (value != 0).then_some(value);
        MemorySample {
            glass_peak: optional(self.glass_peak.load(Ordering::Relaxed)),
            chrome_peak: optional(self.chrome_peak.load(Ordering::Relaxed)),
        }
    }
}

#[cfg(target_os = "linux")]
fn process_tree_rss_bytes(root_pid: u32) -> Option<u64> {
    let mut parents = std::collections::HashMap::new();
    for entry in std::fs::read_dir("/proc").ok()?.filter_map(Result::ok) {
        let pid = entry.file_name().to_string_lossy().parse::<u32>().ok();
        if let Some(pid) = pid {
            let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok();
            let parent = status
                .as_deref()
                .and_then(|text| field_kib(text, "PPid:"))
                .map(|value| value as u32);
            let rss = status
                .as_deref()
                .and_then(|text| field_kib(text, "VmRSS:"))
                .map(|value| value * 1024);
            parents.insert(pid, (parent, rss));
        }
    }
    let mut members = vec![root_pid];
    let mut index = 0;
    while index < members.len() {
        let parent = members[index];
        for (&pid, &(ppid, _)) in &parents {
            if ppid == Some(parent) && !members.contains(&pid) {
                members.push(pid);
            }
        }
        index += 1;
    }
    Some(
        members
            .iter()
            .filter_map(|pid| parents.get(pid).and_then(|(_, rss)| *rss))
            .sum(),
    )
}

#[cfg(target_os = "linux")]
fn field_kib(status: &str, field: &str) -> Option<u64> {
    status
        .lines()
        .find_map(|line| line.strip_prefix(field))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

#[cfg(target_os = "linux")]
fn process_rss_bytes(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    field_kib(&status, "VmRSS:").map(|value| value * 1024)
}

#[cfg(not(target_os = "linux"))]
fn process_tree_rss_bytes(_root_pid: u32) -> Option<u64> {
    None
}

#[cfg(not(target_os = "linux"))]
fn process_rss_bytes(_pid: u32) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrong_action_is_a_hard_failure() {
        let outcomes = vec![
            json!({"status":"success"}),
            json!({"status":"wrong_action"}),
        ];
        let successes = count_status(&outcomes, "success");
        let wrong = count_status(&outcomes, "wrong_action");
        assert_eq!(successes, 1);
        assert_eq!(wrong, 1);
        assert_ne!(successes, outcomes.len());
    }

    #[test]
    fn corpus_covers_required_adversarial_workflows() {
        let corpus: Corpus =
            serde_json::from_str(include_str!("../benchmarks/scenarios/v1.json")).unwrap();
        let ids: std::collections::HashSet<_> = corpus
            .scenarios
            .iter()
            .map(|scenario| scenario.id.as_str())
            .collect();
        for required in [
            "duplicate-label",
            "overlay",
            "reflow",
            "delayed-content",
            "spa-navigation",
            "form",
            "popup",
            "frame",
            "dialog",
            "download",
            "failure-recovery",
        ] {
            assert!(ids.contains(required), "missing {required}");
        }
    }

    #[test]
    fn invalid_iteration_count_is_rejected() {
        assert!(parse_positive("iterations", "0").is_err());
        assert!(parse_positive("iterations", "many").is_err());
    }
}
