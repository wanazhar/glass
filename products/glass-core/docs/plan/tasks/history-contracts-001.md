# History and handoff contracts

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Status: implemented; final verification pending. Roadmap items 14, 26 and 27 remain partial.

`gc history SESSION [--json]` and `/history` expose retained compaction boundaries,
summary previews (4 KiB), raw record positions, retained-tail offsets, prompt hashes,
and persisted branch lineage. At most 32 boundaries and lineage entries are presented.
Original logs are retained. This inspector does not change model context or files.

`gc handoff SESSION [--json]` and `/handoff` expose the most recent user intent
(4 KiB), answer (8 KiB), at most 32 durable notes (512 bytes each), artifact paths,
and an explicit command for retrieving omitted history. Extension capabilities are
not transferred. Text is data, not authorization to execute. Previews can contain
project information; this is deliberately not the redacted diagnostic bundle.

Both commands use the shared native CLI and structured modal renderer. Modules are
loaded statically but do no I/O until requested. Reading full history retains the
existing session-index cost; no new service or background polling is introduced.

Attachment cancellation and preview return a no-change result; only a persisted
attachment gets a success acknowledgement. Branch creation persists typed lineage
in an append-checked display-only event alongside the existing human receipt.

Final acceptance: Unicode bounds, headless JSON, narrow terminal modals, branch
lineage after reopen, compaction across resume, and original log preservation.
