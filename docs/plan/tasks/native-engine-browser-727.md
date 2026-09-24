---
id: native-engine-browser-727
scope: glass-browser/security/rooted-file-frame-csp
status: done
depends-on: [native-engine-browser-726]
---

# Glass native-engine browser slice 727: rooted-file frame CSP

## Objective

Enforce the active rooted-file Document's CSP before selecting the embedded
frame's child document URL, including policies captured from runtime-inserted
head meta elements.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-722.md` through
  `docs/plan/tasks/native-engine-browser-726.md`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- For rooted-file Documents, embedded frame requests use `frame-src`, then
  `child-src`, then `default-src` fallback.
- All enforced header/meta policies intersect. A runtime policy captured from
  a connected head meta element applies to frames created after capture.
- `'self'` authorizes only file resources admitted by the Document's
  most-specific configured root. Explicit `file:` expressions cannot bypass
  configured-root admission.
- An explicit `about:blank` frame remains an allowed initial empty document;
  this does not authorize the requested target of a CSP-blocked frame. The
  behavior is covered by the
  [WPT about:blank frame test](https://github.com/web-platform-tests/wpt/blob/master/content-security-policy/child-src/child-src-about-blank-allowed-by-default.sub.html).
- Evaluate policy before selecting the child engine's initial URL. A denied
  frame follows the existing blocked-frame behavior and remains `about:blank`;
  no requested child-document bytes are loaded.
- Preserve the current network-document frame policy path and existing
  redirect behavior. Report-only file policies and complete CSP conformance
  remain out of scope.

## Path

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-727.md`

## Verification

- Add loader tests for same-root admission, cross-root denial, the frame/child/
  default fallback chain, explicit file-source root bounds, and policy
  conjunction.
- Add a native-session integration regression proving a runtime-inserted head
  policy denies a later same-root iframe before its child document is loaded.
- Run one locked `glass-browser` check before the focused loader and
  process-backed tests, then formatting, whitespace, and static maintainer
  documentation gates.
- Do not run workspace-wide tests or remote CI for this slice.

## Results

Rooted-file frame discovery now consults the live policy ledger before
selecting a child document URL. It enforces `frame-src`/`child-src`/
`default-src` fallback and conjunction, scopes `'self'` to the most-specific
configured root, constrains explicit `file:` matches to configured roots, and
preserves explicit `about:blank` initial frames. A native-session regression
proves a runtime-inserted head policy blocks a later same-root iframe and keeps
it at `about:blank`.

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
  passed.
- `cargo test -p glass-browser --lib --test native_engine --locked --quiet
  rooted_file_frame_csp` passed both focused tests (1 loader, 1 integration).
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Release-documentation truth validated 1,355 Markdown files with zero
  current-claim failures; documentation depth validated 93 guides/19
  contracts; shortcut inventory validated 15 implementation keys/63 markers.
- Live CLI/link inventory was not run because `target/debug/glass` is absent.
  Remote CI was not run. Issue #40 remains open; file-origin data/blob policy,
  report-only file policy, and complete CSP conformance remain unfinished.
