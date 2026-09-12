# Glass native engine browser slice 274: page script error reporting

Status: completed locally.

## Objective

Complete the bounded page-script error-reporting transition. An ignorable
classic or module evaluation failure must remain isolated from document
commit, report through the owning script element, and also expose the standard
window-level error observation surface used by ordinary pages.

## Contract

- A failed page-script root dispatches one non-bubbling `error` event to its
  owning script element.
- The same failure dispatches one window `ErrorEvent` with bounded message,
  filename, line, column, and underlying `Error` fields.
- `window.onerror` receives `(message, filename, lineno, colno, error)` for the
  window event, while ordinary element `onerror` handlers retain event-object
  semantics.
- The module error message retains the underlying evaluator exception rather
  than collapsing to a generic module-failure string.
- The failed root does not continue, and the document still commits and
  reaches its normal ready/load lifecycle.
- Worker/protocol/host-installation failures remain hard errors; they are not
  silently converted into page events.

## Context

- `docs/INDEX.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- Issue [#40](https://github.com/wanazhar/glass/issues/40)

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/tasks/native-engine-browser-274.md`

## Implementation

Page-script evaluation errors now extract a bounded diagnostic message and
send a data-only descriptor through the existing host-evaluation path. The
JavaScript host constructs an `ErrorEvent`, dispatches it to the identified
script element and then to the page window, and installs a window-specific
`onerror` adapter that calls the handler with the five standard arguments.
`ErrorEvent` is exposed as a native Web IDL constructor with the underlying
`Error` value. Module evaluation now preserves QuickJS's caught exception
text in its typed worker error so the descriptor is useful to page code.

## Tradeoffs and follow-up

Line and column values are currently bounded placeholders because the native
script source map/streaming parser does not yet own browser-accurate source
locations. Fetch/resource failures continue to use their existing
element-level event path, and unrecognized runtime/worker failures stay hard
errors. Unhandled promise rejection reporting, parser-accurate timing, source
maps, full `ErrorEvent` descriptor parity, and complete script/Web IDL
semantics remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_inline_script_failure_dispatches_error_without_aborting_document --locked` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine module --locked -- --test-threads=1` (5 passed)

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider issue #40 gates.
