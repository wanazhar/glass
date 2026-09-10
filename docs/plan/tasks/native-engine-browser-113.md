---
id: native-engine-browser-113
scope: glass-browser/native-engine/abort-signal-static-abort
status: done
depends-on: [native-engine-browser-112]
---

# BE-29: bounded AbortSignal.abort constructor

## Objective

Complete the common static already-aborted signal constructor on the existing
AbortSignal owner.

## Contract

- `AbortSignal.abort()` returns an already-aborted native signal with the
  default `AbortError` reason; a supplied reason is preserved exactly.
- Static construction does not dispatch an abort event after creation, while
  `aborted`, `reason`, `throwIfAborted()`, and fetch signal validation reuse the
  existing signal behavior.
- The timeout, any-composition, controller, listener-cleanup, timer-order, and
  bounded allocation contracts remain unchanged.
- Transport cancellation, XHR integration, and complete AbortSignal/Web IDL
  parity remain open.

## Ownership and sequence

```text
AbortSignal.abort(reason) -> already-aborted signal -> existing signal consumers
```

The JavaScript runtime creates the signal directly; no timer, event queue, or
network operation is allocated.

## Deliberate boundary and tradeoffs

This closes the common static constructor without pretending to implement
full DOMException/prototype/descriptor identity. It preserves arbitrary
caller-supplied reasons and the existing bounded signal surface.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

The slice was validated with the affected native-engine integration target.
The issue-level full native-engine suite, strict-Clippy baseline,
documentation, release-truth, remote-CI, publication, and browser-parity
gates remain final issue gates; this task makes no remote or release claim.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_abort_signal_timeout_and_any_follow_host_turns -- --nocapture` — 1 passed
- `python3 scripts/check-documentation-coverage.py` — 763 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  763 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=937; current-claim failures=0
