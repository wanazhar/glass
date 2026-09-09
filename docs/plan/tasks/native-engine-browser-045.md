---
id: native-engine-browser-045
scope: glass-browser/native-engine/form-encoding
status: done
depends-on: [native-engine-browser-044]
---

# BE-02u/BE-03w/BE-04aa: bounded POST form encodings

## Objective

Extend native POST form submission beyond urlencoded bodies while keeping the
request metadata explicit across the parent, content worker, and HTTP loader.

## Contract

- `method="post" enctype="multipart/form-data"` emits a bounded multipart
  body with deterministic collision-checked boundaries, quoted field names,
  CRLF framing, and the correct `Content-Type` boundary parameter.
- `method="post" enctype="text/plain"` emits one `name=value` record per line
  with CRLF separators and `Content-Type: text/plain`.
- The parent request, content-process IPC, redirect state, and HTTP request
  builder preserve the selected content type; a POST-to-GET redirect clears
  both body and content-type state.
- Existing GET and urlencoded-POST behavior, submit events, validation,
  external form ownership, and bounded body limits remain intact.
- Unsupported encodings still fail explicitly instead of being silently
  coerced.

## Deliberate boundary and tradeoffs

This is a bounded text-field encoding slice. It does not implement file input
parts, multipart filenames/content types, `FormData`, live Web IDL identity,
submitter `formenctype` overrides, target contexts, or full HTML encoding
parity. The multipart boundary is deterministic and collision-checked rather
than random; that makes fixtures reproducible while retaining a bounded
fail-closed path for boundary-like field data.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `native_local_form_submission_supports_multipart_and_text_plain`
- `native_content_process_form_submission_supports_multipart_and_text_plain`
- `cargo fmt --all -- --check`
- `git diff --check`

Remote CI, source push, release, tag, registry publication, browser-parity,
and issue-closure claims remain pending the wider browser-complete gates.
