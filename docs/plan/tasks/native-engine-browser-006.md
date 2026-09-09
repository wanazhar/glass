---
id: native-engine-browser-006
scope: glass-browser/native-engine/content-process-computed-style
status: done
depends-on: [native-engine-browser-005]
---

# BE-01e: child-owned computed-style snapshot

## Objective

Move bounded stylesheet parsing, cascade, inheritance, and computed-style
production for external documents into the content worker. The child computes
one `NativeComputedStyle` per parsed node and transfers that bounded typed
snapshot with the DOM arena. The parent validates the cardinality and uses the
received immutable style cache for layout, paint, hit testing, and visibility.

Local/data/fixture documents retain the direct stylesheet path for deterministic
development tests; external HTTP(S) documents no longer reparse stylesheet
sources in the parent.

This is not full CSS or content-process completion. The current CSS grammar is
still intentionally bounded, DOM mutation remains parent-owned, and script,
subresources, storage, sandboxing, and browser parity remain open.

## Contract

- The child parses the document and computes style using the existing bounded
  selector, cascade, inheritance, box-model, flex, text-flow, overflow, and
  paint-value owners.
- The wire snapshot contains exactly one computed style for every DOM node.
  Missing, extra, malformed, or out-of-cardinality style entries fail closed.
- Computed style values use typed serde representations of the existing Rust
  enums/records; no CSS source is returned for the external path and the parent
  does not invoke its stylesheet parser for that snapshot.
- The decoded computed-style/DOM snapshot is capped at 2 MiB and the enclosing
  length-prefixed IPC frame at 4 MiB. The existing document, node, DOM-depth,
  and text quotas are enforced before the child emits the snapshot and again
  while the parent reconstructs it.
- The parent keeps the existing immutable document-generation and commit
  ordering. A malformed style snapshot cannot replace the current document or
  silently fall back to CDP.
- In-process local documents keep `NativeStylesheet` as their style owner so
  deterministic unit fixtures do not require a helper binary.

## Tradeoffs and missed behavior

- Caching computed styles avoids a second CSS parse and makes external style
  ownership explicit, but style changes currently require a new document
  snapshot; DOM mutation cannot yet invalidate/recompute the child cache.
- The full typed style record is larger than a compact binary CSS IR and adds
  serialization work, but it avoids a second private declaration grammar and
  preserves exact enum semantics. A later binary/shared-memory wire can reduce
  copies after the ownership contract stabilizes.
- The parent still owns DOM mutation, revision/effects bookkeeping, and the
  current layout/render projection. This is a partial content boundary, not a
  claim that hostile scripts or all CSS implementation state are isolated.
- CSS diagnostics from stylesheet sources are not yet transferred as a full
  child-owned diagnostic stream; the next diagnostics task must preserve the
  privacy-safe diagnostic contract without returning raw stylesheet text.

## Paths

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

The computed-style boundary was checked and tested as one batch:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine --locked`
- `cargo build -p glass-browser --features native-engine --bin glass-native-content-worker --locked`
- `cargo test -p glass-browser --features native-engine --test native_engine native_content_process_transfers_computed_style_for_layout -- --nocapture`
- `cargo test -p glass-browser --features native-engine --test native_engine native_runtime_session_loads_bounded_external_http_html_without_cdp -- --nocapture`
- `cargo test -p glass-browser --features native-engine --test native_engine native_content_process_rejects_malformed_html_before_parent_commit -- --nocapture`
- `cargo test -p glass-browser --features native-engine --test native_engine native_content_process_enforces_child_owned_document_limit -- --nocapture`

The affected check/build passed without warnings. Child-computed external
layout passed 1/1; external navigation, malformed-document failure atomicity,
and child-owned document-limit coverage each passed 1/1. Full CSS conformance,
mutation ownership, diagnostics transfer, script execution, sandboxing,
cross-platform packaging, WPT, security, and production promotion gates remain
open.
