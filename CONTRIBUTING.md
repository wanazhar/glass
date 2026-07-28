# Contributing to Glass

## Before you start

Install stable Rust with `rustfmt` and Clippy. Install Chrome or Chromium when
you need to run browser tests.

Read the [documentation style](docs/documentation-style.md) before you edit a
user guide. Keep commands, field names, and status statements exact.

## Build and test

Run:

```console
cargo build --locked
cargo test --all-targets --locked
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Run the browser test only when a supported browser is available:

```console
GLASS_E2E=1 cargo test --test browser_smoke -- --nocapture
```

The test uses a local fixture. It does not use a public site.

## Code rules

- Keep each change focused.
- Use four spaces for indentation and the default `rustfmt` format.
- Use typed structures and `Result` for fallible operations.
- Use `tracing` for diagnostics.
- Write intentional CLI results to stdout.
- Keep network and browser tests explicit.
- Put unit tests beside the module under test.
- Put end-to-end tests in `tests/`.
- Update the relevant guide and changelog for user-visible behavior.
- Do not add a platform to the support contract without release evidence.

## Documentation rules

Use short sentences and active voice. State the default, limit, failure result,
and recovery action. Do not use internal planning text in user guides. Do not
make claims that the current tests do not support.

Check documentation changes with:

```console
git diff --check
```

## Commit and review

Use a short conventional commit subject. Examples:

```text
docs(cli): clarify target selection
fix(browser): reject stale target references
test(daemon): cover lease expiry
```

A change request must include:

- the behavior and reason for the change;
- the validation commands;
- the related issue; and
- a terminal screenshot or recording for a TUI change.

## Sensitive data

Do not commit browser profiles, cookies, screenshots with private data,
credentials, or logs from an authenticated session.
