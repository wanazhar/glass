# Glass architecture

Status: Accepted

## Purpose and boundary

Glass is a reusable Rust library plus one `glass` executable from the
`glass-dev` package that gives local automation clients a semantic execution
layer over the Glass-owned native browser runtime by default. It also exposes
an explicit Chromium/CDP migration session and bounded portable sessions for
externally managed Firefox BiDi and Safari WebDriver endpoints. It owns the
client and session lifecycle, bounded extraction, Web IR reconciliation,
deterministic task compilation, and guarded execution; external browser
processes are selected explicitly rather than silently used as a native
fallback.

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
MCP ─────┼──> Native BrowserRuntimeSession ──> NativeEngineBackend
TUI ─────┘                         │
                                  └──> bounded PageContext / software surface
Explicit Chromium ────────────────> BrowserSession ──> CdpClient ──> Chrome
Native Rust API ──────────────────> NativeEngineBackend
```

The native BrowserRuntimeSession is the default browser seam for
feature-enabled CLI, MCP, and TUI entrypoints. `BrowserSession` owns the
explicit Chromium migration semantics. Frontends do not issue raw CDP
commands. High-level tasks must pass through the browser-free Web IR compiler
before the guarded executor dispatches an existing browser operation.
`CdpClient` owns WebSocket request routing and lightweight event delivery.
Chrome lifecycle owns only processes started by Glass.

The portable endpoint path remains separate from the native data plane:
BrowserRuntimeSession can talk to an externally managed Firefox BiDi or Safari
WebDriver endpoint and exposes only its certified semantic subset. It does not
silently enter the TUI/MCP/Chromium lifecycle shown above.

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
- The native engine is enabled by default in feature-enabled products and
  currently owns local and bounded HTTP(S) navigation (including bounded
  standard padded-base64 `data:text/html`) in its current phase,
  exposes bounded presentation/normal-flow and outer/content box geometry with
  physical four-side padding/margin shorthand and longhand cascade plus
  bounded physical min/max width/height constraints,
  root horizontal and vertical viewport scrolling, plus native point hit testing through explicit Rust or native local CLI
  paths. Its current Rust-only presentation artifacts include side-specific
  solid/dashed/dotted-border paint, bounded physical circular border radii, a
  bounded inline-box line placement, bounded fixed pixel line-height flow,
  bounded direct-text flow fragments/source-order text paint, bounded word-aware
  wrapping, bounded source-whitespace boundaries across supported inline flow,
  and bounded `overflow:hidden` clips shared by paint, viewport projection, and
  point hit-testing, bounded axis-specific `overflow-x`/`overflow-y`
  `hidden`/`clip` clips through the same owner, a display list, a logical RGBA software surface, and
  bounded PNG capture; it is the primary selected backend for current browser
  entrypoints, while final Core Web Profile certification remains an explicit
  issue #40 gate.

## Module index

- [Browser data plane](browser.md)
- [Native browser engine](native-engine.md)
- [Automation contracts](automation.md)
- [Semantic execution](../semantic-execution.md)
- [Semantic core hardening](semantic-core-hardening.md)
- [Semantic resource budgets](semantic-resource-budgets.md)
- [Terminal UI](tui.md)
