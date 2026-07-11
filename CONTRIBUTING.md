# Contributing to Glass

## Development setup

Install stable Rust with `rustfmt` and Clippy, plus Chrome or Chromium for
browser smoke tests. Build and inspect the CLI with:

```console
cargo build
cargo run -- --help
```

Keep changes focused and follow `rustfmt` defaults. Use typed structures and
`Result`-based error propagation, emit diagnostics through `tracing`, and keep
intentional CLI results on stdout.

## Validation

Run these checks before submitting a change:

```console
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
```

For browser lifecycle or CDP changes, also run the opt-in local fixture test:

```console
GLASS_E2E=1 cargo test --test browser_smoke -- --nocapture
```

The test uses a local fixture but requires a detectable Chrome/Chromium.

## Tests and documentation

Put deterministic unit tests beside the module under test and end-to-end tests
under `tests/`. Keep network- or Chrome-dependent tests explicit and isolated.
Update the README, relevant guide, and changelog whenever user-visible behavior
changes. Commands in documentation must work with the current CLI.

## Changes and reviews

Use short, imperative commit subjects, such as `browser: handle CDP timeouts`.
A change request should explain behavior and motivation, list validation
commands, link the relevant issue, and include a screenshot or recording for
terminal UI changes.

Never commit browser profiles, cookies, screenshots containing private data,
credentials, or verbose logs from authenticated sessions.
