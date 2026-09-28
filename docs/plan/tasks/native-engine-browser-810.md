---
id: native-engine-browser-810
scope: glass-browser/shared-worker-target-close-and-session-lifetime
status: complete
depends-on: [native-engine-browser-809]
---

# Glass native-engine browser slice 810: SharedWorker target teardown

## Objective

Complete the browser-session SharedWorker connection close path across top-level
targets: route page- and worker-originated `MessagePort.close()` correctly,
disconnect only a target's own ports when that target is closed, preserve
sibling-target connections, and retire the SharedWorker global when its final
live Document owner is destroyed.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) owns the native
  browser-completion mission and closure gates.
- The versioned [Glass Core Web Profile](../native-engine-browser-profile.md)
  requires a coherent worker, MessagePort, target, and failure lifecycle.
- [Native-engine architecture](../../architecture/native-engine.md) records
  process ownership, browser-session coordination, bounds, and local evidence.
- Slice [809](native-engine-browser-809.md) moves SharedWorker creation and
  matching to one `NativeWorkerRegistry` per `BrowserRuntimeSession`; page
  ports remain owned by their page target/frame routes.
- The [HTML worker-lifetime algorithms](https://html.spec.whatwg.org/multipage/workers.html#worker-lifetime)
  track SharedWorker owner Documents separately from their MessagePorts. The
  [MessagePort disentangling algorithm](https://html.spec.whatwg.org/multipage/web-messaging.html#ports-and-garbage-collection)
  fires `close` at the surviving endpoint when a Document is destroyed.
- Before this slice, `NativeMessagePortPageMessage` distinguished data from
  `close`, but the session coordinator did not preserve that bit across the
  page/worker boundary. Target closure cleared its page route without retiring
  the registry route or notifying the SharedWorker endpoint.

## Contract

- Use one process-backed `BrowserRuntimeSession` and real HTTP pages on one
  origin. Two top-level targets connect through distinct ports to one named
  SharedWorker; verify one worker identity and distinct connections.
- A worker-originated close on one connection is delivered to that connection's
  page `MessagePort` as exactly one generic `close` Event, not as a `message`
  event with null data. Its bridge is retired, and another target's port stays
  open and can still exchange a request/reply with the same worker global.
- Closing a page `SharedWorker.port` while its Document remains active closes
  only that connection and delivers exactly one generic `close` Event to its
  worker-side endpoint. It does not erase the Document's SharedWorker owner or
  terminate the global; a later connection from another active Document still
  reuses that worker.
- Closing one top-level target retires all SharedWorker bridges owned by that
  target, purges queued deliveries for those bridges, and delivers the close
  Event to each corresponding worker-side endpoint. Routes owned by other
  targets remain live and continue to work.
- Closing the final top-level Document owner runs worker-side close handling,
  removes the unowned SharedWorker runtime and its matching key, and leaves no
  stale session route. This slice chooses immediate close for an explicitly
  destroyed final owner; it does not implement a between-loads keepalive.
  A later same-origin target using the same worker URL/name/type gets a fresh
  worker global (connection count restarts at one and runtime identity changes).
- Preserve route ownership validation, bounded queues, and existing
  Dedicated/Service Worker ownership. A vanished or mismatched target route
  must not receive events or be resurrected by a stale transfer.
- Verify the behavior with a deterministic process-backed HTTP regression
  crossing target selection, explicit close, survivor traffic, final-client
  close, and fresh reconnection.

## Boundaries and tradeoffs

- This slice covers explicit top-level target close and the session-level
  SharedWorker bridge path. Same-document navigation, child-frame/document
  teardown, BFCache, GC-driven MessagePort close, renderer/process crash,
  storage-key/agent-cluster matching, and full Web Platform Test conformance
  remain separate issue #40 requirements.
- A Document owner can outlive its explicitly closed SharedWorker port. The
  coordinator must therefore track owner Documents independently from live
  bridge routes; using route count as the worker lifetime would terminate a
  still-owned SharedWorker prematurely.

## Paths

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-810.md`

## Verification

Run the scoped type check before the exact process-backed regression:

```console
cargo check -p glass-browser --test native_engine --locked --quiet
cargo test -p glass-browser --test native_engine native_runtime_shared_worker_target_teardown_preserves_survivor_and_reaps_last_client --locked --quiet -- --exact
```

Also run `cargo fmt --all`, `git diff --check`, and the maintainer handbook's
release-documentation truth, documentation-depth, TUI-shortcut, and
documentation-coverage gates. Record exact results, warnings, and boundaries
here. Do not run remote CI or push this local checkpoint.

## Results

- `cargo fmt --all`: passed.
- `cargo check -p glass-browser --test native_engine --locked --quiet`:
  passed. The crate still reports its existing unused legacy HTML-parser
  methods/functions in `native_engine/dom.rs` when warnings are not suppressed.
- `cargo test -p glass-browser --test native_engine native_runtime_shared_worker_target_teardown_preserves_survivor_and_reaps_last_client --locked --quiet -- --exact`:
  1 passed, 0 failed, 862 filtered, 62.26 seconds. The test uses the
  repository's `run_native_browser_worker_test` helper, whose 8 MiB thread
  stack matches the resident browser-worker contract.
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation.json`:
  passed; 1,438 Markdown documents, 83 current documents, 63 previous-version
  hits, 1,611 semantic audit hits, and zero current-claim failures.
- `python3 scripts/check-documentation-depth.py`: passed; 93 current guides
  routed/audited and 19 substantive contracts.
- `python3 scripts/check-tui-shortcuts.py`: passed; 15 implementation help
  keys and 63 documentation markers.
- `python3 scripts/check-documentation-coverage.py`: passed after correcting
  this task's architecture link.
- `git diff --check`: passed.
- No remote CI was run and nothing was pushed. Child-frame/document teardown,
  same-document navigation, BFCache, GC, crashes, full storage-key matching,
  and complete SharedWorker/WPT behavior remain open under issue #40.
