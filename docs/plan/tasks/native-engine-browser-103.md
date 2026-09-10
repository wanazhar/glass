---
id: native-engine-browser-103
scope: glass-browser/native-engine/fetch-headers-init-mutation
status: done
depends-on: [native-engine-browser-102]
---

# BE-24: bounded Fetch Headers initialization and mutation

## Objective

Add a bounded mutable `Headers` surface for Fetch request dictionaries while
keeping response header views read-only and on the existing exposure path.

## Contract

- Native `Headers` accepts bounded records, two-item pair sequences, and other
  native `Headers` instances.
- Names are lowercased; `append` combines duplicate values in order, `set`
  replaces a name, `delete` removes it, and `get`/`has`/snapshot iterators/
  `forEach`/`size` expose the current bounded record.
- Name, value, aggregate-byte, count, and forbidden/internal-header rules are
  enforced before Fetch delivery. Fetch still performs its independent
  JavaScript/Rust normalization and `Content-Type` routing.
- Response `Headers` snapshots remain read-only. Full Web IDL identity,
  exotic iterable inputs, and complete Headers parity remain open.

## Ownership and sequence

```text
Headers init/mutation -> normalized request dictionary -> Fetch policy bridge
```

JavaScript owns the mutable record and snapshot iteration. The existing Fetch
bridge owns request-header conversion, while Rust remains authoritative for
validation, CORS preflight, redirect handling, and transport delivery.

## Deliberate boundary and tradeoffs

The implementation supports the bounded record/sequence forms needed by the
native Fetch path and rejects unsupported iterables. Duplicate values are
combined eagerly, which is deterministic and bounded but does not claim the
full live Web IDL object/iterator identity. Response headers deliberately use
their separate frozen snapshot to prevent page mutation of network metadata.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected compile, Headers behavior and request
delivery, full native-engine target, strict-Clippy baseline, documentation,
and release-truth gates complete. The checkout is local-only: no push, remote
CI, release, tag, or registry-publication claim is made by this task.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_xhr_fetch_bridge -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 382 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline; normalized diagnostics match the 094 baseline exactly (32 pre-existing diagnostics), with no new diagnostic
- `python3 scripts/check-documentation-coverage.py` — 753 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  753 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=926; current-claim failures=0
