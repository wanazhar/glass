use base64::{Engine, engine::general_purpose::STANDARD};
use glass::browser::chrome::detect_chrome;
use glass::browser::session::{BrowserResult, BrowserSession, InteractionMode, SessionOptions};
use serde_json::json;
use std::future::Future;
use std::time::{Duration, Instant};
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
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    drop(listener);

    let startup_started = Instant::now();
    let fast_session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path),
        profile: "benchmark".to_string(),
        incognito: true,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await?;
    let startup_ms = startup_started.elapsed().as_secs_f64() * 1000.0;
    let fixture = include_str!("../tests/fixtures/basic.html");
    let url = format!("data:text/html;base64,{}", STANDARD.encode(fixture));
    fast_session.navigate(&url).await?;
    let human_session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: None,
        profile: "benchmark-human".to_string(),
        incognito: true,
        headed: false,
        interaction_mode: InteractionMode::Human,
    })
    .await?;
    let reduced_iterations = (iterations / 5).max(5);
    click_pair(&fast_session).await?;
    click_pair(&human_session).await?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "tool": "glass",
            "iterations": iterations,
            "startup_ms": startup_ms,
            "browser": "Chrome/Chromium via CDP",
            "results": [
                measure("evaluate", iterations, || async {
                    let _ = fast_session.evaluate("1 + 1").await?;
                    Ok(())
                })
                .await?,
                measure("text", iterations, || async {
                    let _ = fast_session.text().await?;
                    Ok(())
                })
                .await?,
                measure("observe_fresh", reduced_iterations, || async {
                    let _ = fast_session.observe_fresh().await?;
                    Ok(())
                })
                .await?,
                measure("observe_cached", iterations, || async {
                    let _ = fast_session.observe().await?;
                    Ok(())
                })
                .await?,
                measure("observe_fresh_with_screenshot", reduced_iterations, || async {
                    let _ = fast_session.observe_fresh_with_screenshot().await?;
                    Ok(())
                })
                .await?,
                measure("dom_snapshot", reduced_iterations, || async {
                    let _ = fast_session.snapshot().await?;
                    Ok(())
                })
                .await?,
                measure("screenshot", reduced_iterations, || async {
                    let _ = fast_session.screenshot_png().await?;
                    Ok(())
                })
                .await?,
                measure("click_pair_fast", reduced_iterations, || async {
                    click_pair(&fast_session).await
                })
                .await?,
                measure("click_pair_human", reduced_iterations, || async {
                    click_pair(&human_session).await
                })
                .await?
            ]
        }))?
    );
    human_session.close().await?;
    fast_session.close().await
}

async fn click_pair(session: &BrowserSession) -> BrowserResult<()> {
    let _ = session.click("Name").await?;
    let _ = session.click("Save").await?;
    Ok(())
}

async fn measure<F, Fut>(
    name: &str,
    iterations: usize,
    mut operation: F,
) -> BrowserResult<serde_json::Value>
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
        "p95_ms": percentile(0.95)
    }))
}
