use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::Utc;
use glass::browser::chrome::detect_chrome;
use glass::browser::session::{BrowserResult, BrowserSession, InteractionMode, SessionOptions};
use serde_json::json;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;

const DEFAULT_ITERATIONS: usize = 20;
const MAX_ITERATIONS: usize = 200;
const MIN_CLAIM_SAMPLES: usize = 20;
const RECOVERY_EXPECTATION_MS: f64 = 1_000.0;
const POPUP_FIXTURE: &str = r#"<!doctype html><meta charset="utf-8">
<button id="healthy">healthy release control</button>
<button id="missing" onclick="window.open('about:blank', 'missing-popup')">missing ack</button>"#;

#[tokio::main]
async fn main() -> BrowserResult<()> {
    let iterations = iterations()?;
    let chrome_path = std::env::var_os("CHROME_PATH")
        .map(PathBuf::from)
        .or_else(detect_chrome)
        .ok_or("Chrome/Chromium is required for the popup benchmark")?;
    let url = format!("data:text/html;base64,{}", STANDARD.encode(POPUP_FIXTURE));
    let session = BrowserSession::start(&SessionOptions {
        port: available_port().await?,
        chrome_path: Some(chrome_path.clone()),
        profile: "popup-benchmark".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        frame_id: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await?;
    session.navigate(&url).await?;

    let point = session
        .evaluate("(() => { const r=document.querySelector('#healthy').getBoundingClientRect(); return {x:r.left+r.width/2,y:r.top+r.height/2}; })()")
        .await?;
    let control_x = point["x"].as_f64().ok_or("control point has no x")?;
    let control_y = point["y"].as_f64().ok_or("control point has no y")?;
    let mut healthy_ack_ms = Vec::new();
    let mut missing_ack_recovery_ms = Vec::new();
    let mut failures = Vec::new();
    for iteration in 1..=iterations {
        let cdp = session.raw_cdp()?;
        if let Err(error) = cdp
            .dispatch_mouse_event("mousePressed", control_x, control_y, Some("left"), Some(1))
            .await
        {
            failures.push(json!({
                "iteration": iteration,
                "step": "healthy_control_press",
                "error": error.to_string()
            }));
        } else {
            let ack_started = Instant::now();
            match cdp
                .dispatch_mouse_event_with_timeout(
                    "mouseReleased",
                    control_x,
                    control_y,
                    Some("left"),
                    Some(1),
                    Duration::from_millis(500),
                )
                .await
            {
                Ok(_) => healthy_ack_ms.push(ack_started.elapsed().as_secs_f64() * 1_000.0),
                Err(error) => failures.push(json!({
                    "iteration": iteration,
                    "step": "healthy_control_release",
                    "error": error.to_string()
                })),
            }
        }
        let recovery_started = Instant::now();
        match session.click_expect_popup("css=#missing").await {
            Ok(outcome) => {
                if outcome.evidence.release_acknowledged {
                    failures.push(json!({
                        "iteration": iteration,
                        "step": "missing_ack_classification",
                        "error": "synchronous popup release unexpectedly acknowledged"
                    }));
                } else {
                    missing_ack_recovery_ms
                        .push(recovery_started.elapsed().as_secs_f64() * 1_000.0);
                }
                if let Err(error) = session.close_target(&outcome.popup_id).await {
                    failures.push(json!({"iteration": iteration, "step": "close_missing", "error": error.to_string()}));
                }
            }
            Err(error) => failures.push(json!({
                "iteration": iteration,
                "step": "missing_ack_click_expect_popup",
                "error": error.to_string()
            })),
        }
    }
    session.close().await?;

    let recovery_breaches = missing_ack_recovery_ms
        .iter()
        .filter(|latency| **latency >= RECOVERY_EXPECTATION_MS)
        .count();
    let report = json!({
        "schema_version": 1,
        "benchmark": "glass-popup-completion-v1",
        "generated_at_utc": Utc::now().to_rfc3339(),
        "git_revision": command_output("git", &["rev-parse", "HEAD"]),
        "git_worktree_clean": command_output("git", &["status", "--porcelain"])
            .as_deref() == Some(""),
        "raw_artifact_path": std::env::var("GLASS_POPUP_BENCH_ARTIFACT")
            .unwrap_or_else(|_| "stdout".to_string()),
        "command": {
            "program": "cargo",
            "arguments": ["run", "--locked", "--release", "--example", "popup_benchmark"],
            "environment": {
                "CHROME_PATH": chrome_path,
                "GLASS_POPUP_BENCH_ITERATIONS": iterations.to_string()
            }
        },
        "environment": {
            "os": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
            "host": std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string()),
            "chrome_version": command_output(chrome_path.to_string_lossy().as_ref(), &["--version"]),
            "rust": command_output("rustc", &["--version"]),
            "glass_version": env!("CARGO_PKG_VERSION")
        },
        "iterations": iterations,
        "healthy_release_ack_control": distribution(&mut healthy_ack_ms),
        "missing_ack_recovery": {
            "distribution": distribution(&mut missing_ack_recovery_ms),
            "expected_under_ms": RECOVERY_EXPECTATION_MS,
            "expectation_breaches": recovery_breaches
        },
        "failures": failures,
        "claim_policy": {
            "minimum_samples_per_reported_path": MIN_CLAIM_SAMPLES,
            "one_sample_supports_claim": false,
            "healthy_ack_claim_eligible": healthy_ack_ms.len() >= MIN_CLAIM_SAMPLES,
            "missing_ack_recovery_claim_eligible": missing_ack_recovery_ms.len() >= MIN_CLAIM_SAMPLES
                && recovery_breaches == 0
        }
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn command_output(command: &str, arguments: &[&str]) -> Option<String> {
    let output = Command::new(command).args(arguments).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn distribution(samples: &mut [f64]) -> serde_json::Value {
    samples.sort_by(f64::total_cmp);
    json!({
        "samples": samples.len(),
        "p50_ms": percentile(samples, 0.50),
        "p95_ms": percentile(samples, 0.95),
        "max_ms": samples.last().copied()
    })
}

fn percentile(samples: &[f64], quantile: f64) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let index = ((samples.len() - 1) as f64 * quantile).ceil() as usize;
    samples.get(index).copied()
}

fn iterations() -> BrowserResult<usize> {
    let raw = std::env::var("GLASS_POPUP_BENCH_ITERATIONS")
        .unwrap_or_else(|_| DEFAULT_ITERATIONS.to_string());
    raw.parse::<usize>()
        .ok()
        .filter(|count| (1..=MAX_ITERATIONS).contains(count))
        .ok_or_else(|| format!("GLASS_POPUP_BENCH_ITERATIONS must be 1..={MAX_ITERATIONS}").into())
}

async fn available_port() -> BrowserResult<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_sample_is_reported_but_never_claim_eligible() {
        let mut sample = [12.0];
        let report = distribution(&mut sample);
        assert_eq!(report["samples"], 1);
        assert!(sample.len() < MIN_CLAIM_SAMPLES);
    }
}
