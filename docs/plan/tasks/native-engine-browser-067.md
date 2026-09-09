---
id: native-engine-browser-067
scope: glass-browser/native-engine/file-blob-form-data
status: done
depends-on: [native-engine-browser-066]
---

# BE-02/BE-03/BE-04: bounded text-backed File/Blob FormData parts

## Objective

Extend the shared local and sandboxed QuickJS bootstrap so ordinary
text-backed File/Blob uploads can use the existing FormData and fetch owner
without adding a crate or a second transport path.

## Contract

- `Blob` accepts bounded text or Blob parts and exposes capped `size`,
  normalized `type`, `text()`, and bounded `slice()` behavior.
- `File` reuses that text-backed Blob surface and adds bounded `name` and
  `lastModified` metadata.
- FormData `append` and `set` accept Blob/File values and optional filenames;
  `get`, `getAll`, `entries`, and `forEach` return the file value rather than
  the internal wire record.
- The existing multipart serializer emits deterministic `filename` and
  `Content-Type` headers for file parts in both local and child-owned fetches.
- Blob text is capped by the existing script text limit, and unsupported part
  shapes fail with typed JavaScript errors before transport.

## Deliberate boundary and tradeoffs

This is a text-backed upload slice, not binary browser parity. ArrayBuffer and
typed-array parts, streams, binary byte accounting, Blob URLs, file pickers,
disk-backed file inputs, upload progress, and full Blob/File/FormData Web IDL
identity remain open. Existing file input controls still fail closed because
there is no native picker/upload source yet. The implementation reuses the
existing fetch/CORS and multipart owner, keeping request policy and the
two-crate/process boundary unchanged.

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
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_file_and_blob_form_data_are_bounded_and_text_backed -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_uploads_bounded_file_blob_form_data -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_form_data_constructor_collects_text_controls -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_form_data_constructor_collects_text_controls -- --nocapture` — 1 passed

Remote CI, source push, release, tag, registry publication, browser parity,
and issue-closure claims remain pending the wider browser-complete gates.
