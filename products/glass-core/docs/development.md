# Architecture and development

[Documentation index](INDEX.md) · [Project overview](../README.md)

## Layout

```
crates/glass-mod-api   the in-process hook surface, zero dependencies
crates/glass-tui       terminal rendering, input, virtualized transcript
crates/glass           agent loop, providers, native tools, session storage,
                       context, configuration, permissions, extension host
packages/extension-sdk TypeScript extension types, runtime, and examples
extensions/            reference protocol, optional integrations and CLI helpers
```

`glass-mod-api` defines shared hook types and remains dependency-free. Provider,
file, job, and session implementations stay in the main crate; terminal rendering
and input types stay in `glass-tui`.

## Performance notes

The transcript uses cached layout and builds the visible window for each frame.
Its viewport cache is bounded. Large tool output is retained in spill files with
explicit retrieval references, so full logs can remain available without entering
every model request.

Optional integrations are loaded when configured or explicitly requested. No
background model polling is required for local browsing, recaps, or knowledge
inspection. Tool count alone does not predict task latency or token cost.

The first recorded comparison on Linux aarch64 (2026-10-15) used three
alternating runs of the previous and updated release binaries. Representative
updated timings were:

| Measurement | Median of reported values |
| --- | ---: |
| Process launch to interactive prompt | 5.85 ms |
| Typed key becoming visible | 0.131 ms |
| Visible redraw | 0.334 ms |
| Reader frame with 12,000 stored records | 102.51 ms |

These are local fixture results. They exclude provider latency and do not
establish cross-machine performance or coding success. The
[comparison record](plan/reviews/runtime-footprint-2026-10-15.md) includes the
baseline, file timings, binary size, request overhead, and measured tradeoffs.

A [second optimization round](plan/reviews/runtime-footprint-round2-2026-10-15.md)
measures bounded prompt recall, diagnostic writes, and native read payloads.

To measure the current release build:

```sh
GLASS_CORE_TEST_BINARY="$PWD/target/release/glass-core" \
  GC_PERF_REPORT=/tmp/gc-performance.json \
  python3 scripts/check-performance.py
python3 scripts/check-native-performance.py
```

Run measurements after compiler and other test load has stopped.
The release profile uses size optimization with full LTO, one code generation
unit, stripped symbols, and aborting panics. Compare runtime measurements when
changing these settings. [Runtime storage](reference/storage.md) explains the
footprint fixture and default disk writes.

## Tests

Rust tests cover engine and UI contracts. Offline Python suites use local provider
fixtures, real processes, and PTYs. They cover file revisions, output bounds,
stream recovery, cancellation, private storage, session ownership, and terminal
navigation. Mock providers do not establish live-model reliability.

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --locked --package glass-core --bin glass-core
export GLASS_CORE_TEST_BINARY="$PWD/target/release/glass-core"
python3 scripts/smoke-conversations.py
python3 scripts/smoke-cli.py
python3 scripts/smoke-live.py
python3 scripts/smoke-reader.py
python3 scripts/smoke-agents.py
python3 scripts/smoke-review-lifecycle.py
```

PTY suites require a Unix host, Python 3.10 or newer, and `pyte==0.8.2`. Use the
suite relevant to a change, then the integrated checks required by CI. Provider
sign-in and real inference require a separate account-backed trial. Preserve the
configuration, logs, and evidence used to investigate a failure.

## Build and upgrade checks

The GitHub Actions workflow builds and checks Linux and macOS independently.
It runs the Rust tests, Clippy, installer checks, and offline PTY suites (including
20×8 terminals), then performs a real clean install and repeat installation in a
disposable configuration and data directory. These checks require no provider
account. They verify that configuration, credentials, history, and session files
survive installation and that the separate `glass` executable is preserved.

Successful jobs upload a native binary archive containing `glass-core`, the
relative `gc` symlink, and this README, plus a SHA-256 checksum. These are CI
artifacts; the workflow does not publish a release or modify Git tags.

To run the same checks locally:

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --locked --package glass-core --bin glass-core
export GLASS_CORE_TEST_BINARY="$PWD/target/release/glass-core"
python3 scripts/smoke-install.py
python3 scripts/smoke-features.py
python3 scripts/smoke-onboarding.py
python3 scripts/smoke-polish.py
python3 scripts/smoke-workflows.py
python3 scripts/smoke-mobile.py
python3 scripts/smoke-chat.py
python3 scripts/smoke-motion.py
python3 scripts/smoke-live.py
python3 scripts/smoke-navigation.py
python3 scripts/smoke-trust.py
python3 scripts/smoke-sandbox.py
python3 scripts/smoke-init.py
python3 scripts/smoke-web.py
python3 scripts/smoke-mcp.py
python3 scripts/smoke-acp.py
python3 scripts/smoke-runtime.py
python3 scripts/conformance-runtime.py
python3 scripts/smoke-serve.py
python3 scripts/smoke-upgrade.py
```

The PTY suites require a Unix host and Python 3.10 or newer. Real provider
authentication and model behavior require a separate account-backed trial.
Install the VT parser dependency with `python3 -m pip install pyte==0.8.2`
before running the PTY suites.
