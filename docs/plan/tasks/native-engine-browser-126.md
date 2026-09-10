---
id: native-engine-browser-126
scope: glass-browser/native-engine/page-publication-navigation
status: done
depends-on: [native-engine-browser-125]
---

# BE-40: bounded page-publication navigation

## Objective

Carry script-driven `window.location` navigation emitted while a document is
being published back to the Rust navigation owner. The handoff must work for
local documents and process-backed HTTP(S) documents without allowing a page
script to recurse through an unbounded or ownerless navigation path.

## Contract

- Initial page-script evaluation may return at most one typed location
  navigation. The handoff is validated before it crosses the local or content
  process boundary.
- The existing publication phases remain ordered: parser-blocking page scripts,
  resource `load` callbacks, `interactive`/`DOMContentLoaded`, `complete`/`load`,
  and the post-commit `pageshow` callback.
- Local publication navigation is handed back to the synchronous Rust owner;
  process-backed publication navigation is returned in the load or lifecycle
  response and consumed by the asynchronous Rust owner.
- Relative targets resolve against the committed document URL. `assign()` and
  `href`/component setters use push history; `replace()` uses replacement
  history, including across a bounded handoff chain.
- A content-process location handoff is target-free: node index zero, no
  submitter, and `location: true`. Link/form navigation metadata remains a
  separate contract.
- The owner accepts at most eight page-publication handoffs per navigation
  chain. Malformed targets, multiple commands in one page-script/event batch,
  and over-limit chains fail with typed errors rather than being discarded.
- Outgoing `beforeunload`, `pagehide`, `unload`, and `hashchange` re-entry still
  requires a future event-loop/navigation-owner slice. Nested contexts, full
  URL/Location Web IDL parity, and complete URL parsing remain outside this
  slice.

## Ownership and sequence

```text
page script / publication event
        -> typed location handoff
        -> local owner or bounded content IPC response
        -> URL resolution and resource load
        -> document commit and history mutation
        -> next bounded handoff or pageshow
```

The JavaScript runtime owns only the command projection. The Rust engine owns
URL resolution, resource policy, history, lifecycle ownership, and the
handoff budget. The content worker returns navigation metadata alongside its
document/mutation snapshot; it does not load the next document itself.

## Deliberate boundary and tradeoffs

Returning one typed handoff keeps page publication composable with the existing
loader and makes `replace()` observable across local and process-backed
navigation. The eight-handoff limit prevents a fixture or hostile page from
creating an unbounded synchronous/asynchronous navigation chain. Pending page
fetch work is not made into a general event queue by this slice, and outgoing
lifecycle/hashchange re-entry remains an explicit owner error. This preserves
bounded deterministic behavior while leaving the full HTML navigation and task
source algorithms for later profile work.

## Paths

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Verification

The slice is validated locally through the native integration target and the
documentation/release-truth audits. No remote CI, push, release, tag, or
registry-publication claim is made by this local checkpoint.

- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_page_show_location_replaces_publication -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_page_load_location_handoffs_preserve_history -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 395 passed in 86.88s
- `cargo fmt --all -- --check` and `git diff --check` — passed
- `python3 scripts/check-documentation-coverage.py` — 776 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  776 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=971; current-claim failures=0
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine -- -D warnings` — blocked by the established 34 pre-existing repository diagnostics outside this slice; no new diagnostic was identified in the touched publication path
