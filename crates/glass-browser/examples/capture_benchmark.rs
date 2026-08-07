use base64::{Engine, engine::general_purpose::STANDARD};
use glass_browser::browser::chrome::detect_chrome;
use glass_browser::browser::session::{
    BrowserResult, BrowserSession, InteractionMode, SessionOptions,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;

const DEFAULT_ITERATIONS: usize = 50;
const DEFAULT_WARMUP: usize = 10;
const VIEWPORT_WIDTH: u32 = 1_280;
const VIEWPORT_HEIGHT: u32 = 720;
const MICROBENCH_ITERATIONS: usize = 1_000;

struct CaptureSpec {
    name: &'static str,
    format: &'static str,
    params: Value,
}

struct CaptureSample {
    command: Duration,
    payload_copy: Duration,
    decode: Duration,
    base64_len: usize,
    image: Vec<u8>,
}

#[tokio::main]
async fn main() -> BrowserResult<()> {
    let iterations = positive_env("GLASS_CAPTURE_ITERATIONS", DEFAULT_ITERATIONS);
    let warmup = positive_env("GLASS_CAPTURE_WARMUP", DEFAULT_WARMUP);
    let mode = std::env::var("GLASS_CAPTURE_MODE").ok();
    let configured_port = std::env::var("GLASS_CDP_PORT")
        .ok()
        .and_then(|value| value.parse().ok());
    let (port, chrome_path, attach) = match configured_port {
        Some(port) => (port, None, true),
        None => {
            let Some(chrome_path) = detect_chrome() else {
                return Err("Chrome/Chromium is required for the capture benchmark".into());
            };
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let port = listener.local_addr()?.port();
            drop(listener);
            (port, Some(chrome_path), false)
        }
    };

    let session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path,
        profile: if attach {
            "default".to_string()
        } else {
            "capture-benchmark".to_string()
        },
        incognito: !attach,
        attach,
        target_id: None,
        frame_id: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
        audit: false,
        policy: None,
    })
    .await?;
    session
        .raw_cdp()?
        .send(
            "Emulation.setDeviceMetricsOverride",
            Some(json!({
                "width": VIEWPORT_WIDTH,
                "height": VIEWPORT_HEIGHT,
                "deviceScaleFactor": 1,
                "mobile": false
            })),
        )
        .await?;
    let fixture = include_str!("../tests/fixtures/basic.html");
    let url = format!("data:text/html;base64,{}", STANDARD.encode(fixture));
    session.navigate(&url).await?;
    let viewport = session
        .evaluate(
            "({innerWidth, innerHeight, outerWidth, outerHeight, devicePixelRatio, \
             scrollWidth: document.documentElement.scrollWidth, \
             scrollHeight: document.documentElement.scrollHeight})",
        )
        .await?;
    if viewport["innerWidth"] != VIEWPORT_WIDTH || viewport["innerHeight"] != VIEWPORT_HEIGHT {
        return Err(format!("unexpected benchmark viewport: {viewport}").into());
    }
    let chrome_trace = if std::env::var_os("GLASS_CAPTURE_CHROME_TRACE").is_some() {
        Some(trace_current_png(&session).await?)
    } else {
        None
    };

    let specs = [
        CaptureSpec {
            name: "png_baseline_default",
            format: "png",
            params: json!({"format": "png"}),
        },
        CaptureSpec {
            name: "png_optimize_for_speed",
            format: "png",
            params: json!({"format": "png", "optimizeForSpeed": true}),
        },
        CaptureSpec {
            name: "jpeg_quality_80",
            format: "jpeg",
            params: json!({"format": "jpeg", "quality": 80}),
        },
        CaptureSpec {
            name: "webp_default",
            format: "webp",
            params: json!({"format": "webp"}),
        },
        CaptureSpec {
            name: "png_half_scale",
            format: "png",
            params: json!({
                "format": "png",
                "clip": {
                    "x": 0,
                    "y": 0,
                    "width": VIEWPORT_WIDTH,
                    "height": VIEWPORT_HEIGHT,
                    "scale": 0.5
                }
            }),
        },
    ];

    let mut results = Vec::new();
    let mut reference_base64 = None;
    for spec in specs
        .iter()
        .filter(|spec| mode.as_deref().is_none_or(|mode| mode == spec.name))
    {
        for _ in 0..warmup {
            let _ = capture(&session, spec).await?;
        }
        let mut samples = Vec::with_capacity(iterations);
        for _ in 0..iterations {
            samples.push(capture(&session, spec).await?);
        }
        let last = samples
            .last()
            .ok_or("capture benchmark produced no samples")?;
        let size = imagesize::blob_size(&last.image)?;
        validate_format(spec.format, &last.image)?;
        let command = samples.iter().map(|sample| sample.command).collect();
        let payload_copy = samples.iter().map(|sample| sample.payload_copy).collect();
        let decode = samples.iter().map(|sample| sample.decode).collect();
        let total = samples
            .iter()
            .map(|sample| sample.command + sample.payload_copy + sample.decode)
            .collect();
        results.push(json!({
            "operation": spec.name,
            "iterations": iterations,
            "command_including_json_ms": summarize(command),
            "payload_copy_ms": summarize(payload_copy),
            "base64_decode_ms": summarize(decode),
            "total_ms": summarize(total),
            "frames_per_second": 1_000.0 / average_ms(samples.iter().map(|sample| {
                sample.command + sample.payload_copy + sample.decode
            })),
            "base64_bytes": last.base64_len,
            "image_bytes": last.image.len(),
            "width": size.width,
            "height": size.height
        }));
        if spec.name == "png_baseline_default" {
            let response = session
                .raw_cdp()?
                .send("Page.captureScreenshot", Some(spec.params.clone()))
                .await?;
            reference_base64 = response["data"].as_str().map(String::from);
        }
    }

    let microbench = match (
        reference_base64,
        std::env::var_os("GLASS_CAPTURE_SKIP_MICROBENCH"),
    ) {
        (Some(data), None) => Some(run_microbench(&data)?),
        _ => None,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "tool": "glass-capture-benchmark",
            "chrome": "Chrome/Chromium via CDP",
            "viewport": viewport,
            "iterations": iterations,
            "warmup": warmup,
            "results": results,
            "client_microbench": microbench,
            "chrome_trace": chrome_trace
        }))?
    );
    session.close().await
}

