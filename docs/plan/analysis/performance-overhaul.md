# Performance overhaul analysis

## Module decomposition

| Module | Inputs | Outputs | Delivery task |
|---|---|---|---|
| `browser/cdp` | CDP requests/events | minimal response/event transport | perf-001, action-003 |
| `browser/session` | frontend actions | compact context and action results | perf-001, action-003 |
| `browser/chrome`, `profile` | CLI session options | owned/attached Chrome lifecycle | lifecycle-002 |
| `mcp/server`, `cli/runner` | agent/CLI requests | compact persistent frontend responses | mcp-004 |
| `tui` | keys and browser events | non-blocking terminal rendering | tui-005 |
| tests/benchmarks | real sessions/fixtures | performance and behavior gates | verify-006 |

## Integration enumeration

| Producer → consumer | Required integration proof |
|---|---|
| CLI/MCP/TUI → `BrowserSession` | Each frontend uses compact observation and typed actions. |
| `BrowserSession` → `CdpClient` | Deep DOM is opt-in; selector resolution does not fetch a full DOM. |
| `SessionOptions` → Chrome launch | Named/incognito/attach mode selects exactly the documented profile/process behavior. |
| `BrowserSession` → TUI worker | Worker owns session lifecycle and sends bounded events to UI state. |
| MCP/CLI → JSON output | Default agent context is compact and screenshots remain explicit. |
| benchmark → real browser | Cold/warm latency, payload, and memory measurements are reproducible. |

## Delivery constraints

- Do not add Playwright, Chromium wrapper libraries, or an in-binary LLM.
- Do not add a large framework merely to replace existing Tokio/Crossterm/Ratatui code.
- Preserve the existing human pointer implementation and make fast-mode measurements explicit.
- Existing user worktree changes are not part of this delivery unless explicitly staged by a delivery task.
