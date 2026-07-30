# CI-native platform certification

Glass uses native CI runners for release-target evidence. Cross-compilation
proves that a target builds; it does not prove browser launch, terminal startup,
Unix-socket behavior, filesystem permissions, or native sandbox behavior.

## Release matrix

The release workflow in `.github/workflows/release.yml` is the executable
source for this matrix:

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

## Evidence produced by each runner

Each matrix job builds and tests the target, packages and strips the artifact,
runs browser and TUI smoke against that exact artifact, and then records:

- source commit and workflow run URL;
- target, runner OS, runner architecture, image, and image version;
- browser family and exact browser version;
- artifact name, byte size, and SHA-256;
- browser smoke command and bounded raw report;
- CLI help output;
- capability manifest; and
- complete MCP tool names and input schemas.

Contract collection normalizes the packaged executable basename in CLI help
(`Usage: ...`) to `glass`; target-specific artifact names remain recorded in
the surrounding artifact metadata.

The acceptance job fails closed when a target is missing, duplicated, bound to
another source revision, missing artifact or runner metadata, or differs in
CLI, capability, or MCP contract from the other supported targets.
It also verifies that the browser-smoke and contract reports name the same
artifact hash, size, and filename for every target.
The acceptance gate additionally joins the certified reliability, sandbox, and
knowledge-migration matrices to those exact contract artifact hashes before
the final release job assembles its evidence. Client compatibility results are
joined by the same gate when present.
The final release job verifies those hashes and sizes again against the
downloaded artifact bytes before it creates the checksum manifest.

The release workflow runs TypeScript, Python, and npm launcher compatibility
against the downloaded target artifact on each native runner. It also runs the
isolated Cargo install and upgrade check on each target.

See [Release evidence](release-evidence.md) for the complete report inventory,
artifact-binding rules, and the distinction between static local checks and
native release certification.

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

Do not mark a target `certified` from a cross-compiled binary or emulated
browser run alone. Use the machine-readable platform and artifact contract
reports produced by the release workflow.
