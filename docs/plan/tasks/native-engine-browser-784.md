---
id: native-engine-browser-784
scope: glass-browser/window-onbeforeunload-return-value
status: completed
depends-on: [native-engine-browser-783]
---

# Glass native-engine browser slice 784: typed `onbeforeunload` return values

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [HTML Standard event-handler processing algorithm](https://html.spec.whatwg.org/multipage/webappapis.html#event-handlers-on-elements,-document-objects,-and-window-objects)
  gives `onbeforeunload` a nullable `DOMString` return. On a `BeforeUnloadEvent`,
  a non-null return cancels the event and fills `returnValue` only when it is
  empty. The [BeforeUnloadEvent interface](https://html.spec.whatwg.org/multipage/nav-history-apis.html#the-beforeunloadevent-interface)
  also defines `returnValue` as a `DOMString`, initially empty.
- Native sticky-activation checks, shared dialog decisions, and generic
  browser-controlled prompt copy are already implemented. The missing piece is
  mapping the event-handler callback's typed return into the canceled event
  state consumed by that existing path.

## Objective

Implement the Window `onbeforeunload` callback return conversion and
`BeforeUnloadEvent.returnValue` semantics without changing when navigation is
eligible to prompt or how the host obtains a user decision.

## Contract

- Host-generated Window `beforeunload` events are `BeforeUnloadEvent` objects
  with an initially empty `returnValue` and the existing cancelable setting.
- The event-brand WeakMap persists at realm scope, matching the lifetime of
  cached Window event-handler callbacks across bootstrap refreshes.
- `window.onbeforeunload` and body/frameset aliases receive the event object
  with Window as `this`.
- `null` and `undefined` callback returns do not cancel. Any other callback
  result is converted using Web IDL `DOMString` conversion; on a
  `BeforeUnloadEvent`, it sets the canceled state even when the converted
  string is empty.
- If the callback's converted return is non-null and `returnValue` is empty,
  set it to that converted string. If it is already non-empty, preserve the
  existing value. Setting `BeforeUnloadEvent.returnValue` itself performs
  `DOMString` conversion, and a non-empty value cancels at dispatch completion.
- A plain `Event` whose type is `beforeunload` is not a `BeforeUnloadEvent` and
  does not use this special cancellation rule.
- The existing native prompt still requires the existing activation and
  controller policy, displays generic browser-owned copy, and waits for the
  same exact host decision. This slice does not implement script-authored
  prompt text, change navigation authorization, or alter `preventDefault()`.
- Local tests cover null/undefined, empty/non-empty strings, boolean/numeric
  conversion, preexisting `returnValue`, body alias ownership, synthetic plain
  Event distinction, and the existing process-backed prompt accept/dismiss
  flow driven by an IDL handler return.

## Boundaries and tradeoffs

- Conversion exceptions raised while processing an event handler are still
  subject to the native dispatcher's existing callback-exception behavior;
  standards-correct exception reporting and rethrow rules remain separate.
- This does not change sticky-activation policy, dialog transport/UI,
  sandbox-modal policy, event-handler CSP, worker event handlers, WPT,
  cross-platform certification, or issue #40's native-only production gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-784.md`

## Verification

Passed locally:

- `cargo check -p glass-browser --test native_engine --locked --quiet` passed.
- `cargo test -p glass-browser --test native_engine --locked --quiet -- native_content_process_beforeunload_waits_for_exact_user_decision --exact --test-threads=1`
  (1 passed; conversions, body alias, synthetic events, and prompt accept/dismiss)
- `rustfmt --edition 2024 --check crates/glass-browser/src/browser/native_engine/javascript.rs crates/glass-browser/tests/native_engine.rs`
- `python3 scripts/check-documentation-coverage.py`
  (1,412 Markdown files; coverage validated)
- `git diff --check` passed.

Remote CI, full WPT, cross-platform certification, and issue #40's native-only
production gates remain open.
