---
id: native-engine-browser-848
scope: glass-browser/native-engine/content-process-resource-event-effects
status: done
depends-on: [native-engine-browser-847]
---

# Glass native-engine browser slice 848: image and media event effects

## Objective

Preserve page-script effects emitted by process-backed image and media resource
`load`/`error` handlers. Execute each handler in its owning document turn and
route Fetch, WebSocket, EventSource, navigation, and document effects through
the existing parent broker and mutation settlement path.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md` — resource loading and content-process
  effect ownership.
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/tasks/native-engine-browser-843.md` — fail-closed child transport
  and parent cookie authority.
- `docs/plan/tasks/native-engine-browser-846.md` — autonomous timer owner turns.
- `docs/plan/tasks/native-engine-browser-847.md` — stylesheet event callback
  effects and parent-brokered Fetch regression.
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- A live page mutation that loads an HTTP(S) image or media resource dispatches
  exactly one appropriate `load` or `error` event in the same document owner.
- When a script-created node receives its native node identity, its registered
  event-listener and event-handler keys are rebound from the temporary script
  identity to that native identity, and its event-owner identity reports the
  assigned native ID. Resource events must not silently miss handlers because
  their node ID changed during mutation settlement.
- Commands from those event handlers are not reduced to document mutations:
  supported Fetch, WebSocket, EventSource, navigation, and DOM effects must
  retain their normal bounded settlement behavior.
- HTTP(S) callback requests use the captured owner-checked parent broker. The
  parent alone selects request cookies and accepts response cookies; raw cookie
  headers, HttpOnly values, and complete cookie state do not cross IPC.
- CSP-denied image requests do not reach transport, but their `error` callback
  still runs and can perform an allowed parent-brokered request.
- A failed or CSP-denied source is settled once per unchanged source. Later
  callback Fetch settlements must not retry it or replay its `error` event;
  changing the selected source permits a new attempt.
- No callback may target a replacement document or cause a child-side direct
  HTTP(S) retry. `document.cookie` remains only the URL-scoped script-visible
  projection.
- A real loopback-server process-backed regression covers successful image and
  media events plus a policy-blocked image error. It verifies callback Fetch
  order, parent HttpOnly-cookie rotation/reuse, event counts, no blocked
  transport, and an empty script-visible cookie projection.
- This slice does not certify general event-loop fairness, rendering
  opportunities, WPT conformance, or cross-platform behavior.

## Path

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-848.md`
- `docs/plan/reviews/native-engine-browser-848-01.md`

## Verification

- `cargo check -p glass-browser --features native-engine --test native_engine
  --locked --quiet` — passed; the crate reports existing dead-code warnings.
- `cargo build -p glass-browser --features native-engine --bin
  glass-native-content-worker --locked --quiet` — passed.
- `cargo test -p glass-browser --features native-engine --test native_engine
  native_content_process_idle_timer_ --locked -- --quiet` — passed (2 passed;
  914 filtered; 41.31 seconds).
- `cargo fmt --all -- --check` and `git diff --check` — passed locally.
- `python3 scripts/check-release-documentation.py --require-previous-version
  --report /tmp/glass-release-documentation.json` — passed (1,479 Markdown
  documents; zero current-claim failures).
- `python3 scripts/check-documentation-depth.py` — passed (93 current guides,
  19 substantive contracts).
- `python3 scripts/check-tui-shortcuts.py` — passed (15 implementation keys,
  63 documentation markers).
- `python3 scripts/check-documentation-coverage.py --glass
  /home/ubuntu/work/glass/target/debug/glass --glass-browser
  /home/ubuntu/work/glass/target/debug/glass-browser` — passed (1,479 Markdown
  files; 346 full-product MCP tools; 101 browser-only tools; 17 examples;
  22 public modules).
- Review: [native-engine-browser-848-01](../reviews/native-engine-browser-848-01.md)
  — direct self-review; no independent agent review was performed.
- Local evidence only: no remote CI, WPT, or cross-platform conformance result
  is claimed.
