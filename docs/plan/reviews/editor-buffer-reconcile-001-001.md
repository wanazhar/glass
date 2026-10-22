# Review: editor-buffer-reconcile-001

Reviewed commits: `faf65716` (`feat: reconcile external editor buffer changes`) and
`baa9b554` (`docs: complete editor reconciliation task`). Source review only; no
builds or tests were run during review. The verification results below are the
ones recorded in the task file, not independently rerun here.

## Findings

- **P2, non-blocking — resolved by `6a2cdf66`.** The initial index-by-index
  comparison could misrepresent insertions and deletions. The follow-up aligns
  common lines with LCS when the product of input line counts is at most
  1,000,000, and falls back to a common-prefix/suffix replacement block above
  that cap; output remains capped
  at eight changed lines and 100 characters per line (`crates/glass-dev/src/tui/state.rs`,
  `aligned_line_operations` and `bounded_line_diff`). Work is bounded by the
  table cap or a linear fallback, plus linear processing of the bounded buffer
  and snapshot inputs. The new `external_compare_aligns_insertions_before_bounding_changed_lines`
  regression asserts an insertion at the start and a later line replacement
  both appear, and separately asserts that many changes emit the truncation
  marker (`crates/glass-dev/src/tui/state.rs`, test of that name).

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
passing. The follow-up adds the insertion/later-change/truncation diff test and
unchanged/changed-identity reload attribution test. The coordinator separately
reports this post-follow-up check passed:
`CARGO_TARGET_DIR=/home/ubuntu/work/glass/.worktrees/issue63-mixed-tools-001/target cargo clippy -p glass-dev --lib --locked -- -D warnings`.
No builds or tests were rerun as part of this review.

## Conclusion

**pass** — no blocking contract mismatch found. The prior P2 diff-alignment
finding is resolved by `6a2cdf66`; its regression coverage verifies later
changes remain visible after an insertion and that omitted changes are marked
as truncated.
