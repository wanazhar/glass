# Direct native file tools

Expose read/edit/write using the existing bounded revision-checked file engine.
Keep bash for search, pipelines, builds, tests and processes. Schemas stay compact;
advanced CLI helpers remain available. No duplicate file implementation.

Contracts: workspace confinement and no-follow paths; read-only profile refuses
mutations; hook/mod/extension authorization precedes execution; read-before-write
tracks native reads; previews do not claim writes; native calls retain issue order
and service input while running. File errors are ordinary repairable tool results.
Reserve builtin names against extension/mod shadowing. Existing planning mode stays
inspect-only. Validate release provider-wire and PTYs at 20x8/40x24/80x30, final
workspace tests, clippy and performance smoke, then Luna medium review.

## Verification

Completed against `target/release/glass-core`:

- 824 workspace tests pass; strict workspace clippy with all targets passes.
- Direct provider dispatch passes writable/read-only headless cases and PTYs
  at 20x8, 40x24 and 80x30, outside Git repositories. Exercises blind overwrite
  refusal, native reads, preview/apply, create-only writes, stale revisions,
  line edits, workspace escape refusal and durable paired tool records.
- Tool repair, background jobs, tooling foundation, context inspection,
  neuralnets and agent smoke regressions pass.
- Offline performance smoke passes, including input/redraw and 12,000-record
  history. Report: `/tmp/gc-direct-files-performance.json` (local measurement).
- GPT-6-Luna medium reviewed dispatch, cancellation, permissions, schemas and
  presentation; no blocker found. Read-before-write can precede the read-only
  permission error for unread existing files; both refuse the mutation.

No PATH binary installation or publication was performed.
