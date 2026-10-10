# Six-codebase feature wave — delivery plan

Source: [six-codebases-2026-10-10.md](../../research/six-codebases-2026-10-10.md).
The user authorized shipping all candidates. This plan orders them into five
waves of disjoint tasks. Two standing requirements bind every task.

## Standing requirements

1. **Performance.** The startup path and the per-request hot path must not
   regress. Every task states its measured effect on: binary startup, time to
   first token, per-tool-call overhead, and resident memory. Anything lazy
   must be lazily initialized — no new work on ordinary startup, no new
   background service. Tasks that add bounded memory state the bound.
   Where a hot path changes, measure before and after with
   `scripts/bench-edit-formats.py`-style fixtures or a new timing check.
2. **Design consistency.** New user-facing surfaces reuse the shared theme,
   the modal input, the Markdown renderer, the tool-card presenter, and the
   palette-command pattern. No bespoke colors, no new dialog
   choreography, no second way to open a tool detail. New model-facing
   surfaces follow the two-tool default: everything beyond `bash`/`state`
   ships as an extension tool unless the turn loop itself needs it.
   Record shapes follow the existing `Record::` conventions with
   forward-compatible JSON; bounds are stated in output, never silently
   truncated.

## Delivery contract

- Root implements; Luna medium reviews after implementation, per slice.
- Each wave ships behind its own task file with disjoint path ownership
  (the ext-lifecycle relocation lesson). Verify clippy + tests + smoke +
  PTY per wave, commit locally, update the 43-item ledger with new
  checkpoint sections. No push.
- A feature is not done by scaffolding: each acceptance names the
  user-visible behavior and the headless JSON contract.

## Wave matrix

| Wave | Theme | Items (candidate # from research doc) |
|---|---|---|
| A | Agent-loop correctness | 2 loop detection, 3 read recency, 15 sticky routing, 1 cache affinity |
| B | Security hardening | 10 process hardening, 7 admin requirements layer, 9 egress policy proxy |
| C | Context management | 12 checkpoint/rewind tools, 13 wait tool, 14 prewalk, 11 branch summarization |
| D | Representation + perf | 5 mermaid, 20 fs-scan cache, 21 blob addressing, 6 OSC 8 hyperlinks |
| E | Extensibility + review | 4 pre-tool-use hook, 25 in-message citations, 8 typed notify events, 16 codemode parity hardening |

Deferred deliberately (with reasons): 16 codemode full sandbox parity needs
a JS engine decision — the parity audit runs in wave E and the outcome, not
a rewrite, is committed; 17 apply-patch extension ships after wave C
merge-back needs it; 18/19 auth broker + secret redaction require the
remote protocol (43-item #15) to land first; 22 DAP stays deferred per
43-item #35; 30/34/31/32/33/35/36/37/38 are recorded in the research doc as
dependent on surfaces GC does not have yet.

## Wave orders and gates

Each wave: implement → clippy `-D warnings` → `cargo test --locked
--workspace` → smoke scripts → PTY at 40×24 and 20×8 → measure the
perf gates → independent review → local commit + ledger entry.

## Tracked completion

- [ ] Wave A: agent-loop correctness (4 items)
- [ ] Wave B: security hardening (3 items)
- [ ] Wave C: context management (4 items)
- [ ] Wave D: representation + perf (4 items)
- [ ] Wave E: extensibility + review (4 items)
