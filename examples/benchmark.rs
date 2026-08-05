use base64::{Engine, engine::general_purpose::STANDARD};
use glass::browser::chrome::detect_chrome;
use glass::browser::session::{
    BrowserResult, BrowserSession, InteractionMode, SessionOptions, StartupDiagnostics,
    WaitCondition,
};
use serde_json::{Value, json};
use std::{
    future::Future,
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::net::TcpListener;

const DEFAULT_ITERATIONS: usize = 50;
const MAX_PAGE_CLASS_ITERATIONS: usize = 100;

struct PageClassFixture {
    name: &'static str,
    page_class: &'static str,
    description: &'static str,
    url: String,
}

fn local_page_class_fixtures(normal_html: &str) -> Vec<PageClassFixture> {
    let dynamic_listing_html = r##"<!doctype html>
<html><head><title>Dynamic listing</title></head>
<body><main id="listing"></main><script>
const items = ["Alpha", "Bravo", "Charlie", "Delta"];
document.querySelector("#listing").innerHTML =
  "<h1>Results</h1><ul>" + items.map(item => "<li>" + item + "</li>").join("") + "</ul>";
</script></body></html>"##;
    let challenge_html = r#"<!doctype html>
<html><head><title>Checking your browser</title></head>
<body><main><h1>Checking your browser before accessing</h1><p>Complete the security check to continue.</p></main></body></html>"#;
    let empty_html = r#"<!doctype html><html><head><title></title></head><body></body></html>"#;

    [
        (
            "normal_static",
            "normal",
            "static local document",
            normal_html,
        ),
        (
            "dynamic_listing",
            "normal",
            "synchronously rendered local listing",
            dynamic_listing_html,
        ),
        (
            "challenge_interstitial",
            "challenge",
            "local challenge/interstitial document",
            challenge_html,
        ),
        (
            "empty_unknown",
            "empty",
            "local empty document with no actionable content",
            empty_html,
        ),
    ]
    .into_iter()
    .map(|(name, page_class, description, html)| PageClassFixture {
        name,
        page_class,
        description,
        url: format!("data:text/html;base64,{}", STANDARD.encode(html)),
    })
    .collect()
}

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
    let expensive_iterations = std::env::var("GLASS_BENCH_EXPENSIVE_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|iterations| *iterations > 0)
        .unwrap_or(iterations);
    let fixture = include_str!("../tests/fixtures/basic.html");
    let url = format!("data:text/html;base64,{}", STANDARD.encode(fixture));
    let page_class_fixtures = local_page_class_fixtures(fixture);

    // Keep browser startup independent from navigation and observation latency.
    // The cold run reports repeated owned-session startup samples, while the
    // first post-navigation observe remains a separate diagnostic.
    let page_class_iterations = std::env::var("GLASS_BENCH_PAGE_CLASS_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|iterations| *iterations > 0)
        .unwrap_or(iterations)
        .min(MAX_PAGE_CLASS_ITERATIONS);
    let cold_start_result = bench_cold_start(&chrome_path, &url, expensive_iterations).await?;

    let rss_before_start = process_rss_bytes();
    let startup_started = Instant::now();
    let fast_port = available_port().await?;
    let fast_session = BrowserSession::start(
        &SessionOptions::builder()
            .port(fast_port)
            .chrome_path(chrome_path.clone())
            .profile("benchmark-fast")
            .incognito(true)
            .interaction_mode(InteractionMode::Fast)
            .build()?,
    )
    .await?;
    let warm_session_start_ms = startup_started.elapsed().as_secs_f64() * 1000.0;
    let rss_after_fast_start = process_rss_bytes();

    let human_port = available_port().await?;
    let human_session = BrowserSession::start(
        &SessionOptions::builder()
            .port(human_port)
            .chrome_path(chrome_path)
            .profile("benchmark-human")
            .incognito(true)
            .interaction_mode(InteractionMode::Human)
            .build()?,
    )
    .await?;
    let rss_after_sessions_start = process_rss_bytes();
    let owned_browser_ws_url = match fast_session.owned_browser_ws_url() {
        Some(url) => url.to_string(),
        None => {
            let _ = human_session.close().await;
            let _ = fast_session.close().await;
            return Err("warm benchmark session did not own a browser endpoint".into());
        }
    };
    // Attach benchmarking is opt-in. When enabled, it reuses the already
    // verified owned endpoint; the default run never requires an externally
    // managed Chrome instance.
    let attach_existing_startup_result =
        match bench_attach_existing_startup(fast_port, &owned_browser_ws_url, expensive_iterations)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                let _ = human_session.close().await;
                let _ = fast_session.close().await;
                return Err(error);
            }
        };
    let attach_existing_startup_path = attach_existing_startup_result.clone();

    let benchmark_result: BrowserResult<Value> = async {
        // Keep the cold-start envelope for existing consumers, while also
        // exposing its established operation summaries as top-level results.
        let cold_start_object = cold_start_result
            .as_object()
            .ok_or_else(|| "cold_start benchmark returned a non-object".to_string())?;
        let cold_owned_session_startup_result = cold_start_object
            .get("cold_owned_session_startup")
            .cloned()
            .ok_or_else(|| "cold_start omitted cold_owned_session_startup".to_string())?;
        let cold_first_observe_result = cold_start_object
            .get("cold_first_observe")
            .cloned()
            .ok_or_else(|| "cold_start omitted cold_first_observe".to_string())?;

        let page_class_latency_result =
            bench_page_class_latencies(&fast_session, &page_class_fixtures, page_class_iterations)
                .await?;

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

        // ── Warm semantic bootstrap benchmark ──────────────────────────
        let semantic_bootstrap_result = bench_semantic_bootstrap(&fast_session, iterations).await?;

        // ── Dedicated cached compact observe benchmark ─────────────────
        let compact_observe_result = bench_compact_observe(&fast_session, iterations).await?;
        let full_observation_result =
            measure("observe_compact_fresh", expensive_iterations, || async {
                let _ = fast_session.observe_fresh().await?;
                Ok(())
            })
            .await?;
        let warm_session_reuse_result = compact_observe_result.clone();
        let mut warm_session_reuse_path = warm_session_reuse_result;
        warm_session_reuse_path["operation"] = Value::String("warm_session_reuse".to_string());
        let mut full_observation_path = full_observation_result.clone();
        full_observation_path["operation"] = Value::String("full_observation".to_string());
        let cold_owned_startup_path = cold_owned_session_startup_result.clone();
        let semantic_bootstrap_path = semantic_bootstrap_result.clone();

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
            full_observation_result,
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
            semantic_bootstrap_result,
            client_overhead_result,
            cold_owned_session_startup_result,
            cold_first_observe_result,
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
            "warm_session_start_ms": warm_session_start_ms,
            "attach_existing_startup": attach_existing_startup_result,
            "latency_paths": {
                "cold_owned_startup": cold_owned_startup_path,
                "attach_existing_startup": attach_existing_startup_path,
                "warm_session_reuse": warm_session_reuse_path,
                "semantic_bootstrap": semantic_bootstrap_path,
                "full_observation": full_observation_path,
            },
            "page_class_latency": page_class_latency_result,
            "warm_startup_diagnostics": fast_session.startup_diagnostics(),
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
/// Measure startup when attaching to the endpoint owned by the warm benchmark
/// session. This is deliberately opt-in: the default benchmark never assumes
/// an externally managed Chrome and never changes the ownership of the warm
/// browser. Attached sessions are created and closed sequentially.
async fn bench_attach_existing_startup(
    port: u16,
    expected_browser_ws_url: &str,
    iterations: usize,
) -> BrowserResult<Value> {
    let enabled = std::env::var("GLASS_BENCH_ATTACH").ok().as_deref() == Some("1");
    if !enabled {
        return Ok(json!({
            "operation": "attach_existing_startup",
            "status": "skipped",
            "mode": "owned_endpoint_opt_in",
            "iterations": 0,
            "startup": Value::Null,
            "startup_diagnostics": Value::Null,
            "reason": "set GLASS_BENCH_ATTACH=1 to measure attach startup against the benchmark-owned Chrome",
        }));
    }

    let mut startup_samples = Vec::with_capacity(iterations);
    let mut startup_diagnostics = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let started = Instant::now();
        let attached = BrowserSession::start_attached_to_browser_ws_url(
            &SessionOptions::builder()
                .port(port)
                .attach(true)
                .interaction_mode(InteractionMode::Fast)
                .build()?,
            expected_browser_ws_url,
        )
        .await?;
        startup_samples.push(started.elapsed());
        startup_diagnostics.push(*attached.startup_diagnostics());
        attached.close().await?;
    }

    Ok(json!({
        "operation": "attach_existing_startup",
        "status": "measured",
        "mode": "owned_endpoint_opt_in",
        "iterations": iterations,
        "startup": summarize_samples(
            "attach_existing_startup",
            iterations,
            startup_samples,
        )?,
        "startup_diagnostics": summarize_startup_diagnostics(&startup_diagnostics)?,
        "reason": Value::Null,
    }))
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

/// Benchmark the warm semantic bootstrap path separately from authoritative
/// observation. The result is intentionally discarded: bootstrap evidence is
/// only a readiness hint and must not resolve or authorize an action.
async fn bench_semantic_bootstrap(
    session: &BrowserSession,
    iterations: usize,
) -> BrowserResult<Value> {
    measure("semantic_bootstrap_warm", iterations, || async {
        let _ = session.observe_bootstrap().await?;
        Ok(())
    })
    .await
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

/// Measure deterministic local fixture classes without charging bootstrap or
/// authoritative inspection for navigation. The fixture URLs are data URLs, so
/// public-network latency is deliberately outside this matrix.
async fn bench_page_class_latencies(
    session: &BrowserSession,
    fixtures: &[PageClassFixture],
    iterations: usize,
) -> BrowserResult<Value> {
    let first_fixture = fixtures
        .first()
        .ok_or_else(|| "page-class benchmark has no fixtures".to_string())?;
    let mut summaries = serde_json::Map::new();

    for fixture in fixtures {
        let navigation = measure("navigation", iterations, || async {
            session.navigate(&fixture.url).await?;
            Ok(())
        })
        .await?;
        let semantic_bootstrap = measure("semantic_bootstrap", iterations, || async {
            let _ = session.observe_bootstrap().await?;
            Ok(())
        })
        .await?;
        let full_observation = measure("full_observation", iterations, || async {
            let _ = session.observe_fresh().await?;
            Ok(())
        })
        .await?;

        summaries.insert(
            fixture.name.to_string(),
            json!({
                "fixture": fixture.name,
                "page_class": fixture.page_class,
                "classification_source": "bounded_local_fixture_label",
                "description": fixture.description,
                "navigation": navigation,
                "semantic_bootstrap": semantic_bootstrap,
                "full_observation": full_observation,
            }),
        );
    }

    // Leave the caller on the normal fixture so the established benchmark
    // operations retain their original page and click targets.
    session.navigate(&first_fixture.url).await?;

    Ok(json!({
        "operation": "page_class_latency",
        "mode": "local_deterministic_fixtures",
        "iterations": iterations,
        "network_latency": {
            "included": false,
            "scope": "not measured",
            "reason": "all fixtures use data:text/html URLs; public-site latency is separate and opt-in",
        },
        "summaries": Value::Object(summaries),
    }))
}

/// Measure owned-session startup separately from the first post-navigation
/// compact observation. Startup timing ends when `BrowserSession::start`
/// establishes CDP; navigation is awaited before the observe timer starts.
///
/// The scalar `chrome_launch_ms` and `cold_first_observe_ms` fields retain the
/// first-sample fields used by existing consumers. The nested result objects
/// add p50/p95 distributions for repeated cold samples.
async fn bench_cold_start(
    chrome_path: &std::path::Path,
    fixture_url: &str,
    iterations: usize,
) -> BrowserResult<Value> {
    let mut launch_samples = Vec::with_capacity(iterations);
    let mut observe_samples = Vec::with_capacity(iterations);
    let mut startup_diagnostics = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        // The startup sample ends after BrowserSession::start establishes CDP.
        // Navigation is deliberately outside this timer.
        let port = available_port().await?;
        let launch_started = Instant::now();
        let session = BrowserSession::start(
            &SessionOptions::builder()
                .port(port)
                .chrome_path(chrome_path.to_path_buf())
                .profile("benchmark-cold")
                .incognito(true)
                .interaction_mode(InteractionMode::Fast)
                .build()?,
        )
        .await?;
        startup_diagnostics.push(*session.startup_diagnostics());
        launch_samples.push(launch_started.elapsed());

        // Navigation is completed before timing the first uncached observe, so
        // network and navigation latency cannot be charged to either metric.
        session.navigate(fixture_url).await?;
        let observe_started = Instant::now();
        let _ = session.observe_fresh().await?;
        observe_samples.push(observe_started.elapsed());

        session.close().await?;
    }

    let first_launch_ms = launch_samples
        .first()
        .map(|sample| sample.as_secs_f64() * 1000.0)
        .unwrap_or(0.0);
    let first_observe_ms = observe_samples
        .first()
        .map(|sample| sample.as_secs_f64() * 1000.0)
        .unwrap_or(0.0);

    Ok(json!({
        "operation": "cold_start",
        // Preserve the original scalar fields for existing consumers. They
        // remain the first sample; distributions below are the comparison
        // metrics for the repeated cold run.
        "chrome_launch_ms": first_launch_ms,
        "cold_first_observe_ms": first_observe_ms,
        "cold_owned_session_startup": summarize_samples(
            "cold_owned_session_startup",
            iterations,
            launch_samples,
        )?,
        "cold_first_observe": summarize_samples(
            "cold_first_observe",
            iterations,
            observe_samples,
        )?,
        "cold_startup_diagnostics": summarize_startup_diagnostics(&startup_diagnostics)?,
    }))
}

fn summarize_startup_diagnostics(samples: &[StartupDiagnostics]) -> BrowserResult<Value> {
    if samples.is_empty() {
        return Err("startup diagnostics produced no samples".into());
    }

    let summarize =
        |name: &str, values: Vec<Duration>| summarize_samples(name, values.len(), values);
    let field = |read: fn(&StartupDiagnostics) -> u64, name: &str| {
        summarize(
            name,
            samples
                .iter()
                .map(|sample| Duration::from_millis(read(sample)))
                .collect(),
        )
    };

    Ok(json!({
        "launch_endpoint": field(|sample| sample.launch_endpoint_ms, "startup_launch_endpoint")?,
        "page_target_wait": field(|sample| sample.page_target_wait_ms, "startup_page_target_wait")?,
        "cdp_connect": field(|sample| sample.cdp_connect_ms, "startup_cdp_connect")?,
        "target_attach": field(|sample| sample.target_attach_ms, "startup_target_attach")?,
        "event_setup": field(|sample| sample.event_setup_ms, "startup_event_setup")?,
        "policy_arm": field(|sample| sample.policy_arm_ms, "startup_policy_arm")?,
        "total_startup": field(|sample| sample.total_startup_ms, "startup_total")?,
    }))
}

/// Instrument click overhead: separate Glass client-side wrapper logic from
/// time spent awaiting actual CDP responses. The CDP client records the
/// response wait for every command, avoiding the distortion caused by using a
/// lightweight-command median for heavier commands such as AX snapshots.
///
/// Returns a JSON object with per-sample breakdowns and aggregated
/// statistics.
async fn bench_client_overhead(
    session: &BrowserSession,
    iterations: usize,
) -> BrowserResult<Value> {
    let mut total_samples = Vec::with_capacity(iterations);
    let mut cdp_wait_samples = Vec::with_capacity(iterations);
    let mut overhead_samples = Vec::with_capacity(iterations);
    let mut cdp_call_counts = Vec::with_capacity(iterations);

    let targets = ["Name", "Save"];

    for i in 0..iterations {
        let target = targets[i % targets.len()];

        let cdp_before = session.cdp_request_count();
        let started = Instant::now();
        let (click_result, cdp_wait_nanos) = session.measure_cdp_wait(session.click(target)).await;
        let _ = click_result?;
        let total_ms = started.elapsed().as_secs_f64() * 1000.0;
        let cdp_after = session.cdp_request_count();

        let cdp_calls = cdp_after.saturating_sub(cdp_before);
        let cdp_wait_ms = cdp_wait_nanos as f64 / 1_000_000.0;
        let overhead_ms = if total_ms > cdp_wait_ms {
            total_ms - cdp_wait_ms
        } else {
            0.0
        };

        total_samples.push(total_ms);
        cdp_wait_samples.push(cdp_wait_ms);
        overhead_samples.push(overhead_ms);
        cdp_call_counts.push(cdp_calls);
    }

    total_samples.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    cdp_wait_samples.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
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
        "avg_cdp_calls_per_click": avg_cdp_calls,
        "total_click": summarize_f64("click_total", &total_samples),
        "observed_cdp_wait": summarize_f64("click_cdp_wait", &cdp_wait_samples),
        "glass_overhead": summarize_f64("click_glass_overhead", &overhead_samples),
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

    #[test]
    fn local_page_class_fixtures_are_bounded_and_network_free() {
        let fixtures = local_page_class_fixtures("<html><body>normal</body></html>");
        let classes: Vec<_> = fixtures.iter().map(|fixture| fixture.page_class).collect();
        assert_eq!(classes, vec!["normal", "normal", "challenge", "empty"]);
        let names: Vec<_> = fixtures.iter().map(|fixture| fixture.name).collect();
        assert_eq!(
            names,
            vec![
                "normal_static",
                "dynamic_listing",
                "challenge_interstitial",
                "empty_unknown",
            ]
        );
        assert!(
            fixtures
                .iter()
                .all(|fixture| fixture.url.starts_with("data:text/html;base64,"))
        );
        assert!(MAX_PAGE_CLASS_ITERATIONS > 0);
    }
}
