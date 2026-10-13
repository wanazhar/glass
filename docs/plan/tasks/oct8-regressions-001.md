---
id: oct8-regressions-001
status: complete
scope: confirmed defects from recent shipping commit review
---
# Oct8 regressions

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Confirmed LSP defects

1. Content-Length body reads must observe the same deadline as header reads. A server that sends a partial body then holds the pipe open must time out within the configured budget, including during shutdown.
2. If initialization fails after spawning a language server, close its pipes and terminate/reap that test-owned server. The manager must not forget a still-running child.
3. Notifications cannot extend a request past its overall deadline. Process
   diagnostics already buffered in memory before waiting for pipe readability.

Preserve framing across buffered messages, diagnostics and shutdown semantics. Add deterministic partial-header/body, buffered-frame and failed-initialize cleanup regression checks. Run the extension protocol smoke and affected PTY/host integration checks. Preserve all unrelated user edits. Additional confirmed findings may be recorded here before implementation; do not change intentional behavior merely because it differs from an assumed convention.

## Verification scope

Recent shipping commits: 3780736, 037a4b1, de987c5. Workspace tests, strict Clippy, fresh release and every smoke script are under independent review. Existing CI edit adds ten new wave suites and is preserved. Local commits are authorized. No default installation changes, real provider account calls or push are requested.

The user subsequently authorized local commits. The three original edits were
reviewed and committed as `174a125`. LSP regression self-tests now run from
`scripts/smoke-lsp.py`, so the CI suite exercises the failure paths.
Baseline verification: 477 workspace unit tests, strict Clippy, release build,
and all 29 smoke scripts passed. The final frozen-source pass also passed all 29 suites; the exact release SHA256 is
`663da87fa0e37859743cdebc5aa50d5e97388bc07204e424f2efb4625ee748e9`.

Independent verification and constrained PTY results are recorded in
[the review](../reviews/oct8-regressions-001-01.md). At 20x8, the loop
acknowledgment scrolls out of the live frame; execution, clear and reader
access were verified separately rather than counting that immediate-frame
fixture assertion as passing.
