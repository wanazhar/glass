---
id: native-engine-browser-811
scope: glass-browser/shared-worker-document-owner-lifecycle
status: done
depends-on: [native-engine-browser-810]
---

# Glass native-engine browser slice 811: SharedWorker Document-owner replacement

## Objective

Retire SharedWorker ownership and page bridges when cross-document
navigation/recovery replaces their exact Document, including descendant frame
Documents destroyed by that replacement. Keep surviving Documents and
replacement Documents independent even when they reuse the same context and
frame route.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) owns the
  native-browser completion mission and closure gates.
- The versioned [Glass Core Web Profile](../native-engine-browser-profile.md)
  defines worker lifecycle and browser-visible behavior.
- [Native-engine architecture](../../architecture/native-engine.md) records
  backend and frame ownership.
- Slice [810](native-engine-browser-810.md) established that a live Document
  owns a SharedWorker independently of its ports, and that destroying the last
  explicit owner immediately closes an unowned worker in this bounded profile.
- The [HTML worker-lifetime algorithm](https://html.spec.whatwg.org/multipage/workers.html#worker-lifetime)
  distinguishes owner Documents from MessagePorts. Native identity must
  therefore distinguish successive Documents that reuse a frame route.

## Contract

- Identify one SharedWorker owner by `(context ID, frame ID, Document
  generation)`. The generation is the committed native Document generation,
  not a MessagePort count or a reusable frame ID.
- A successfully committed cross-document navigation or successful native
  recovery retires the outgoing Document's page-side SharedWorker bridges,
  sends close to their worker-side endpoints, and removes only that outgoing
  Document owner. A cancelled, blocked, same-document, or failed navigation
  does not retire the current owner.
- Replacing an ancestor Document retires the owners and ports belonging to
  every descendant Document destroyed with its frame subtree. It must not close
  a sibling frame outside that subtree, another top-level target, or a
  replacement Document using the same frame route.
- Process outgoing lifecycle effects before owner teardown. Process the
  replacement Document's worker-create effects only after outgoing owners and
  destroyed descendants have been retired. If the replacement immediately
  creates the same SharedWorker, its new bridge and owner remain live.
- If other Documents still own the worker, preserve its global and their
  connections. If the last owner is removed, use Slice 810's immediate-close
  policy, run worker-side close handling for live owned ports, and remove the
  worker and matching registry entry.
- No stale bridge delivery may be redirected to the replacement Document.
  Routes and pending messages belonging to destroyed Documents are removed;
  surviving routes continue to exchange messages.
- Same-document navigation keeps the current Document owner. Dynamic iframe
  removal from a still-live parent Document is not covered by this replacement
  path and remains an issue #40 requirement. This slice does not add BFCache
  suspension/restoration, GC-driven port closure, browser crashes,
  between-loads SharedWorker retention, or full WPT conformance.
- SharedWorker matching still uses the currently implemented URL/name/type
  identity. Storage-key, partition, agent-cluster, and credentials matching
  remain an explicit separate issue #40 requirement; this task must not imply
  that those keys are complete.

## Paths

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-811.md`

## Verification

Use the scoped browser package check before the exact process-backed HTTP
regression. Run formatting and the maintainer handbook's release-documentation
truth, documentation-depth, TUI-shortcut, and documentation-coverage gates.
The regression covers committed replacement, child-subtree destruction,
successful recovery, surviving-owner isolation, and replacement-route
survival. Record exact commands, warnings, elapsed test time, and remaining
boundaries. Do not run remote CI or push this local checkpoint.

## Results

- `cargo fmt --all -- --check` and `git diff --check` passed.
- `cargo check -p glass-browser --test native_engine --locked --quiet` passed
  in 21.98 seconds. It emitted only existing dead-code warnings for legacy
  parser routines in `native_engine/dom.rs`.
- `cargo test -p glass-browser --test native_engine native_runtime_shared_worker_document_navigation_retires_only_replaced_owners --locked --quiet -- --exact`
  passed: 1 passed, 863 filtered, 68.92 seconds. The HTTP regression covers
  same-document navigation, child cross-document replacement, ancestor
  replacement of the child subtree, successful recovery, preservation of a
  surviving top-level owner, and delivery through replacement routes.
- The test helper waits up to ten seconds for asynchronously delivered page
  messages; an immediate sample was too early. The test selects the created
  sibling target before inspecting its page events and waits for the sibling's
  post-recovery close list rather than reusing an earlier pong.
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation.json`
  passed: 1,439 Markdown documents, 83 current documents, 63 previous-version
  hits, 1,613 semantic audit hits, zero current-claim failures.
- `python3 scripts/check-documentation-depth.py` passed: 93 current guides
  routed/audited and 19 substantive contracts.
- `python3 scripts/check-tui-shortcuts.py` passed: 15 implementation help keys
  and 63 documentation markers.
- `python3 scripts/check-documentation-coverage.py` passed: 1,439 Markdown
  files, 346 full-product MCP tools (101 browser-only), 17 examples, and 22
  public modules.
- Dynamic iframe removal from a still-live parent Document, storage-key,
  partition, agent-cluster, and credentials matching, BFCache, GC-driven port
  closure, browser crashes, and full SharedWorker/WPT conformance remain open
  issue #40 work.
