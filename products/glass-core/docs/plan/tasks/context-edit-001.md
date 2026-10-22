---
id: context-edit-001
scope: crates/glass session records and projection, repl commands
status: done
depends-on: []
---
# Context edit

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Objective

`context_edit`, borrowed from pi's `ContextEditEntry`: append-only edits that
change only *future model context* while the raw history, transcript,
exports, and accounting keep the original.

## Context

- `Record::ContextEdit { turn, mode, text }` with modes `hide`, `replace`,
  `restore`; display-only, shown as a Meta line in the transcript and an
  italic marker in exports.
- Commands: `/forget <turn>` hides a whole turn (user, assistant, tool
  records), `/forget` alone lists turns with their edit state,
  `/rewrite <turn> <text>` substitutes that turn's user message only, and
  `/unforget <turn>` restores. The latest edit per turn wins, exactly as an
  append-only log implies.
- `Session::messages()` applies edits to the **raw** records before
  compaction projects them, because turn numbers must match `/branch` and
  the transcript, which both count raw-log turns. Active edits are read from
  raw history so compaction cannot drop their metadata. Edits to a turn
  before the latest compaction marker invalidate its derived summary and
  state notes. The model then receives the first surviving task and a
  bounded suffix of whole edited turns, preserving tool pairs. Edits to
  later turns retain the existing compaction projection. Without edits the
  in-memory projection is unchanged.
- Deliberate difference from pi: no session tree, so "navigating before the
  edit reveals the original" does not apply; the raw log is the escape
  hatch, and edits are branch-relative only in the sense that forks copy the
  log.

## Paths

- `crates/glass/src/{session,compact,export,polish,main}.rs` (record,
  projection, report section, commands, help, palette).
- `scripts/smoke-editcontext.py`.

## Verification

- Unit tests: hide removes a turn from the model view while the log keeps
  it; rewrite substitutes only the user message; restore wins over an
  earlier hide; tool records of a hidden turn disappear; validation
  (turn 0, out-of-range, non-numeric, missing replacement).
- `scripts/smoke-editcontext.py`: two turns, `/forget 1` and the next
  request lacks the first turn; `/unforget 1` brings it back; the raw log
  contained it throughout.
