# Tooling foundation: file recovery and explicit shell inputs

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Status: delivered first slice. Roadmap items 38–43 remain partially complete.

Evidence: [delivery review](../reviews/tooling-foundation-001.md).

Implementation requires independent review. Preserve the two model tools and existing native helpers. No new dependency or startup service.

## Contracts

- File reads remain UTF-8 regular files capped at 8 MiB and previews at 24 KiB. A truncated page provides an absolute UTF-8 byte continuation cursor and whole-file expected revision. Continuing a stale file fails explicitly. Existing line-range reads remain supported.
- Read CLI accepts `--byte-offset N` and `--expected SHA256`; unknown arguments fail rather than being ignored. Byte offsets must land on UTF-8 boundaries. Line numbers continue to identify original-file lines.
- File mutations support `preview:true`: validate and compute the same proposed result without committing. Receipts distinguish proposed, applied and unchanged, include base/new revisions, and correctly identify empty-file creation as a change.
- Exact-edit failures distinguish no match from repeated matches, identify the failed operation, and recommend a targeted reread. All operations validate before a commit.
- Mixed edit formats and unknown operation fields fail explicitly. Shell timeout cleanup kills TERM-ignoring descendants even if the shell exits first.
- State mutations, including clears and nested extension result-hook updates, persist before the outer result is acknowledged. A failed durable append rolls back notes. Compaction honors the latest clear rather than finding an older nonempty snapshot.
- Explicit invalid bash `cwd` or `timeout_ms` fails before spawning. Omitted values retain defaults; numeric timeout strings remain compatible, and valid values retain the documented bounds.
- Existing advisory directory locks serialize cooperative GC file writers. Atomic replacement does not promise compare-and-swap against arbitrary external writers in the final check/rename interval. Existing permission, symlink and hardlink protections remain.

## Verification

Regression cases cover long Unicode lines and successive bounded pages, stale continuation, invalid cursors, empty creates, dry-run edits/validation, unknown CLI arguments and invalid shell inputs without side effects. Run focused unit tests, release build and actual release PTYs including 20×8 and 40×24. Record measured evidence in a review document before marking delivered.

## Remaining roadmap scope

This slice does not complete all tooling work: process streaming/ownership, state receipts, full shared error contracts, artifact retention and representative task benchmarks remain tracked in the roadmap.
