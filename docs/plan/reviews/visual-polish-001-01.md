# Independent review: visual-polish-001

## Scope

Reviewed the updated color and motion contract, semantic theme helpers, Painter/event pump ownership, busy composer rendering, modal workers and profile theme persistence. Latest user steering removes reduced-motion settings; active work animates by default.

## Findings

- **P2, blocking, resolved:** the transcript backend originally used direct stdout while transcript widgets retained explicit colors. This violated `docs/plan/visual-polish.md` NO_COLOR contract. `crates/glass/src/main.rs` now wraps that backend with `TerminalWriter`; actual PTY transcript frames contain no color SGR under NO_COLOR.
- No remaining blocking findings in the reviewed source and debug PTY checks.

## Evidence

Independent fixture `/tmp/gc-independent-motion.py` runs actual delayed SSE and a delayed Bash process through a real PTY, separate from developer smoke scripts. Debug verification log: `/tmp/gc-independent-motion.log`.

Verified at 65x30 and 20x8:

- Active provider and tool activity redraw in place; tick-only windows contain no LF.
- Draft text remains visible while activity runs; 65x30 to 20x8 resize preserves the draft.
- Cancellation returns to a usable composer; idle output remains empty after cancellation and completed work.
- NO_COLOR removes foreground/background SGR from startup, streamed responses, Appearance, Help, transcript and provider errors while retaining neutral emphasis and terminal controls.

Code review confirms original theme indices remain 0 Aurora, 1 Amber, 2 Monochrome; Orchid and Ocean are appended. Profiles retain the existing numeric theme field. Tick redraws run on the existing input pump with a 125ms minimum interval; workers do not write activity to stdout. Busy draft rendering saves/restores the stream cursor and confines its viewport to one row while preserving the full text buffer.

## Conclusion

**Pass.** All seven PTY suites and the independent fixture passed against installed `/home/ubuntu/.cargo/bin/gc`. Installed logs are `/tmp/gc-installed-visual-{features,onboarding,polish,workflows,mobile,chat,motion}.log` and `/tmp/gc-installed-independent-motion.log`. Normal runs explicitly remove inherited NO_COLOR; the motion suite and independent fixture separately verify NO_COLOR. The new shipping suite also proves all five requested sizes, actual modal loading frames, multiline busy drafts, Orchid profile restart persistence and exact headless JSON. Root owns workspace tests, Clippy and installation/upgrade evidence.
