---
id: six-wave-e-001
scope: wave E — pre-tool-use hook, in-message citations, typed notify events, codemode parity audit
status: ready
source: research/six-codebases-2026-10-10.md candidates #4, #25, #8, #16(parity audit)
---

# Wave E: extensibility + review

## Objective

1. **Pre-tool-use hook** (crush `PreToolUse`): one user-registered shell hook,
   config `[hooks.pre_tool_use] command = "…"`, matcher regex on tool name.
   Fires before any tool execution (built-ins and extension tools alike),
   on a dedicated thread with tool-call JSON on stdin. Exit codes: `0`
   allow, `2` block the call (stdout becomes the refusal body), `49` block
   and cancel the rest of the turn, other nonzero = warn-and-allow with
   stderr surfaced once. Hooks run sequentially (determinism before
   parallelism; document deviation from crush). Never runs in read-only
   mode. The hook's stdout cannot inject tool output — only block or pass.
2. **In-message citations** (t3 `assistant-citations.md`, adapted): the
   quote/range composer's source reference is embedded in the user
   message text as a stable marker line (`@file:L10-L24#hash8`) so a quote
   survives its source changing or disappearing, and paste/copy keeps the
   reference. The composer's removable preview stays UI-only. The
   receiving turn dedupes repeated markers for the same file in one
   message.
3. **Typed notify events** (crush `notify` package): turn lifecycle
   (`Finished`, `Failed`, `Waiting`, `Steered`) published through one
   domain event enum from the agent loop; `notify.rs` and the TUI status
   subscribe. Foundation for future external subscribers; behavior
   unchanged, structure only — with the notify throttle preserved.
