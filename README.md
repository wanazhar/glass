# Glass

Glass is a Rust browser control tool. It controls Chrome or Chromium through
the Chrome DevTools Protocol (CDP). It does not include a browser runtime.

Glass runs declared browser actions. It does not create an action plan or
change a target without an explicit request.

## Support status

| Item | Status |
|---|---|
| Linux x86-64 | Supported target |
| Linux arm64 | Supported target; use system Chromium or `--chrome-path` |
| macOS x86-64 | Supported target |
| macOS arm64 | Supported target |
| Windows | Unsupported |
| 0.2.0 | Current release |
| Chrome and Chromium | Supported browser families |
| Firefox, WebKit, and Safari | Unsupported browser families |

The command-line interface (CLI), terminal user interface (TUI), Model
Context Protocol (MCP) server, and Rust library use the same session runtime.

## Install the local checkout

Prerequisite: install stable Rust and a supported Chrome or Chromium browser.

Run:

```console
cargo install --path . --locked
glass --help
```

The command builds and installs the local `glass` executable.

The latest published package can be installed with:

```console
cargo install glass-browser --locked
```

Use `glass install-chromium` when no supported system browser is available.

Read [Installation and operations](docs/installation.md) for browser
discovery, profiles, attach mode, logging, policy, and deployment.

## Run Glass

Start the TUI for a visible session:

```console
glass tui
```

Enter these commands in the TUI:

```text
navigate https://example.com
observe
```

Run one operation from the CLI:

```console
glass navigate https://example.com
glass --incognito --headed navigate https://example.com
```

Use `--profile NAME` for persistent cookies and storage. Use `--incognito`
for a disposable browser profile.

## Interfaces

| Interface | Use | Entry point |
|---|---|---|
| CLI | Scripts and one operation | `glass <command>` |
| TUI | An interactive session | `glass tui` |
| MCP | A long-lived stdio connection | `glass --mcp` |
| Rust library | An embedded session runtime | crate `glass-browser`, import `glass` |

Example MCP configuration:

```json
{
  "mcpServers": {
    "glass": {
      "command": "/absolute/path/to/glass",
      "args": ["--mcp"]
    }
  }
}
```

Use an absolute path when the MCP client does not inherit your shell path.
Keep stdout reserved for MCP frames. Read the [MCP guide](docs/mcp.md) for
frame limits, tools, lifecycle, and security.

## Main capabilities

Glass provides these browser operations:

- navigate, click, double-click, hover, drag, type, clear, check, uncheck, and
  select;
- scroll, wait, inspect text, inspect the DOM, evaluate JavaScript, and capture
  screenshots or PDFs;
- upload files, handle JavaScript dialogs, and dismiss recognized consent
  controls;
- select page targets and frames;
- inspect and export cookies and web storage;
- run bounded batches and workflows; and
- use revision guards and bounded verification evidence.

Semantic observations provide bounded page and region data. Start with:

```console
glass observe --level summary
glass observe --level interactive
```

Read the [semantic observation guide](docs/semantic-observation.md) before
using observation references for actions.

Intent resolution compares semantic candidates before an action. Use:

```console
glass resolve-intent request.json
glass execute-intent execution.json
```

Read the [intent resolution guide](docs/intent-resolution.md) for evidence,
ambiguity, and revision rules.

Glass also includes workflow authoring and reliability checks. Read [Workflow
authoring](docs/workflow-authoring.md) and [Reliability laboratory](docs/reliability.md).

## Targets and revisions

An observation returns bounded accessibility data and revisioned target
references such as `r7:b42`. You can also use explicit locators:

```console
glass click 'name=Save'
glass click 'role=button;name=Save'
glass click 'css=button.primary'
```

Guard an action with the revision from the observation:

```console
glass click r7:b42 --expected-revision 7
glass type 'hello' --target r7:b43 --expected-revision 7
```

Glass rejects a stale revision before it sends the browser action. Read the
[action guide](docs/actions.md) for result fields and recovery.

## Rust library and clients

After publication, add the crate as `glass`:

```toml
[dependencies]
glass = { package = "glass-browser", version = "0.2" }
```

The library provides `BrowserSession`, session options, policies, observations,
revision-safe actions, target and frame management, storage, downloads,
screenshots, PDFs, semantic observations, and the MCP server.

The optional [TypeScript client](clients/typescript) and [Python
client](clients/python) start a local Glass binary. They do not include a
browser runtime.

## Safety and scope

Glass requires a local Chrome or Chromium process and a local CDP connection.
It does not provide a hosted browser service or Windows support.

Use a dedicated browser profile. Keep the CDP endpoint local. Treat profiles,
screenshots, DOM output, cookies, and logs as sensitive. Read the
[security policy](SECURITY.md) before using authenticated sessions or attach
mode.

## Documentation

Start with the [documentation index](docs/INDEX.md). It links to installation,
CLI, actions, workflows, MCP, daemon mode, policy, security, architecture,
and release guides.

## Development

Run the checks before you submit a change:

```console
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Run the browser smoke test only when a supported browser is available:

```console
GLASS_E2E=1 cargo test --test browser_smoke -- --nocapture
```

## License

Glass is licensed under the [MIT License](LICENSE).
