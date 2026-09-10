---
id: native-engine-browser-075
scope: glass-browser/native-engine/cross-process-storage-event-journal
status: done
depends-on: [native-engine-browser-074]
---

# BE-02/BE-03/BE-04/BE-07: cross-process Web Storage event journal

## Objective

Deliver live `localStorage` and `sessionStorage` events between independent
native engine owners, including sandboxed content workers, without depending
on an in-process registry. Reuse the existing profile lock and keep the
transport bounded, source-safe, origin-filtered, and explicit about recovery
and retention limits.

## Contract

- A profile at `P` has a newline-delimited event journal at `P` with the
  extension replaced by `.events`. The journal uses the retained `P.lock`
  advisory lock for reads, cursor initialization, tail repair, and appends.
- Each engine receives a bounded unique writer ID and initializes its cursor
  at the current journal end. It receives only records appended after it
  became live and excludes records from its own writer ID.
- Journal records retain the private source browsing-context ID and the full
  bounded storage-event descriptor. Receivers apply same-origin routing;
  `sessionStorage` additionally requires the receiving context ID to match the
  source context ID. The page sees only the existing bounded `StorageEvent`.
- Local and session changes from local realms and content-worker responses
  are appended after their state/profile work succeeds. A content worker does
  not write the journal directly; its parent engine is the writer authority,
  preventing duplicate records.
- The journal is capped at 4 MiB. Malformed complete records, invalid event
  fields, over-limit files, and truncated cursors are typed failures. A
  partial final line is not delivered and is safely truncated at the next
  append before the new complete records are written.
- This is live event delivery, not an acknowledged durable queue. Records are
  not compacted in this slice; profile startup begins at the journal end, and
  the journal does not replace the revisioned local-storage profile merge.

## Ownership and sequence

```text
runtime storage mutation
  -> revisioned profile merge under P.lock
  -> append validated event record under P.lock
  -> receiver polls journal before page operation
  -> writer/origin/context filter
  -> receiver state update + page StorageEvent dispatch
```

The parent native engine owns journal writes for both local and sandboxed
documents. The child worker continues to receive already-filtered events over
the existing bounded IPC command; no third crate or worker-side journal
access is introduced.

## Deliberate boundary and tradeoffs

The journal removes the old same-process-only delivery boundary and uses the
same file-lock namespace as profile persistence. Polling is intentionally
operation-bound rather than a background thread, so a quiet receiver does not
consume a task or process budget; its next navigation, action, fetch, or script
operation observes pending events. The fixed cap makes disk usage predictable,
but without acknowledgements/compaction a busy profile eventually reaches the
cap and reports a typed limit error until a later maintenance slice provides a
safe retention policy.

The event file and profile snapshot are separate commits, so a crash between
them can leave a persisted mutation without a delivered journal record (or a
record after a later retry). Receivers remain recoverable and profile state is
authoritative; transactional journaling and cross-profile event isolation are
future work. Cookie profile persistence, IndexedDB, quota policy, and the
wider browser-complete contract remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Final verification is recorded after the affected native target and docs
gates complete:

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_delivers_local_storage_events_between_documents -- --nocapture` — 1 passed with aliased profile paths
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine storage -- --nocapture` — 11 passed (also covered by the final full target)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` — 367 passed, 0 failed
- `python3 scripts/check-documentation-coverage.py` — 725 Markdown files; 345 full-product MCP tools (100 browser-only); 17 examples; 22 public modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides routed/audited; 19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` — 725 Markdown documents; 83 current documents; 59 previous-version hits; 898 semantic audit hits; 0 current-claim failures

Strict affected-package Clippy remains a known baseline gate with 32
pre-existing diagnostics; it is not relabeled as passing by this slice.
