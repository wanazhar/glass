---
id: visual-015
scope: visual capture and verification
status: complete
depends-on: [topology-011, observe-013]
---

# Add exact visual capture and comparison

## Objective

Support reproducible viewport, element, and full-page evidence plus opt-in
streaming without retaining image data in default session state.

## Context

- `docs/architecture/automation.md`
- `benchmarks/capture-report.md`

## Path

- `src/browser/cdp.rs`
- `src/browser/session.rs`
- `src/mcp/`
- `examples/`
- `benchmarks/`
- `tests/`
- `docs/architecture/`

## Requirements

- Make viewport, scale, clip, format, quality, and full-page semantics explicit.
- Return image dimensions and capture metadata.
- Route screencast frames through a dedicated bounded channel with guaranteed
  acknowledgement, lag/drop metrics, and cleanup.
- Add an optional comparison primitive with bounded output; do not add a heavy
  image stack to default builds without scorecard evidence.
- Move base64/image payloads once and never cache them implicitly.

## Verification

- Dimension, clip, full-page, HiDPI, element, dynamic-page, stream-lag, and
  cleanup tests on real Chrome.
- Visual paths report latency, peak RSS, bytes, and binary-size delta.

## Completion evidence

- Exact viewport, clip, scale, HiDPI, full-page, element, dynamic geometry, and
  scoped screencast behavior pass the real-Chrome smoke test.
- The 1280x720 JPEG stream delivered 10 distinct frames at 60.9 FPS with zero
  drops; serial screenshot polling delivered 10.0 FPS on the same fixture.
- Short release runs peaked at 209,960 KiB RSS for capture and 207,184 KiB for
  screencast (Glass benchmark process, including session orchestration).
- The optional `visual-compare` feature added zero bytes to the 5,054,640-byte
  release CLI because the library-only primitive is removed when unused. It
  remains absent from default dependency builds.
- `cargo test --all-targets --all-features`, strict all-feature Clippy, and the
  focused real-Chrome test pass at `649b226`.
