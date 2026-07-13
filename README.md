# Glass

Glass is a lightweight, local-first browser automation tool written in Rust. It
drives Chrome or Chromium directly through the Chrome DevTools Protocol (CDP),
without Playwright, WebDriver, or an embedded browser runtime.

Glass provides four interfaces over the same browser session:

- a command-line interface for scripts and one-shot actions;
- an interactive terminal UI;
- an MCP stdio server for AI clients; and
- a Rust library for embedding browser control.

## Status

Glass is preparing its `0.1.0` release. Treat the CLI and Rust API as
pre-stable: breaking changes may occur before `1.0.0`.

Glass does not currently claim to be best in class. Its release gate publishes
wrong-action, task-success, resource, and unsupported-adapter evidence; any
failed hard gate blocks that language. See the
[competitive acceptance guide](benchmarks/README.md#competitive-acceptance).

## Requirements

- Linux, macOS, or Windows on a supported Rust target;
- Chrome, Chromium, or Chrome for Testing; and
- Rust stable when building from source.

Glass searches for an explicitly supplied browser first, then its managed
Chrome for Testing installation, and finally a system Chrome/Chromium binary.

## Install

Build the optimized binary from this checkout:

```console
cargo build --release
./target/release/glass --help
```

To install it into Cargo's binary directory:

```console
cargo install --path . --locked
glass --help
```

If Chrome is not already installed, Glass can download Chrome for Testing
(`unzip` must be available):

```console
glass install-chromium
```

For production hosts, prefer an independently updated system Chrome/Chromium.
See the [installation and operations guide](docs/installation.md) for browser
selection, platform notes, profiles, and deployment guidance.

## Quick start

```console
glass navigate https://example.com
glass text
glass observe
glass screenshot --output page.png
```

Run `glass` or `glass tui` for the terminal UI. One-shot prompt aliases are
also supported:

```console
glass "navigate to https://example.com"
```

Glass launches headless Chrome by default. Use `--headed` to show the browser,
`--incognito` for a disposable session, or `--profile NAME` for persistent
cookies and storage.

```console
glass --headed --profile demo navigate https://example.com
glass --incognito observe
```

See the [CLI reference](docs/cli.md) for all commands and session options.

## MCP

Start the MCP server over stdio:

```console
glass --mcp
```

An MCP client should launch Glass as a local process and communicate over its
standard input and output. Do not wrap this command in a shell that prints
startup text to stdout. See the [MCP guide](docs/mcp.md) for configuration,
available tools, and security considerations.

## Behavior and safety

- Compact observations contain the URL, title, bounded visible text, and
  accessible interactive controls. Full DOM and screenshots are opt-in.
- Clicks use bounded, smooth pointer movement by default. Use
  `--interaction fast` for direct pointer events.
- Glass never adopts an occupied CDP port implicitly. Use `--attach` to connect
  to an existing endpoint and `--target-id` when it has multiple page targets.
- CDP grants broad control over a browser. Only attach to endpoints you trust,
  and do not expose the debugging port to untrusted networks.

See [SECURITY.md](SECURITY.md) before using Glass with authenticated sessions
or sensitive data.

## Documentation

- [Documentation index](docs/INDEX.md)
- [Installation and operations](docs/installation.md)
- [CLI reference](docs/cli.md)
- [MCP integration](docs/mcp.md)
- [Architecture](docs/architecture/README.md)
- [Benchmarks](benchmarks/README.md)
- [Contributing](CONTRIBUTING.md)
- [Changelog](CHANGELOG.md)

## Development

```console
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

The opt-in browser smoke test requires a detectable Chrome/Chromium:

```console
GLASS_E2E=1 cargo test --test browser_smoke -- --nocapture
```

## License

Glass is licensed under the [MIT License](LICENSE).
