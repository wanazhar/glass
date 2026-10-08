# Pi shipping independent verification

Reviewed `0954ab1` and `41b71a1` (after `1f3c8f3`) against
`durability-001.md`, `context-edit-001.md`, and `program-status-001.md`.
The baseline review confirmed four defects, which were corrected in the
working tree before this final gate:

- A valid JSON record without a final LF could be joined to the next append.
- Torn-tail truncation used a normalized byte count and could remove valid
  blank/CRLF bytes or a complete record.
- Partial recovery could remove its sidecar before checked log writes, and
  stale-partial detection could match an assistant response from another raw
  turn.
- Context edits after compaction could bypass the compaction projection and
  expose stale summaries/state or unbounded old context.

## Verification

- `cargo test --workspace --locked`: passed, 426 `glass-core`, 11
  `glass-mod-api`, and 72 `glass-tui` tests; one doctest ignored.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- Removed only generated release artifacts with `cargo clean --release`, then
  built with `cargo build --release --locked --package glass-core --bin glass-core`.
  SHA-256 of `target/release/glass-core`:
  `c0fdaa64a744f3c4f1f7c87dd63d4585cdc57a5fcf9816eb819da223e5c82f72`.
- All 32 `scripts/smoke-*.py` suites passed. Smoke runners received
  `GLASS_CORE_TEST_BINARY` set to the release binary above. The installer-only
  smoke checks its cargo arguments using a fake cargo executable; the upgrade
  smoke exercises its isolated install path. Per-suite output is in
  `/tmp/pi-shipping-smokes-final/`.
- Additional feature PTY checks passed at 40×24 and 20×8 using streamed PTY
  bytes and the smoke harness VT screen projection. They covered context edit
  listing, hide, rewrite, restore, rewritten model context, OSC 7501 on/off,
  response visibility, valid terminal cursor positions, and prompt recovery.
  Existing chat/live/motion/navigation suites also cover the transcript reader
  and composer at constrained sizes.
- Repair fixtures verify a newline-less valid record gets a separator before
  append, and torn tails preserve exact preceding blank/CRLF bytes and valid
  records. Recovery tests verify same-turn stale matching, retaining a sidecar
  when a checked append fails, and consuming it after successful recovery.
  Compaction tests verify edits do not reintroduce a compacted summary, stored
  state, or unbounded history containing hidden text.
- `git diff --check` passed. The two user-owned `.commandcode/taste` edits were
  preserved; no commit was made by this verification.

## Limits

The constrained PTY projection uses the repository's small VT parser, so it
checks emitted control sequences and projected cells rather than emulating
every terminal implementation. The end-to-end provider suites use local mock
endpoints and disposable configuration/data directories.
