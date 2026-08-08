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

The command installs the full `glass` environment from `glass-dev`. For the
independent browser control plane, install `glass-browser`; it owns the
non-conflicting `glass-browser` executable and the `glass_browser` Rust
library.

The latest published package can be installed with:

```console
cargo install glass-dev --locked
# or, for browser automation without the development workspace:
cargo install glass-browser --locked
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

For an iPhone or another narrow SSH terminal, Glass automatically selects a
single-pane phone workspace. Force it when terminal dimensions are misleading:

```console
glass --tui-layout mobile
```

Use `1` through `5` or `Tab` to switch Home, Agent, App, Diff, and More views.
Home is an attention inbox rather than a raw log. Enter `tap` to select one of
the current revision-bound semantic actions by number, `verify card` for a
compact evidence summary, `notify on` for opt-in terminal alerts, and
`capsule save|show|clear` to manage restart continuity. Clean TUI exits save a
non-sensitive capsule automatically.
The mobile workspace is structured-first and leaves continuous pixels off by
default. Add `--tui-live on` (or enter `live on`) for an adaptive terminal-native
view: Glass prefers Herdr-owned graphics, then direct Kitty, and uses true-color
ANSI as the SSH/Mosh-safe fallback. Run Glass inside [Herdr](https://herdr.dev/)
for agent-aware detach/reattach persistence. Enter `safari` for the stable,
full-fidelity private SSH port-forwarding path; Glass never publicly exposes the
dev server or Chrome CDP. See [Mobile and remote development](docs/mobile-remote.md).

`live quality auto` adapts within the bounded data/balanced/smooth profiles
using delivery pressure while the App view is visible.

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
glass project run check --command "cargo check" --wait --root .
glass project diff --root .
glass agent prompt "read README.md" --root .
```

The project runtime keeps files workspace-confined, runs commands in a bounded
PTY, records actor-attributed events, and exposes the same operations through
MCP and the TUI Development workspace. Rust diagnostics use a real bounded
rust-analyzer LSP path; live-update and source/runtime claims remain pending
unless browser revisions or explicit markers provide evidence.

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
| Browser CLI | Browser-only control plane | `glass-browser <command>` |
| Rust library | An embedded session runtime | crate `glass-browser`, import `glass_browser` |

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

The library provides `BrowserSession`, session options, policies, structured
observations, revision-safe actions, stable Web IR, Task Protocol compilation,
workflows, advisory knowledge, project-development contracts, presentation and
backend abstractions, reliability evidence, and the MCP server. Read the
[Rust SDK guide](docs/rust-sdk.md), [feature reference](docs/features.md), and
[runnable example catalog](docs/examples.md); API docs are published from the
`glass-browser` crate with all Cargo features enabled.

The repository-only [TypeScript client](clients/typescript) and [Python
client](clients/python) remain experimental repository clients for the 0.3.2
release line. They are not published as npm or PyPI packages, do not
include a browser runtime, and do not change the primary Cargo installation
path. Both expose typed browser and Development Runtime helpers plus a bounded,
cursor-based project event subscription API.

## Safety and scope

Glass requires a local Chrome or Chromium process. The primary backend uses a
local CDP connection; the bounded WebDriver BiDi backend remains experimental.
Glass does not provide a hosted browser service or Windows support.

Use a dedicated browser profile. Keep the CDP endpoint local. Treat profiles,
screenshots, DOM output, cookies, and logs as sensitive. Read the
[security policy](SECURITY.md) before using authenticated sessions or attach
mode.

## Documentation

Start with the role-based [documentation index](docs/INDEX.md), then use
[Getting started](docs/getting-started.md) for the first safe session. The
[feature reference](docs/features.md) maps every capability to CLI, MCP, TUI,
Rust, TypeScript, and Python surfaces; the [CLI reference](docs/cli.md) and
[complete MCP catalog](docs/mcp-tools.md) enumerate the command interfaces.
Architecture, operations, policy, security, benchmarking, maintenance, and
release evidence remain indexed from the same page.

## Development

Run the checks before you submit a change:

```console
cargo fmt --all -- --check
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo package --package glass-browser --locked
cargo package --package glass-dev --locked --no-verify --config 'patch.crates-io.glass-browser.path="crates/glass-browser"'
```

Run the browser smoke test only when a supported browser is available:

```console
GLASS_E2E=1 cargo test --test browser_smoke -- --nocapture
```

## License

Glass is licensed under the [MIT License](LICENSE).
