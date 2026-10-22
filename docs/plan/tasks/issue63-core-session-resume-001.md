---
id: issue63-core-session-resume-001
scope: issue-63-exact-core-session-recovery
status: complete
depends-on: [issue63-durable-threads-001]
---

# Exact Core session recovery

## Objective

Explicitly resume the Core session ID attached to a durable Glass thread,
verify its session identity, mixed ownership mode, and canonical project
directory, and hydrate its transcript before the recovered worker becomes
Ready. Never create a replacement session or replay a prior prompt.

## Paths

- `crates/glass-dev/src/core_runtime.rs`
- `crates/glass-dev/src/resident_harness.rs`
- `crates/glass-dev/src/agents.rs`
- `crates/glass-dev/src/thread_store.rs`
- `crates/glass-dev/src/tools.rs`
- `crates/glass-dev/src/tui/command.rs`
- `crates/glass-dev/src/tui/snapshot.rs` and relevant TUI tests
- `docs/plan/analysis/issue63-core-session-resume.md`
- `docs/plan/README.md`, current runtime/TUI docs as needed

## Acceptance

- [x] The existing Agent picker can explicitly resume a recovery-pending row;
      `:agent resume [AGENT_ID]` and `glass.agent.resume` target one exact row.
      TUI recovery uses its single ordinary resume-action confirmation; after
      recovery, thread interaction follows the existing session flow without
      a new confirmation for each tool call.
- [x] Recovery requires a persisted Core session reference and Core engine;
      it does not act on unavailable, terminal, or already-live rows.
- [x] Runtime startup negotiates v2/mixed tools, resumes only the stored ID,
      and never falls back to `session/new` on any failure.
- [x] A post-resume `session/describe` confirms exact session ID, `toolMode`
      `mixed`, and canonical project cwd before sends become possible.
- [x] Transcript is read from bounded `session/entries` pages and restored
      before Ready; provider/auth/config fields from the manifest are never
      copied into Glass state or user-visible output.
- [x] Successful recovery preserves ThreadId/Core session ID and appends a
      fresh random RunId. Failed recovery preserves the old durable reference
      and reports an actionable error without sending a prompt.
- [x] Fake-runtime restart journey verifies exact resume ID, transcript,
      ordering before Ready, and absence of `session/new` and `turn/prompt`.
- [x] Tests reject missing sessions, session-ID mismatch, non-mixed tool mode,
      and noncanonical/wrong project cwd; capability negotiation remains
      version-independent.
- [x] Core provider/auth configuration and Pi legacy behavior remain unchanged.
- [x] Focused `glass-dev` tests, format, strict scoped Clippy, docs validators,
      and diff review pass using the shared Cargo target; no full native suite.
- [x] Independent review passes; commit locally with a Conventional Commit.
      Do not push, publish, tag,
      globally install, restart services, or close issue #63.

## Verification evidence

- `reopened_thread` registry journey: 2 passed; exact session adapter: 2
  passed, including endless empty-page termination; thread-store tests: 11
  passed; resume router/TUI routes: 2 passed; snapshot worker check: 1 passed.
- `cargo clippy -p glass-dev --lib --bins --locked -- -D warnings` passed.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Documentation coverage passed (1,797 Markdown files); depth passed (97
  routed guides, 20 contracts); release-truth passed (0 current-claim failures).
- Independent read-only review passed after adding the total page bound; the
  fix is covered by the empty-page regression test. Committed locally as
  `a0907119`; no push or merge.
