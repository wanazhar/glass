# Release checklist

Use this checklist for each public release.

## Release status

The current published release is `glass-browser` version `0.2.5`. This
checklist is being used for the next crates.io release, `0.2.6`. Linux
x86-64, Linux arm64, macOS x86-64, and macOS arm64 remain declared targets;
only Linux ARM64 evidence is currently recorded. Other declared targets
require their own native validation. Windows is unsupported.

Every version tag must have a matching, published, non-draft GitHub Release
entry. The release entry contains generated notes and does not imply native
binary distribution.
The newest published release must be explicitly marked `Latest`; older
release records must not carry that marker.

Versioning and annotated `vX.Y.Z` tags remain part of the process. Each release
publishes the crate to crates.io and creates a source-only GitHub Release.
GitHub release binaries, checksum manifests, Sigstore bundles, and the npm
native launcher are not release deliverables.

Pushing a `vX.Y.Z` tag runs `.github/workflows/crates-release.yml`. The action
checks the tag and package version, runs the release validation suite, performs
a crates.io dry run, publishes the crate when needed, and creates the matching
GitHub Release with generated notes. Existing crate or release records are
detected idempotently; native binary artifacts are never uploaded.

## Prepare

- [x] Confirm development version `0.2.6`; release date remains pending
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
GLASS_PREVIOUS_VERSION=0.2.5 scripts/smoke-clean-install.sh
```

Then complete these release checks:

- [x] Run the real-browser smoke test in the recorded Linux ARM64 validation
      environment.
- [x] Record the target environment, architecture, Rust target, browser
      version, and commands used for the platform check.
- [x] Keep other declared targets labeled uncertified unless their own native
      environments are tested separately.
- [x] Inspect `cargo package --list` and the unpacked package.
- [x] Review dependency, license, and vulnerability JSON reports.
- [x] Run a clean-machine crates.io install and upgrade test after publication.

## Verify GitHub release records

Run after the tagged release workflow or with authenticated `gh` access:

```console
python3 scripts/check-github-releases.py
```

This check compares every `vX.Y.Z` tag with published, non-draft,
non-prerelease GitHub Release records and fails if any tag is missing.

The browser and package checks are evidence for the tested environment only.

## Publish

- [x] Publish and verify the `0.2.5` crates.io package.
- [x] Create and verify the matching published GitHub Release for `v0.2.5`.
- [x] Create the signed annotated tag `v0.2.5`.
- [x] Publish `glass-browser` from the tagged commit with `cargo publish
      --locked` after the package checks passed and publication was approved.
- [x] Include the Linux ARM64 validation boundary and known
      limitations in the 0.2.5 release notes or changelog.
- [x] Verify installation and upgrade smoke checks for the published release.
- [x] Restore an empty `Unreleased` changelog section after publication.
- [x] Run the full 0.2.5 release validation suite and package dry runs.
- [x] Push the release commit and tag after explicit approval.
- [x] Update issue #30 with final verified 0.2.5 release evidence.
- [ ] Run the full 0.2.6 release validation suite and package dry runs.
- [ ] Create the signed annotated `v0.2.6` tag after publication approval.
- [ ] Publish `glass-browser` from the tagged commit after explicit approval.
- [ ] Create and verify the matching published GitHub Release for `v0.2.6`.

A release is not complete while any required checkbox is open.
