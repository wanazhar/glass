# Neuralnets and model guidance protocol

Local, project-scoped learning with explicit provenance. Model proposals are pending;
only a human review promotes them into retrieved reference facts. Memory never grants
permissions or overrides current instructions. No embeddings, background model calls,
or extra core tools. Existing reviewed project memory remains compatible.

Protocol v1 exposes discovery and recipes through state.guidance and scoped memory
through state.neuralnets. CLI and modal views use the same implementation. Discovery
is generated from the CLI help catalogue; detailed command schemas remain available
through GC_BIN command --help. Learning content is data, never executable guidance.

Acceptance: bounded private atomic storage; corrupt stores fail closed; locked writes;
revision-checked review/forget; pending entries excluded from context; bounded search;
read-only model access; model proposals cannot approve themselves; CLI/modal parity;
release PTY verification and final independent review.

## Delivered interfaces and boundaries

- `gc guidance search QUERY`, `read TOPIC`, `catalogue`, and `/guidance`.
- `state {"guidance":{"action":"read","topic":"files"}}` uses the same recipes.
- `gc neuralnets list [OFFSET]`, search/read/propose, revision-checked
  approve/forget/restore/purge; `/neuralnets` provides paginated native inspection and human review.
- Models use `state {"neuralnets":{"action":"propose","kind":"lesson","text":"..."}}`.
  Source records the session and call ID. Candidates are never automatically approved.
- Existing `gc memory` facts remain independent and compatible; no migration or
  deletion of old optional extension files is performed.
- Confirmed reference injection is capped at 8 entries / 2 KiB, refreshed once per
  user turn and reused across tool rounds. Proposals leave the prefix unchanged.
  Explicit review or forgetting changes the next turn's reference snapshot, including
  snapshots retained by compaction. Context manifests disclose loaded references.
- Store is bounded to 512 entries / 1 MiB, protected by a nonblocking file lock,
  private atomic writes, strict parsing, and revision checks for human review.
- Review gates protect the native model API. An unrestricted shell can invoke CLI
  actions or modify files; this is not an OS authorization boundary.
- No embeddings, external service, hidden summarizer, automatic preference inference,
  global cross-project sharing, or autonomous self-approval. Planning mode retains its
  existing inspect-only tool surface. Discovery does not change configured permissions.

## Verification

Unit tests exercise recipe discovery, strict namespace input, model review refusal,
read-only mutation refusal and compaction reference refresh. Release smoke scripts
exercise lifecycle, stale review, restore, pagination, private/corrupt storage, native
state provider dispatch, stable prefixes and headless plus 20x8/40x24/80x30 PTYs.

Final verification (2026-10-12): optimized release build, 822 workspace tests (716 core, 13 mod API, 93 TUI) and strict
binary clippy passed. Both neuralnets release smoke scripts passed, including real
state-tool dispatch over a mock OpenAI stream, confirmed reference loading, pending
model proposals, stable request prefixes, and human modal approval at 20x8, 40x24,
and 80x30. Offline performance smoke passed: worst median interactive startup 8.3 ms,
key visibility 0.43 ms, redraw 0.45 ms, and 12,000-record history frame 121.4 ms.
Luna medium review found lifecycle/context-budget bugs that were fixed; final review
found no blocker. These are local/mock-provider checks, not live-model reliability proof.
