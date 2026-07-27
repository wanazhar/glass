# Installation and operations

## Build and install

Glass uses the Rust 2024 edition. Build with the current stable Rust toolchain
and the checked-in dependency lockfile:

```console
cargo build --release --locked
```

The optimized executable is `target/release/glass` on Linux and macOS. To
install from a checkout:

```console
cargo install --path . --locked
```

Install the published crate and build the `glass` binary locally:

```console
cargo install glass-browser --locked
```

Cargo is the supported installation path for the current release. It builds the
native `glass` executable for Linux x86-64 and macOS x86-64/arm64 without
installing Playwright or another browser runtime.

## Browser selection

Glass resolves the browser in this order:

1. the path passed through `--chrome-path PATH` (or its `--chrome` alias);
2. Chrome for Testing previously downloaded by `glass install-chromium`; then
3. a detected system Chrome or Chromium installation.

Override discovery when deployments use a nonstandard browser location:

```console
glass --chrome-path /opt/chrome/chrome navigate https://example.com
```

`glass install-chromium` installs the Chrome for Testing version pinned by this
Glass release for Linux x86-64 or macOS x86-64/arm64.
Glass streams the bounded archive, checks its pinned size and digest, extracts
it with an in-process ZIP reader, and atomically publishes only a validated
executable. It does not require an external `unzip` program. Reinstall the
release-pinned browser explicitly with `glass install-chromium --update`.
Production systems may instead update a system browser independently.

## Session modes

By default, Glass launches headless Chrome on CDP port `9222` with the named
profile `default`.

- `--profile NAME` persists cookies and browser storage in Glass's local data
  directory. Profile names may contain ASCII letters, digits, `_`, and `-`.
- `--incognito` creates a disposable user-data directory and removes it after
  the owned session closes.
- `--headed` displays the browser window.
- `--port PORT` changes the local CDP port.

List and manage named profiles with:

```console
glass profiles
glass profiles create work
glass profiles delete work
```

Deleting a profile permanently removes its browser data. Do not place files
you want to retain inside a Glass-managed profile directory.

## Attach to an existing browser

Glass will not silently take over a process already listening on the selected
CDP port. Start Chrome with remote debugging enabled, then opt in explicitly:

```console
glass --attach --port 9222 observe
```

When the endpoint exposes multiple page targets, select one explicitly:

```console
glass --attach --port 9222 --target-id TARGET_ID observe
```

Attach mode uses settings owned by the existing Chrome process. It therefore
rejects `--incognito`, `--headed`, `--chrome-path`, and a non-default named
profile.

## Logging

Glass writes structured diagnostics through `tracing`. Set `RUST_LOG` to
control verbosity:

```console
RUST_LOG=glass=info glass observe
RUST_LOG=glass=debug glass --headed navigate https://example.com
```

CLI results use stdout, while diagnostics use stderr. MCP clients must keep
stdout reserved for protocol messages.

## Safety presets

`--policy development` is the default and preserves normal local workflows.
For an untrusted caller, use an owned disposable session with exact host rules:

```console
glass --policy hardened --incognito \
  --policy-allow-host example.com \
  navigate https://example.com
```

Repeat `--policy-allow-host` or `--policy-deny-host` for exact DNS names or
public IPv4 literals. Wildcards, overlapping allow/deny rules, private or
reserved addresses, and hardened startup without an allow host are rejected.
Owned hardened sessions pin allowed names in Chrome to prevent a second DNS
resolution from rebinding to a private address. Hardened attach therefore
requires an explicit `attach` capability and IP-literal host rules.

Privileged capabilities are `attach`, `persistent-profile`, `evaluate`,
`upload`, `download`, `screenshot`, and `raw-cdp`. For example, this allows one
confirmed screenshot and then returns to confirmation-required behavior:

```console
glass --policy hardened --incognito \
  --policy-allow-host example.com \
  --policy-confirm screenshot --policy-confirm-once screenshot \
  screenshot --output evidence.png
```

Policy flags are global and apply identically to one-shot CLI, MCP, and TUI
sessions. Invalid combinations fail before Chrome starts.
`raw-cdp` is an unlimited library escape hatch and therefore supports explicit
allow only, not confirmation tokens.

## Production deployment

- Build with `cargo build --release --locked` on the target platform.
- Keep Chrome/Chromium patched and compatible with CDP.
- Bind the debugging endpoint only to a trusted local interface and do not
  publish its port through a container or firewall.
- Use a dedicated OS account and a dedicated profile for automation.
- Prefer `--incognito` for jobs that do not require persistent login state.
- Treat screenshots, DOM output, logs, and profiles as potentially sensitive.
- Gracefully terminate Glass so it can close Chrome and remove disposable
  profile data.

See [SECURITY.md](../SECURITY.md) for the complete trust model.
