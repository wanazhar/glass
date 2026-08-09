# Installation and operations

## Requirements

Native browser use requires:

- stable Rust for a source build;
- a Linux or macOS environment with a declared Rust target; and
- Chrome, Chromium, or Chrome for Testing.

Validate native browser behavior in the environment where you deploy Glass.

The 0.3.3 source line runs browser-free Windows CI, but native Windows browser,
PTY, and TUI behavior is not certified. Do not infer deployment support from a
successful source build. Glass does not install Playwright or another browser
runtime.

## Install from source

Build both release executables:

```console
cargo build --release --locked
```

The executables are `target/release/glass` and
`target/release/glass-browser`; `glass-dev` packages both entry points.

Install the local checkout:

```console
cargo install --path crates/glass-dev --locked
glass --help
glass-browser --help

# Browser control plane only
cargo install --path crates/glass-browser --locked
glass-browser --help
```

Install the published crate after a release:

```console
cargo install glass-dev --locked
cargo install glass-browser --locked
```

`glass-dev` owns both installed executable names for the complete product;
`glass-browser` independently owns `glass-browser` for core-only installs and
exports the `glass_browser` Rust library. Cargo therefore rejects installing
both packages into the same root. Choose one product, or uninstall the old
owner before switching. `glass-dev` depends one-way on the exact matching
browser crate version. Neither package installs Pi or Node.

Use these explicit ownership transitions; `cargo install --list` shows the
current owner before you change it:

```console
# core only -> full suite
cargo uninstall glass-browser
cargo install glass-dev --locked

# full suite -> newer full suite
cargo install glass-dev --locked --force

# full suite -> core only
cargo uninstall glass-dev
cargo install glass-browser --locked
```

The release smoke additionally exercises Cargo's direct `--force` replacement
in both directions in an isolated root. Uninstall/install is the clearest
interactive route because it leaves no stale package-ownership record.

## Fully uninstall Glass

Stop any running TUI, CLI, MCP clients, and owned browser sessions first. If
the local daemon is running, stop it while the full command is still present:

```console
glass daemon stop
```

Inspect Cargo's package records, then uninstall both possible package owners:

```console
cargo install --list
cargo uninstall glass-dev
cargo uninstall glass-browser
```

Normally only one uninstall succeeds because the packages intentionally
compete for the `glass-browser` executable in a Cargo root. “package ... is not
installed” is therefore expected for the other command. If Glass was installed
with `cargo install --root /exact/root`, repeat both uninstall commands with
the same `--root /exact/root`. Repeat this for every custom root used on the
machine.

Refresh the shell's command cache and verify that neither executable resolves:

```console
hash -r
command -v glass
command -v glass-browser
```

Both `command -v` calls should produce no path. PowerShell users can verify
with `Get-Command glass, glass-browser -ErrorAction SilentlyContinue`.

`cargo uninstall` removes package records and installed executables. It does
not remove Glass profiles, knowledge, snapshots, workspaces, results,
development timelines, daemon state, or a managed Chrome for Testing build.
Those may contain authenticated browser data or project history. Back them up
if needed, inspect each resolved path, and only then remove the complete
`glass` directory at each applicable location:

| State class | Linux default | macOS default | Windows default |
|---|---|---|---|
| Configuration and persistent browser profiles | `${XDG_CONFIG_HOME:-$HOME/.config}/glass` | `$HOME/Library/Application Support/glass` | `%APPDATA%\glass` |
| Local data, daemon state, timelines, managed Chromium | `${XDG_DATA_HOME:-$HOME/.local/share}/glass` | `$HOME/Library/Application Support/glass` | `%LOCALAPPDATA%\glass` |
| Diagnostic/result cache | `${XDG_CACHE_HOME:-$HOME/.cache}/glass` | `$HOME/Library/Caches/glass` | `%LOCALAPPDATA%\glass` |
| Abandoned disposable profiles and launch locks | `${TMPDIR:-/tmp}/glass` | `$TMPDIR/glass` | `%TEMP%\glass` |

`GLASS_CONFIG_HOME` changes the first row to
`$GLASS_CONFIG_HOME/glass`. Platform directories can overlap; remove a
resolved path once. Example Linux purge after inspection:

```console
rm -rf -- "${GLASS_CONFIG_HOME:-${XDG_CONFIG_HOME:-$HOME/.config}}/glass"
rm -rf -- "${XDG_DATA_HOME:-$HOME/.local/share}/glass"
rm -rf -- "${XDG_CACHE_HOME:-$HOME/.cache}/glass"
rm -rf -- "${TMPDIR:-/tmp}/glass"
```

Equivalent macOS purge:

