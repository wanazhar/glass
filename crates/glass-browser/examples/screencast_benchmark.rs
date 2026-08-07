use base64::{Engine, engine::general_purpose::STANDARD};
use glass_browser::browser::chrome::detect_chrome;
use glass_browser::browser::session::{
    BrowserResult, BrowserSession, InteractionMode, SessionOptions, VisualFormat,
};
use serde_json::{Value, json};
use std::collections::{HashSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::task::JoinSet;

const DEFAULT_FRAMES: usize = 120;
const DEFAULT_WARMUP_FRAMES: usize = 10;
const VIEWPORT_WIDTH: u32 = 1_280;
const VIEWPORT_HEIGHT: u32 = 720;
const ANIMATED_FIXTURE: &str = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <title>Glass Capture Loop</title>
    <style>
      html, body { width: 100%; height: 100%; margin: 0; overflow: hidden; }
      body { background: linear-gradient(135deg, #07152d, #23658d); color: white; }
      #card {
        position: absolute; width: 260px; height: 160px; border-radius: 28px;
        display: grid; place-items: center; font: 700 28px system-ui;
        background: linear-gradient(135deg, #ffb703, #fb5607);
        box-shadow: 0 24px 80px #0008; will-change: transform;
      }
      #counter { position: absolute; left: 32px; top: 24px; font: 24px ui-monospace; }
    </style>
  </head>
  <body>
    <div id="counter">0</div><div id="card">Glass</div>
    <script>
      const card = document.querySelector('#card');
      const counter = document.querySelector('#counter');
      let frame = 0;
      function animate(time) {
        const x = 500 + Math.sin(time / 430) * 420;
        const y = 280 + Math.cos(time / 610) * 210;
        card.style.transform = `translate(${x}px, ${y}px) rotate(${time / 30}deg)`;
        counter.textContent = String(++frame);
        requestAnimationFrame(animate);
      }
      requestAnimationFrame(animate);
    </script>
  </body>
</html>"#;

#[derive(Debug)]
struct DecodedFrame {
    index: usize,
    bytes: usize,
    width: usize,
    height: usize,
    hash: u64,
}

#[tokio::main]
async fn main() -> BrowserResult<()> {
    let frames = positive_env("GLASS_SCREENCAST_FRAMES", DEFAULT_FRAMES);
    let warmup = positive_env("GLASS_SCREENCAST_WARMUP", DEFAULT_WARMUP_FRAMES);
    let format = std::env::var("GLASS_SCREENCAST_FORMAT").unwrap_or_else(|_| "jpeg".to_string());
    if !matches!(format.as_str(), "jpeg" | "png") {
        return Err("GLASS_SCREENCAST_FORMAT must be jpeg or png".into());
    }
    let quality = std::env::var("GLASS_SCREENCAST_QUALITY")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|quality| *quality <= 100)
        .unwrap_or(80);

    let Some(chrome_path) = detect_chrome() else {
        return Err("Chrome/Chromium is required for the screencast benchmark".into());
    };
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path),
        profile: "screencast-benchmark".to_string(),
        incognito: true,
        attach: false,
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
    let url = format!(
        "data:text/html;base64,{}",
        STANDARD.encode(ANIMATED_FIXTURE)
    );
    session.navigate(&url).await?;

    let polling = benchmark_polling(&session, &format, quality, frames, warmup).await?;
    let screencast = benchmark_screencast(&session, &format, quality, frames, warmup).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "tool": "glass-screencast-poc",
            "semantics": "deterministic animated viewport; compressed images; no file I/O",
            "viewport": {
                "width": VIEWPORT_WIDTH,
                "height": VIEWPORT_HEIGHT,
                "deviceScaleFactor": 1
            },
            "format": format,
            "quality": quality,
            "frames": frames,
            "warmup_frames": warmup,
            "polling_capture_screenshot": polling,
            "cdp_start_screencast": screencast
        }))?
    );
    session.close().await
}

async fn benchmark_polling(
    session: &BrowserSession,
    format: &str,
    quality: u32,
    frames: usize,
    warmup: usize,
) -> BrowserResult<Value> {
    let params = capture_params(format, quality);
    for _ in 0..warmup {
        let _ = capture_and_decode(session, params.clone()).await?;
    }
    let started = Instant::now();
    let mut decoded = Vec::with_capacity(frames);
    for index in 0..frames {
        let mut frame = capture_and_decode(session, params.clone()).await?;
        frame.index = index;
        decoded.push(frame);
    }
    let elapsed = started.elapsed();
    summarize_frames(
        decoded,
        elapsed,
        json!({
            "commands": frames,
            "responses": frames,
            "optimizeForSpeed": true
        }),
    )
}

