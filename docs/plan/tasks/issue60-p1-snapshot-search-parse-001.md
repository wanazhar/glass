---
id: issue60-p1-snapshot-search-parse-001
scope: glass-dev/snapshot-worker-and-project-search-and-tui-parse-metadata
status: complete
depends-on: []
---

# Issue #60: reap snapshot workers and preserve bounded metadata

## Objective

Resolve the current snapshot worker lifecycle, file-search coverage, and
incremental-parse metadata findings in Issue #60. Also make browser screenshot
result projection match the canonical tool contract exactly.

## Current-source findings

- F70 was present in `IncrementalSyntax::sync_tree`: an unchanged-source cache
  hit set `last_cache_hit` but left `last_edit` and `last_changed_ranges` from
  the previous edit visible to consumers.
- F69 was present in `SnapshotWorker::drop`: the join handle was dropped without
  joining when `running_job` was set, leaving shutdown dependent on an
  unobserved background thread exit.
- F82 was present in `ProjectWorkspace::search`: it called `list_files()`,
  which returned only the entries and discarded the bounded traversal's
  `truncated`, `limit`, ignored-directory, and skipped-symlink metadata before
  `glass.file.search` serialized the hits.
- F68 was present in `browser_result`: substring matching treated any tool name
  containing `screenshot` as the canonical `glass.browser.screenshot` result.

## Contract and implementation

- A parse-cache hit clears per-sync edit and changed-range metadata while
  retaining the cached tree and setting `last_cache_hit`.
- `SnapshotWorker::drop` sends shutdown and joins the worker even when a tool
  call is active. The TUI's terminal guard is dropped first, so joining cannot
  leave the terminal in raw mode.
- Project search returns ranked hits together with a `fileListing` coverage
  envelope containing `truncated`, `limit`, `ignoredDirectories`, and
  `skippedSymlinks`. The TUI continues to render the same hits and reports when
  the file listing was bounded.
- Only the exact `glass.browser.screenshot` tool receives the screenshot
  summary projection; other tool results use the generic projection.

## Verification

- `cargo test -p glass-dev --lib --locked --offline tui::parse::tests:: -- --nocapture`
- `cargo test -p glass-dev --lib --locked --offline tui::snapshot::tests:: -- --nocapture`
- `cargo test -p glass-dev --lib --locked --offline development::agent::tests::file_search_preserves_truncated_tree_metadata -- --exact --nocapture`
- `cargo test -p glass-dev --lib --locked --offline tui::projection::tests:: -- --nocapture`
- `cargo test -p glass-dev --lib --locked --offline tui::snapshot::tests:: -- --nocapture`

The cache metadata regression and truncated search envelope regressions were
first run against the old behavior and failed on the stale/missing fields,
respectively. The focused suites passed after the fixes.
