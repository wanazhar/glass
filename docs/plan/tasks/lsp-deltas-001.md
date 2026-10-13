---
id: lsp-deltas-001
scope: roadmap item 30 — debounced diagnostics deltas and stale-checked multi-file code actions
status: implemented
depends-on: []
---

# LSP diagnostic deltas and stale-checked code actions

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Roadmap item 30. This task closes the acceptance gap in
[research/t3code-parity-2026-10-10.md](../../../research/t3code-parity-2026-10-10.md)
item 30. It does not claim the rest of item 30 is complete.

## Objective

Close the three verified gaps in the LSP extension, without adding startup cost:

1. **Diagnostic deltas.** Today `store_diagnostics` replaces the whole per-URI
   list and `format_diagnostics` renders the entire list on every tool result.
   Compare the previous published set with the new one and report only what
   changed: newly introduced problems and problems that disappeared. Retain the
   full current set for context, but lead the rendered text with the delta so a
   model is not asked to re-read an unchanged list every turn.
2. **Multi-file coverage.** `handle_tool_result` currently `break`s after the
   first valid path, so only the newest written file is ever checked. Check every
   path the edit actually wrote, bound the count, and say explicitly when the
   bound truncated the set.
3. **Stale-checked code actions.** Code-action and rename results are preview
   only and explicitly tell the model to use revision-checked file edits. Make
   that mechanical: capture the file revision when the action is requested,
   compare it immediately before applying, and refuse with the reread hint when
   it moved. Reuse the existing revision-checked edit path; never write through
   an unchecked channel.

## Context

- [LSP task](lsp-001.md) — the existing contract this task refines.
- [Tooling foundation](tooling-foundation-001.md) — revision-checked read and
  edit semantics. The LSP path must reuse these, not invent a second writer.
- `docs/INDEX.md` — document index.

## Path

Own these paths only. Do not edit any Rust file; another task owns the Rust tree
in this wave.

- `extensions/lsp/lsp.py`
- `scripts/smoke-lsp.py` (the existing LSP smoke check)
- `scripts/lsp_fixture_server.py` (the existing fixture server, if the new
  cases need more diagnostics to be published)
- `docs/plan/tasks/lsp-deltas-001.md`

## Constraints

- LSP stays optional and lazy. No server starts until the extension is enabled,
  and nothing here may add work to terminal startup.
- A code action must never be applied to a file whose revision changed since
  the action was computed. Refuse and say which file moved.
- Multi-file checks are bounded, and the bound is stated in the output rather
  than silently truncating.
- Diagnostics that disappear are reported as resolved; a delta that is empty
  should say so briefly instead of reprinting the unchanged list.
- Never send `workspace/applyEdit` or `executeCommand` for an unverified edit.

## Verification

- Python unit coverage in the extension for: delta computation (added, removed,
  unchanged, and changed-message cases), multi-path iteration including the
  truncation message, and a stale-revision refusal that does not write.
- `cargo clippy --workspace --all-targets -- -D warnings` stays green (no Rust
  change is expected from this task).
- Run the LSP smoke script(s) and report the exact command and result.
- Confirm the extension still loads disabled-by-default and that enabling it
  spawns a server only on demand.

## Status (2026-10-11)

Verified implemented on master: `diagnostic_delta` + `delta_rows` render
the changed/added/resolved delta ahead of the retained set (acceptance 1);
`handle_tool_result` checks every path in the edit with a stated bound
(acceptance 2); `PendingAction` captures SHA-256 revisions at request time
and refuses to apply when any file moved (acceptance 3). `smoke-lsp`
green. Status ledger corrected from `ready` — the worktree branch and
master carry the same `lsp.py` (1358 lines).
