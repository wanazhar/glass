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

Glass `0.1.14` is available on crates.io. The supported release targets are
Linux x86-64 and macOS x86-64/arm64. The CLI, terminal UI, MCP server, and Rust
library share the same browser session runtime.

## Requirements

- Linux x86-64 or macOS x86-64/arm64 for the published release artifacts;
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

For a machine without a Rust toolchain, download a stripped release binary and
verify the published checksum. Set `GLASS_VERSION` to the release tag you want:

```console
GLASS_VERSION=v0.1.14
curl -fL "https://github.com/wanazhar/glass/releases/download/${GLASS_VERSION}/glass-linux-x86_64" -o glass
curl -fL "https://github.com/wanazhar/glass/releases/download/${GLASS_VERSION}/glass-linux-x86_64.sha256" -o glass.sha256
sha256sum -c glass.sha256
chmod +x glass
./glass --help
```

On macOS, use `glass-macos-x86_64` for Intel or `glass-macos-aarch64` for
Apple Silicon. The same release assets and checksum convention apply.

The published `glass-browser` crate builds the `glass` binary locally on your
platform:

```console
cargo install glass-browser --locked
```

To embed the Rust library, keep the familiar `glass` import name while using
the published package:

```toml
[dependencies]
glass = { package = "glass-browser", version = "0.1" }
```