```console
rm -rf -- "${GLASS_CONFIG_HOME:-$HOME/Library/Application Support}/glass"
rm -rf -- "$HOME/Library/Application Support/glass"
rm -rf -- "$HOME/Library/Caches/glass"
rm -rf -- "${TMPDIR:-/tmp}/glass"
```

Equivalent PowerShell purge on Windows:

```powershell
$glassPaths = @(
    if ($env:GLASS_CONFIG_HOME) {
        Join-Path $env:GLASS_CONFIG_HOME "glass"
    } else {
        Join-Path $env:APPDATA "glass"
    }
    Join-Path $env:LOCALAPPDATA "glass"
    Join-Path $env:TEMP "glass"
) | Select-Object -Unique

$glassPaths | ForEach-Object {
    if (Test-Path -LiteralPath $_) {
        Remove-Item -LiteralPath $_ -Recurse -Force
    }
}
```

Also remove Glass MCP entries from Claude Code, Codex, or other clients that
were created from `glass mcp-config`; uninstalling a binary cannot edit those
external client configurations. Experiments may have Git worktrees under a
sibling `.glass-worktrees/REPOSITORY` directory. Inspect them with
`git worktree list` and remove each unwanted worktree with
`git worktree remove /exact/worktree` before deleting the empty directory.
Glass does not automatically remove experiment branches.

Do not delete the shared Cargo registry/cache, a system Chrome installation,
source checkouts, `glass.toml`, or `.glass.toml`: Cargo and other projects may
own them. Removing the local-data directory above does remove Chromium that was
installed specifically by `glass install-chromium`.

## Diagnose an installation

Run the browser-free diagnostic before starting an agent session:

```console
glass doctor
glass doctor --json
```

The JSON report includes stable finding codes, executable and browser paths,
platform, policy and capability status, profile/store visibility, daemon
health, and actionable remediation. It does not mutate a real browser profile
or launch a browser smoke test.

Generate deterministic MCP configuration using the installed executable path:

```console
glass mcp-config --client generic
glass mcp-config --client claude-code
glass mcp-config --client codex
glass mcp-config --print
```

Large diagnostic evidence is stored locally and referenced by result ID:

```console
glass result show RESULT_ID
glass result show RESULT_ID --section trace
glass result purge --older-than 7d
```

## Select a browser

Glass checks these locations in this order:

1. the path from `--chrome-path PATH` or `--chrome`;
2. the Chrome for Testing installation from `glass install-chromium`; and
3. a detected system Chrome or Chromium installation.

Use an explicit path when the browser is in a non-standard location:

```console
glass --chrome-path /opt/chrome/chrome navigate https://example.com
```

Install the Glass-pinned Chrome for Testing build:

```console
glass install-chromium
glass install-chromium --update
```

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

```console
glass profiles
glass profiles create work
glass profiles delete work
```

Profile deletion removes the browser data. Do not store other files in a
Glass-managed profile directory.

## Attach to a browser

Glass does not take over an occupied CDP port without an explicit option.

Start Chrome with remote debugging. Then attach:

```console
glass --attach --port 9222 observe
```

If the endpoint has more than one page target, select one:

```console
glass --attach --port 9222 --target-id TARGET_ID observe
```

Attach mode uses the settings of the existing Chrome process. It rejects
`--incognito`, `--headed`, `--chrome-path`, and a non-default named profile.

## Logging

Glass writes diagnostics with `tracing`. Set `RUST_LOG` to change the level:

```console
RUST_LOG=glass=info glass observe
RUST_LOG=glass=debug glass --headed navigate https://example.com
```

CLI results use stdout. Diagnostics use stderr. MCP clients must keep stdout
reserved for protocol messages.

## Use a safety policy

The default `development` policy supports local work. For untrusted input, use
a disposable session and an exact host allowlist:

```console
glass --policy hardened --incognito \
  --policy-allow-host example.com \
  navigate https://example.com
```

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
- `raw-cdp`;
- `read-form-values`;
- `read-sensitive-form-values`;
- `read-sensitive-extraction`;
- `coordinate-click`;
- `consent-dismissal`; and
- `declared-agent-identity`.

Use `glass capabilities` for the installed version's status and constraints.
Allow only the capability needed by the operation. Reading form values,
sensitive fields, cookies/storage, or extracted records can expose secrets;
coordinate clicks have no semantic target; consent dismissal supports only
recognized consent UX and is not an anti-bot bypass. Declared agent identity
records identity metadata but does not expand browser authority.

Request one confirmed screenshot:

```console
glass --policy hardened --incognito \
  --policy-allow-host example.com \
  --policy-confirm screenshot --policy-confirm-once screenshot \
  screenshot --output evidence.png
```

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
