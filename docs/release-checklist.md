# Release checklist

Use this checklist for each public release.

## Release status

The current published release is `glass-browser` version `0.2.4`. This
checklist is being used for the next crates.io release, `0.2.5`. Linux
x86-64, Linux arm64, macOS x86-64, and macOS arm64 remain declared targets;
only Linux arm64 has been verified on the current machine. Windows is
unsupported.

Versioning and annotated `vX.Y.Z` tags remain part of the process. Starting
with 0.2.1, crates.io is the only publication channel. GitHub release binaries, checksum manifests, Sigstore bundles, and the npm native launcher are not release deliverables.

Pushing a `vX.Y.Z` tag runs `.github/workflows/crates-release.yml`. The action
checks the tag and package version, runs the release validation suite, performs
a crates.io dry run, and publishes only the crate. If that exact crate version
is already on crates.io, the action records the existing publication and skips
a duplicate upload. It does not create a GitHub release or upload native
binary artifacts.

## Prepare

- [x] Confirm development version `0.2.5`; release date remains pending
      publication.
- [x] Check the package name, description, license, README, and repository
      metadata in `Cargo.toml`.
- [x] Record the 0.2.0 release date in the changelog.
- [x] Record the 0.2.1 release date in the changelog.
- [x] Record the 0.2.2 release date in the changelog.
- [x] Record the 0.2.3 release date in the changelog.
- [x] Check the README installation commands against `glass --help`.
- [x] Review dependency and browser-facing security changes.
- [x] Check that the working tree has no profiles, screenshots, logs, or other
      generated data.

## Validate the checkout

Run:

```console
cargo fmt --all -- --check
python3 scripts/check-version-sync.py
cargo test --all --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --locked --no-deps
cargo package --locked --no-verify
cargo publish --locked --dry-run --no-verify
cargo deny check
cargo audit
cargo check --manifest-path fuzz/Cargo.toml --bins
GLASS_PREVIOUS_VERSION=0.1.18 scripts/smoke-clean-install.sh
```

Then complete these release checks:

- [x] Run the real-browser smoke test on the current Linux ARM64 machine with
      its installed Chromium binary.
- [x] Record the host OS, architecture, Rust target, browser version, and
      commands used for the local platform check.
- [x] Keep other declared targets labeled unverified unless their own native
      environments are tested separately.
- [x] Inspect `cargo package --list` and the unpacked package.
- [x] Review dependency, license, and vulnerability JSON reports.
- [ ] Run a clean-machine crates.io install and upgrade test after publication.

The local browser and package checks are evidence for this machine only.

## Publish

- [x] Publish and verify the `0.2.4` crates-only release boundary.
- [x] Create a signed annotated tag such as `v0.2.4`.
- [x] Publish `glass-browser` from the tagged commit with `cargo publish
      --locked` after the package checks passed and publication was approved.
- [x] Include the local Linux ARM64 verification boundary and known
      limitations in the 0.2.4 release notes or changelog.
- [x] Verify installation and upgrade smoke checks for the published release.
- [x] Restore an empty `Unreleased` changelog section after publication.
- [ ] Run the full 0.2.5 release validation suite and package dry runs.
- [ ] Create the signed annotated `v0.2.5` tag after publication approval.
- [ ] Publish `glass-browser` from the tagged commit after explicit approval.
- [ ] Push the release commit and tag after explicit approval.
- [ ] Update issue #30 with final verified 0.2.5 release evidence.

A release is not complete while any required checkbox is open.
