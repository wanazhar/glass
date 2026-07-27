# Glass

Glass is a lightweight browser control tool written in Rust. It drives Chrome
or Chromium directly through the Chrome DevTools Protocol (CDP), without
Playwright, WebDriver, or an embedded browser runtime.

Glass executes explicit browser commands. It does not choose a workflow or
silently retarget an action.

## Status

Glass `0.1.18` is prepared as the next `glass-browser` release. The release
workflow builds Linux x86-64 and macOS x86-64/arm64 binaries. Windows is not a supported
target.

The command-line interface, terminal UI, MCP server, and Rust library share the
same session runtime.

## Install

Install the published command with Cargo (or use the checkout commands below
while the next release is being prepared):

```console
cargo install glass-browser --locked
glass --help
```

Or build from a checkout:

```console
cargo install --path . --locked
```

Prebuilt Linux and macOS binaries will be available from the
[0.1.18 GitHub release](https://github.com/wanazhar/glass/releases/tag/v0.1.18).
The assets include SHA-256 checksum files. For example:

```console
GLASS_VERSION=v0.1.18
curl -fL "https://github.com/wanazhar/glass/releases/download/${GLASS_VERSION}/glass-linux-x86_64" -o glass
curl -fL "https://github.com/wanazhar/glass/releases/download/${GLASS_VERSION}/glass-linux-x86_64.sha256" -o glass.sha256
sha256sum -c glass.sha256
chmod +x glass
```

Use `glass-macos-x86_64` on Intel Macs or `glass-macos-aarch64` on Apple
Silicon. The binary must be able to find Chrome, Chromium, or Chrome for
Testing. Use `glass install-chromium` to install the release-pinned Chrome for
Testing build when a system browser is not available.

See [installation and operations](docs/installation.md) for browser discovery,
profiles, attach mode, logging, policy presets, and deployment guidance.

## First run

The terminal UI keeps one browser session open for a sequence of commands:

```console
glass tui
```

Inside the UI:

```text
navigate https://example.com
observe
```

For a single explicit operation, use a CLI subcommand. Results are JSON on
stdout and diagnostics are written to stderr:

```console
glass navigate https://example.com
glass --incognito --headed navigate https://example.com
```

Use `--profile NAME` for persistent cookies and storage, or `--incognito` for
a disposable browser profile.

## Interfaces

| Surface | Use it for | Entry point |
|---|---|---|
| CLI | Scripts and one-shot commands | `glass <command>` |
| Terminal UI | A visible, interactive session | `glass tui` |
| MCP | A long-lived stdio session from an MCP-compatible client | `glass --mcp` |
| Rust library | Embedding the session runtime | `glass-browser` crate, imported as `glass` |

Configure an MCP client to launch the binary directly:

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

Keep stdout reserved for MCP frames. Use an absolute binary path when the
client does not inherit the shell's `PATH`. The [MCP guide](docs/mcp.md) lists
the tools, framing limits, lifecycle, and security rules.

## Command surface

The main browser operations are available from the CLI and MCP:

- navigate, click, double-click, hover, drag, type, clear, check, uncheck, and
  select;
- scroll, wait, inspect visible text, inspect DOM, evaluate JavaScript, and
  capture screenshots or PDFs;
- upload files, handle JavaScript dialogs, dismiss recognized consent walls,
  and verify a popup opened from a click;
- list and select page targets and frames;
- inspect cookies and web storage;
- export/import cookies and bounded session checkpoints; and
- run bounded typed batches and reconcile revisioned references.

Use `glass --help` for the complete CLI surface. Detailed command syntax is in
the [CLI reference](docs/cli.md), and the corresponding MCP names and
arguments are in the [MCP guide](docs/mcp.md).

## Targets and revisions

Observations publish bounded accessibility data and revisioned element
references such as `r7:b42`. Locators can also use an accessible name, an
explicit role and name, text, CSS, or an ordinal:

```console
glass click 'name=Save'
glass click 'role=button;name=Save'
glass click 'css=button.primary'
```

For an action tied to a specific observation, pass its revision:

```console
glass click r7:b42 --expected-revision 7
glass type 'hello' --target r7:b43 --expected-revision 7
```

The guarded click, type, fill, and navigation APIs return a structured status,
the previous and current revisions, and bounded verification metadata. A
stale revision is rejected before the browser action runs. Legacy commands
without an expected revision remain available. See the [action contract](docs/actions.md)
for result fields, failure kinds, and recovery examples.

## Rust library

Add the published crate as `glass` in `Cargo.toml`:

```toml
[dependencies]
glass = { package = "glass-browser", version = "0.1" }
```

The library exposes `BrowserSession`, typed session options and policies,
compact observations, revision-safe actions, target/frame management, storage,
downloads, screenshots, PDFs, and the MCP server implementation.

Small optional MCP clients are included for [TypeScript](https://github.com/wanazhar/glass/tree/main/clients/typescript)
and [Python](https://github.com/wanazhar/glass/tree/main/clients/python). They launch a local Glass binary and do not
include a browser runtime.

## Scope and safety

Glass currently supports Linux x86-64 and macOS x86-64/arm64. It requires a
local Chrome, Chromium, or Chrome for Testing process and a local CDP
connection. It does not provide a browser runtime, a hosted browser service,
or cross-platform Windows support.

Use a dedicated browser profile, keep the CDP endpoint local, and treat
profiles, screenshots, DOM output, cookies, and logs as sensitive. Review
[SECURITY.md](SECURITY.md) before using authenticated sessions or attach mode.

## Documentation

- [Documentation index](docs/INDEX.md)
- [Installation and operations](docs/installation.md)
- [CLI reference](docs/cli.md)
- [Action contract](docs/actions.md)
- [MCP integration](docs/mcp.md)
- [Profiles and authenticated sessions](docs/profile-ergonomics.md)
- [Policy reference](docs/policy.md)
- [Security policy](SECURITY.md)
- [Architecture](docs/architecture/README.md)
- [Changelog](CHANGELOG.md)
- [Contributing](CONTRIBUTING.md)

## Development

```console
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
```

The opt-in browser smoke test requires a detectable Chrome or Chromium:

```console
GLASS_E2E=1 cargo test --test browser_smoke -- --nocapture
```

## License

Glass is licensed under the [MIT License](LICENSE).
