# Platform support and local certification

The release path is crates-only. It does not build or publish native release
binaries. Cross-compilation or source-level CI proves that code can build; it
does not prove browser launch, terminal startup, Unix-socket behavior,
filesystem permissions, or native sandbox behavior on another OS.

## Declared target inventory

The feature parity file is a declared implementation inventory, not a claim
that these targets have been verified on the current machine:

| Target | Native runner | Rust target | Browser source |
|---|---|---|---|
| Linux x86-64 | `ubuntu-latest` | `x86_64-unknown-linux-gnu` | Managed Chrome for Testing |
| Linux arm64 | `ubuntu-24.04-arm` | `aarch64-unknown-linux-gnu` | Installed system Chromium |
| macOS x86-64 | `macos-15-intel` | `x86_64-apple-darwin` | Managed Chrome for Testing |
| macOS arm64 | `macos-14` | `aarch64-apple-darwin` | Managed Chrome for Testing |

Linux ARM64 may also be reproduced on the project’s native OCI ARM64
environment when the hosted ARM64 runner is unavailable. That evidence is a
separate Linux ARM64 row. It must not replace the Linux x86-64 or macOS rows,
and it must record the installed Chromium package and version.

## Current local evidence

The current development machine is Linux ARM64. Its local support claim is
limited to the checked-out source, its Rust target, and the installed Chromium
runtime. The other rows above remain unverified here.

Record the host, Rust target, browser version, and commands when refreshing
the local evidence. See [Release evidence](release-evidence.md) for the
crates-only publication boundary.

## Local reproduction

Run the platform-independent checks locally:

```console
cargo fmt --all -- --check
cargo test --all-targets --locked
python3 scripts/check-feature-parity.py
```

Run the native browser row only on a machine with the matching target and a
supported Chrome or Chromium installation:

```console
GLASS_E2E=1 cargo test --test browser_smoke --locked -- --nocapture --test-threads=1
```

Do not mark another target `certified` from a cross-compiled binary or
emulated browser run alone. The current local result is a Linux ARM64 result,
not a cross-platform release claim.
