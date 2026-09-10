---
id: native-engine-browser-093
scope: glass-browser/native-engine/url-search-params
status: done
depends-on: [native-engine-browser-092]
---

# BE-14: bounded URLSearchParams surface

## Objective

Expand the native `URLSearchParams` implementation from string-only
construction and basic mutation to the bounded pair/record, sorting, callback,
and iterator behavior used by ordinary web applications.

## Contract

- The constructor accepts a query string (with an optional leading `?`), an
  existing native `URLSearchParams`, a bounded sequence of two-value pairs, or
  a bounded record of values.
- `append`, `set`, `delete` (including an optional value), `get`, `getAll`, and
  `has` preserve insertion order and enforce the existing key, value, and
  entry limits.
- `size`, stable UTF-16 key sorting, `forEach`, `keys`, `values`, `entries`,
  and the default iterator are available. Iterators use bounded snapshots so
  later mutations cannot invalidate an in-progress iteration.
- Serialization uses the bounded application/x-www-form-urlencoded rules,
  including `+` for spaces and percent-encoding for the remaining reserved
  characters; malformed percent escapes retain the runtime's existing error
  behavior.
- The shared local/content-worker bootstrap is exercised through the existing
  URL-encoded fetch path. Full live Web IDL iterator identity, exotic iterable
  inputs, unlimited records, and complete URLSearchParams parity remain open.

## Ownership and sequence

```text
constructor/mutation -> bounded ordered pairs -> snapshot iterator -> form encoding
```

JavaScript owns pair normalization, ordering, callbacks, snapshots, and
serialization. Rust and the content worker retain ownership of the bounded
fetch command and HTTP request policy.

## Deliberate boundary and tradeoffs

Snapshot iterators keep memory and mutation behavior predictable inside the
existing limits, but they do not preserve the identity and liveness details
of a browser's platform iterator objects. Record/pair inputs are intentionally
bounded rather than a general iterable bridge.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected compile, worker URL-encoded request
behavior, full native-engine target, documentation, and release-truth gates
complete. The checkout is local-only: no push, remote CI, release, tag, or
registry-publication claim is made by this task.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_fetches_bounded_url_search_params -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 381 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; no new diagnostics
- `python3 scripts/check-documentation-coverage.py` — 743 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version`
  — 743 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=916; current-claim failures=0
