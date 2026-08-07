# Capture path investigation

Measured on 2026-07-10 with Chromium 149.0.7827.114, Rust 1.97.0, and a
four-core Arm Neoverse-N1 Linux host. All focused comparisons use a fixed
1280×720 viewport at device scale factor 1 and exclude browser startup,
navigation, and file I/O.

## Recommendation

`Page.startScreencast` is the only proposed strategy that fits Glass now. It
uses the existing CDP command actor and event subscription, works with stock
Chrome, and can be tested without changing the production screenshot API.
Keep `Page.captureScreenshot` for exact on-demand and future full-page captures.

The other strategies are not near-term capture optimizations:

1. OS capture requires headed windows, permissions, window/tab mapping,
   cropping, HiDPI handling, and separate Windows/macOS/Linux backends. It also
   risks capturing unrelated desktop content.
2. A shared-memory ring needs a browser-side producer. Stock Chrome does not
   expose compositor frames to Glass, so this means maintaining a Chromium
   patch or an embedder.
3. CEF/Servo/Wry embedding replaces the Chrome lifecycle and automation
   backend. Wry delegates to platform webviews rather than providing a uniform
   raw-frame API; Servo and CEF would be separate backend projects.

CDP screencast does not eliminate per-frame wire traffic. For N frames,
polling sends N capture commands and receives N responses. Screencast sends one
start command, N frame events, N acknowledgement commands/responses, and one
stop command. Its advantage is removing the serial capture request from the
frame-production critical path and pipelining compositor frames. The API is
experimental and still sends compressed base64 images in JSON.

References:

