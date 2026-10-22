---
id: round3-regressions-001-01
scope: independent regression verification of the round-3 shipping fixes
status: done
depends-on: [round3-regressions-001]
---
# Round3 regressions

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## What was verified

The confirmed contract regressions listed in
[the task](../tasks/round3-regressions-001.md), against the frozen source tree
and the freshly built release binary.

## Evidence

- `cargo test --workspace --locked`: 521 passed (437 `glass-core`, 73
  `glass-tui`, 11 `glass-mod-api`); doc tests clean, one pre-existing ignored.
- Strict Clippy: `cargo clippy --workspace --all-targets --locked -- -D
  warnings` passed.
- Fresh release: `cargo clean --release && cargo build --release --locked`
  passed. Binary SHA-256:
  `fca1ef879e053f57459f08f663a79392d1dfa08feb31d915f8002a6693b9a6ac`.
- All 33 `scripts/smoke-*.py` suites passed against that release binary,
  including the clean install/upgrade smoke, in disposable fixtures. Per-suite
  output is under `/tmp/gc-round3-final-smokes/`.
- Round-three PTY suite passed at 40x24 and 20x8 using the actual VT cell
  snapshots. The harness matched wrapped phrases across frame borders; at
  20x8 it scrolled the skill-doctor dialog to the accounting text before
  asserting it. All title, review finding/refusal, and follow-up-turn checks
  passed at both sizes.
- `logs <id>` and workspace-resolved `logs` passed with a malformed trailing
  log record and a `.live.json` sidecar present. The session log and sidecar
  bytes remained identical after each invocation.
- Independent runtime fixtures against the same release SHA verified that a
  resumed request uses the saved compaction system prompt after prompt config
  changes, while a newer checkpoint with `system: null` selects the current
  prompt. Pre-compaction extension custom entries were observed after seeding
  in headless, interactive, and served sessions. ACP seeded a pre-compaction
  custom entry after `session/new`, then an empty second session seeded an
  empty snapshot and removed the prior state. A served session also completed
  `stop` successfully on its default socket path.
- Stop framing unit coverage exercised split exact JSON, unrelated/spoof
  frames, incomplete trailing frames, and timeout without EOF; the full serve
  lifecycle smoke passed on the release binary.

## Notes and limits

- This verification did not change production source or alter a user
  installation, credential, account, or session. The upgrade smoke confines
  installation state to its temporary directory.
- Extension append acknowledgments still have the documented queue-to-log
  crash window: the host drains queued custom entries after the tool call or
  turn. The review did not expand the protocol to acknowledge only after the
  durable append.
- The declared extension tool list remains frozen for cache stability. A
  disable takes effect immediately for execution, while the declared request
  list can retain the old declaration until the next session; the existing
  transcript note describes the freeze.
- Runtime checkpoint/custom-seeding cases used disposable local provider and
  extension fixtures. They do not exercise a real provider account.
