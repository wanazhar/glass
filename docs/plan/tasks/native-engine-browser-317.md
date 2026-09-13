# Glass native engine browser slice 317: native-first product routing

Status: completed locally; issue #40 browser-profile conformance and remote CI
remain open.

## Objective

Make the Glass-owned native browser the primary runtime for feature-enabled
browser products while keeping Chromium/CDP available only through an explicit
runtime selection. Route the standalone browser TUI through the same native
session seam so its first navigation, observation, semantic actions, target
selection, history controls, and PNG presentation do not start or attach to a
Chromium process.

## Contract

- `native-engine` is enabled by default in `glass-browser`; `glass-dev` enables
  the same feature explicitly at its dependency boundary.
- Feature-enabled CLI parsing defaults `--browser-runtime` to `native`;
  no-feature builds retain `chromium` as their default and omit the native enum
  value.
- Browser commands and MCP select the native runtime without a CDP or browser
  process fallback. Administrative commands continue to dispatch without
  opening a browser session.
- Native TUI startup owns a local `BrowserRuntimeSession`; `l`, `n`, `observe`,
  semantic activation, selected-target typing, scrolling, target listing and
  selection, guarded history/reload controls, and PNG presentation use native
  operations. Chromium launch/attach remains available when
  `--browser-runtime chromium` is explicit.
- Native viewport configuration is accepted by the TUI and is translated into
  the bounded native viewport contract.
- Existing native limitations remain typed and visible. This slice does not
  claim final Glass Core Web Profile conformance, complete workflow parity,
  or production certification for hostile web content.

## Path

- `crates/glass-browser/Cargo.toml`
- `crates/glass-dev/Cargo.toml`
- `crates/glass-browser/src/cli/args.rs`
- `crates/glass-browser/src/cli/runner.rs`
- `crates/glass-browser/src/tui/app.rs`
- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/src/lib.rs`
- `crates/glass-browser/src/browser/mod.rs`
- current browser and architecture documentation

## Implementation

The TUI stores its browser session behind a boxed enum so adding the native
runtime does not inflate the interactive constructor's stack frame. A small
adapter normalizes Chromium and native observations into the workspace's page,
revision, semantic-entity, target, control, and screenshot projections. Native
actions carry the same workspace revision guard as Chromium actions. Native
reload uses the page location owner and native stop-loading acknowledges the
completed synchronous navigation lifecycle; neither path creates an alternate
browser process.

Runtime dispatch is ordered so browser operations use native by default while
browser-free administration, documentation, profile, and product commands do
not get intercepted by the browser session selector. The no-feature check
keeps the explicit Chromium product path buildable.

## Tradeoffs and follow-up

Native-first selection makes the default product direction unambiguous, but it
also makes the native feature part of the normal dependency/build path and
exposes incomplete native command families earlier. Chromium remains an
explicit migration backend until each Glass Core Web Profile gate is proven;
there is no silent fallback. Native workflow execution through the TUI,
complete screenshot evidence formats, full Web IDL/HTML/CSS/media coverage,
security/process isolation, cross-platform release binaries, and final remote
CI certification remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo check --quiet -p glass-browser --no-default-features --tests --locked`
- focused native default-runtime and TUI tests
- repository documentation coverage, depth, and release-documentation
  validators
- `git diff --check`

The scoped checks and focused tests pass locally. Remote CI, publication,
release, and final native/CDP parity claims remain pending the wider issue #40
gates.