- [CDP captureScreenshot and screencast protocol](https://chromedevtools.github.io/devtools-protocol/tot/Page/)
- [Chromium PageHandler encoders and screencast implementation](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/content/browser/devtools/protocol/page_handler.cc)
- [Windows Desktop Duplication API](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/desktop-dup-api)
- [Apple ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit)
- [Linux ScreenCast portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html)

## Baseline code path and implemented follow-up

At the start of the investigation, `BrowserSession::screenshot_png()` called
`Page.captureScreenshot` with only `{"format":"png"}`. Chrome performed image
encoding. Rust received a WebSocket text message, parsed the full JSON into
`serde_json::Value`, copied its base64 string, and decoded that into
still-compressed PNG bytes with `base64` 0.22. There was no Rust pixel decode or
image encode.

After the measurements below, the production path adopted only the supported
low-risk changes:

- `optimizeForSpeed=true` selects Chrome's lossless fast PNG encoder.
- The CDP base64 `String` is moved out of the JSON value instead of copied.
- MCP and structured visual observations reuse the base64 payload rather than
  decoding and immediately re-encoding it.

The production path did not adopt lossy JPEG, slow WebP, forced half-resolution
capture, or `base64-simd`.

Re-running the existing warm-session benchmark after this change produced 25
production PNG screenshots at 38.09 ms/frame versus the 49.36 ms/frame baseline,
a 22.8% latency reduction (about 20.3 to 26.3 FPS). Structured observation with
an explicit screenshot fell from 49.02 to 38.31 ms/frame.

There is no `clip`, `captureBeyondViewport`, full-page stitching, or device
metrics override in the runtime path. Current output is viewport-only. The
normal `--window-size=1280,720` launch produced a 1280×633 content viewport in
this environment, while the Playwright comparison explicitly used 1280×720.
The focused benchmark fixes this with `Emulation.setDeviceMetricsOverride` and
asserts the encoded image dimensions.

## Controlled capture benchmark

Run:

    RUST_LOG=warn \
      GLASS_CAPTURE_ITERATIONS=50 \
      GLASS_CAPTURE_WARMUP=10 \
      cargo run --release --example capture_benchmark

The fixed local page is `crates/glass-browser/tests/fixtures/basic.html`. Results below are 50
captures after 10 warmups.

| Mode | Mean ms | FPS | Encoded bytes | Effect versus PNG |
| --- | ---: | ---: | ---: | --- |
| PNG, baseline defaults | 49.99 | 20.01 | 9,415 | baseline |
| PNG, `optimizeForSpeed=true` | 39.00 | 25.64 | 22,630 | 22.0% lower latency; 140% larger |
| JPEG quality 80 | 38.66 | 25.87 | 10,559 | 22.7% lower latency; lossy |
| WebP default | 83.32 | 12.00 | 4,898 | 66.7% higher latency |
| PNG, 640×360 clip | 49.99 | 20.00 | 3,683 | no meaningful latency change |

Independent 30-capture runs reproduced the same ordering: baseline PNG 50.02
ms, fast PNG 38.88 ms, JPEG 38.90 ms, WebP 83.89 ms, and half-scale PNG 48.83
ms. Resolution is therefore not the limiter for this page. WebP is a size
optimization but a capture-loop regression.

Client microbenchmarks on a representative 12,556-byte base64 response:

| Client stage | Mean ms |
| --- | ---: |
| `serde_json` response parse | 0.00297 |
| base64 payload copy | 0.00036 |
| `base64` 0.22 decode | 0.00651 |
| `base64-simd` 0.8 decode | 0.00418 |

SIMD improved the decoder by about 36%, but saved only 0.0023 ms per frame.
It cannot materially change a 50 ms capture cycle. Avoiding decode/re-encode in
MCP image responses remains sensible for allocations, but is not an FPS fix.

## Profiling

`cargo flamegraph` was run over 300 baseline-PNG captures because a single
50 ms cycle yields too few on-CPU samples. Chrome was prelaunched so its child
processes were not inherited into the Rust profile. The release profiling build
kept debug symbols. The host initially blocked `perf` with
`perf_event_paranoid=4`; it was temporarily set to 1 and restored after capture.

    CARGO_TARGET_DIR=/tmp/glass-profile \
      CARGO_PROFILE_RELEASE_DEBUG=2 \
      CARGO_PROFILE_RELEASE_STRIP=none \
      cargo build --release --example capture_benchmark

    CARGO_TARGET_DIR=/tmp/glass-profile \
      CARGO_PROFILE_RELEASE_DEBUG=2 \
      CARGO_PROFILE_RELEASE_STRIP=none \
      GLASS_CDP_PORT=9333 \
      GLASS_CAPTURE_MODE=png_baseline_default \
      GLASS_CAPTURE_ITERATIONS=300 \
      GLASS_CAPTURE_WARMUP=10 \
      GLASS_CAPTURE_SKIP_MICROBENCH=1 \
      cargo flamegraph --profile release \
        --example capture_benchmark \
        --output /tmp/glass-capture.svg

Only 136 on-CPU samples were collected over roughly 15 seconds, confirming
that the Rust process spends nearly all wall time asleep waiting for Chrome.
Within those sparse CPU samples, WebSocket/event processing, base64, and JSON
were visible, but CPU percentages must not be confused with wall-time shares.

Stage timing over the same run was more useful:

- CDP command through parsed response: 49.968 ms/frame.
- Base64 payload copy: 0.0027 ms/frame.
- Base64 decode: 0.0223 ms/frame under profiling.

A ten-capture Chrome trace using the `devtools` category measured
`EncodeBitmapAsPngSlow` at 13.59 ms/frame while the capture command averaged
50.27 ms. Approximately 36.7 ms remained in browser surface capture/frame
scheduling, process/transport wait, and small client overhead. A Rust
flamegraph cannot subdivide that separate-process wait.

To repeat the Chrome trace:

    GLASS_CAPTURE_MODE=png_baseline_default \
      GLASS_CAPTURE_ITERATIONS=1 \
      GLASS_CAPTURE_WARMUP=1 \
      GLASS_CAPTURE_SKIP_MICROBENCH=1 \
      GLASS_CAPTURE_CHROME_TRACE=1 \
      GLASS_CAPTURE_TRACE_ITERATIONS=10 \
      cargo run --release --example capture_benchmark

## Screencast PoC

The standalone `examples/screencast_benchmark.rs` uses a deterministic animated
data-URL page. It subscribes before starting the stream, acknowledges each frame
before expensive work, moves base64 decoding to Tokio's blocking pool, validates
image dimensions, and hashes every image to prove that frames change.

    RUST_LOG=warn \
      GLASS_SCREENCAST_FRAMES=120 \
      GLASS_SCREENCAST_WARMUP=10 \
      cargo run --release --example screencast_benchmark

Both sides use the same page, viewport, format, quality, and Chrome binary.
Polling uses `optimizeForSpeed=true`, matching Chromium's screencast encoder.

| Format | Polling FPS | Screencast FPS | Relative throughput | Event lag |
| --- | ---: | ---: | ---: | ---: |
| JPEG quality 80 | 12.00 | 60.00 | 5.00× | 0 |
| PNG fast encode | 7.68 | 43.68 | 5.68× | 0 |

All 120 frames in each run had the expected 1280×720 dimensions and distinct
hashes. JPEG averaged about 14 KB/frame. PNG averaged about 845 KB/frame, which
lowered stream FPS and increased acknowledgement latency; JPEG is the practical
streaming default.

This result proves that streaming semantics outperform serial screenshot
polling on this animated workload. It does not prove an intrinsic Rust advantage:
Playwright can access the same raw CDP screencast methods. Screencast also cannot
replace an exact-time one-shot screenshot or a future full-page capture API.

Production integration now routes screencast frames through a dedicated
two-frame typed channel rather than the generic event broadcast. Glass
acknowledges every valid frame before bounded delivery, reports received and
dropped counts, pins start/stop/frame delivery to the selected CDP session, and
stops the stream when its scope is dropped. Callers should retain a polling
fallback because the CDP screencast API remains experimental.
