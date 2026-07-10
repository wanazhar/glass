use base64::{Engine, engine::general_purpose::STANDARD};
use glass::browser::chrome::detect_chrome;
use glass::browser::session::{BrowserResult, BrowserSession, SessionOptions};
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
        .unwrap_or(DEFAULT_ITERATIONS);
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    drop(listener);

    let startup_started = Instant::now();
    let session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path),
        profile: "benchmark".to_string(),
        incognito: true,
        headed: false,
    })
    .await?;
    let startup_ms = startup_started.elapsed().as_secs_f64() * 1000.0;
    let fixture = include_str!("../tests/fixtures/basic.html");
    let url = format!("data:text/html;base64,{}", STANDARD.encode(fixture));
    session.navigate(&url).await?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "tool": "glass",
            "iterations": iterations,
            "startup_ms": startup_ms,
            "browser": "Chrome/Chromium via CDP",
            "results": [
                measure("evaluate", iterations, || async {
                    let _ = session.evaluate("1 + 1").await?;
                    Ok(())
                })
                .await?,
                measure("text", iterations, || async {
                    let _ = session.text().await?;
                    Ok(())
                })
                .await?,
                measure("dom_snapshot", (iterations / 5).max(5), || async {
                    let _ = session.snapshot().await?;
                    Ok(())
                })
                .await?,
                measure("screenshot", (iterations / 5).max(5), || async {
                    let _ = session.screenshot_png().await?;
                    Ok(())
                })
                .await?
            ]
        }))?
    );
    session.close().await
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