async fn trace_current_png(session: &BrowserSession) -> BrowserResult<Value> {
    let trace_iterations = positive_env("GLASS_CAPTURE_TRACE_ITERATIONS", 10);
    let mut events = session.raw_cdp()?.subscribe_events_with_params();
    session
        .raw_cdp()?
        .send(
            "Tracing.start",
            Some(json!({
                "categories": "devtools",
                "options": "record-as-much-as-possible",
                "transferMode": "ReportEvents"
            })),
        )
        .await?;
    let mut commands = Vec::with_capacity(trace_iterations);
    let mut image = Vec::new();
    for _ in 0..trace_iterations {
        let started = Instant::now();
        let response = session
            .raw_cdp()?
            .send("Page.captureScreenshot", Some(json!({"format": "png"})))
            .await?;
        commands.push(started.elapsed());
        image = STANDARD.decode(
            response["data"]
                .as_str()
                .ok_or("traced capture response contained no data")?,
        )?;
    }
    session.raw_cdp()?.send("Tracing.end", None).await?;

    let trace_events = tokio::time::timeout(Duration::from_secs(10), async {
        let mut collected = Vec::new();
        loop {
            let event = events.recv().await.map_err(|error| error.to_string())?;
            match event.method.as_str() {
                "Tracing.dataCollected" => {
                    if let Some(values) = event.params["value"].as_array() {
                        collected.extend(values.iter().cloned());
                    }
                }
                "Tracing.tracingComplete" => break Ok::<_, String>(collected),
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| "timed out waiting for Chrome trace data")??;

    let mut durations_by_name: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for event in trace_events {
        let Some(name) = event["name"].as_str() else {
            continue;
        };
        let Some(duration_us) = event["dur"].as_f64() else {
            continue;
        };
        durations_by_name
            .entry(name.to_string())
            .or_default()
            .push(duration_us / 1_000.0);
    }
    let phases = durations_by_name
        .into_iter()
        .map(|(name, durations)| {
            let total = durations.iter().sum::<f64>();
            json!({
                "name": name,
                "count": durations.len(),
                "average_ms": total / durations.len() as f64,
                "total_ms": total,
                "maximum_ms": durations.iter().copied().fold(0.0, f64::max)
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "iterations": trace_iterations,
        "capture_command_ms": summarize(commands),
        "image_bytes": image.len(),
        "devtools_complete_events": phases
    }))
}

async fn capture(session: &BrowserSession, spec: &CaptureSpec) -> BrowserResult<CaptureSample> {
    let started = Instant::now();
    let response = session
        .raw_cdp()?
        .send("Page.captureScreenshot", Some(spec.params.clone()))
        .await?;
    let command = started.elapsed();
    let data = response["data"]
        .as_str()
        .ok_or("capture response contained no base64 data")?;

    let copy_started = Instant::now();
    let data = data.to_string();
    let payload_copy = copy_started.elapsed();
    let decode_started = Instant::now();
    let image = STANDARD.decode(data.as_bytes())?;
    let decode = decode_started.elapsed();
    Ok(CaptureSample {
        command,
        payload_copy,
        decode,
        base64_len: data.len(),
        image,
    })
}

fn run_microbench(data: &str) -> BrowserResult<Value> {
    let response_json = serde_json::to_string(&json!({
        "id": 1,
        "result": {"data": data}
    }))?;
    let json_parse = time_loop(MICROBENCH_ITERATIONS, || {
        let parsed: Value = serde_json::from_str(black_box(&response_json)).unwrap();
        black_box(parsed["result"]["data"].as_str().unwrap().len());
    });
    let payload_copy = time_loop(MICROBENCH_ITERATIONS, || {
        black_box(black_box(data).to_string());
    });
    let base64_decode = time_loop(MICROBENCH_ITERATIONS, || {
        black_box(STANDARD.decode(black_box(data.as_bytes())).unwrap());
    });
    let base64_simd_decode = time_loop(MICROBENCH_ITERATIONS, || {
        black_box(
            base64_simd::STANDARD
                .decode_to_vec(black_box(data.as_bytes()))
                .unwrap(),
        );
    });
    Ok(json!({
        "iterations": MICROBENCH_ITERATIONS,
        "representative_base64_bytes": data.len(),
        "serde_json_parse_ms": summarize(json_parse),
        "payload_copy_ms": summarize(payload_copy),
        "base64_0_22_decode_ms": summarize(base64_decode),
        "base64_simd_0_8_decode_ms": summarize(base64_simd_decode)
    }))
}

fn time_loop(mut iterations: usize, mut operation: impl FnMut()) -> Vec<Duration> {
    let mut samples = Vec::with_capacity(iterations);
    while iterations > 0 {
        let started = Instant::now();
        operation();
        samples.push(started.elapsed());
        iterations -= 1;
    }
    samples
}

fn summarize(mut samples: Vec<Duration>) -> Value {
    samples.sort_unstable();
    let total: Duration = samples.iter().copied().sum();
    let average = total.as_secs_f64() * 1_000.0 / samples.len() as f64;
    let percentile = |ratio: f64| {
        let index = ((samples.len() - 1) as f64 * ratio).round() as usize;
        samples[index].as_secs_f64() * 1_000.0
    };
    json!({
        "average": average,
        "p50": percentile(0.50),
        "p95": percentile(0.95),
        "p99": percentile(0.99)
    })
}

fn average_ms(samples: impl Iterator<Item = Duration>) -> f64 {
    let samples: Vec<_> = samples.collect();
    samples.iter().sum::<Duration>().as_secs_f64() * 1_000.0 / samples.len() as f64
}

fn validate_format(format: &str, bytes: &[u8]) -> BrowserResult<()> {
    let valid = match format {
        "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpeg" => bytes.starts_with(&[0xff, 0xd8]) && bytes.ends_with(&[0xff, 0xd9]),
        "webp" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(format!("invalid {format} capture payload").into())
    }
}

fn positive_env(name: &str, fallback: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}
