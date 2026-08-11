# Platform support and target certification

The release path publishes the crates.io package and a source-only GitHub
Release containing generated notes. It does not build or publish native release
binaries. Cross-compilation or source-level CI proves that code can build; it
does not prove browser launch, terminal startup, Unix-socket behavior,
filesystem permissions, or native sandbox behavior on another OS.

## Declared target inventory

The feature parity file is a declared implementation inventory. It does not
certify these targets by itself:

| Target | Native runner | Rust target | Browser source |
|---|---|---|---|
| Linux x86-64 | `ubuntu-latest` | `x86_64-unknown-linux-gnu` | Managed Chrome for Testing |
| Linux arm64 | `ubuntu-24.04-arm` | `aarch64-unknown-linux-gnu` | Installed system Chromium |
| macOS x86-64 | `macos-15-intel` | `x86_64-apple-darwin` | Managed Chrome for Testing |
| macOS arm64 | `macos-14` | `aarch64-apple-darwin` | Managed Chrome for Testing |
| Windows x86-64 | `windows-latest` | `x86_64-pc-windows-msvc` | Browser-free daemon contract |

Linux ARM64 may also be reproduced on the project’s native OCI ARM64
environment when the hosted ARM64 runner is unavailable. That evidence is a
separate Linux ARM64 row. It must not replace the Linux x86-64 or macOS rows,
and it must record the installed Chromium package and version.

## Recorded Linux ARM64 evidence

A native Linux ARM64 check has been recorded for one validation environment.
Its scope is the checked-out source, the matching Rust target, and the
installed Chromium runtime. Other target rows require their own native checks.

Record the target environment, Rust target, browser version, and commands when
refreshing the evidence. See [Release evidence](release-evidence.md) for the
crates.io package and GitHub Release boundaries.

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
emulated browser run alone. A Linux ARM64 check is not a cross-platform release
claim.

## Evidence required for certification

Each target row must record the source commit, native OS and architecture, Rust
toolchain and target, browser name/version/source, terminal environment, and
the exact commands with their exit status. Browser certification additionally
requires launch/close, structured observation, revision-guarded input, profile
persistence, and the opt-in smoke fixture. Development Runtime certification
requires native PTY start/read/stop, process-tree cleanup, atomic editing,
filesystem containment, and responsive TUI startup. Windows durable-workspace
certification additionally requires the named-pipe start/status/stop,
workspace/tool/reconnect, endpoint-rejection, and cleanup integration test;
Unix socket evidence cannot substitute for it.

| Result | Allowed claim |
|---|---|
| Cross-compile succeeds | source compiles for the target |
| Browser-free tests pass natively | non-browser contracts pass on that host |
| Smoke fixture passes with recorded browser | bounded browser path passes in that environment |
| All native release gates pass | target may be marked `certified` for that exact commit |

A later dependency, browser, runner-image, or platform change invalidates the
affected evidence until it is refreshed. Store the bounded record in
[release evidence](release-evidence.md); do not infer current support from a
historical green workflow badge.
