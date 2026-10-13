# Ship Next 20: Wave 1 Independent Verification

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

**Result:** PASS for wave 1 at production source freeze `04335e5`. The verification checkout was at `7884c3c71cd6fe25f49bdbac4a5b2be37850f39a`; the intervening changes are limited to smoke assertions in `scripts/smoke-motion.py` and `scripts/smoke-workflows.py`.

## Checks

- `cargo test --locked --workspace`: passed in parallel after the export-test fixture fix. This ran 453 `glass-core` tests and 75 `glass-tui` tests; one doctest was ignored.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed.
- Fresh locked release build in `/tmp/gc-wave1-target`: passed. Binary SHA-256: `1566eecea7cee6feca614acec14d0b7f156415ed739c1d1ac4da893e36e3002e`.
- All 37 `scripts/smoke-*.py` checks passed against that release binary. The final script-only motion and workflow assertion fixes were rerun successfully; the other 35 scripts passed on the same binary before those assertion-only changes.
- Doctor checks confirmed the default diagnostics path is offline and redacts secret values, endpoint URLs, environment-variable names, and local paths. Reader checks exercised search, draft preservation, resize, and transcript navigation against terminal cells at 40x24 and 20x8.
- Reliability checks passed for crash recovery and durable server request deduplication. An independent partial-write EIO injection confirmed that a failed tool-intent write prevents tool execution, records no provider retry, and rolls back the incomplete log tail. Active-writer export and symlink-alias locking checks also passed.
- `scripts/check-performance.py` passed all configured budgets on the release binary, including 12,000-frame history. Measured worst medians: prompt startup 5.73 ms, key visibility 0.16 ms, redraw 0.33 ms, reader frame 71.79 ms; history search 1.84 ms; child peak RSS 3,364 KiB.
- Final reruns of `scripts/smoke-motion.py` and `scripts/smoke-workflows.py` passed. Motion exercised five terminal sizes through 20x8; workflows passed all six flows at 80x24, 40x12, 24x10, and 20x8.

## Scope and limits

Smoke providers and network listeners were local test fixtures; no real provider credentials or external provider calls were used. Installer/upgrade smoke coverage used temporary test roots. Performance figures are from this host and are not a cross-platform benchmark. This report covers wave 1 only and is not approval of the remaining ship-next-20 items.
