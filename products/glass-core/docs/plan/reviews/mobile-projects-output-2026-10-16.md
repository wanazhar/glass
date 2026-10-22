# Mobile outputs and project browsing

## Changes

- Compact command overlay with bounded scrolling and correct opening position.
- Closing the palette removes a standalone slash while preserving longer drafts.
- Notices can be dismissed, expire automatically, and clear when output resumes.
- Separate Files and Activity shortcuts, with consistent file and path typography.
- Table wrapping, CSV copy, expansion, bounded scrolling, and sticky headers.
- Serialized Mermaid rendering, theme-aware cache, unique SVG references, local
  source-preserving errors, and retry after a failed module import.
- All-project sidebar with full paths, counts, expandable groups, global search,
  project filtering, and older-page loading.
- Metadata-free and unavailable-directory archives can be read without starting
  a worker or guessing a project directory.
- Thread switches clear stale file panes. Metadata lookup supports direct links
  to conversations outside the loaded page. Search requests cancel stale work.

The native list already included bound conversations across projects. The browser
presentation did not make that scope clear, and the index excluded legacy chats
without project metadata. The new browser index includes those archives without
changing the terminal's existing resume behavior.

## Review and verification

One Luna medium reviewer inspected the changes and reviewed the subsequent fixes.
The review identified SVG ID replacement affecting labels, stale archive loads,
metadata lookup beyond page one, incomplete search coverage, and stale project
filter presentation. These findings were addressed.

- Workspace tests: 1,009 passed; two environment-specific tests remain ignored.
- Workspace Clippy, all targets: passed with warnings treated as errors.
- Exact optimized binary: passed the desktop/mobile browser and native protocol
  smoke, including project filtering, archive viewing, direct links, pagination,
  global search over more than 512 records, and crash-safe queue delivery.
- Diagram audit: 40 checks passed, covering 19 chart families in both themes,
  duplicate cached renders, and malformed input.
- Live mobile audit: 390 by 844, scale 1, no browser errors. Palette geometry was
  366 by 347 px and remained inside the viewport during its opening animation.
- Live index: 131 conversations across 25 project groups, including 64 unassigned
  archives. Index requests took 16–24 ms in the sampled browser checks.
- Performance guard passed on the final isolated run: CLI startup median 1.32 ms,
  redraw median 0.28 ms, and worst history-reader median 118.56 ms. A prior run
  exceeded the history threshold while other host work was active; measurements
  are sensitive to contention. No unrelated process was interrupted.

The installed binary matches the release output. The gateway was restarted and
thread workers were preserved. Recent hashed preview assets were retained under
16 MiB to support open tabs during this update. Browser assets and the native
release were checked against the installed files.

## Limits

Machine-wide means the current user's GC data directory. Other OS accounts and
separate data stores are not merged. Missing-workspace archives remain read-only.
Large diagrams retain source when preview limits or unsupported syntax prevent
rendering. Browser tests use Chromium mobile emulation; physical iPhone behavior
still requires user feedback.
