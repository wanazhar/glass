# Glass native engine browser slice 272: external script evaluation failures

Status: completed locally.

## Objective

Make external script evaluation failures follow the browser resource lifecycle:
the owning script element receives `error`, no false `load` is delivered, and
the HTML document continues to its committed ready/load state.

## Contract

- External classic and module roots retain the owning script element identity
  through staging and evaluation.
- An ignorable QuickJS evaluation failure for an external root dispatches one
  non-bubbling `error` event on that element.
- A failed external root does not receive the provisional `load` event.
- The document continues through commit, ready-state, DOMContentLoaded, and
  window-load processing; later page scripts and lifecycle observers remain
  usable.
- Successfully evaluated external roots retain their existing `load` event.

## Context

- `docs/INDEX.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- Issue [#40](https://github.com/wanazhar/glass/issues/40)

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/tasks/native-engine-browser-272.md`

## Implementation

`NativePageScript` now carries an optional external script node index through
classic/module staging. When the bounded evaluator returns an ignorable script
failure, the content-process owner dispatches a typed `Error` event against
that element and records the node as failed. The later resource-event pass
skips its provisional `Load`, so one external root cannot report both outcomes.
The staged graph is still removed on static dependency failure before this
evaluation path, preserving the 271 contract.

## Tradeoffs and follow-up

The existing bounded evaluator classifies only its recognized script failures
as ignorable; unrelated worker/protocol failures remain hard errors. Inline
script element identity and full window `error`/`ErrorEvent` reporting are not
expanded here. Browser-grade task scheduling, parser timing, source maps,
module Web IDL identity, and complete script/error semantics remain issue #40
work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_reports_external_module_evaluation_failure_without_aborting_document --locked -- --nocapture --exact` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine module --locked -- --test-threads=1 --nocapture` (5 passed)

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider issue #40 gates.
