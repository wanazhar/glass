use base64::{Engine, engine::general_purpose::STANDARD};
use glass::browser::chrome::detect_chrome;
use glass::browser::session::{
    BrowserResult, BrowserSession, InteractionMode, SessionOptions, WaitCondition,
};
use serde_json::{Value, json};
use std::{
    future::Future,
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::net::TcpListener;

const DEFAULT_ITERATIONS: usize = 50;

#[tokio::main]
async fn main() -> BrowserResult<()> {
    let Some(chrome_path) = detect_chrome() else {
        return Err("Chrome/Chromium is required for the benchmark".into());
    };
    let iterations = std::env::var("GLASS_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|iterations| *iterations > 0)
        .unwrap_or(DEFAULT_ITERATIONS);
    let expensive_iterations = (iterations / 5).max(5);
    let fixture = include_str!("../tests/fixtures/basic.html");
    let url = format!("data:text/html;base64,{}", STANDARD.encode(fixture));

    let rss_before_start = process_rss_bytes();
    let startup_started = Instant::now();
    let fast_session = BrowserSession::start(&SessionOptions {
        port: available_port().await?,
        chrome_path: Some(chrome_path.clone()),
        profile: "benchmark-fast".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await?;
    let cold_start_ms = startup_started.elapsed().as_secs_f64() * 1000.0;
    let rss_after_fast_start = process_rss_bytes();

    let human_session = BrowserSession::start(&SessionOptions {
        port: available_port().await?,
        chrome_path: Some(chrome_path),
        profile: "benchmark-human".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        headed: false,
        interaction_mode: InteractionMode::Human,
    })
    .await?;
    let rss_after_sessions_start = process_rss_bytes();

    let benchmark_result: BrowserResult<Value> = async {
        fast_session.navigate(&url).await?;
        human_session.navigate(&url).await?;
        warm_click_targets(&fast_session).await?;
        warm_click_targets(&human_session).await?;

        // Measure agent-facing contexts separately from latency. The payloads
        // are `PageContext` JSON bytes and intentionally exclude JSON-RPC
        // framing or an MCP image-content envelope.
        let payload_bytes = collect_payload_bytes(&fast_session).await?;

        // Ensure the cache is populated before timing repeated compact turns.
        let _ = fast_session.observe().await?;
        let results = vec![
            measure("evaluate", iterations, || async {
                let _ = fast_session.evaluate("1 + 1").await?;
                Ok(())
            })
            .await?,
            measure("wait_js_true", iterations, || async {
                let _ = fast_session
                    .wait(
                        WaitCondition::JavaScript("true".to_string()),
                        Duration::from_secs(1),
                    )
                    .await?;
                Ok(())
            })
            .await?,
            measure("text", iterations, || async {
                let _ = fast_session.text().await?;
                Ok(())
            })
            .await?,
            measure("observe_compact_fresh", expensive_iterations, || async {
                let _ = fast_session.observe_fresh().await?;
                Ok(())
            })
            .await?,
            measure("observe_compact_cached", iterations, || async {
                let _ = fast_session.observe().await?;
                Ok(())
            })
            .await?,
            measure("deep_dom", expensive_iterations, || async {
                let _ = fast_session.deep_dom().await?;
                Ok(())
            })
            .await?,
            measure("screenshot_base64", expensive_iterations, || async {
                let _ = fast_session.screenshot_base64().await?;
                Ok(())
            })
            .await?,
            measure_alternating_clicks("click_fast", &fast_session, expensive_iterations).await?,
            measure_alternating_clicks("click_human", &human_session, expensive_iterations).await?,
        ];

        Ok(json!({
            "tool": "glass",
            "browser": "Chrome/Chromium via raw CDP",
            "environment": {
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
            },
            "iterations": iterations,
            "expensive_iterations": expensive_iterations,
            "cold_start_ms": cold_start_ms,
            "payload_bytes": payload_bytes,
            "glass_process_memory": {
                "scope": "Glass client process only; Chrome child-process memory is excluded",
                "rss_bytes_before_start": rss_before_start,
                "rss_bytes_after_fast_start": rss_after_fast_start,
                "rss_bytes_after_all_sessions_start": rss_after_sessions_start,
                "rss_bytes_after_workload": process_rss_bytes(),
            },
            "glass_binary_size_bytes": glass_binary_size_bytes(),
            "results": results,
        }))
    }
    .await;

    let human_close_result = human_session.close().await;
    let fast_close_result = fast_session.close().await;
    let output = benchmark_result?;
    human_close_result?;
    fast_close_result?;
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

async fn available_port() -> BrowserResult<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

async fn warm_click_targets(session: &BrowserSession) -> BrowserResult<()> {
    let _ = session.click("Name").await?;
    let _ = session.click("Save").await?;
    Ok(())
}

async fn collect_payload_bytes(session: &BrowserSession) -> BrowserResult<Value> {
    let compact = session.observe_fresh().await?;
    let with_deep_dom = session.observe_fresh_with_dom().await?;
    let with_screenshot = session.observe_fresh_with_screenshot().await?;
    let screenshot_base64_bytes = with_screenshot.screenshot.as_ref().map_or(0, String::len);

    Ok(json!({
        "page_context_json_bytes": {
            "compact": serde_json::to_vec(&compact)?.len(),
            "with_deep_dom": serde_json::to_vec(&with_deep_dom)?.len(),
            "with_screenshot": serde_json::to_vec(&with_screenshot)?.len(),
        },
        "screenshot_base64_bytes": screenshot_base64_bytes,
    }))
}

async fn measure_alternating_clicks(
    name: &str,
    session: &BrowserSession,
    iterations: usize,
) -> BrowserResult<Value> {
    let mut samples = Vec::with_capacity(iterations);
    for index in 0..iterations {
        let target = if index % 2 == 0 { "Name" } else { "Save" };
        let started = Instant::now();
        let _ = session.click(target).await?;
        samples.push(started.elapsed());
    }
    summarize_samples(name, iterations, samples)
}

async fn measure<F, Fut>(name: &str, iterations: usize, mut operation: F) -> BrowserResult<Value>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = BrowserResult<()>>,
{
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let started = Instant::now();
        operation().await?;
        samples.push(started.elapsed());
    }
    summarize_samples(name, iterations, samples)
}

fn summarize_samples(
    name: &str,
    iterations: usize,
    mut samples: Vec<Duration>,
) -> BrowserResult<Value> {
    if samples.is_empty() {
        return Err(format!("{name} produced no benchmark samples").into());
    }
    samples.sort_unstable();
    let total: Duration = samples.iter().copied().sum();
    let percentile = |ratio: f64| {
        let index = ((samples.len().saturating_sub(1)) as f64 * ratio).round() as usize;
        samples[index].as_secs_f64() * 1000.0
    };
    Ok(json!({
        "operation": name,
        "iterations": iterations,
        "total_ms": total.as_secs_f64() * 1000.0,
        "average_ms": total.as_secs_f64() * 1000.0 / iterations as f64,
        "p50_ms": percentile(0.50),
        "p95_ms": percentile(0.95),
    }))
}

fn glass_binary_size_bytes() -> Option<u64> {
    let explicit = std::env::var_os("GLASS_BINARY_PATH").map(PathBuf::from);
    let default_name = if cfg!(windows) { "glass.exe" } else { "glass" };
    let default = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("release")
        .join(default_name);
    explicit
        .or_else(|| default.is_file().then_some(default))
        .and_then(|path| std::fs::metadata(path).ok())
        .map(|metadata| metadata.len())
}

#[cfg(target_os = "linux")]
fn process_rss_bytes() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()
        .map(|kibibytes| kibibytes * 1024)
}

#[cfg(all(not(target_os = "linux"), not(target_os = "windows")))]
fn process_rss_bytes() -> Option<u64> {
    let process_id = std::process::id().to_string();
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &process_id])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then_some(())
        .and_then(|()| String::from_utf8(output.stdout).ok())?
        .trim()
        .parse::<u64>()
        .ok()
        .map(|kibibytes| kibibytes * 1024)
}

#[cfg(target_os = "windows")]
fn process_rss_bytes() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_reports_expected_percentiles() {
        let summary = summarize_samples(
            "fixture",
            4,
            vec![
                Duration::from_millis(1),
                Duration::from_millis(2),
                Duration::from_millis(3),
                Duration::from_millis(4),
            ],
        )
        .unwrap();

        assert_eq!(summary["p50_ms"], 3.0);
        assert_eq!(summary["p95_ms"], 4.0);
    }
}
