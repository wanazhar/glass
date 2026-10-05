---
id: native-engine-browser-874
scope: glass-browser/native-engine/parent-owned-video-poster
status: in-progress
depends-on: [native-engine-browser-873]
---

# Glass native-engine browser slice 874: parent-owned video poster

## Objective

Implement the uncovered HTML `<video poster>` image request and static poster
paint path for process-backed HTTP(S) documents. Preserve the browser parent's
exclusive network and cookie authority, and keep poster state independent of
the video's media-source state.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#parent-owned-video-posters`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/architecture/native-engine.md` — native resource brokers, media and
  image paint paths, profile capability matrix
- `crates/glass-browser/src/browser/native_engine/content_process.rs` —
  parent-brokered image and media loading, dynamic document mutation
- `crates/glass-browser/src/browser/native_engine/dom.rs` — image/media state,
  URL discovery, wire bounds
- `crates/glass-browser/src/browser/native_engine/paint.rs` and `layout.rs` —
  replaced-element paint and video geometry
- `crates/glass-browser/src/browser/native_engine/javascript.rs` —
  `HTMLVideoElement.poster` reflection and attribute mutation
- `crates/glass-browser/tests/native_engine.rs` — process-backed HTTP fixture
- [HTML Standard: video element and poster algorithm](https://html.spec.whatwg.org/multipage/media.html#the-video-element)
- [HTML Standard: replaced-element rendering](https://html.spec.whatwg.org/multipage/rendering.html#replaced-elements)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- For each attached HTML `<video>` with a nonempty `poster`, resolve the URL
  against its owner Document and initiate a poster request for initial
  documents and runtime-created/updated elements. Reflect the `poster`
  attribute through `HTMLVideoElement.poster`. Replacing or removing the
  attribute replaces or clears the poster without restarting the media `src`.
- HTTP(S) poster requests use the parent image broker and image CSP policy.
  Match the HTML request's `include` credentials mode and use-URL-credentials
  flag; the owner Document supplies the effective referrer policy. The parent
  selects request cookies and accepts eligible response cookies. The content
  process receives only bounded image data and its URL-scoped script-visible
  cookie projection; it never receives cookie headers, the complete jar, or
  HttpOnly values.
- Poster decode/network/CSP failure leaves no poster and does not fail
  navigation or dispatch `<img>` load/error events on the `<video>`. The
  poster request participates in the owner Document's load delay. An unchanged
  poster URL is not fetched repeatedly; stale results must not replace a newer
  poster after attribute mutation.
- While no decoded video frame is available, paint the poster centered in the
  video replaced-element box with its aspect ratio preserved. Poster storage
  is separate from the media source resource and is bounded by existing
  native image transfer/dimension/resource limits.
- Do not claim video frame decoding/playback, lazy poster resumption, general
  preload behavior, complete media/referrer/CSP conformance, WPT coverage, or
  cross-platform certification.

## Tradeoff

The native media owner currently has no decoded video-frame paint source.
Showing a parent-fetched poster gives ordinary pages their intended visual
preview without pretending to provide playback. The dedicated poster state
allows a later decoder/frame pipeline to supersede it without coupling an
image request to media-source readiness or playback state.

## Path

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs` (only if the
  existing local-document image lifecycle requires matching poster state)
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-874.md`
- `docs/plan/reviews/native-engine-browser-874-01.md`

## Verification

- Add a process-backed HTTP regression for an initial poster and a runtime
  `video.poster` change. Use separate loopback ports to verify the parent
  includes the eligible cookie on the cross-origin poster request. Verify
  relative URL resolution, request ordering, matching image paint/aspect
  ratio, no duplicate unchanged request, removal or replacement behavior,
  and that poster failure does not fail navigation.
- Prove cookie ownership in the same real-HTTP path: the parent selects an
  HttpOnly request cookie and accepts an HttpOnly poster response cookie for
  a later authorized request, while `document.cookie` stays filtered.
- Cover the image-CSP denial path and typed/quiet failure behavior; do not
  expose response headers or cookie values to the content process.
- Use the shared `/home/ubuntu/work/glass/target`. After the coherent code
  batch, run one package/target-scoped `cargo check` before the exact
  process-backed regression and focused deterministic tests. Do not invoke
  Cargo after individual edits, run unrelated workspace targets, or run
  `cargo clean`.
- Run Rust formatting, `git diff --check`, and all four maintainer
  documentation gates after final docs edits. Reuse explicit existing CLI
  binaries for documentation coverage.
- Commit implementation locally with a focused Conventional Commit before
  independent review. Do not push or claim remote CI. Keep Issue #40 open.
