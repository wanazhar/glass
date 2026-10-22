# Review: editor-buffer-reconcile-001

Reviewed commits: `faf65716` (`feat: reconcile external editor buffer changes`) and
`baa9b554` (`docs: complete editor reconciliation task`). Source review only; no
builds or tests were run during review. The verification results below are the
ones recorded in the task file, not independently rerun here.

## Findings

- **P2, non-blocking — insertion/deletion comparisons can misrepresent the
  changed region.** The design calls for a bounded unified diff
  ([analysis, Contract](../analysis/editor-buffer-external-reconcile.md)); the
  implementation compares lines only at identical indexes and emits a
  synthetic `@@` marker for each mismatch
  (`crates/glass-dev/src/tui/state.rs:10676`).
  A one-line insertion near the beginning shifts all following lines, so the
  display can report unchanged content as changed and spend its eight-line
  budget before reaching the actual end of the insertion. The current test
  checks the bound and truncation for same-index replacements, but not shifted
  alignment (`crates/glass-dev/src/tui/state.rs:11532`).
  Consider a bounded LCS/patience-style alignment or an existing diff helper;
  keep the character and changed-line caps.

## Contract checks

- **Legacy serde compatibility: pass.** `original_exists` defaults to `true`
  when absent (`crates/glass-dev/src/development/project.rs:141`); a regression test removes the serialized field and verifies that default ([project.rs test `legacy_buffer_without_existence_flag_defaults_to_existing_file`]).
- **Missing-path identity: pass.** Reconciliation compares existence as well as
  content hash (`crates/glass-dev/src/development/project.rs:1077`); the save guard also checks expected existence and hash before replacement (`crates/glass-dev/src/development/project.rs:1453`). New empty buffers do not spuriously report a deletion, and deleted existing buffers remain resolvable by keeping local text.
- **Compare/reload/keep-local and dirty/deletion behavior: pass.** Dirty content is retained while the latest external snapshot/deletion is recorded; clean readable changes refresh the buffer. Reload requires readable disk content and clears history; keep-local rebases identity and remains dirty when the path is absent or content differs. The project and TUI tests cover these paths.
- **Attribution: pass.** Filesystem snapshots carry no actor; clean external refresh and reload of an already-open buffer use `Unknown`, not `Human` or `EmbeddedAgent`. Keep-local preserves the local buffer actor because the human-authored buffer text remains local.
- **Replacement guard and unrelated revision: pass with documented limit.** Save writes and syncs its sibling temporary file, re-reads the target identity, then renames (`crates/glass-dev/src/development/project.rs:1445`). This rejects a target identity that changed before that check and ignores unrelated project revision changes. There is no cross-process lock or CAS for a non-cooperating write after the check and before rename. The task wording in the reviewed worktree now accurately states that limit (`docs/plan/tasks/editor-buffer-reconcile-001.md:44`, `docs/plan/tasks/editor-buffer-reconcile-001.md:71`); the original “concurrent target-file change” wording would have overstated the guarantee.
- **Polling and documentation: pass.** The TUI calls reconciliation on a 500 ms interval, while the state method skips filesystem reads when there are no open buffers (`crates/glass-dev/src/tui/mod.rs:1188`, `crates/glass-dev/src/tui/state.rs:6267`). The architecture and runtime docs describe the resolution actions, deletion behavior, actor attribution, and target identity check consistently with the design.

## Verification evidence

The task record reports 22 project tests, focused TUI reconciliation/diff tests,
scoped strict Clippy, formatting, diff checks, and documentation validators as
passing. These were not repeated because the review request prohibited builds
and recorded results were available.

## Conclusion

**pass** — no blocking contract mismatch found. Record the bounded diff
alignment improvement as non-blocking follow-up.
