# Reasoning replay and mobile interaction review

## Frontend

Independent source review passed for `044ea34`. Native details semantics, keyboard selection, current command lookup, scoped touch scrolling, stable list nodes, and viewport transitions were inspected. No global touch cancellation or delayed focus workaround was introduced.

The web build and syntax/DOM audits passed. Served shell `91d5d80cc2e7` matched installed build metadata during visual inspection; the subsequent private-record UI guard is deployed as `bc1c8f508607`, also matched against public build metadata. A controlled read-only browser view used actual served HTML/CSS/fonts with API requests blocked and representative local DOM. Expanded project chevrons rotate 90 degrees; open headings have a distinct surface and thread groups indent 18 px.

At 393 x 852 the palette occupies y=146..706; at a keyboard-shaped 393 x 420 viewport it occupies y=12..408. Both retain overflowing command lists and visible header/footer. These are layout observations, not physical iPhone keyboard/touch verification.

No tests or model requests were run. No user prompts, real thread selection, or source workspace data changed during the visual inspection.

## Provider

Independent source review passed for provider commit `f9d3038`. `cargo check --workspace` passed after merge preparation; no test suites or live inference were run. The implementation captures Chat Completions reasoning separately, associates it with exact assistant content and tool IDs, preserves it through context projection/rewind/compaction, includes it in token estimates and request hashes, and excludes it from Responses/Anthropic requests and ordinary history/export views. Limits reject oversized reasoning and tool batches before tool effects.

The affected historical conversation has one completed tool round, three saved display reasoning chunks, and subsequent HTTP 400 errors requiring reasoning_content. Legacy display reasoning lacks an explicit provider wire format marker.

A targeted recovery script passed independent source review and a read-only dry run. It requires the exact single-turn shape, original 116 bytes of reasoning and one matching tool call/result, a completed Kilo trace followed by the required-field error, no indexed checkpoint records, an offline worker, and an exclusive writer lock. It backs up the original file and atomically inserts only the association record. Historical wire format remains a strong inference explicitly accepted for this single repair; no general migration or fabricated reasoning is used. Activation and repair completed.

## Activation evidence

The optimized release build completed successfully. Installed binary SHA-256 is `1c6032c6e5eb5b57a1c4446e41db42b0fc466321ede280e7ea3965be6e171d04` (7,411,304 bytes). All 21 real chat runtimes and the gateway match its executable inode. Public `/api/status` returned HTTP 200; served frontend shell is `bc1c8f508607`.

The targeted repair inserted one internal association record with the original 116-byte reasoning payload. Excluding that marker, the existing conversation is byte-identical to its private backup, including the tool result. No model requests or tool executions occurred. Operational reports and backups remain private local artifacts.

Gateway rotation after the first batch hit an address-in-use error caused by TCP TIME_WAIT. Activation stopped, the port was observed until clear, then the gateway was restored using unchanged routing/security settings. A guarded resume updated the remaining nine original idle workers without repeating the repair or updating already-current workers. Final checks confirmed all workers use the installed candidate. No active task cancellation or forced worker termination occurred.

Physical iPhone first-keyboard touch scrolling and a real provider continuation have not been exercised. The latter was deliberately left for the user's explicit retry rather than automatically submitting a potentially billed request.
