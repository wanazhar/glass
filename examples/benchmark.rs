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

/// Compute a percentile from a sorted slice of f64 values.
///
/// Uses the nearest-rank method: `index = round((len - 1) * p)`.
/// Panics if `data` is empty.
pub fn percentile(data: &[f64], p: f64) -> f64 {
    assert!(!data.is_empty(), "percentile requires non-empty data");
    assert!((0.0..=1.0).contains(&p), "percentile p must be in [0, 1]");
    let index = ((data.len().saturating_sub(1)) as f64 * p).round() as usize;
    data[index]
}

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

    // ── Cold-start measurement (separate Chrome instance) ─────────────
    let cold_start_result = bench_cold_start(&chrome_path, &url).await?;

    let rss_before_start = process_rss_bytes();
    let startup_started = Instant::now();
    let fast_session = BrowserSession::start(&SessionOptions {
        port: available_port().await?,
        chrome_path: Some(chrome_path.clone()),
        profile: "benchmark-fast".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        frame_id: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
        audit: false,
    })
    .await?;
    let warm_session_start_ms = startup_started.elapsed().as_secs_f64() * 1000.0;
    let rss_after_fast_start = process_rss_bytes();

    let human_session = BrowserSession::start(&SessionOptions {
        port: available_port().await?,
        chrome_path: Some(chrome_path),
        profile: "benchmark-human".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        frame_id: None,
        headed: false,
        interaction_mode: InteractionMode::Human,
        audit: false,
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

        // ── Dedicated compact observe benchmark ───────────────────────
        let compact_observe_result = bench_compact_observe(&fast_session, iterations).await?;

        // ── Client-overhead instrumentation for clicks ────────────────
        let client_overhead_result =
            bench_client_overhead(&fast_session, expensive_iterations).await?;

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
            compact_observe_result,
            client_overhead_result,
            cold_start_result,
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
            "warm_session_start_ms": warm_session_start_ms,
            "payload_bytes": payload_bytes,
            "glass_process_memory": {
                "scope": "Glass client process only; Chrome child-process memory is excluded",
                "rss_bytes_before_start": rss_before_start,
                "rss_bytes_after_fast_start": rss_after_fast_start,
                "rss_bytes_after_all_sessions_start": rss_after_sessions_start,
                "rss_bytes_after_workload": process_rss_bytes(),
            },
            "glass_binary_size_bytes": glass_binary_size_bytes(),
            "cdp_request_count_after_workload": fast_session.cdp_request_count(),
            "results": results,
        }))
    }
    .await;

    let human_close_result = human_session.close().await;
    let fast_close_result = fast_session.close().await;
    let output = benchmark_result?;
    human_close_result?;
    fast_close_result?;

    // ── Write report file when GLASS_BENCH_REPORT is set ──────────────
    if let Ok(report_path) = std::env::var("GLASS_BENCH_REPORT") {
        let json = serde_json::to_string_pretty(&output)?;
        std::fs::write(&report_path, json)?;
        eprintln!("Benchmark report written to {report_path}");
    }

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
    let ms: Vec<f64> = samples.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    let average_ms = total.as_secs_f64() * 1000.0 / iterations as f64;
    Ok(json!({
        "operation": name,
        "iterations": iterations,
        "total_ms": total.as_secs_f64() * 1000.0,
        "average_ms": average_ms,
        "mean_ms": average_ms,
        "p50_ms": percentile(&ms, 0.50),
        "p95_ms": percentile(&ms, 0.95),
        "min_ms": ms.first().copied().unwrap_or(0.0),
        "max_ms": ms.last().copied().unwrap_or(0.0),
    }))
}

/// Benchmark compact observe latency over N iterations.
///
/// Uses `session.observe()` (which may hit the compact-context cache when
/// the page hasn't mutated). Reports mean, p50, p95, min, max in ms.
async fn bench_compact_observe(
    session: &BrowserSession,
    iterations: usize,
) -> BrowserResult<Value> {
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let started = Instant::now();
        let _ = session.observe().await?;
        samples.push(started.elapsed());
    }
    summarize_samples("compact_observe", iterations, samples)
}

