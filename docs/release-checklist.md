# Release checklist

Use this checklist for each public release.

## Release status

The current published release is `glass-browser` version `0.2.0`. This
checklist is being used for the next release, `0.2.1`. The target platforms
are Linux x86-64, Linux arm64, macOS x86-64, and macOS arm64.
Windows is unsupported.

The `0.2.0` GitHub release, tag, crates.io package, and binary artifacts are
published. Do not describe a `0.2.1` GitHub release, tag, crates.io package,
npm package, or binary artifact as published until the corresponding operation
succeeds.

## Prepare

- [ ] Confirm the version and release date.
- [ ] Check the package name, description, license, README, and repository
  metadata in `Cargo.toml`.
- [x] Record the 0.2.0 release date in the changelog.
- [ ] Record the 0.2.1 release date in the changelog.
- [x] Check the README installation commands against `glass --help`.
- [ ] Review dependency and browser-facing security changes.
- [ ] Check that the working tree has no profiles, screenshots, logs, or other
  generated data.

## Validate the checkout

Run:

```console
cargo fmt --all -- --check
python3 scripts/check-version-sync.py
cargo test --all --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --locked --no-deps
cargo build --release --locked
cargo package --locked --no-verify
cargo deny check
cargo audit
cargo check --manifest-path fuzz/Cargo.toml --bins
GLASS_PREVIOUS_VERSION=0.1.18 scripts/smoke-clean-install.sh
```

Then complete these release checks:

- [ ] Run the real-browser matrix on Linux x86-64/arm64 and macOS
      x86-64/arm64. Linux ARM64 uses an installed system Chromium binary.
- [ ] Run `--help`, navigation, observation, screenshot, TUI startup, and
      MCP initialization with each exact packaged release binary.
- [ ] Compare packaged-artifact CLI, capability, and MCP contract evidence
      across all four release targets.
- [ ] Record native runner, browser, source revision, artifact hash, and raw
      smoke evidence for every target.
- [ ] Bind reliability-suite and native-sandbox evidence to the exact packaged
      artifact before merging their scorecards.
- [ ] Certify the knowledge-store boundary against each packaged target and
      merge the four-target migration matrix.
- [ ] Run TypeScript, Python, npm launcher, and isolated Cargo install/upgrade
      checks against every target artifact.
- [ ] Merge machine-readable client compatibility evidence for all four exact
      target artifacts.
- [ ] Inspect `cargo package --list` and the unpacked package.
- [ ] Download each artifact and verify `SHA256SUMS` with
      `sha256sum -c`.
- [ ] Verify downloaded artifact bytes against the machine-readable contract
      evidence before signing the checksum manifest.
- [ ] Verify the Sigstore bundle with `cosign verify-blob`.
- [ ] Review dependency, license, and vulnerability JSON reports.
- [ ] Run a clean-machine install and upgrade test.

The isolated install command above has passed on the current Linux ARM64 host.
Repeat it on each release runner before checking this release gate.

## Publish

- [ ] Commit the version and changelog update.
- [ ] Create a signed annotated tag such as `v0.2.1`.
- [ ] Publish artifacts only from the tagged commit.
- [ ] From the tagged commit, publish the crate manually in a terminal with
      `cargo publish --locked` after the package checks pass.
- [ ] Include supported platforms, checksums, changelog entries, and known
      limitations in the release entry.
- [ ] Verify installation from each published artifact in a clean environment.
- [ ] Publish the crate and client packages only after artifact checks pass.
- [ ] Restore an empty `Unreleased` changelog section.

A release is not complete while any required checkbox is open.
