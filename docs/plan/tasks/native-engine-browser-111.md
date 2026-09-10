---
id: native-engine-browser-111
scope: glass-browser/native-engine/abort-signal-combinators
status: done
depends-on: [native-engine-browser-110]
---

# BE-29: bounded AbortSignal timeout and composition

## Objective

Complete the common static `AbortSignal` constructors on top of the existing
bounded host-turn scheduler and signal listener owner.

## Contract

- `AbortSignal.timeout(delay)` accepts a finite non-negative delay within the
  bounded host timer range and aborts once on a later due host turn with a
  `TimeoutError` reason. Invalid or unbounded delays fail with `RangeError`.
- `AbortSignal.any(iterable)` accepts a bounded iterable of native signals,
  returns a never-aborted signal for an empty iterable, and propagates the
  first already-aborted or later-aborted input reason exactly once.
- Composed signals dispatch one `abort` event, preserve `reason`, and remove
  their source listeners after abort. A bounded input count prevents an
  unbounded composition allocation.
- The existing `AbortController`, fetch rejection, timer ordering, and late
  host-result suppression contracts remain unchanged.
- Transport-level socket cancellation, `AbortSignal.throwIfAborted` Web IDL
  identity, `AbortSignal.abort()` static construction, XHR abort/timeout
  integration, and complete Web IDL parity remain open.

## Ownership and sequence

```text
AbortSignal.timeout/any -> native timer/listener owner -> one reasoned abort
  -> existing fetch/controller rejection path
```

The JavaScript runtime owns signal composition; the existing timer pump owns
time progression, and no second scheduler or network cancellation path is
introduced.

## Deliberate boundary and tradeoffs

This covers common timeout and fan-in cancellation patterns with deterministic
bounded allocation. Timeout delivery is host-turn based rather than a direct
socket interrupt, so a request may still finish inside its existing deadline;
full DOM/Web IDL and transport cancellation semantics remain outside the slice.

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
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_scripts_drain_microtasks_and_next_turn_timers -- --nocapture` — 1 passed
- `python3 scripts/check-documentation-coverage.py` — 761 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  761 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=935; current-claim failures=0
