# Installation and operations

## Requirements

Glass requires:

- stable Rust for a source build;
- Chrome, Chromium, or Chrome for Testing; and
- Linux x86-64, Linux arm64, macOS x86-64, or macOS arm64.

Windows is not a supported target. Glass does not install Playwright or another
browser runtime.

## Install from source

Build the release executable:

`console
cargo build --release --locked
`

The executable is `target/release/glass`.

Install the local checkout:

`console
cargo install --path . --locked
`

Install the published crate after a release:

`console
cargo install glass-browser --locked
`

## Select a browser

Glass checks these locations in this order:

1. the path from `--chrome-path PATH` or `--chrome`;
2. the Chrome for Testing installation from `glass install-chromium`; and
3. a detected system Chrome or Chromium installation.

Use an explicit path when the browser is in a non-standard location:

`console
glass --chrome-path /opt/chrome/chrome navigate https://example.com
`

Install the Glass-pinned Chrome for Testing build:

`console
glass install-chromium
glass install-chromium --update
`

Chrome for Testing does not currently provide a Linux ARM64 archive. On Linux
ARM64, install a system Chromium build or provide an explicit executable with
`--chrome-path`. `glass install-chromium` reports this limitation and does not
create a partial installation.

The installer checks the archive size and digest. It extracts the archive in
the Glass process. It publishes the browser only after validation. It does not
require the `unzip` program.

## Start a session

By default, Glass starts headless Chrome on CDP port `9222` with the profile
`default`.

Use these options:

| Option | Result |
|---|---|
| `--profile NAME` | Use persistent cookies and browser storage. |
| `--incognito` | Use a disposable profile and remove it when Glass exits. |
| `--headed` | Show the browser window. |
| `--port PORT` | Use another local CDP port. |

Profile names may contain ASCII letters, digits, `_`, and `-`.

List and manage profiles:

`console
glass profiles
glass profiles create work
glass profiles delete work
`

Profile deletion removes the browser data. Do not store other files in a
Glass-managed profile directory.

## Attach to a browser

Glass does not take over an occupied CDP port without an explicit option.

Start Chrome with remote debugging. Then attach:

`console
glass --attach --port 9222 observe
`

If the endpoint has more than one page target, select one:

`console
glass --attach --port 9222 --target-id TARGET_ID observe
`

Attach mode uses the settings of the existing Chrome process. It rejects
`--incognito`, `--headed`, `--chrome-path`, and a non-default named profile.

## Logging

Glass writes diagnostics with `tracing`. Set `RUST_LOG` to change the level:

`console
RUST_LOG=glass=info glass observe
RUST_LOG=glass=debug glass --headed navigate https://example.com
`

CLI results use stdout. Diagnostics use stderr. MCP clients must keep stdout
reserved for protocol messages.

## Use a safety policy

The default `development` policy supports local work. For untrusted input, use
a disposable session and an exact host allowlist:

`console
glass --policy hardened --incognito \
  --policy-allow-host example.com \
  navigate https://example.com
`

Repeat `--policy-allow-host` or `--policy-deny-host` for exact host names or
public IPv4 literals. Glass rejects wildcards, overlapping rules, private
addresses, reserved addresses, and hardened startup without an allow host.

Hardened owned sessions pin allowed host names in Chrome. Hardened attach mode
requires the `attach` capability and IP-literal host rules.

These capabilities require policy decisions:

- `attach`;
- `persistent-profile`;
- `evaluate`;
- `upload`;
- `download`;
- `screenshot`; and
- `raw-cdp`.

Request one confirmed screenshot:

`console
glass --policy hardened --incognito \
  --policy-allow-host example.com \
  --policy-confirm screenshot --policy-confirm-once screenshot \
  screenshot --output evidence.png
`

Policy flags have the same meaning in CLI, MCP, and TUI sessions. Glass
rejects invalid combinations before it starts Chrome. `raw-cdp` supports an
explicit allow decision. It does not support a confirmation token.

## Deploy Glass

For a deployment:

1. Build on the target platform with `cargo build --release --locked`.
2. Keep Chrome or Chromium current.
3. Keep the CDP endpoint on a trusted local interface.
4. Use a dedicated operating-system account and browser profile.
5. Use `--incognito` when the job does not need persistent login state.
6. Treat screenshots, DOM output, logs, and profiles as sensitive.
7. Stop Glass cleanly so it can close owned Chrome and remove disposable data.

Read [the security policy](../SECURITY.md) for the trust model.