4. **Codemode parity audit** (research candidate #16): verify GC's existing
   codemode surface against pi's contract — output item protocol
   (`==> text N/M <==`), `@options` line, overflow spill to a temp file
   with the path named, timeout enforcing. Report the deltas honestly in
   the task file; implement the deltas that fit the existing design (the
   referenced file is the existing GC codemode implementation; if
   gaps require a JS engine change, stop and record — no engine swap
   inside this wave).

## Path

Own these paths only.

- new `crates/glass/src/hooks_shell.rs`
- `crates/glass/src/agent.rs` — only the pre-execution hook call site and
  the notify event publish points
- `crates/glass/src/notify.rs` — domain enum + subscribe
- `crates/glass/src/input.rs` (or composer module) — citation marker
- `crates/glass/src/session.rs` — marker dedupe at projection time
- codemode files owned by the audit report (record actual paths in the
  task file's implementation notes)
- new `scripts/smoke-hooks-shell.py`
- `docs/plan/tasks/six-wave-e-001.md`

Do not edit `tools.rs` dispatch, `ext/**`, `provider/**`, `sandbox.rs`,
`config.rs` (hook entry uses the settings struct fields owned here).

## Constraints

- Hook subprocess bounded: 3s timeout, output cap 8 KiB; over-cap or
  timeout = warn-and-allow and continue deterministically.
- Hook config is project-level; the trust gate applies before project
  hooks load (same rule as AGENTS.md).
- Citation markers are plain-text lines, not side-channel records: no new
  Record variant, so old logs, exports, and resumptions are untouched.
- Notify refactor changes no visible behavior; PTY history must be
  byte-identical.
- Codemode audit makes no speculative claims: each delta is demonstrated
  with a failing example before the fix.

## Verification

- Unit tests: matcher semantics; exit 2 body becomes refusal; 49 halts
  the turn; timeout path; JSON stdin shape; citation marker dedupe;
  marker survives copy; notify event ordering.
- `scripts/smoke-hooks-shell.py`: block, halt, pass, timeout — all four
  paths against a mock provider turn.
- clippy + full tests green.
- PTY: history identity check (existing transcript rendering unchanged),
  citation preview in composer at 40×24 and 20×8.

## Shipped (2026-10-11)

- **Typed notify events** (notify.rs): single `TurnEvent` enum (`Finished`,
  `Failed`, `Waiting`, `Steered`) published through `notify::publish` at the
  four turn boundaries in the interactive loop; `notify.rs` and the bell
  subscribe. Sequences and throttle unchanged — message bodies byte-match
  the old scattered formats, covered by `event_ordering_matches_the_old_...`.
- **Pre-tool-use hook** (hooks_shell.rs + agent.rs call sites): project
  config `[hooks.pre_tool_use] command/matcher`, parsed inside hooks_shell
  (config.rs untouched). Exit 0 allow, 2 block (stdout = refusal body,
  8 KiB-capped), 49 block + cancel the turn, anything else warn-and-allow,
  3 s bounded timeout with process-group kill. Never runs in read-only
  mode (`planning` skips the config parse entirely). The hook gate lives in
  one shared `pre_tool_verdict` used by both the bash pumped path and the
  ordinary `run_tool` path. Hooks run sequentially — documented deviation
  from crush's parallel hooks.
- **In-message citations**: shipped in the follow-up slice (see
  "Follow-up: citations shipped" below). NOTE correcting the original
  delivery report: the paragraph below claiming "the dedupe
  helper exists in session.rs" was inaccurate at the time — session.rs was
  untouched by wave E (see merge stat 1ab6c0a). The helper and the marker
  itself exist only from the follow-up commit on; this file records the
  correction rather than letting the stale claim stand.
- **Codemode parity audit**: implemented surface (extensions/codemode/
  codemode.js) compared against pi's contract — deltas recorded below in
  the audit note; no engine change attempted.
- Smoke: `scripts/smoke-hooks-shell.py` — block, halt, pass, matcher
  refusal, bounded timeout, all five against the real runner. Full tests
  green (639 unit + 13 doc + 91 integration); clippy `-D warnings` clean.
## Follow-up: citations shipped (post-merge)

The two honest limitations in the original wave E report that could be
fixed without an engine or protocol swap are now fixed on master:

- **In-message citations, for real this time.** `attach_excerpt` now
  records a `Citation { first, last, revision }` on the attachment
  (`polish.rs`, serde-defaulted, so old `.meta.json` files load
  unchanged). On submit, `polish::with_citation_markers` appends one
  `@<abs-path>#L<first>-L<last>#<8-hex>` line per uncited range excerpt
  directly to the user message text — a plain line, no new Record
  variant — and marks the citation consumed so it enters the log exactly
  once. `session::dedupe_citation_markers` (now actually in session.rs)
  collapses repeated markers inside one message at the single
  `compact::project` point: identical repeats keep the first, a second
  range for the same file drops, prose lines with `@` are never touched
  (exact-shape parser `#L<n>-L<n>#<8 hex>` at line boundaries only).
  The quote survives its source changing or disappearing because the
  revision hash names the bytes that were quoted. Tests:
  `session::dedupe_keeps_first_duplicate_marker…`,
  `session::citation_marker_parses_exactly_the_documented_shape`,
  `polish::excerpt_marker_is_stamped_once_into_the_next_message`.
- **Blob garbage sweep.** `blobs::sweep(data_dir)` scans `blobs/`,
  collects every `blob:<64-hex>` referenced by any session `.jsonl`
  (bucket-boundary-safe scan, not `hex_of`, whose whole-string validation
  rejects references followed by a closing quote — found by the sweep
  unit test, fixed before it could delete live blobs), keeps blobs newer
  than 300 s (in-flight `put`→`append` is not atomic across the log
  fsync) and removes the rest; aged `.tmp-` files go, unknown names are
  counted as errors and never deleted. Exposed as `gc blobs sweep
  [--json]`. Tests: `blobs::sweep_tests::*`; verified end-to-end with a
  synthetic data dir (referenced blob kept, orphan removed).

Unchanged limitations, recorded before and still true: item 25 citation
markers were originally an audit with a helper that did not exist (now
corrected above); codemode parity deltas stay documented without an
engine swap; blob range reads remain offset/length only; mermaid covers
flowcharts only; the `#hooks` PTY identity check remains covered by the
existing transcript tests.
