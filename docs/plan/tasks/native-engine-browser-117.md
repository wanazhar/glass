---
id: native-engine-browser-117
scope: glass-browser/native-engine/response-clone
status: done
depends-on: [native-engine-browser-116]
---

# BE-31: bounded Fetch Response cloning

## Objective

Add a bounded `Response.clone()` projection for ordinary and filtered native
Fetch responses while retaining independent response-body owners.

## Contract

- Every native Fetch response exposes `clone()` and returns a fresh response
  projection with the same bounded metadata and a distinct body owner.
- Cloned ordinary responses expose an independent native `ReadableStream`
  body, so reading or cancelling the clone does not invalidate the original
  response's existing text/json/blob/ArrayBuffer/bytes methods or stream.
- Cloning opaque and `opaqueredirect` responses preserves their filtered type,
  metadata, `body === null`, and rejected body-method behavior.
- Full `bodyUsed`/disturbance checks, clone rejection for locked or consumed
  bodies, shared tee/backpressure semantics, Request/Response constructors,
  and complete Response Web IDL parity remain open.

## Ownership and sequence

```text
Fetch payload -> response projection A -> independent body owners
             \-> response projection B -> independent body owners
```

The clone reuses the bounded immutable payload description but allocates a
fresh response shell, response-header snapshot, and stream owner. Existing
body-method projections remain deliberately independent under the current
bounded contract.

## Deliberate boundary and tradeoffs

This supports common application code that clones a response before consuming
one view, without redesigning the transport or adding body disturbance state.
The bounded implementation intentionally does not claim browser-grade teeing,
backpressure, or body-lock rejection; those semantics remain explicit future
gates rather than being implied by the method's presence.

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

- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_opaque_no_cors_response -- --nocapture` — 1 passed
- `cargo fmt --all -- --check` — passed after the final assertion formatting patch
- `git diff --check` — passed
- `python3 scripts/check-documentation-coverage.py` — 767 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  767 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=941; current-claim failures=0
