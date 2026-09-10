---
id: native-engine-browser-105
scope: glass-browser/native-engine/xhr-timeout
status: done
depends-on: [native-engine-browser-104]
---

# BE-26: bounded asynchronous XHR timeout

## Objective

Expose a bounded asynchronous XHR timeout through the existing Fetch and
content-worker request bridge.

## Contract

- XHR exposes a numeric `timeout` property. Finite non-negative values are
  stored as integer milliseconds; values above 4,000 ms, negative values, and
  non-finite values fail closed with `RangeError`.
- `timeout=0` disables the extra XHR deadline. A non-zero timeout is carried
  through the existing Fetch command and applies to the bounded HTTP request,
  including its response-body transfer.
- A timed-out XHR reports `readyState=4`, status zero, an empty response and
  response-header view, one bounded `readystatechange`, and one `timeout`
  event through `ontimeout`. It does not invoke `onerror` or `onload`.
- The existing abort and stale-continuation checks remain authoritative;
  unrelated Fetch requests retain their established 30-second network limit.
- Upload progress, transport cancellation beyond the bounded request
  deadline, streaming, synchronous XHR, and full XHR Web IDL parity remain
  open.

## Ownership and sequence

```text
XHR.timeout -> Fetch command timeout_ms -> reqwest bounded deadline
                                         -> timeout payload -> XHR timeout event
```

The JavaScript surface owns event/state publication. The content process
validates the bounded command and transfers the deadline to the resource
loader; the loader owns the actual request deadline and does not expose raw
transport errors.

## Deliberate boundary and tradeoffs

This slice uses the existing worker request as the cancellation boundary. The
HTTP client stops waiting at the bounded deadline, but the engine does not
claim a general task scheduler, upload/progress events, streaming, or full
socket-lifecycle cancellation. The 4-second cap leaves room under the
content-worker script deadline and keeps a timeout test deterministic.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

The slice was validated with the affected native-engine target and the full
XHR-focused subset. The issue-level full native-engine suite, strict-Clippy
baseline, documentation, release-truth, remote-CI, publication, and browser
parity gates remain final issue gates; this task makes no remote or release
claim.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_xhr_timeout_is_bounded_and_observable -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine xhr -- --nocapture` — 3 passed, 0 failed
- `python3 scripts/check-documentation-coverage.py` — 755 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  755 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=929; current-claim failures=0
