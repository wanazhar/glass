# Glass native engine browser slice 273: inline script evaluation failures

Status: completed locally.

## Objective

Preserve inline script-root identity through native DOM discovery and
evaluation so an inline runtime failure has the same bounded resource outcome
as an external script failure: the owning script element receives `error`, the
failed root stops, and the document still completes its lifecycle.

## Contract

- Inline classic and module roots retain their owning script element index from
  DOM discovery through local and content-process staging.
- An ignorable QuickJS evaluation failure for an inline root dispatches one
  non-bubbling `error` event on that element.
- Code after the throwing statement in the failed root does not execute.
- The failed root does not prevent document commit, ready-state completion, or
  later JavaScript evaluation.
- External-root behavior from slice 272 remains unchanged.

## Context

- `docs/INDEX.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- Issue [#40](https://github.com/wanazhar/glass/issues/40)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/tasks/native-engine-browser-273.md`

## Implementation

`NativePageScriptSource::Inline` and `ModuleInline` now carry the source
script's node index. Local inline execution and content-process staging pass
that index into the already-bounded script failure path. When evaluation
returns an ignorable failure, the owner dispatches the typed element `Error`
event, records the failed node so no provisional `Load` is emitted, and
continues the document commit/lifecycle sequence. The same identity is used
for classic and module roots.

## Tradeoffs and follow-up

This slice deliberately reuses the existing bounded element-event contract;
full browser `window` error propagation, `ErrorEvent` fields, parser-accurate
streaming execution timing, source maps, and complete script/Web IDL semantics
remain issue #40 work. Unrecognized evaluator or worker/protocol failures stay
hard errors rather than being silently converted into element events.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_inline_script_failure_dispatches_error_without_aborting_document --locked` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_ --locked` (114 passed)

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider issue #40 gates.
