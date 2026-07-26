# Release checklist

Use this checklist for each public release.

## Prepare

- [ ] Confirm the intended version and release date.
- [ ] Ensure `Cargo.toml` contains the correct version, description, license,
      README, and finalized repository metadata.
- [ ] Move changelog entries from `Unreleased` into a dated version section.
- [ ] Verify README installation steps and `glass --help` output.
- [ ] Review dependency and browser-facing security changes.
- [ ] Confirm the working tree contains no profiles, screenshots, logs, or
      other generated data.

## Validate

```console
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
cargo build --release --locked
cargo package --locked
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
- [ ] Create a signed, annotated version tag such as `v0.1.0`.
- [ ] Publish or upload artifacts only from the tagged commit.
- [ ] Include supported platforms, checksums, changelog notes, and known
      limitations in the release entry.
- [ ] Verify installation from the published artifact in a clean environment.
- [ ] Restore an empty `Unreleased` changelog section for ongoing work.
