---
id: six-wave-d-001
scope: wave D — mermaid renderer, fs-scan cache, blob addressing, OSC 8 hyperlinks
status: ready
source: research/six-codebases-2026-10-10.md candidates #5, #20, #21, #6
---

# Wave D: representation + perf

## Objective

1. **Mermaid renderer** (codex `mermaid`): a bounded terminal-native
   diagram renderer over the shared Markdown renderer: flowcharts
   (`A --> B`, labeled edges), sequence diagrams and class/states are out
   of scope this slice — flowchart plus node/edge labels only, honestly
   stated. Edges own lanes; crossings never join; width-capped to the
   reader column; unsupported syntax or overflow returns the source in a
   fenced block (never a halfway drawing). Reuses the existing
   `glass_tui` writer conventions and theme palette. Extension-gated: the
   fenced block is unchanged when disabled.
2. **Fs-scan cache** (OMP `pi-walker`): one in-process bounded cache of
   workspace directory walks used by the todo/plugins/skills discovery
   paths that glob repeatedly in one session. Owned entries keyed by
   path; invalidated explicitly on every write/edit/rename tool result
   (and bash `mv`/`rm`/`git` commands via the existing output scan).
   Grep stays uncached. Bound and hit/miss counts reported in
   `/usage`-style diagnostics, never silently.
3. **Blob addressing** (OMP blob-artifacts): bash spill files and model
   image payloads are content-addressed under the data dir
   (`blobs/<sha256>`) with a `blob:sha256:…` reference kept in the
   record; large tool outputs already spilling to disk gain the address
   so a later session or handoff can re-read by content. Dedup is a
   correctness property: identical payload written twice stores once.
   No range protocol in this slice beyond offset/length read support in
   the spill reader.
4. **OSC 8 hyperlinks** (opencode): reader and tool-card URLs render with
   OSC 8 escapes when the terminal advertises support (existing
   capability map) — including inside wrapped lines, which simply works
   because the escape is applied to the source text before layout.
   Plain text fallback automatic.

## Path

Own these paths only.

- `crates/glass-tui/src/**` — mermaid block type and renderer
- `crates/glass/src/reader.rs` — OSC 8 emission
- new `crates/glass/src/mermaid.rs` — parser/decoder feeding glass_tui
- new `crates/glass/src/fs_scan.rs` — walk cache + invalidation API
- `crates/glass/src/partial.rs`, `crates/glass/src/session.rs` spill
  paths — blob addressing (touch only the spill write/read functions)
- `crates/glass/src/plugins.rs`, `crates/glass/src/skills.rs`,
  `crates/glass/src/loops.rs` — consume the cache
- new `scripts/smoke-mermaid.py`, `scripts/smoke-blob-addressing.py`
- `docs/plan/tasks/six-wave-d-001.md`

Do not edit `agent.rs`, `tools.rs` dispatch, `ext/**`, `provider/**`.

## Constraints

- Renderer cost bounded: max nodes per diagram (64), max edges (128),
  width cap; exceeded → source fallback. Zero allocations on plain
  code blocks — the fast path for non-mermaid fenced blocks in the reader
  must be unchanged (measure: reader redraw time on the existing
  history fixture is within 2%).
- Cache is bounded in entries and total bytes; stated in diagnostics.
  Invalidation is conservative: any failed match invalidates the
  directory.
- Blob writes are atomic tmp-rename; a second write of the same content is
  a no-op. Blobs without a referencing record at gc startup are left
  alone (no GC sweep in this slice — honest limitation in docs).
- OSC 8 emission is behind the existing capability detection; when
  unsupported, output is byte-identical to today (test the bytes).

## Verification

- Unit tests: flowchart parses/lays out within caps; overflow → source
  fallback; cache hit/miss semantics + invalidation on write; blob
  dedup; spill roundtrip with address; plain-code-block fast path
  unchanged.
- `scripts/smoke-mermaid.py`: history containing a mermaid block renders
  in PTY at 40×24 with the source fallback path provable.
- clippy + full tests green.
- Perf gate: reader redraw benchmark before/after within 2%; startup
  unchanged.

## Shipped (2026-10-11)

- **Mermaid** (glass/mermaid.rs + glass-tui gate): flowchart subset — node
  statements, `-->`, `-- text -->`, `-->|text|`, `--|text|>` edges, plus the
  `flowchart TD|TB|LR` header. Caps at 64 nodes / 128 edges / 16 ranks /
  column width; anything over falls back to the fenced source, never a
  halfway drawing. Host registers the renderer via
  `glass_tui::markdown::set_mermaid_renderer`; until `mermaid = true` no
  bytes change on any fenced block.
- **Fs-scan cache** (glass/fs_scan.rs): bounded (64 dirs, named-byte bound)
  directory walk cache for plugins/skills discovery, hit/miss/invalidation
  counters surfaced in `gc usage`. Conservative invalidation: any failed
  name match re-walks; every completed bash command invalidates all walks
  (untracked writes reach unknown paths). Grep and bash output stay
  uncached.
- **Blob addressing** (glass/blobs.rs): content-addressed objects under
  `data/blobs/<sha256>`, atomic tmp-rename writes, second write of identical
  content is a no-op lookup. Spill writes bind their address and the
  session record carries `blob:<sha256>`; `load_spill` reads back through
  the reference (ranged, same 64 KiB display cap). Known limitation, stated
  in the module doc: unreferenced blobs are not swept yet.
- **OSC 8 hyperlinks** (glass/osc8.rs): conservative capability map (TERM
  hints or explicit `hyperlinks = on|off|auto`); escapes wrap bare URLs in
  the plain-text history/handoff report; unsupported terminals get
  byte-identical output (asserted on the bytes).
- Smoke: `scripts/smoke-mermaid.py` (gate on/off behavior), 
  `scripts/smoke-blob-addressing.py` (spill carries `blob:<sha256>`, object
  name equals its digest, no tmp leftovers). 643 unit + 13 doc + 93
  integration tests green; clippy `-D warnings` clean.
