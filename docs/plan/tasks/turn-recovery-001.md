# Turn recovery and consistent tool cards

Status: delivered. Evidence: [delivery review](../reviews/tooling-foundation-001.md).

## Contracts

- Main interactive and headless turns continue until completion, cancellation, steering or an actual failure. The implicit 24 tool-round cutoff is removed. Explicit teammate round/output/token limits remain effective.
- A reasoning-only or output-exhausted response gets one answer-focused recovery request per response chain. A successful response restores recovery eligibility for subsequent tool rounds. Completed tools stay in history and are not replayed by the harness.
- Recovery instructions are appended to the conversation, not inserted into the cached system prompt. The recovery request asks for reduced reasoning; adapters apply supported controls. A model that ignores those controls and again produces no answer still reports a real failure rather than looping indefinitely or presenting reasoning as a completed answer.
- Summary requests use the configured model, fit the provider's estimated context allowance, reserve output capacity and request reduced reasoning. Failed model summarization falls back to a bounded extractive checkpoint. This checkpoint is explicitly identified, retains the opening goal, state and recent work excerpts, and leaves the full original transcript in the durable log.
- Local fallback is committed before replacing the in-memory projection, and replay does not reintroduce oversized raw tail outputs. This is an extractive recovery path, not a claim that every detail survives compaction.
- `/compact` and `/compaction` invoke manual compaction.
- Scroll/live transcript layouts retain typed card metadata and use the shared colored, bounded tool renderer. Full output opens only after explicit expansion. Running, failed, successful, truncated and spilled outputs retain distinct indications. Width-specific layouts remain cached; scrolling does not invoke a parser process or another provider request.

## Verification

- Unit coverage: stable system prefix through two separate reasoning-only failures, a main turn beyond 24 rounds, summary total bounds, durable failed-summary fallback/reopen, bounded tool cards and successful truncated output styling.
- Actual release PTYs: recovery through 28 tool rounds, manual compaction with a failed summarizer, scroll/return to composer and follow-up submission at 80×30, 40×24 and 20×8.
- Existing native file/job PTYs and performance checks remain regression gates.
