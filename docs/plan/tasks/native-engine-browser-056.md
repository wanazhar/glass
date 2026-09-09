---
id: native-engine-browser-056
scope: glass-browser/native-engine/fetch-formdata-text
status: done
depends-on: [native-engine-browser-055]
---

# BE-03aa/BE-04al: bounded text FormData fetch bodies

## Objective

Support the common text-field `FormData` submission path through the existing
fetch/XHR request owner without adding a second multipart encoder or file
system surface.

## Contract

- The page realm exposes bounded `FormData` append/set/delete/get/getAll/has,
  `entries`, and `forEach` helpers for string fields.
- `fetch()` and XHR `send()` accept text-only FormData for POST requests and
  serialize deterministic multipart boundaries, field names, and values.
- The generated `Content-Type` includes the generated multipart boundary and
  remains subject to the existing size, CSP, mixed-content, cookie, referrer,
  redirect, CORS, and preflight policy.
- File-valued FormData parts are rejected instead of being converted into
  misleading text.

## Deliberate boundary and tradeoffs

Blob/file parts, file chooser integration, upload progress, streaming bodies,
multipart filename/content-type fidelity, `URLSearchParams`, structured clone,
and full FormData/Web IDL iterator identity remain open. The deterministic
boundary is session-local and bounded; it is sufficient for ordinary text API
forms but is not a complete browser multipart implementation.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `native_content_process_fetches_bounded_text_form_data` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine fetch -- --nocapture` — 9 passed

Remote CI, source push, release, tag, registry publication, browser parity,
and issue-closure claims remain pending the wider browser-complete gates.