async fn benchmark_screencast(
    session: &BrowserSession,
    format: &str,
    quality: u32,
    frames: usize,
    warmup: usize,
) -> BrowserResult<Value> {
    let visual_format = match format {
        "jpeg" => VisualFormat::Jpeg,
        "png" => VisualFormat::Png,
        _ => return Err("unsupported screencast format".into()),
    };
    let started_command = Instant::now();
    let mut screencast = session
        .start_screencast(
            visual_format,
            quality as u8,
            VIEWPORT_WIDTH,
            VIEWPORT_HEIGHT,
        )
        .await?;
    let start_command = started_command.elapsed();

    for _ in 0..warmup {
        screencast
            .next_frame()
            .await
            .ok_or("screencast ended during warmup")?;
    }

    let wire_started = Instant::now();
    let mut first_arrival = None;
    let mut last_arrival = None;
    let mut decoders = JoinSet::new();
    let mut received = 0_usize;
    while received < frames {
        let frame = screencast
            .next_frame()
            .await
            .ok_or("screencast ended before the requested frame count")?;
        let arrival = Instant::now();
        first_arrival.get_or_insert(arrival);
        last_arrival = Some(arrival);
        let index = received;
        decoders.spawn_blocking(move || decode_frame(index, frame.data));
        received += 1;
    }
    let wire_elapsed = wire_started.elapsed();
    let stream_stats = screencast.stop().await?;
    let mut decoded = Vec::with_capacity(frames);
    while let Some(result) = decoders.join_next().await {
        decoded.push(result.map_err(|error| error.to_string())??);
    }
    decoded.sort_by_key(|frame| frame.index);
    let decoded_elapsed = wire_started.elapsed();
    let arrival_span = match (first_arrival, last_arrival) {
        (Some(first), Some(last)) => last.duration_since(first),
        _ => Duration::ZERO,
    };
    let details = json!({
        "start_command_ms": duration_ms(start_command),
        "wire_receive_ms": duration_ms(wire_elapsed),
        "wire_frames_per_second": frames as f64 / wire_elapsed.as_secs_f64(),
        "arrival_span_frames_per_second": if frames > 1 && !arrival_span.is_zero() {
            Some((frames - 1) as f64 / arrival_span.as_secs_f64())
        } else {
            None
        },
        "receive_plus_decode_ms": duration_ms(decoded_elapsed),
        "receive_plus_decode_fps": frames as f64 / decoded_elapsed.as_secs_f64(),
        "received_frames": stream_stats.received,
        "dropped_frames": stream_stats.dropped,
        "commands": frames + warmup + 2,
        "frame_events": frames + warmup,
        "note": "Glass ACKs frames before bounded channel delivery; decode uses Tokio's blocking pool"
    });
    summarize_frames(decoded, decoded_elapsed, details)
}

async fn capture_and_decode(
    session: &BrowserSession,
    params: Value,
) -> BrowserResult<DecodedFrame> {
    let response = session
        .raw_cdp()?
        .send("Page.captureScreenshot", Some(params))
        .await?;
    let data = response["data"]
        .as_str()
        .ok_or("capture response contained no data")?;
    decode_frame(0, data.to_string()).map_err(Into::into)
}

fn decode_frame(index: usize, data: String) -> Result<DecodedFrame, String> {
    let image = STANDARD
        .decode(data.as_bytes())
        .map_err(|error| error.to_string())?;
    let size = imagesize::blob_size(&image).map_err(|error| error.to_string())?;
    let mut hasher = DefaultHasher::new();
    image.hash(&mut hasher);
    Ok(DecodedFrame {
        index,
        bytes: image.len(),
        width: size.width,
        height: size.height,
        hash: hasher.finish(),
    })
}

fn summarize_frames(
    frames: Vec<DecodedFrame>,
    elapsed: Duration,
    details: Value,
) -> BrowserResult<Value> {
    if frames.is_empty() {
        return Err("capture produced no frames".into());
    }
    if frames.iter().any(|frame| {
        frame.width != VIEWPORT_WIDTH as usize || frame.height != VIEWPORT_HEIGHT as usize
    }) {
        return Err("capture produced an unexpected image size".into());
    }
    let distinct: HashSet<_> = frames.iter().map(|frame| frame.hash).collect();
    let average_bytes = frames.iter().map(|frame| frame.bytes).sum::<usize>() / frames.len();
    Ok(json!({
        "elapsed_ms": duration_ms(elapsed),
        "frames_per_second": frames.len() as f64 / elapsed.as_secs_f64(),
        "average_image_bytes": average_bytes,
        "distinct_frame_hashes": distinct.len(),
        "width": VIEWPORT_WIDTH,
        "height": VIEWPORT_HEIGHT,
        "details": details
    }))
}

fn capture_params(format: &str, quality: u32) -> Value {
    if format == "jpeg" {
        json!({"format": format, "quality": quality, "optimizeForSpeed": true})
    } else {
        json!({"format": format, "optimizeForSpeed": true})
    }
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn positive_env(name: &str, fallback: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}
