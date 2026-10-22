# Request context transparency

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

2026-10-09

## Delivered

- Main agent requests capture metadata before dispatch: model, assembled system size, instruction and text attachment source sections and sizes, current skill catalogue presence and paths, history role counts, image count, available tool names/schema sizes, and approximate input tokens. Summary requests are identified separately. Transport retries reuse their request manifest; background cache warming and other sessions remain separate.
- `/context` shows the last recorded request and distinguishes it from current configuration. “Inspect past model requests” opens a scrollable history browser; `glass-core context ID --json` includes the latest manifest without a terminal. Reports survive instruction file edits/deletion and session reopen. Older sessions explicitly show no captured manifest.
- Private `.context` sidecars store metadata only, outside conversation and compaction records. Reads/writes reject symlinks and non-regular files on Unix; writes sync and roll back on failure. Session deletion removes sidecars.
- Skill descriptions are distinguished from skill bodies. Tools/results remain inspectable through `/details`; the report does not claim model cognition or filesystem reads that have not occurred.
- Footer labels turn input/output and session total tokens accumulated during this run. Context fill uses final model request input rather than cumulative session or multi-round turn usage; unknown final usage does not show a fill estimate.

## Verification

- 578 workspace tests pass (491 core, 11 mod API, 76 TUI), one ignored doctest; strict all-target Clippy passes.
- Unit coverage: metadata omits prompt/file/result bodies; sidecars leave model/compaction records unchanged; Unix permissions and symlink rejection; two-round usage retains 91k aggregate input separately from 1k final-request input; context fill ignores cumulative session spend.
- Real offline PTY smoke-context at 40x24/20x8 checks actual HTTP sources vs saved metadata, catalogue/body distinction, token labels, deletion-independent source reports, headless inspection, and request-history modals.
- Debug smoke-scroll-submit, smoke-chat, smoke-reader and smoke-live pass through 20x8.
- Final release build passes; release smoke-context (including scroll-to-bottom of request details) and smoke-scroll-submit pass at 40x24/20x8. Installed PATH binary preserved; updated workspace executable is target/release/gc.
- GPT-6 Luna medium review identified compaction retention and aggregated token-fill concerns; both fixed and covered by tests. Image estimate wording corrected to disclose its fixed rough allowance.
