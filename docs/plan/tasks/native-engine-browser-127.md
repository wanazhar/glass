---
id: native-engine-browser-127
scope: glass-browser/native-engine/native-cli-inspection
status: done
depends-on: [native-engine-browser-126]
---

# BE-41: native inspection and CSS target bridge

## Objective

Move the native runtime's existing CSS selector, semantic DOM, and software
capture owners through the public native session and one-shot CLI seams. This
is the first promotion slice for normal inspection and targeting: a native
caller must be able to use the same explicit `css=` locator vocabulary as the
Chromium path, inspect the native semantic tree, evaluate script, scroll, and
write a native PNG without creating a CDP session.

## Contract

- `css=<selector>` resolves through the native stylesheet selector matcher;
  supported compound and descendant selectors retain document order and must
  resolve exactly one element before mutation.
- Malformed or unsupported selectors fail with a typed native locator error;
  they are never treated as an ID, text, or broad-match fallback.
- The native session exposes bounded semantic-node inspection and PNG capture
  from the same engine instance used for navigation, script, and actions.
- Native one-shot dispatch accepts `evaluate`, `click-at`, `key`, `scroll`,
  `dom`, and `screenshot` in addition to the existing navigate/click/type/text/
  observe/targets commands. Native screenshot output is PNG only and uses the
  existing policy output-path check.
- Native dispatch continues to reject commands whose behavior is not yet
  mapped to a native owner; it must not route those commands to Chromium/CDP.
- No third installable crate, remote endpoint, hidden browser process, or
  implicit backend fallback is introduced.

## Ownership and sequence

```text
CLI css= locator -> NativeDocument CSS matcher -> unique node
native engine -> NativeEngineBackend -> BrowserRuntimeSession
native capture/tree -> policy-checked CLI projection -> result/file
```

The CSS matcher remains owned by the native CSS module and consumes the same
DOM ancestry used by stylesheet cascade. The session bridge only projects
already-owned semantic nodes and renderer bytes; it does not duplicate page
state or mutate a second document.

## Deliberate boundary and tradeoffs

Reusing the existing selector parser keeps target matching and style matching
consistent and avoids adding a second selector grammar. The selector grammar
is still the bounded native grammar (compound and descendant forms), so CSS
pseudo-classes, sibling combinators, and the full selector standard remain
future profile work. Exposing semantic nodes and PNG now makes the implemented
native owners usable by real Glass commands while preserving typed denial for
unmapped high-level commands; this avoids claiming parity from a thin adapter.

## Paths

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/interaction.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/src/cli/runner.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
  passed in 17.16 seconds after the final projection adjustment.
- `cargo check --quiet -p glass-browser --lib` passed in 31.78 seconds with
  the default feature set.
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine`
  passed: 396 tests, 0 failures, 74.85 seconds of test execution and 2:02.14
  wall time including build/link.
- The session bridge regression covers semantic-node publication and PNG
  signature capture; the CSS regression covers descendant matching, duplicate
  rejection, and malformed-selector rejection. Runner unit coverage verifies
  the native-only inspection command matrix and preserves Firefox/Safari
  rejection of native-only commands.
- `git diff --check` passed after the implementation batch.
- `cargo fmt --all -- --check` passed.
- After the final camelCase projection adjustment, the focused session bridge
  test passed 1/1 in 0.57 seconds and the focused CSS bridge test passed 1/1
  in 0.27 seconds.
- `python3 scripts/check-documentation-coverage.py` passed: 777 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, and 22 public
  modules.
- `python3 scripts/check-documentation-depth.py` passed: 93 current guides and
  19 substantive contracts.
- `python3 scripts/check-release-documentation.py --require-previous-version`
  passed: 777 Markdown documents, 83 current documents, 59 previous-version
  hits, 972 semantic-audit hits, and 0 current-claim failures.
- `cargo clippy --quiet -p glass-browser --features native-engine --test
  native_engine -- -D warnings` remains blocked by the established 34
  pre-existing native-feature diagnostics; no diagnostic points to the 127
  bridge changes.
- The runner test target requires `RUST_MIN_STACK=8388608` because the existing
  Clap-derived command parser overflows the default test-thread stack when the
  native feature is enabled; with that explicit harness setting, 18/18 runner
  tests passed. Remote CI remains pending because this checkout has not been
  pushed.
