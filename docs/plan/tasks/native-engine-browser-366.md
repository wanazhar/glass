# Native queued transport host turns (366)

```yaml
id: native-engine-browser-366
scope: native-engine/content-task-sources
status: done
depends-on:
  - native-engine-browser-365
```

## Objective

Deliver callbacks already queued by open page WebSocket or EventSource
transports during ordinary content-process host turns, without requiring the
caller to create a new transport command or wait on top-level JavaScript.

## Delivered behavior

- The content-process scheduler admits open page WebSocket and EventSource
  connections as background work on every subsequent script evaluation.
- Queued callbacks are dispatched through the existing rotating task-source
  scheduler and serialized QuickJS/mutation owner.
- Existing Fetch Stream work continues to share the same bounded turn when
  background transport work is admitted.
- Callback ordering, bounded event-loop turns, transport close handling, and
  follow-up DOM/network commands retain their existing limits.
- The regression witness installs handlers before transport events arrive and
  verifies a queued WebSocket callback through evaluations with no `await`.

## Contract and tradeoffs

This is operation-boundary background delivery: a host call services events
that have arrived since the previous turn. It does not create a resident event
loop, wait on an idle ordinary evaluation, throttle hidden pages, or claim
full ordering across workers, Service Workers, MessagePorts, rendering, and
browser-wide context queues. A page that has no host turn cannot observe a new
callback until its next Glass operation; this preserves bounded synchronous
API latency and avoids an additional background thread.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-366.md`

## Verification

The following checks passed locally against the current checkout:

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_content_process_delivers_queued_websocket_events_on_ordinary_turns -- --exact --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine --locked native_content_process_drives_websocket -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine --locked native_content_process_drives_event_source -- --nocapture` — 1 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
