# Native page networking task-order FIFO (356)

```yaml
id: native-engine-browser-356
scope: native-engine/page-network-task-order
status: done
depends-on: [native-engine-browser-355]
```

## Objective

Remove the concrete page-network scheduling defect in which Fetch commands
were collected in script order but resolved with a stack pop:

- retain the bounded Fetch queue as FIFO;
- resolve concurrent page Fetch requests in script-emission order;
- preserve the persistent JavaScript owner's microtask drain between response
  handoffs; and
- add a content-process witness covering both server request order and Promise
  continuation order.

This slice is a networking-order correction, not a claim of complete HTML
task-source scheduling. Cross-source arbitration, timer/message/worker
fairness, rendering opportunities, durable task queues, cancellation ordering,
and final production certification remain issue #40 promotion work.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-355.md`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Path

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-356.md`

## Contract and tradeoffs

`fetch_commands()` now returns a bounded `VecDeque`, and the content-process
script event loop consumes it with `pop_front()`. The command collection
already preserves the order in which the page realm emitted Fetch calls, so
the host now issues and resolves those requests in that same order. Each
response is handed back through `NativeJavaScriptRuntime::resolve_fetch()`;
the normal persistent-runtime evaluation then drains its pending Promise jobs
before the loop takes the next networking task.

FIFO improves determinism and matches the intended task-queue contract, but it
does not make network completion physically concurrent: a later response can
still be delayed behind an earlier host operation. That is deliberate for this
bounded correction. A future networking scheduler can separate request I/O
from response-delivery tasks while retaining sequence metadata and the same
observable ordering rules.

The queue remains local to one content-process script turn. WebSocket,
EventSource, fetch-stream, timer, MessagePort/worker, rendering, and other
host sources still use their existing handoff points until the broader
task-source owner is implemented. The new witness therefore checks the
networking source only and does not overstate browser-wide event-loop parity.

## Verification

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_DEV_DEBUG=0 cargo check --quiet -p glass-browser --lib --bins --test native_engine --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_preserves_fifo_network_task_order --locked -- --exact --nocapture`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_resolves_page_script_fetch_before_publish --locked -- --exact`
- `git diff --check`
- repository documentation coverage, depth, release-truth, and TUI shortcut
  validators

All evidence is local unless a later explicitly authorized push produces
remote CI evidence.

## Local evidence

- `cargo fmt --all -- --check` — passed
- `CARGO_PROFILE_DEV_DEBUG=0 cargo check --quiet -p glass-browser --lib --bins --test native_engine --locked` — passed
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_preserves_fifo_network_task_order --locked -- --exact --nocapture` — passed; 1 test, 0 failures, with `/page`, `/first`, `/second` request and continuation order
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_resolves_page_script_fetch_before_publish --locked -- --exact` — passed; 1 test, 0 failures
- `git diff --check` — passed
