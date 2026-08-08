# Getting started

Glass is a local browser intelligence and development runtime. It does not
host a browser or infer an autonomous plan. The caller chooses an operation;
Glass resolves current evidence, enforces policy and revision guards, performs
the bounded operation, and returns verification or recovery data.

## Select the product

| Package | Installs | Use it for |
|---|---|---|
| `glass-dev` | `glass`, `glass-browser` | Full CLI, TUI, MCP server, project runtime, mobile workspace, and browser control |
| `glass-browser` | `glass-browser` and Rust crate `glass_browser` | Standalone browser control plane and embeddable Rust API |

Install the full environment from a checkout:

```console
cargo install --path crates/glass-dev --locked
glass doctor
```

Or install only the browser control plane:

```console
cargo install --path crates/glass-browser --locked
glass-browser doctor
```

`doctor` is browser-free. It reports paths, platform support, policy,
profiles, stores, daemon state, extension status, and remediation without
starting Chrome.

## Select an interface

```text
human terminal ──> glass CLI or Ratatui TUI ─┐
MCP client ──────> glass --mcp ──────────────┼─> shared session/policy runtime
Rust program ────> glass_browser crate ──────┘             │
                                                             └─> Chrome/Chromium
```

| Interface | First command | Contract |
|---|---|---|
| CLI | `glass navigate https://example.com` | One process and bounded result per invocation |
| TUI | `glass` | Long-lived local browser and development workspace |
| MCP | `glass mcp-config --client generic` | Long-lived stdio JSON-RPC session |
| Rust | `cargo add glass-browser --rename glass` | Owned `BrowserSession` lifecycle |

## Observe before acting

Start with structured page evidence:

```console
glass navigate https://example.com
glass observe --level interactive
```

An interactive observation includes a revision and revisioned references such
as `r7:b42`. Use that revision when acting:

```console
glass click r7:b42 --expected-revision 7
```

Glass rejects a stale revision before sending browser input. Name, role, text,
CSS, and ordinal locators are supported, but each must resolve exactly one
target. Revisioned references are preferable in automation.

Normal observation does not request screenshots, full DOM, or form values.
Those are explicit operations:

```console
glass screenshot --output evidence.png
glass dom
glass observe --form-values
```

Policy may require an allow or confirmation for sensitive evidence.

## Use a safe session

For disposable browsing:

```console
glass --incognito navigate https://example.com
```

For an authenticated local profile:

```console
glass profiles create work
glass --profile work --headed navigate https://example.com
```

For untrusted input, pin an exact public host under hardened policy:

```console
glass --policy hardened --incognito \
  --policy-allow-host example.com \
  navigate https://example.com
```

Keep CDP on a trusted local interface. Do not expose port `9222` publicly.

## Open the terminal workspace

```console
glass
```

Desktop mode provides browser and Development workspaces. Narrow SSH and Mosh
terminals automatically use the phone layout; force it with:

```console
glass --tui-layout mobile
```

Continuous pixels are off by default. `live on` enables an ephemeral Herdr,
Kitty, or ANSI view. `safari` prints the stable private SSH-forward workflow
for an iPhone. See [mobile and remote development](mobile-remote.md).

## Connect an MCP client

```console
glass mcp-config --client generic
```

Use the emitted absolute executable path. The client must initialize,
negotiate Glass schemas/capabilities, send `notifications/initialized`, and
only then call tools. See [MCP integration](mcp.md) and the
[complete tool catalog](mcp-tools.md).

## Embed in Rust

```toml
[dependencies]
glass = { package = "glass-browser", version = "0.3" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

The Rust crate name is `glass_browser` unless Cargo renames it. Read the
[Rust SDK guide](rust-sdk.md) for lifecycle, semantic extraction, Task
Protocol, knowledge, backend, and development-runtime APIs.

## Recover and close

Mutation failures can be `indeterminate` when Chrome may have accepted input
before transport failure. Do not blindly retry. Use returned recovery data or
`glass recover-run EXECUTION_ID`, re-observe, and reconcile state.

Owned Rust sessions should call `BrowserSession::close().await`; this asks
owned Chrome to close so profile state can flush before process fallback.
Attach mode never owns or closes the external Chrome process.

## Next steps

- [Complete feature reference](features.md)
- [CLI reference](cli.md)
- [Rust SDK](rust-sdk.md)
- [Actions and revisions](actions.md)
- [Semantic execution](semantic-execution.md)
- [Security policy](../SECURITY.md)
