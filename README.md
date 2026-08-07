# Glass

Glass is a local, revision-safe browser intelligence runtime for humans and
agents. It combines semantic memory, multi-surface understanding, verified
workflows, a terminal browser workspace, MCP, CLI, and Rust APIs while keeping
every browser action explicit and bounded.

Glass controls Chrome or Chromium through a transport-neutral backend
contract. CDP is the primary production backend, WebDriver BiDi is an
experimental bounded backend, and unsupported capabilities fail closed. Glass
does not include a browser engine or create an autonomous action plan. Install
the executable with Cargo, then use the safe observe → guarded action → verify
loop.

## Support status

| Item | Status |
|---|---|
| Linux x86-64 | Declared target; native runtime certification pending |
| Linux arm64 | Declared target; native runtime certification pending |
| macOS x86-64 | Declared target; native runtime certification pending |
| macOS arm64 | Declared target; native runtime certification pending |
| Windows | Unsupported |
| 0.3.2 | Current local release candidate |
| 0.3.1 | Previous source release |
| 0.3.0 | Previous published release |
| 0.2.9 | Earlier published release |
| 0.2.8 | Earlier published release |
| Chrome and Chromium | Supported browser families |
| Firefox, WebKit, and Safari | Unsupported browser families |

The command-line interface (CLI), terminal user interface (TUI), Model
Context Protocol (MCP) server, and Rust library use the same session runtime.
The support table describes declared targets and target-specific validation
requirements. Native runtime certification is not implied by a source build.
See the [cross-platform feature parity
matrix](docs/feature-parity.md) for implementation inventory and the [release
evidence guide](docs/release-evidence.md) for the crates.io package and
source-only GitHub Release boundary.

## Install the local checkout

Prerequisite: install stable Rust and a supported Chrome or Chromium browser.

Run:

```console
cargo install --path crates/glass-dev --locked
glass --help
```

The command builds and installs the local `glass` executable from the
`glass-dev` package. The reusable browser library is the separate
`glass-browser` package.

The latest published package can be installed with:

```console
cargo install glass-dev --locked
```

Use `glass install-chromium` when no supported system browser is available.

Read [Installation and operations](docs/installation.md) for browser
discovery, profiles, attach mode, logging, policy, and deployment.
See [Experimental capabilities](docs/experimental-capabilities.md) before
enabling opt-in features.

## Run Glass

Start the interactive terminal workspace (both forms are equivalent):

```console
glass
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

Open the terminal-native development workspace without starting Chrome:

```console
glass project inspect --root .
glass project files --root .
glass project run dev --command "cargo run" --root .
glass project diff --root .
glass agent prompt "read README.md" --root .
```

The project runtime keeps files workspace-confined, runs commands in a bounded
PTY, records actor-attributed events, and exposes the same operations through
MCP and the TUI Development workspace. It does not claim framework-specific
LSP, hot-module replacement, or source maps unless explicit evidence is
available.

Compile a bounded Task Protocol plan against stable Glass Web IR v1 without
starting Chrome:

```console
glass task compile task.json web-ir.json
glass task compile task.json web-ir.json --explain
```

Validate authored Task Protocol JSON without compiling or starting Chrome:

```console
glass task validate task.json
```

The canonical plan remains on stdout; `--explain` writes deterministic,
redacted compilation metadata to stderr.

Inspect or diff validated browser-free Glass Web IR v1 documents:

```console
glass ir validate ir.json
glass ir inspect ir.json
glass ir diff before.json after.json
glass ir diff before.json after.json --summary
glass ir continuity before.json after.json field-1
glass ir canonical ir.json
```

`ir diff` prints detailed local diagnostics by default; `--summary` emits the
bounded canonical diff projection used by the protocol helpers.

Use `--profile NAME` for persistent cookies and storage. Use `--incognito`
for a disposable browser profile.

## Interfaces

| Interface | Use | Entry point |
|---|---|---|
| CLI | Browser and project operations | `glass <command>` |
| TUI | Browser or Development workspace | `glass tui` |
| MCP | A long-lived stdio connection | `glass --mcp` |
| Rust library | An embedded session runtime | crate `glass-browser`, import `glass` |

The bounded extraction contract and stable Glass Web IR v1 data model are
available from the Rust crate root (`ExtractionRequest`, `ExtractionEvidence`,
and `GlassWebIrV1`). `BrowserSession::extract_evidence` and
`BrowserSession::extract_web_ir` acquire fresh, budgeted live-page evidence;
browser-free reconciliation, validation, diff, and continuity remain available
for offline callers.

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
glass = { package = "glass-browser", version = "0.3" }
```

The library provides `BrowserSession`, session options, policies, observations,
revision-safe actions, target and frame management, storage, downloads,
screenshots, PDFs, semantic observations, and the MCP server.

The repository-only [TypeScript client](clients/typescript) and [Python
client](clients/python) remain experimental repository clients for the 0.3.2
release line. They are not published as npm or PyPI packages, do not
include a browser runtime, and do not change the primary Cargo installation
path.

## Safety and scope

Glass requires a local Chrome or Chromium process. The primary backend uses a
local CDP connection; the bounded WebDriver BiDi backend remains experimental.
Glass does not provide a hosted browser service or Windows support.

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
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo package --package glass-browser --locked --no-verify
cargo package --package glass-dev --locked --no-verify
```

Run the browser smoke test only when a supported browser is available:

```console
GLASS_E2E=1 cargo test --test browser_smoke -- --nocapture
```

## License

Glass is licensed under the [MIT License](LICENSE).
