# Native durability and terminal output verification

Date: 2026-10-15. Native Rust implementation; no additional runtime dependencies.

## Scope

The new optional host API supplies atomic entry/document/task commits, version
migrations, checkpoint recovery, idempotent input admission, submission ordering,
provider identity, owned cancellation and waits, forks, resets, and committed
observation. It reuses GC's agent engine and provider/tool protocol. See
[native durable runtime](../../reference/durable.md) for supported methods,
storage bounds, scheduling boundaries, and differences from Pi's package APIs.

Terminal changes add native language highlighting and a bounded LaTeX-to-Unicode
formatter. The compact Ctrl+D dialog follows Glass Dev's presentation and retains
the chat and composer. Math is a documented terminal subset, not a TeX typesetter.

## Evidence

| Check | Result |
| --- | --- |
| `cargo test --workspace` | 982 passed; two existing integration tests ignored |
| `cargo build --release -p glass-core --bin glass-core` | Passed |
| `scripts/conformance-durable.py` against release | Passed |
| `scripts/conformance-runtime.py` against release | Passed |
| Runtime TypeScript client `npm test` against release | Passed |
| `scripts/smoke-rendering-audit.py` | 45 release PTY cases passed |
| `scripts/smoke-native-output.py` | Six release code/math and exit PTY cases passed |
| `scripts/check-performance.py` against final release | All budgets passed |
| `scripts/check-native-performance.py` | Passed; no unsolicited checker process |
| Updated local documentation links and `git diff --check` | Passed |
| Independent GPT-6 Luna medium review | No unresolved findings |

The external durable fixture verifies admission before execution, FIFO input
visibility, atomic documents/tasks, process-local task registration, checkpoint
recovery, deduplication after reopen, stable provider affinity, paginated reads,
fork document policy, abort acknowledgement, and context reset. It also kills its
own runtime after tool dispatch and verifies that recovery does not repeat the
unsafe external effect. A separate abort-then-new-conversation case guards against
stale cancellation state.

The terminal fixtures exercise 80x30, 40x24, and 20x8 screens, exact source records,
streaming and replay, modal Escape/Enter behavior, draft preservation, and resize.
Unit tests also compare character-by-character math/code streaming with replay,
preserve unsupported inline math source, and check language token styles.

Root verification caught and fixed premature queued-input visibility, abort
recovery, draft disappearance under the exit dialog, cancellation while a host
tool is blocked, and cancellation leaking into a subsequent conversation.

## Performance

Final release: 6,821,264 bytes, 327,680 bytes above the prior installed build
(about 0.31 MiB). Ordinary sessions do not open the optional durable journal or
receive extra model prompt text.

Measured on this machine, with compilation stopped:

| Metric | Median or worst median |
| --- | --- |
| `--help` startup | 1.26 ms |
| Interactive prompt startup | 6.09 ms |
| Visible key response | 0.13 ms |
| Resize redraw | 0.31 ms |
| Reader frame, 12,000 records | 142.85 ms |
| History search, 12,000 records | 5.19 ms |
| Native 1 MiB range read | 4.00 ms |
| Native 1 MiB edit | 26.85 ms |

An earlier reader timing run exceeded its budget during concurrent compilation;
both unloaded reruns passed. These are local smoke measurements, not guarantees
for other machines. The durable reference documents metadata cloning and journal
scans so future indexed storage work has an explicit baseline.
