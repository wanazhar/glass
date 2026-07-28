# Release checklist

Use this checklist for each public release.

## Current candidate

The local candidate is `glass-browser` `0.2.0`, prepared on 2026-07-28. The
supported release targets are Linux x86-64 and macOS x86-64/arm64. Windows is
outside the release contract. This checkout is prepared locally only; a GitHub
release, crates.io publication, npm publication, and release tag are separate
steps and must not be described as complete until they have succeeded.

## Prepare

- [ ] Confirm the intended version and release date.
- [ ] Ensure `Cargo.toml` contains the correct version, description, license,
      README, and finalized repository metadata.
- [x] Keep the local `0.2.0` changelog entry explicitly marked unreleased.
- [x] Verify README installation steps and `glass --help` output.
- [ ] Review dependency and browser-facing security changes.
- [ ] Confirm the working tree contains no profiles, screenshots, logs, or
      other generated data.

## Validate

```console
cargo fmt --all -- --check
cargo test --all --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --locked --no-deps
cargo build --release --locked
cargo package --locked --no-verify
cargo deny check
cargo audit
cargo check --manifest-path fuzz/Cargo.toml --bins
```

- [ ] Confirm the tagged release matrix ran the real-browser smoke on Linux
      x86-64 and macOS x86-64/arm64.
- [ ] Smoke-test `--help`, `navigate`, `observe`, `screenshot`, TUI startup,
      and MCP initialization using the release binary.
- [ ] Inspect `cargo package --list` and unpacked package contents.
- [ ] Download every artifact, verify `SHA256SUMS` with `sha256sum -c`, and
      verify the Sigstore bundle with `cosign verify-blob`.
- [ ] Review the attached dependency, license, and vulnerability JSON reports.

## Publish

- [ ] Commit the version and changelog update.
- [ ] Create a signed, annotated version tag such as `v0.2.0`.
- [ ] Publish or upload artifacts only from the tagged commit.
- [ ] Include supported platforms, checksums, changelog notes, and known
      limitations in the release entry.
- [ ] Verify installation from the published artifact in a clean environment.
- [ ] Restore an empty `Unreleased` changelog section for ongoing work.
