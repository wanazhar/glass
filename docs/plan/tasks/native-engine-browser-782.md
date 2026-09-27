---
id: native-engine-browser-782
scope: glass-browser/window-onerror-return-cancellation
status: completed
depends-on: [native-engine-browser-781]
---

# Glass native-engine browser slice 782: Window `onerror` return cancellation

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [HTML Standard runtime script error algorithm](https://html.spec.whatwg.org/multipage/webappapis.html#runtime-script-errors)
  fires a cancelable `ErrorEvent` at the global object. Its event-handler
  processing algorithm uses the legacy five `Window.onerror` arguments and
  treats a `true` return as cancellation.
- Native script-error events already reach Window listeners and `Window.onerror`
  receives the legacy arguments, but the generated events are non-cancelable
  and the special `true` return is not reflected in cancellation state.

## Objective

Implement the special return-value and cancelable-event behavior for native
script errors delivered to Window `onerror`, without changing ordinary
`error` EventHandler invocation or the established error-report transport.

## Contract

- A generated script-report `ErrorEvent` is cancelable.
- Special legacy invocation applies only when the callback is processing an
  `ErrorEvent` whose type is `error` at the Window. It receives
  `(message, filename, lineno, colno, error)` and `this === window`.
- For that special case, returning exactly `true` sets the event's canceled
  state, so `defaultPrevented` is true and dispatch returns false. Returning
  false or another value does not cancel through the special rule.
- A plain `Event` named `error` does not take the five-argument path and a
  `true` return does not cancel it through the special rule.
- Existing ordinary element-targeted error events keep their event-object
  callback argument and current owner. Existing content-attribute CSP and
  handler replacement behavior remain intact.
- Regression coverage verifies both direct dispatch semantics and cancellation
  observed on native script-error reporting, without aborting later scripts or
  the committed document.

## Boundaries and tradeoffs

- This does not implement cross-origin muted-error redaction, source mapping,
  reporting recursion/console policy, exception propagation from event
  listeners, or complete Worker `onerror` behavior.
- General EventHandler `return false` conversion and `onbeforeunload`'s
  non-null string/cancellation behavior remain separate profile work.
- Full Web IDL callback semantics, WPT, remote CI, cross-platform
  certification, and issue #40's native-only production gates remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-782.md`

## Verification

Passed locally:

- `rustfmt --edition 2024 --check crates/glass-browser/tests/native_engine.rs`
- `cargo check -p glass-browser --test native_engine --locked --quiet`
  (existing parser/dead-code warnings only)
- `cargo test -p glass-browser --test native_engine --locked --quiet -- native_local_inline_script_failure_dispatches_error_without_aborting_document --exact --test-threads=1`
  (1 passed; actual uncaught scripts plus direct special/ordinary dispatch)
- `python3 scripts/check-documentation-coverage.py`
  (1,410 Markdown files; coverage validated)
- `git diff --check`

Remote CI, full WPT, cross-platform certification, and issue #40's
native-only production gates were not run and remain open.
