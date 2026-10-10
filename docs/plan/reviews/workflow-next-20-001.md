# Workflow delivery verification — 2026-10-10

## Scope

All twenty features in [the delivery matrix](../workflow-next-20.md) have functional v1 implementations. [The user guide](../../workflows.md) documents commands, optional dependencies and limits.

Root implemented the changes; a GPT-6-Luna medium reviewer reviewed runtime, validation, accounting and integration behavior. Review fixes include canonical ownership paths, strict state validation, bounded LSP frames, private browser artifacts, reviewer output limits, missing-usage reservation release and actual model identity in request metadata.

## Final release evidence

Verified `/home/ubuntu/work/glass-core/target/release/glass-core`, with `target/release/gc` pointing to it. The installed PATH binary was not replaced.

- `cargo build --release --package glass-core --bin glass-core`: passed.
- `cargo test --workspace`: 632 passed, no failures; one ignored documentation test.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `scripts/smoke-workflow-next.py`: 22 cases passed, including 15 overlay PTY checks at 20×8, 40×24 and 80×30. Includes task/dependency/ownership validation, role budget, checkpoint deduplication, typed parent handoff notice, capture and cross-project preview/fork.
- Deterministic real coding fixture: 208 ms, five requests (four main and one explicitly triggered reviewer), 2300 reported input tokens including 400 cached, 120 output tokens; 73,286 PTY bytes. These are fixture results, not live-provider latency claims.
- `scripts/check-performance.py`: help startup 1.20 ms median; worst terminal-size median prompt startup 10.19 ms, input visibility 0.35 ms, redraw 2.16 ms. A 12,000-record history reader measured 141.27 ms worst median; all configured budgets passed.
- `scripts/check-native-performance.py`: approximately 9.6 ms / 9.4 ms / 42.0 ms median for 100 KB / 1 MB / 8 MB reads; 26.0 ms for a 1 MB edit. No unsolicited Python checker process.

Existing release workflow, agent, CLI, help, context, reader, presentation, block, plugin and security suites also passed on the preceding candidate; the final source change only bounded workspace owner/agent labels. LSP fixture coverage passed for navigation, UTF-16 positions, preview edits and invalid inputs. Optional browser coverage passed both dependency-free fixtures and an actual test-owned Chromium instance, verifying screenshot, console capture and preservation of the connected browser.

## Deliberate v1 limits

- Claims coordinate cooperating agents; they do not isolate processes or prove task checks ran.
- Rename and code actions are previews. Review checkpoints are explicit and bounded; there is no background model watcher.
- Policy shell matching is a global textual denial guard, not semantic shell confinement. External sidecars have their own trust boundaries.
- Request/context views store bounded metadata and hashes; missing usage and prices remain unknown or separately labelled estimates.
- Browser verification needs optional Playwright and an explicitly configured loopback CDP browser. It checks at DOMContentLoaded; dynamic pages may need another verification.
- Skill drafts require human review before saving or use. Sanitization is not a complete secret detector.
- Extension profiling is opt-in; its append-only local log currently has no automatic rotation.

No push, publication, installed binary replacement or interruption of user processes was performed.
