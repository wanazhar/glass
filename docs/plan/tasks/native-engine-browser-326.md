# Native-first resident browser ownership (326)

Status: implemented locally in the native-first resident integration.

This slice moves the normal `glass-dev` browser workspace onto the
Glass-owned native runtime. `glass.browser.start` creates a
`BrowserRuntimeSession` backed by `NativeEngineBackend`; it does not launch
Chromium or connect to CDP. The existing Chromium resident path remains
available through the explicit `glass.browser.attach` tool, which is treated as
a migration/interop operation and is reported as `browserBackend: "chromium"`.

The resident worker keeps one selected session behind a typed enum and routes
the existing lifecycle, observation, semantic observation, Web IR, target,
navigation, history, reload, stop-loading, highlight validation, action,
workflow, screenshot, remote-view, and remote-input contracts through the
selected owner. Native actions preserve revision checks and the shared action
envelope; native profile storage remains Rust-owned and incognito remains
volatile. Native remote point input uses the configured logical viewport, and
text input without an explicit target is delivered to the focused native text
control.

The public resident state now reports the selected backend and leaves the
browser process ID empty for native ownership. Reconnect closes and recreates
the same selected backend from its saved configuration. No resident operation
silently falls through from native to Chromium.

## Tradeoffs

Native is now the product default for the development suite, so ordinary users
exercise the native engine continuously instead of only through an opt-in
one-shot route. Explicit Chromium attach remains useful for migration and
interop, but it is no longer an accidental dependency of normal startup.
Keeping the two owners behind one worker enum avoids duplicate lifecycle and
revision logic, at the cost of a temporary adapter branch while the native
surface reaches complete Core Web Profile parity. This slice does not claim
cross-platform certification or close issue #40.

## Local evidence

- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo check --quiet -p glass-dev --tests --locked`
- `cargo test --quiet -p glass-dev --lib --locked resident_browser_defaults_to_native_runtime -- --nocapture`
- `cargo fmt --all -- --check`
- `git diff --check`

The resident test starts the actual native owner, verifies the backend marker,
performs observation/Web IR reads, and stops the owner. This checkpoint is
local-only; it has not been pushed and has no remote CI result.
