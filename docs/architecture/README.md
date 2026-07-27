# Glass architecture

Status: Accepted

## Purpose and boundary

Glass is a single Rust binary that gives local automation clients direct
control of Chrome through raw CDP. It owns the client and session lifecycle;
Chrome remains the browser process. Glass does not generate workflows.

## Product constraints

- Raw JSON over CDP WebSockets; no Playwright or Chromium automation wrapper.
- Keep the client binary, resident memory, CDP round trips, and returned
  context small.
- Screenshots are explicit visual requests, never an implicit observation cost.
- `human` interaction preserves the existing Bézier pointer path. `fast` is the throughput path.
- CLI, MCP, and TUI call the same browser data plane.

## Scope relationships

```text
CLI ─────┐
MCP ─────┼──> BrowserSession ──> CdpClient ──> Chrome
TUI ─────┘          │
                     ├──> Profile/Chrome lifecycle
                     └──> compact PageSnapshot
```

`BrowserSession` owns browser semantics. Frontends do not issue raw CDP commands. `CdpClient` owns WebSocket request routing and lightweight event delivery. Chrome lifecycle owns only processes started by Glass.

## Main concepts

| Concept | Definition |
|---|---|
| owned session | A Chrome process launched by Glass and a page selected by Glass. |
| attached session | An explicitly requested connection to an existing CDP endpoint and target. |
| compact observation | URL, title, bounded text, and accessible interactive controls; no full DOM or screenshot. |
| deep DOM | An explicitly requested full DOM tree intended for debugging or narrow inspection. |
| snapshot revision | A monotonically changing page-state generation used to reject stale element references. |

## Cross-module decisions

- Existing CDP endpoints are never silently adopted. `--attach` is explicit,
  ignores only the default profile value, and rejects launch-only profile flags.
- Named profile data is Chrome's user-data directory; it is the single persistence source of truth.
- Incognito sessions use both Chrome's `--incognito` flag and a Glass-owned disposable user-data directory.
- Default observations are compact. Full DOM and images are separate operations.
- CLI and MCP serialize structured results as compact single-line JSON. Their
  `observe` operations return compact context unless `includeDom`/`--deep-dom`
  or `includeScreenshot`/`--screenshot` is requested; `getDOM`/`dom` is an
  explicit deep-inspection operation.
- The TUI preserves its current layout, but browser I/O runs in a worker task rather than the render/input loop.

## Module index

- [Browser data plane](browser.md)
- [Automation contracts](automation.md)
- [Terminal UI](tui.md)
