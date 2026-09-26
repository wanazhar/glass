---
id: native-engine-browser-757
scope: glass-browser/cross-context-document-adopt-node
status: done
depends-on: [native-engine-browser-756]
---

# Glass native-engine browser slice 757: transfer adopted nodes across related contexts

## Context

- [Issue #40](https://github.com/wanazhar/glass/issues/40) remains the
  authority for native browser completion.
- Slice 756 implements bounded identity-preserving `Document.adoptNode()`
  only when source and target Documents share one browsing-context command
  owner. It rejects other live owners before mutating the source.
- Native same-origin frame scripts are routed to distinct frame owners. Their
  node indices and temporary-node maps are owner-local; the current
  `FrameScript` command explicitly does not transfer a native engine or a
  JavaScript object between realms. Treating a source node index as a target
  index, or cloning into the destination and deleting the source, would lose
  native identity and could leave a partial transfer.

## Objective

Implement bounded, identity-preserving `Document.adoptNode()` between live
Documents in one same-origin top-level browsing-context tree. Cover transfers
between the top-level Document and an accessible frame Document, and between
accessible sibling frame Documents. Keep the source node's observable
JavaScript identity and route later supported mutations to its new native
owner.

## Contract

- Validate source and destination context identity, same-origin access,
  document generation, and node ownership against the current live context
  topology before changing either Document. A stale frame, replaced Document,
  inaccessible/cross-origin owner, unsupported node, or exceeded transfer
  bound fails with a typed error and leaves both Documents unchanged.
- Transfer only within one top-level browsing-context tree in this slice.
  Independent top-level targets, popups, and opener-related contexts remain
  explicitly out of scope and fail closed; they are not silently treated as
  local node indices or supported by cloning.
- Preserve the original caller-visible Node object and `adoptNode(node)` return
  identity. Do not implement adoption as `importNode()`/clone plus source
  removal. The adopted node, supported descendants, attributes, and nested
  template-content fragments must resolve to the destination Document and its
  correct inert template-owner Document.
- Cover the node kinds supported by slice 756: Elements, Attrs, Text,
  Comments, DocumentTypes, and DocumentFragments. Preserve subtree structure,
  attribute ownership, template fragment identity, event-listener state held
  by the original JS objects, and detached/attached parentage according to
  the standard adoption operation.
- Preflight the complete bounded transfer before publication. Source detach,
  destination registration, identity remapping, document-owner changes, and
  revision publication form one coordinated transaction: a stale generation,
  destination validation/commit error, or owner/worker loss must not expose a
  source-only detach, duplicate live node, or half-remapped subtree.
- After adoption, insertion into the destination in the same script turn must
  use the destination owner. Subsequent supported mutations through the
  retained node objects must continue routing to that owner after script-realm
  refresh and frame projection refresh. Existing source- and destination-side
  mutation/event ordering must remain deterministic and must not apply a
  command twice.
- Bound transfer envelope size, node/depth counts, attributes, template
  contents, and retained identity mappings using existing native resource
  limits or an explicitly documented stricter limit. Validate the whole
  envelope before mutating either owner.
- Keep one authoritative browser-state coordinator for the transfer. Do not
  create a third installable crate, move browser policy into CDP, or add a
  fallback path. `importNode()` and same-owner `adoptNode()` behavior remain
  unchanged.
- This slice does not claim transfer between independent tabs/popups, access
  across origins, custom-element `adoptedCallback`/registry behavior, Shadow
  DOM adoption, unsupported DOM kinds, full DOM conformance, remote CI,
  cross-platform certification, or issue #40 completion.

## Tradeoffs

The current frame protocol has independent owner-local node identity and
mutation logs. Cross-context adoption therefore needs an explicit transfer
identity plus a coordinated source/destination commit; a best-effort pair of
ordinary mutations is insufficient. The protocol should preserve the original
realm object while associating it with the destination's native identity and
command route. Scoping the first transfer path to a single same-origin context
tree keeps that ownership transition bounded; separate top-level targets and
popup lifetimes remain distinct issue #40 gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-757.md`

## Verification evidence

Local Linux checks passed:

- `cargo fmt --all -- --check` and `git diff --check`.
- `cargo check -p glass-browser --lib --tests --locked --quiet`.
- The process-backed `native_documents_adopt_nodes_within_their_browsing_contexts`
  regression covers top-to-frame, frame-to-top, and sibling-frame transfers;
  same-object return identity; detach/owner state; nested template and
  attribute state; same-turn insertion and mutation; and destination routing
  after script-realm refresh.
- `native_document_adoption_preflight_failures_leave_both_owners_unchanged`
  verifies that stale-generation and invalid-identity preflight failures leave
  both native document drafts unchanged.
- The backend checks caller/source/destination origins and requires all three
  routed frames to belong to the same live top-level target before coordinating
  a transfer. Independent-root routes and stale document generations fail
  closed. Content-worker synchronization attempts to restore both snapshots
  before publishing either Rust document when a transfer fails.

The process-backed regression does not inject a worker crash between the two
content-worker synchronizations. That rollback branch is code-reviewed but not
fault-injection-certified. Remote CI and Windows/macOS runtime certification
remain separate issue #40 gates; this slice is not browser-completion evidence.
