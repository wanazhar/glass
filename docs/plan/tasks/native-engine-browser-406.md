# Native CSP live meta-policy container (406)

status: done
scope: native-engine/csp-live-meta-policy-container
issue: 40

## Objective

Make the native engine model the live-document CSP meta-policy container for
enforced `Content-Security-Policy` elements. Parser-discovered policies must
be installed once, newly inserted head policies must append to the existing
container, and later DOM edits must not weaken a policy that has already been
processed.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-398.md`
- `docs/plan/tasks/native-engine-browser-405.md`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- [CSP Level 3](https://www.w3.org/TR/CSP3/)

The 398 slice correctly composed parser-time head meta policies with response
headers, but the live document path still needed an ownership rule. Replacing
the policy list after every mutation would let removing a processed meta node
or changing its `content` attribute relax earlier policy. CSP meta processing
is additive: a newly processed enforced head meta policy joins the existing
policy container, while report-only meta processing is not introduced through
this path.

## Contract

- Parser-discovered enforced CSP meta elements in the document head are
  committed exactly once to the document's policy container during document
  load. They intersect with every response-header policy and earlier meta
  policy.
- A newly attached, non-empty `<meta http-equiv="Content-Security-Policy"
  content="...">` in the document head appends one bounded enforced policy in
  document order. Its policy applies to later native resource, script, frame,
  and inline-style decisions through the shared Rust loader.
- Removing a processed meta element does not remove its policy. Editing the
  `content` attribute of a processed element does not replace or relax its
  policy. A distinct newly inserted node is the only way for a later meta
  declaration to be processed.
- Report-only meta elements remain outside this enforced meta-policy ledger;
  response report-only declarations retain their existing observation and
  delivery path. A meta policy never authorizes a resource that another active
  policy denies.
- The policy-list cap, URL validation, credential rejection, network-scheme
  boundary, and atomic failure behavior remain enforced. If appending a new
  policy cannot be accepted, the document ledger is not advanced.
- The content-process initial-script, script-mutation, click, type, form,
  legacy-key, keydown/keyup, shortcut, beforeunload, lifecycle, hash-change,
  and scroll callback paths commit pending meta policies after their callback
  work and before the next resource or inline-style decision.
- No second policy matcher or CDP fallback is introduced.

## Non-goals

This slice does not add Service Worker policy-container propagation,
redirect-count-aware CSP path handling, Subresource Integrity, report-only
meta support, or the complete CSP conformance corpus. Those remain separate
issue #40 gates.

## Implementation path

- `dom.rs`: retain a bounded document-local ledger of processed CSP meta node
  identities and expose only newly attached head policies to the owner.
- `resource_loader.rs`: keep parser-time meta application as a replacement of
  the initial meta portion, add an atomic append operation for live policies,
  and enforce the combined header/meta policy limit.
- `content_process.rs`: commit parser policies after the actual document is
  built, append newly inserted policies after every normal script/input/event
  turn, and refresh inline-style enforcement against the updated container.
- Add focused loader/DOM unit coverage and native HTTP integration witnesses
  for dynamic insertion and post-parse content edits.
- Update the architecture, active plan, analysis, and this task with exact
  evidence.

## Tradeoffs

- A node-identity ledger preserves the browser rule that processed meta policy
  cannot be relaxed by DOM mutation, at the cost of retaining one bounded
  identity per processed DOM node for the life of the document generation.
- Initial parsing can still install its discovered meta list in one operation,
  while live mutations append incrementally. This keeps startup simple and
  makes policy updates observable, but requires every content-process turn to
  run the same small pending-policy check.
- The native DOM reports policies only after a node is attached beneath the
  document head. This avoids applying arbitrary detached markup, while a
  bounded engine may still reject or delay nonstandard parser timing until a
  later conformance slice.
- Rechecking inline styles after an append keeps the current computed-style
  projection aligned with the policy container, but it may invalidate cached
  style work on a mutation that adds only a script policy. The correctness
  boundary is preferred until style invalidation is independently optimized.

## Delivered

- `NativeDocument` now tracks processed enforced CSP meta node indexes per
  document generation. Parser-time documents and content-process wire
  documents mark their initial head policies as processed; later discovery
  returns only attached, non-empty, head-enforced policies not in the ledger.
- `NativeCspPolicy` retains the parser-load replacement operation and now also
  appends live meta policies without discarding response-header or earlier
  meta declarations. Header plus meta counts are checked against the shared
  bounded policy limit.
- All normal content-process mutation bridges pass the resource-loader owner
  and commit pending policies after script callbacks and scroll work. Inline
  style authorization is refreshed against the resulting policy state.
- A live inserted head meta policy now blocks a subsequently created external
  script before its network request, even when the same turn edits and removes
  the meta element before creating that script. Editing an already processed
  meta element's `content` attribute leaves the original policy container
  active.
- No report-only meta behavior, policy relaxation, second matcher, or CDP
  fallback was added.

## Verification

- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --lib csp_meta --locked -- --nocapture`
  (4 passed)
- `cargo test --quiet -p glass-browser --test native_engine
  native_content_process_appends_dynamically_inserted_head_meta_csp --locked
  -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine
  native_content_process_ignores_post_parse_csp_meta_content_edits --locked --
  --nocapture` (1 passed)
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked
  --no-fail-fast` (1,123 passed, 1 ignored)
- `cargo fmt --all -- --check`
- `git diff --check`

Documentation validators passed with 1,056 Markdown documents, 83 current
documents, 62 previous-version hits, 1,232 semantic-audit hits, 0 current-
claim failures, 93 routed or audited current guides, 19 substantive
contracts, 15 implementation help keys, 63 documentation markers, 346
full-product MCP tools (101 browser-only), 17 examples, and 22 public
modules.

Remote CI, push, release, tag, and registry publication remain outside this
local checkpoint.
