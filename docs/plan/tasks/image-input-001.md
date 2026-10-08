---
id: image-input-001
scope: crates/glass-mod-api Message, session records, providers, attachments
status: done
depends-on: []
---

## Objective

Image and screenshot input: the cheapest real capability gap in the delta
survey (every competitor accepts images; a bash-only agent cannot see a
failing UI). Implemented as byte passthrough — no codec, so the README's
size argument survives.

## Context

- `Message::Image { media_type, data }` (base64) and `Record::Image` with
  json/from_json/projection; the transcript shows a bounded placeholder
  (`[image: png · N KB]`) and export renders an italic marker.
- `polish::attach` sniffs PNG/JPEG/GIF/WebP magic bytes; images are capped
  at 1 MiB each and 2 MiB total (no decoding, so no resize — documented) and
  are appended as records at attach time, which makes them part of the
  conversation: every wire format replays them and resume keeps them.
- Adapters: Anthropic image block, OpenAI chat `image_url` data URI,
  Responses `input_image`. `default_estimate` gives an image a flat
  allowance (~2,300 tokens) — counting base64 bytes as text would invent
  roughly 1.7M phantom tokens and trigger instant compaction.
- Text attachments are unchanged and stay in the system prompt; images never
  do (they'd have no home in a text system block).

## Paths

- `crates/glass-mod-api/src/lib.rs`, `session.rs`, `polish.rs`,
  `provider/{mod,anthropic,openai}.rs`, `export.rs`, `main.rs` (attach call
  sites take `&mut Session` now, including `/context`).

## Verification

- Unit tests: block shapes for all three wire formats, flat token estimate,
  magic-byte sniffing, record placeholder, 1 MiB refusal.
- `scripts/smoke-image.py`: a real 1×1 PNG through `run --attach` reaches
  the mock endpoint as an image block and lands in the session log as
  `k: image`.
- `cargo test -p glass-core` (377 tests) and clippy.
