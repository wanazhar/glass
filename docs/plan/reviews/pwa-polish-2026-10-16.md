# PWA layout and output review

## Scope

Correct standalone phone layout, simplify interface copy, preserve streamed
content, enrich previews, and add a bounded file browser with directory-based
thread creation. No frontend runtime dependencies were added.

## Research

Inspected current T3 Code source for stable streamed content, grouped work rows,
live-edge following, settled Mermaid rendering, and preview loading boundaries:
[WorkLog](https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/chat/WorkLog.tsx),
[ChatMarkdown](https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/ChatMarkdown.tsx),
and [scroll anchoring](https://github.com/pingdotgg/t3code/blob/main/apps/web/src/components/chat/timelineScrollAnchoring.ts).
GC keeps its own native gateway and plain JavaScript interface. No application
source was copied.

## Independent audits

Two GPT-6 Luna agents audited mobile layout and output performance. Root made
all changes. Findings addressed: missing safe top and landscape insets, Safari
input focus zoom, drawer hit targets and focus isolation, full message DOM
replacement during streaming, redundant rendering after preview caps, diff source
copying, relative file preview images, and hidden-file empty states. Explicit
symlink navigation is permitted by policy; recursive usage scans never follow
links. Directory sorting is per page, not a snapshot.

## Verification

Browser checks cover desktop, four phone/landscape sizes, simulated standalone
safe areas, drawer access, 16px touch inputs, directory navigation, workspace-bound
thread creation, repeated Markdown previews and relative images, rendered diagrams
with source toggles, diffs, math, PDF, HTML sandboxing, stable streamed blocks,
text-to-tool transitions, and oversized output caps. Native tests cover paginated
metadata, path boundaries, symlink handling, and usage scans. Production dependency
count is unchanged. The initial script remains about 26 KiB gzip.

Physical iPhone Safari validation remains a user check. Chromium phone emulation
and injected safe-area values do not reproduce every WebKit or home-screen behavior.

## Final results

- Workspace: 1,002 tests passed; two existing tests ignored.
- Strict Clippy and release browser/lifecycle checks passed.
- Live HTTP audit: owner-authenticated directory listing succeeded; a mobile
  browser reported scale 1, workspace left 0, closed sidebar right 0, and no page
  errors. The existing unrelated HTTPS route was preserved.
- Local performance fixture: 5.85 ms worst median TUI startup, 0.30 ms redraw,
  120.28 ms history frame, and 4,008 KiB peak child RSS. These measurements do not
  represent provider latency or physical iPhone performance.
- The installed executable matches the verified release build.
