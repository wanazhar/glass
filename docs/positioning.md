# Positioning: Where Glass Fits

Glass is a small, local control layer for Chrome and Chromium. It turns
explicit commands into browser actions and keeps browser ownership, targeting,
policy, and session state in one process.

Glass is not a test framework, a hosted browser, or a workflow generator.

## The landscape

| Category | Typical use | Glass |
|---|---|---|
| Local browser control | Explicit commands against a local browser | Yes |
| Test framework | Assertions, fixtures, trace viewers, and cross-browser suites | No |
| Workflow generator | Choose and recover an entire browser workflow | No |
| Hosted browser | Run browsers on remote infrastructure | No |

## Use Glass when

- Chrome or Chromium can run on the local Linux or macOS host.
- The calling program can provide explicit browser commands.
- Target ambiguity should fail instead of selecting the first match.
- Compact observations and bounded responses are useful.
- Browser state should remain local and a dedicated profile is acceptable.
- A CLI, terminal UI, MCP server, or Rust library is the right integration
  surface.

## Choose another tool when

- You need Firefox, WebKit, Safari, or Windows release support.
- You need a full QA framework with assertions, test fixtures, code generation,
  or a trace viewer.
- You need a hosted browser fleet or remote rendering infrastructure.
- You want one command to invent and execute an entire workflow.
- You need stealth, fingerprint modification, CAPTCHA solving, or bot-evasion
  features.

## What distinguishes Glass

- It uses raw CDP through a native Rust runtime.
- It does not silently adopt an existing CDP endpoint; attach mode is explicit.
- Locators resolve uniquely, with bounded candidate information on ambiguity.
- Observations publish revisioned references, and guarded actions can reject
  stale page state before mutation.
- Screenshots, full DOM, form values, downloads, uploads, evaluation, and other
  privileged operations are explicit and policy-gated.
- The CLI, terminal UI, MCP server, and library use the same browser session
  semantics.

For command details, see the [CLI reference](cli.md), [MCP guide](mcp.md), and
[action contract](actions.md). For resource measurements and comparison
methodology, see the [category metric guide](category-metric.md) and
[benchmarks](../benchmarks/README.md); those measurements are version- and
environment-specific and are not product guarantees.
