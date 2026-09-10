---
id: native-engine-browser-115
scope: glass-browser/native-engine/headers-live-iterators
status: done
depends-on: [native-engine-browser-114]
---

# BE-29: bounded live request Headers iterators

## Objective

Make mutable request `Headers` iterators live and self-iterating while keeping
immutable response-header views as bounded snapshots.

## Contract

- Mutable request `Headers.entries()`, `keys()`, and `values()` retain their
  owner and observe bounded later `append()`/`set()` mutations through the
  existing normalized `_entries` list.
- Each request-Header iterator exposes `next()` and returns itself from
  `[Symbol.iterator]()`; `Headers.prototype[Symbol.iterator]` remains the
  `entries` method.
- Existing JavaScript/Rust name/value validation, forbidden-header protection,
  entry/byte bounds, request transfer, and response-header exposure remain
  unchanged. Response Headers remain immutable bounded snapshots.
- Full Headers Web IDL prototype/descriptor parity, complex delete/reorder
  mutation semantics, raw response headers, and trailers remain open.

## Ownership and sequence

```text
mutable Headers owner -> live cursor -> normalized bounded _entries
  -> existing request transfer
```

The request Headers object remains the sole mutable entry owner. Response
header objects keep their separate immutable projection owner.

## Deliberate boundary and tradeoffs

Common mutable-header iterator identity and append/update visibility are now
available without eager per-iterator snapshots. The compact cursor behavior for
complex delete/reorder races is retained, and no raw response-header or trailer
surface is introduced.

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
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_request_headers_iterators_are_live_and_self_iterating -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises -- --nocapture` — 1 passed
- `python3 scripts/check-documentation-coverage.py` — 765 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  765 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=939; current-claim failures=0