/// Cold-start measurements: Chrome launch latency + first observe after launch.
///
/// Returns a JSON object with:
/// - `chrome_launch_ms`: wall-clock time to launch Chrome and establish CDP
/// - `cold_first_observe_ms`: latency of the very first `observe()` call
///   (before any cache priming), which includes full a11y-tree + screenshot
///   collection.
async fn bench_cold_start(
    chrome_path: &std::path::Path,
    fixture_url: &str,
) -> BrowserResult<Value> {
    // ── fresh Chrome launch ───────────────────────────────────────────
    let port = available_port().await?;
    let launch_started = Instant::now();
    let session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path.to_path_buf()),
        profile: "benchmark-cold".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        frame_id: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
        audit: false,
    })
    .await?;
    let chrome_launch_ms = launch_started.elapsed().as_secs_f64() * 1000.0;

    // Navigate to the fixture so the page is ready.
    session.navigate(fixture_url).await?;

    // ── first observe (cold, no cache) ────────────────────────────────
    let observe_started = Instant::now();
    let _ = session.observe_fresh().await?;
    let cold_first_observe_ms = observe_started.elapsed().as_secs_f64() * 1000.0;

    let _ = session.close().await;

    Ok(json!({
        "operation": "cold_start",
        "chrome_launch_ms": chrome_launch_ms,
        "cold_first_observe_ms": cold_first_observe_ms,
    }))
}

/// Instrument click overhead: separate Glass client-side wrapper logic
/// from estimated CDP round-trip time.
///
/// Strategy (non-invasive – no internal session changes):
/// 1. Measure a baseline single-CDP-command round-trip via `evaluate("1+1")`.
/// 2. For each click, count CDP requests issued and time the full
///    `session.click()` call.
/// 3. Estimate CDP time as `cdp_call_count × baseline_cdp_roundtrip`.
/// 4. Estimate Glass overhead as `total_click_time − estimated_cdp_time`.
///
/// Returns a JSON object with per-sample breakdowns and aggregated
/// statistics.
async fn bench_client_overhead(
    session: &BrowserSession,
    iterations: usize,
) -> BrowserResult<Value> {
    // Baseline: single lightweight CDP round-trip
    let mut baseline_samples = Vec::with_capacity(10);
    for _ in 0..10 {
        let started = Instant::now();
        let _ = session.evaluate("1 + 1").await?;
        baseline_samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    baseline_samples.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    let baseline_cdp_ms = baseline_samples[baseline_samples.len() / 2]; // median

    let mut total_samples = Vec::with_capacity(iterations);
    let mut cdp_estimated_samples = Vec::with_capacity(iterations);
    let mut overhead_samples = Vec::with_capacity(iterations);
    let mut cdp_call_counts = Vec::with_capacity(iterations);

    let targets = ["Name", "Save"];

    for i in 0..iterations {
        let target = targets[i % targets.len()];

        let cdp_before = session.cdp_request_count();
        let started = Instant::now();
        let _ = session.click(target).await?;
        let total_ms = started.elapsed().as_secs_f64() * 1000.0;
        let cdp_after = session.cdp_request_count();

        let cdp_calls = cdp_after.saturating_sub(cdp_before);
        let estimated_cdp_ms = cdp_calls as f64 * baseline_cdp_ms;
        let overhead_ms = if total_ms > estimated_cdp_ms {
            total_ms - estimated_cdp_ms
        } else {
            0.0
        };

        total_samples.push(total_ms);
        cdp_estimated_samples.push(estimated_cdp_ms);
        overhead_samples.push(overhead_ms);
        cdp_call_counts.push(cdp_calls);
    }

    total_samples.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    cdp_estimated_samples.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    overhead_samples.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());

    let summarize_f64 = |label: &str, data: &[f64]| -> Value {
        if data.is_empty() {
            return json!({ "operation": label, "error": "no samples" });
        }
        let sum: f64 = data.iter().sum();
        let n = data.len() as f64;
        json!({
            "operation": label,
            "iterations": data.len(),
            "average_ms": sum / n,
            "mean_ms": sum / n,
            "p50_ms": percentile(data, 0.50),
            "p95_ms": percentile(data, 0.95),
            "min_ms": data.first().copied().unwrap_or(0.0),
            "max_ms": data.last().copied().unwrap_or(0.0),
        })
    };

    let avg_cdp_calls: f64 =
        cdp_call_counts.iter().map(|c| *c as f64).sum::<f64>() / cdp_call_counts.len() as f64;

    Ok(json!({
        "operation": "client_overhead",
        "iterations": iterations,
        "baseline_cdp_roundtrip_ms": baseline_cdp_ms,
        "avg_cdp_calls_per_click": avg_cdp_calls,
        "total_click": summarize_f64("click_total", &total_samples),
        "estimated_cdp_time": summarize_f64("click_cdp_estimated", &cdp_estimated_samples),
        "estimated_glass_overhead": summarize_f64("click_glass_overhead", &overhead_samples),
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
