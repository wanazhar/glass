---
id: worktree-apply-001
scope: crates/glass worktree module, subagent scratch location
status: done
depends-on: [subagent-fanout-001]
---
# Worktree apply

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Objective

The return path for worktree-isolated fan-out (Codex worktrees default-on,
Amp Restack, GitHub stacked PRs): `glass-core worktree list|apply`. Before
this, a subagent that finished its edits left a detached checkout with no
supported way back.

## Context

- Subagent scratch moved from `temp_dir/glass-subagent-<pid>` to
  `data_dir/subagents`, so worktrees survive the process and are
  discoverable. Nested sessions share the directory; session browsers bind
  by workspace and never list them.
- `apply` runs `git add -A` inside the checkout (new files included), takes
  `git diff --cached --binary HEAD`, and pipes it into
  `git -C <workspace> apply --3way -`. Conflicts stay in the working tree
  with git's markers and are reported as errors — no silent side-picking,
  no committing. `--remove` runs `git worktree remove --force` + prune.
- Git's `--3way` also stages the applied destination patch. The command
  creates no commit; inspect the staged diff before committing it.
- Cross-repository applies are refused by comparing `--git-common-dir`.
- `list` reports name, changed-file count, and age; `--json` for scripts.

## Paths

- `crates/glass/src/worktree.rs` (new) + `main.rs` dispatch, scratch paths,
  help text.
- `scripts/smoke-worktree.py`.

## Verification

- Unit tests: edits + new file round-trip with `--remove`; clean checkout
  reports "nothing to apply".
- `scripts/smoke-worktree.py`: through the binary against a real repo.
- `cargo test --workspace` (385 in glass-core) + clippy.