If Chrome is not already installed, Glass can download Chrome for Testing:

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
glass "fill name=John email=john@example.com and click Submit"
```

Glass launches headless Chrome by default. Use `--headed` to show the browser,
`--incognito` for a disposable session, or `--profile NAME` for persistent
cookies and storage.

```console
glass --headed --profile demo navigate https://example.com
glass --incognito observe
```

### Key features

- **Fill forms**: `glass "fill username=admin password=secret"` — resolves
  fields by accessible name and applies the appropriate action (type, check,
  select, click).
- **Cookies & storage**: `glass cookies` reads browser cookies;
  `glass set-cookie name=value domain=example.com` sets them.
  `glass localStorage` and `glass sessionStorage` inspect DOM storage.
- **PDF generation**: `glass pdf --output page.pdf` renders the current page
  via CDP `Page.printToPDF` with configurable page size and margins.
- **Clipboard access**: `glass clipboard read` and `glass clipboard write "text"`
  interact with the system clipboard through the browser.
- **WebAuthn**: `glass webauthn --protocol ctap2` enables a virtual authenticator
  for testing passkey and WebAuthn flows.
- **Geolocation & timezone**: `glass geolocation set 40.7128 -74.0060` overrides
  position; `glass timezone America/New_York` sets the timezone for testing
  locale-dependent pages.
- **Retry**: `glass click --retry 3 "button"` retries on transport/CDP errors
  while failing immediately on ambiguous targets.
- **Network recording**: `glass network start` / `glass network stop` capture
  HAR-like request/response entries.
- **Request interception**: `glass intercept "*.google-analytics.com/*"` enables
  CDP Fetch domain interception with scoped guards.
- **Batch operations**: execute up to 32 ordered actions in a single policy
  pre-flight, with typed outcomes per step.
- **Session checkpoints**: `glass checkpoint export` / `glass checkpoint import`
  serialize session state for cross-process handoff.
- **Screenshots**: element, full-page, clip-region, and HiDPI captures via
  `glass screenshot` with configurable format, quality, and scale.
- **Drag & drop**: `glass drag source destination` for drag-based interactions.
- **File upload**: `glass upload target file1.pdf file2.pdf` sets files on a file
  input element.
- **Dialogs**: `glass accept-dialog` and `glass dismiss-dialog` handle JavaScript
  alerts, confirms, and prompts.
- **Consent walls**: `glass dismiss-consent` handles visible OneTrust/Cookiebot
  controls with typed outcomes; it never clicks generic page text.
- **Frame & target management**: `glass frames` lists frames; `glass select-frame`
  switches context. `glass targets` lists page targets with explicit select/close.
- **Popup witness**: `glass click-expect-popup target` verifies causal popup
  opening from a click.
- **Parallel targets**: `BrowserSession::with_targets` opens multiple page
  targets for concurrent operations, with automatic cleanup and context
  restoration.
- **Accessibility diff**: `diff_accessibility` computes structured diffs
  between two accessibility snapshots for UI transition verification.

See the [CLI reference](docs/cli.md) for all commands and session options.

Thin clients are available in [`clients/typescript`](clients/typescript) and
[`clients/python`](clients/python). Both launch an absolute Glass binary over
MCP without a Playwright dependency.

## Using an Existing Login

Glass supports persistent browser profiles so agents can reuse authenticated
sessions without pasting credentials into the model. See the
[profile ergonomics guide](docs/profile-ergonomics.md) for lifecycle details.

### Cookbook: Start from an existing login

1. **Create a named profile and log in once** (use `--headed` to interact
   with the browser):

   ```console
   glass --headed --profile work navigate https://app.example.com/login
   ```

   Complete the login flow manually, then close the browser. Cookies, local
   storage, and session state persist in the profile.

2. **Reuse the authenticated session** in any subsequent run:

   ```console
   glass --profile work "navigate to https://app.example.com/dashboard"
   glass --profile work observe
   ```

   To move a reviewed cookie set between profile runs, use bounded JSON files:

   ```console
   glass --profile work export-cookies ./work-cookies.json
   glass --profile work import-cookies ./work-cookies.json
   ```

   Treat the file as a secret and keep it out of source control.

### Inspect and manage cookies at runtime

Glass exposes three MCP tools for cookie inspection within a profile session:

| Tool | Description |
|------|-------------|
| `cookies` | Read all browser cookies for the current page |
| `setCookies` | Set browser cookies (requires `name`, `value`, `domain`) |
| `clearCookies` | Clear all browser cookies |

Cookie operations are policy-gated: they require the `PersistentProfile`
capability. In `hardened` policy mode, profiles are denied by default; use
`--policy-allow persistent-profile` to enable them.

### Inspect DOM storage

| Tool | Description |
|------|-------------|
| `localStorage` | Read localStorage items (bounded to 64 entries, 1 KiB per value) |
| `sessionStorage` | Read sessionStorage items (same bounds) |

All storage operations use the active profile's origin-scoped data.

### Example MCP workflow

```
# Agent initializes with a named profile, navigates, and reads session state:
1. tools/call: navigate { url: "https://app.example.com" }
2. tools/call: cookies
3. tools/call: localStorage
# ... perform authenticated actions ...
4. tools/call: setCookies { cookies: [{ name: "pref", value: "dark", domain: "app.example.com" }] }
# On teardown:
5. tools/call: clearCookies
```

Profiles, cookies, and storage are covered in depth in the
[profile ergonomics](docs/profile-ergonomics.md) and
[MCP integration](docs/mcp.md) guides.

## MCP

Start the MCP server over stdio:

```console
glass --mcp
```

An MCP client should launch Glass as a local process and communicate over its
standard input and output. Do not wrap this command in a shell that prints
startup text to stdout. See the [MCP guide](docs/mcp.md) for configuration,
available tools, and security considerations.

### MCP Client Recipes

Add Glass to your MCP client in one copy-paste step.

**Claude Desktop** (`~/Library/Application Support/Claude/claude_desktop_config.json`
on macOS or `~/.config/Claude/claude_desktop_config.json` on Linux):

```json
{
  "mcpServers": {
    "glass": {
      "command": "glass",
      "args": ["--mcp", "--policy", "hardened"]
    }
  }
}
```

**Cursor** (`.cursor/mcp.json` in your project root, or
`~/.cursor/mcp.json` for all projects):

```json
{
  "mcpServers": {
    "glass": {
      "command": "glass",
      "args": ["--mcp", "--policy", "dev"]
    }
  }
}
```

**Generic MCP host** (any client that speaks JSON-RPC over stdio):

```json
{
  "mcpServers": {
    "glass": {
      "command": "glass",
      "args": ["--mcp"]
    }
  }
}
```

If the client does not inherit your `PATH`, replace `"glass"` with the absolute
path to the downloaded or installed binary. The process must be launched
directly; stdout is reserved for MCP frames.

> **Important:** Glass writes to stderr only; stdout is reserved for MCP
> messages. Do not add shell wrappers that print to stdout. Use absolute
> paths if `glass` is not on the client's `PATH`. The `--policy hardened`
> preset denies `evaluate` and network interception; `--policy dev` allows
> everything. See `glass --help` for policy presets.

## Scoreboard

Glass publishes and maintains a public scoreboard of category-defining metrics.
These numbers are reproducible with the same fixtures and Chrome build.

| Metric | Current value | Comparator |
|--------|--------------|------------|
| Runner RSS (peak) | ~8.9 MB | Playwright MCP ~196 MB |
| Tests passing | 227 | — |
| MCP tools | 59 | Chrome DevTools MCP ~33 |
| Wrong actions | 0 (adversarial suite) | Playwright: 0 |
| Compact observe bytes | Published as median/p95 in each release acceptance artifact | Playwright MCP and agent-browser when measurable |
| Session modules | 24 | — |

See the [category metric guide](docs/category-metric.md) for methodology and
reproduction steps. See the [benchmarks README](benchmarks/README.md) for
competitive acceptance results.

The reproducible [MCP registry metadata](docs/mcp-registry.json) is available
for directory submissions, and the [30-second demo recipe](docs/demo.md)
shows the navigate → compact observe → revisioned click flow.

## MCP Schema Budget

Glass maintains a bounded, audited MCP tool surface. With 59 tools using compact
JSON Schema definitions, the full `tools/list` response is about 13.6 KiB —
well under the Chrome DevTools MCP ~18k-token class.

- **Stable verbs, not tool sprawl.** One fixed action set with typed parameters.
- **Locator forms are a single parameter.** `target` accepts all locator forms
  (ref, name, role+name, text, CSS, ordinal).
- **Heavy payloads are opt-in.** Screenshots and DOM require explicit flags.
- **All arrays have documented caps.** `batch.steps` ≤ 32, `fillForm.fields` ≤ 16.

See the [MCP schema budget](docs/mcp-schema-budget.md) for the full tool
inventory, design principles, and rejection criteria.

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
- [MCP schema budget](docs/mcp-schema-budget.md) — 59 tools, ~13.6 KiB
- [Profile ergonomics](docs/profile-ergonomics.md) — logged-in session cookbook
- [Bot protection & consent walls](docs/bot-protection.md) — honest access guide
- [Category metric & scoreboard](docs/category-metric.md)
- [Positioning & comparison](docs/positioning.md)
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
