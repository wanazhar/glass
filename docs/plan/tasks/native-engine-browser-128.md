---
id: native-engine-browser-128
scope: glass-browser/native-engine/form-control-actions
status: done
depends-on: [native-engine-browser-127]
---

# BE-42: native form-control action parity

## Objective

Move the existing Glass form-control action vocabulary through the stable
semantic backend contract and native engine owner. Native callers must be able
to clear editable controls, ensure checkable controls are checked or
unchecked, and select one exact option value without opening a CDP session.

## Contract

- `Clear { target }` resolves one visible editable input or textarea and
  commits an empty value with focus/input/change ownership in the native DOM.
- `Check { target }` and `Uncheck { target }` use native click semantics for
  checkbox/radio controls, including radio-group state and event cancellation;
  an already-satisfied state is a successful no-op.
- `Select { target, value }` resolves one visible, enabled `<select>` and one
  exact enabled option value, updates selection according to single- or
  multiple-select rules, and emits input/change effects when selection
  changes.
- Local documents and process-backed HTTP(S) documents use the same action
  contract. Child mutations return a validated document snapshot and bounded
  event list; the parent remains the sole revision/history owner.
- Chromium’s semantic adapter maps the same actions to the existing
  `BrowserSession` methods. Firefox, Safari, and proof adapters reject them
  explicitly until their own action profiles certify the behavior.
- Native one-shot CLI dispatch maps `clear`, `check`, `uncheck`, and `select`;
  it does not route unsupported commands to Chromium or another backend.

## Deliberate boundary and tradeoffs

The semantic contract carries action intent rather than a CDP object handle,
so target resolution stays owned by each backend. Native check/uncheck reuses
the already-tested click/default-action path to preserve event cancellation and
radio behavior. Clear and select have dedicated DOM mutations because a key
sequence cannot represent an atomic clear or exact option selection in the
current native input model. Option disabledness and control actionability are
validated before mutation; no partial child snapshot is committed on error.

Adding these variants expands the public backend enum and requires exhaustive
handling in every adapter. That compile-time pressure is intentional: a new
action cannot accidentally become an unreviewed no-op on a non-native runtime.

## Paths

- `crates/glass-browser/src/browser_backend.rs`
- `crates/glass-browser/src/browser/backend_adapter.rs`
- `crates/glass-browser/src/browser/native_engine/interaction.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/bidi_backend.rs`
- `crates/glass-browser/src/browser/webdriver_backend.rs`
- `crates/glass-browser/src/browser/proof_backend.rs`
- `crates/glass-browser/src/cli/runner.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --lib` passed
  after the implementation batch.
- `cargo test --quiet -p glass-browser --features native-engine --test
  native_engine native_form_actions_clear_check_and_select_controls --
  --nocapture` passed: 1 test, 0 failures.
- `cargo test --quiet -p glass-browser --features native-engine --test
  native_engine native_content_process_owns_clear_and_select_form_actions --
  --nocapture` passed: 1 test, 0 failures.
- `cargo test --quiet -p glass-browser --features native-engine --test
  native_engine` passed: 398 tests, 0 failures, 71.28 seconds of test
  execution.
- `cargo fmt --all` completed and `git diff --check` passed.
- `python3 scripts/check-documentation-coverage.py` passed: 778 Markdown
  files, 345 full-product MCP tools, 17 examples, and 22 public modules.
- `python3 scripts/check-documentation-depth.py` passed: 93 current guides and
  19 substantive contracts.
- `python3 scripts/check-release-documentation.py --require-previous-version`
  passed: 778 Markdown documents, 83 current documents, 59 previous-version
  hits, 972 semantic-audit hits, and 0 current-claim failures.
- The known native-feature Clippy baseline remains the existing 34 diagnostics
  recorded by task 127; this task does not claim native/CDP parity or default
  promotion.
