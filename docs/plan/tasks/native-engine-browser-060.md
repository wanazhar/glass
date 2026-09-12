id: native-engine-browser-060
scope: glass-browser/native-engine/form-data-constructor
status: done
depends-on: [native-engine-browser-059]
---

This task records the historical text-only FormData checkpoint. Its
file-control rejection boundary was superseded by
`native-engine-browser-247`, which added bounded File/FileList and
file-valued FormData support.

# BE-03ae/BE-04ap: bounded FormData form constructor

## Objective

Make `new FormData(form)` useful for normal text-based forms while keeping
the existing Rust-owned form association and successful-control rules as the
single source of truth in both local and process-backed documents.

## Contract

- The constructor accepts a form element and collects named, enabled
  successful controls in document order, including controls associated through
  an external `form` attribute.
- Submit/reset/button/image controls and unchecked checkbox/radio controls are
  excluded; the existing bounded append/set/delete/read and fetch/XHR
  serialization paths remain unchanged.
- File controls fail closed with a `TypeError` rather than producing a false
  string value or an invalid multipart body.
- The behavior is covered in the local owner and the sandboxed content-process
  owner; Rust-provided form-owner metadata crosses the existing typed snapshot
  boundary.

## Deliberate boundary and tradeoffs

This slice intentionally does not claim `File`, `Blob`, `FileList`, picker or
upload-progress behavior, the `formdata` event, submitter overloads, live Web
IDL iterator identity, or browser-parity exception/prototype details. A file
control rejects the whole constructor so callers cannot mistake an incomplete
file representation for a valid upload; file/blob FormData support remains a
later dependency-ordered slice.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine form_data_constructor -- --nocapture` — 2 passed

Remote CI, source push, release, tag, registry publication, browser parity,
and issue-closure claims remain pending the wider browser-complete gates.
