---
id: native-engine-browser-121
scope: glass-browser/native-engine/response-headers-identity
status: done
depends-on: [native-engine-browser-120]
---

# BE-35: bounded response Headers identity

## Objective

Give Fetch and asynchronous XHR response-header projections native `Headers`
identity while retaining immutable filtered snapshots.

## Contract

- Response-header views satisfy `headers instanceof Headers` and retain their
  existing case-insensitive lookup, duplicate-name combination, iteration,
  `forEach()`, size, and same-origin/CORS-exposed filtering behavior.
- `append()`, `set()`, and `delete()` on response-header views fail with a
  typed immutability error; no caller mutation reaches the response owner.
- Request `Headers` remain mutable and continue to use their existing
  validation, forbidden-header policy, live iterators, and transport transfer.
- Fetch, XHR, Response constructors, clone projections, opaque responses, and
  filtered redirects share the bounded response-header identity owner.
- Full Headers Web IDL descriptors, raw header bytes, trailers, live response
  mutation, and complete Fetch/XHR parity remain open.

## Ownership and sequence

```text
Rust/content-worker header records -> immutable Headers projection
Request Headers owner             -> separate mutable request projection
```

The response projection carries a private normalized entry list and a
read-only mutator surface. It intentionally does not reuse the mutable request
owner.

## Deliberate boundary and tradeoffs

Prototype identity improves common response-header feature detection while
preserving the fail-closed filtered snapshot. Mutation methods reject rather
than silently diverge; descriptor, byte-level, trailer, and live-update
parity remain explicit future work.

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
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_response_objects_expose_bounded_constructors_and_identity -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises -- --nocapture` — 1 passed
- `cargo fmt --all -- --check` and `git diff --check` — passed
- `python3 scripts/check-documentation-coverage.py` — 771 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  771 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=945; current-claim failures=0
