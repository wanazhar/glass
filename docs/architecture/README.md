# Glass architecture

Status: Accepted

## Purpose and boundary

Glass is a reusable Rust library plus one `glass` executable from the
`glass-dev` package that gives local automation clients a semantic
execution layer over direct Chrome control through raw CDP. It owns the client
and session lifecycle, bounded extraction, Web IR reconciliation, deterministic
task compilation, and guarded execution; Chrome remains the browser process.

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
MCP ─────┼──> Task Protocol ──> Web IR compiler ──> guarded task executor ──┐
TUI ─────┘                                                                  │
CLI ─────┐                                                                  ▼
MCP ─────┼───────────────────────────────> BrowserSession ──> CdpClient ──> Chrome
TUI ─────┘                                        │
                                                  ├──> Profile/Chrome lifecycle
                                                  └──> bounded PageContext
```

`BrowserSession` owns browser semantics. Frontends do not issue raw CDP
commands. High-level tasks must pass through the browser-free Web IR compiler
before the guarded executor dispatches an existing browser operation.
`CdpClient` owns WebSocket request routing and lightweight event delivery.
Chrome lifecycle owns only processes started by Glass.

## Main concepts

| Concept | Definition |
|---|---|
| owned session | A Chrome process launched by Glass and a page selected by Glass. |
| attached session | An explicitly requested connection to an existing CDP endpoint and target. |
| compact observation | URL, title, bounded text, and accessible interactive controls; no full DOM or screenshot. |
| deep DOM | An explicitly requested full DOM tree intended for debugging or narrow inspection. |
| snapshot revision | A monotonically changing page-state generation used to reject stale element references. |
| Glass Web IR v1 | Stable, bounded semantic entities, relationships, evidence quality, coverage, and limits reconciled from one page revision. |
| Task Protocol v1 | Strict high-level intent contract with semantic scope, risk, ambiguity, revision, postcondition, and resource policies. |
| compiled task plan | Deterministic, value-free operations and preconditions bound to one validated Web IR revision. |

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
- Live task execution extracts one fresh Web IR, compiles the task without CDP,
  enforces revision and confirmation preconditions, dispatches only through the
  guarded browser runtime, and verifies bounded postconditions.
- Browser-free CLI, MCP, protocol, and Rust helpers use the same stable Web IR,
  Task Protocol, and compiler contracts as live execution.
- The TUI preserves its current layout, but browser I/O runs in a worker task rather than the render/input loop.

## Module index

- [Browser data plane](browser.md)
- [Automation contracts](automation.md)
- [Semantic execution](../semantic-execution.md)
- [Terminal UI](tui.md)
