---
id: issue63-durable-composer-drafts-001
scope: issue-63-durable-composer-drafts
status: complete
depends-on: [issue63-durable-threads-001, issue63-thread-isolation-001]
---

# Durable per-thread composer drafts

## Objective

Keep the current process-local AgentId draft isolation and additionally restore
each durable ThreadId's composer text and UTF-8 byte cursor after a TUI restart.
Do not persist AgentId, cursor positions that are not valid UTF-8 boundaries,
sent messages, pending work, or the unassociated composer. On Windows retain
the existing process-local behavior because the thread registry does not set
protected ACLs there; do not claim private durable drafts on that platform.

## Schema and lifecycle

- Add an optional `composerDraft` object to each versioned thread record. It
  stores bounded `text` and a byte `cursor`; `null` means no draft. Existing
  records without the field decode as empty drafts, and strict unknown-field,
  project-identity, registry-size, and atomic private-write rules stay intact.
  The registry remains v1; this additive optional field retains v1 forward
  reads, with no promise that older Glass versions preserve it on rewrite.
- Validate draft text against the existing 16 KiB composer limit, reject NUL,
  require the cursor to be in range and on a UTF-8 character boundary, and cap
  aggregate stored draft text at 64 KiB within the 256 KiB registry budget.
- On Unix, enqueue outgoing saves and incoming loads through one coalescing
  background worker; resolve AgentId-to-ThreadId and access the ledger there,
  never on the UI thread. Keep process-local AgentId drafts for immediate
  switching and send-failure recovery. Apply a late load only when that
  agent's edit generation is unchanged and no local draft superseded it.
- Persist text and cursor after composer key handling; deleting/sending a draft
  persists the empty state. Windows skips these durable operations and keeps
  the existing in-memory behavior.
- If save enqueue fails or an asynchronous write failure is known, keep the
  outgoing agent selected with its draft intact. Quit performs the bounded
  final flush; failure requires visible confirmation to discard before exiting.
  Discard runs on the same worker after any in-flight write, clears the selected
  thread draft, and quits only after the clear is acknowledged. A clear failure
  keeps the warning visible and allows retry; cancelling keeps editing.

## Acceptance

- [x] Restart-like reopen restores text and cursor by ThreadId even when the
      process-local AgentId changes.
- [x] Switching between agents retains the existing in-process draft behavior.
- [x] Invalid drafts fail closed without replacing the prior registry.
- [x] Windows keeps process-local drafts and makes no private-durability claim.
- [x] Focused tests, format, strict scoped Clippy, documentation validators,
      and local diff checks pass; local Conventional Commit is recorded.
- [ ] Independent review of the async load/save and switch-race fix is pending.

## Verification

Passed: `cargo test -p glass-dev --lib draft --locked` (16 passed),
the durable-load tests (2 passed), the writer-responsiveness test (1 passed),
and the in-flight discard-after-timeout test (1 passed),
`cargo fmt --all -- --check`, `git diff --check`, strict scoped Clippy, the
documentation coverage/depth/release-truth validators, and the TUI shortcut
validator. The refreshed live MCP catalog measures 342 tools and 181,498 UTF-8
bytes; the baseline fixture and budget were corrected because facdc577 already
registered `glass.agent.resume` but omitted it from those inventories.

Windows cross-target compilation could not run in this environment: the
`ring` build script selected GNU `cc` for the MSVC target and rejected its
`-m64` argument; an MSVC-compatible cross C compiler is unavailable. Windows
serde defaults and opaque preservation have dedicated cfg-gated source tests.
