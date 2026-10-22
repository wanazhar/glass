---
id: program-status-001
scope: crates/glass status module, config, repl hooks
status: done
depends-on: []
---
# Program status

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Objective

OSC 7501 program status (pi's implementation is the reference): the terminal
can show whether the tab is working, waiting on the user, done, or failed —
without the harness inventing a notification per state change.

## Context

- `status.rs` builds reports exactly as the pinned spec (Program Status
  Protocol v0.3) requires: `state=…:app=glass-core[:kind=…][:msg=…]`,
  `msg` standard base64 of one control-character-free line, capped at 200
  chars, `kind` only with `blocked` and only for known values. Root record
  only.
- States wired in the repl: `idle` at the prompt and after a cancel,
  `working` for a turn, `done`/`error` at turn end (error carries the first
  line of the reason), `blocked:kind=permission` around the project-trust
  modal and `blocked:kind=auth` around `/login`.
- **Documented deviation from pi:** pi performs the optional reply
  handshake and only reports after the terminal answers. That reply arrives
  as raw bytes, and glass-core's input layer reads through crossterm's
  event parser, which cannot observe it without replacing the reader. The
  spec explicitly allows reporting without detection ("sending reports
  without detecting support is safe" — terminals ignore unknown OSCs;
  multiplexers swallow them), so reports go out directly, gated to an
  interactive terminal with a real `TERM` so pipes and headless runs stay
  byte-clean. Adding the handshake later means a raw-byte input path.
- `program_status = "on" | "off"`, default on.

## Paths

- `crates/glass/src/status.rs`, `config.rs`, `main.rs` (configure, idle,
  working, done/error, trust and login hooks).
- `scripts/smoke-status.py`.

## Verification

- Unit tests: report framing, base64 msg, cap at 200 chars, `kind` rules
  (blocked-only, known values only), emit framing, and that a non-interactive
  process never enables reports.
- `scripts/smoke-status.py`: a turn emits `working` then `done`; with
  `program_status = "off"` the byte stream contains no `]7501;` at all.
