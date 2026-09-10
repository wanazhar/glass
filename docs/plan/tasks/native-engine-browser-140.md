---
id: native-engine-browser-140
scope: glass-browser/native-engine/prompts
status: completed
depends-on: [native-engine-browser-139]
---

# Native engine browser slice 140: prompt lifecycle

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Expose a bounded native JavaScript prompt lifecycle through the same runtime,
CLI, and MCP seams used by the rest of Glass. A page-owned `alert`, `confirm`,
or `prompt` call must become observable native state and must never require a
Chromium session.

## Contract

- page JavaScript can raise `alert`, `confirm`, and `prompt` through the
  native QuickJS host projection;
- the native owner preserves dialog type, bounded message, optional bounded
  default value, and the committed document URL;
- local page scripts, lifecycle/event scripts, external HTTP(S) page-load
  scripts, and content-worker evaluations all forward prompt metadata through
  the typed native worker boundary;
- dialog metadata is FIFO, bounded by the native queue and text limits, and
  malformed or unsupported wire values fail closed;
- the runtime backend exposes prompt inspection and accept/dismiss operations,
  and CLI/MCP route `acceptDialog` and `dismissDialog` without creating
  Chromium;
- native `dialogOpen` verification observes the same pending-dialog owner;
- resolving a dialog removes one pending record and reports whether a dialog
  was handled;
- the current deterministic script bridge returns `false` for `confirm` and
  `null` for `prompt`; suspended JavaScript continuation with a caller-supplied
  response remains a separate browser-loop milestone and is not represented as
  completed here.

## Tradeoffs

The page realm reports prompt metadata as an explicit worker response instead
of blocking a content-process thread on an external UI callback. This keeps
the native scheduler, IPC, and parent operation lock bounded and testable, but
means the current bridge records the prompt and continues the script with its
deterministic default result. Accept/dismiss therefore resolves the user-facing
pending state now; a later continuation milestone must add suspended evaluation
and response injection without allowing stale or cross-context decisions.

The queue is intentionally finite and text is bounded so hostile pages cannot
turn prompt handling into unbounded parent memory or diagnostic output. Dialog
metadata is retained as a public observation, while page source, evaluated
input, and credentials remain outside logs and error text.

## Implementation surface

- `browser/native_engine/javascript.rs`: host dialog functions, command
  validation, bounded runtime event queue, and page-script result projection;
- `browser/native_engine/content_process.rs`: typed dialog decoding, worker
  response merging, and load/mutation/script forwarding;
- `browser/native_engine/engine.rs`: pending-dialog ownership, queue limits,
  navigation replacement, history restoration, and resolution;
- `browser/native_backend.rs`, `browser/runtime.rs`: prompt backend and
  `dialogOpen` verification seams;
- `cli/runner.rs`, `mcp/server.rs`: native accept/dismiss dispatch;
- `tests/native_engine.rs` and MCP unit coverage: local, external worker, and
  public-tool lifecycle regression tests.

## Verification

```text
cargo check --quiet -p glass-browser --features native-engine --tests
cargo test --quiet -p glass-browser --features native-engine --test native_engine native_dialogs_are_owned_by_the_page_realm_and_prompt_backend -- --exact --nocapture
cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_forwards_page_dialogs_without_chromium -- --exact --nocapture
cargo test --quiet -p glass-browser --features native-engine native_mcp_routes_dialog_lifecycle_without_chromium -- --nocapture
cargo test --quiet -p glass-browser --features native-engine native_mcp_routes_core_browser_tools_without_chromium -- --nocapture
```

Completed evidence:

- the feature-gated `glass-browser` test target check passed;
- local and HTTP/content-process prompt lifecycle tests passed without a
  Chromium endpoint;
- native MCP evaluate, `dialogOpen`, and `acceptDialog` routing passed in a
  dedicated small-stack test, and the existing all-tools native MCP test
  remained green;
- a worker response overwrite bug was repaired by merging page-load dialog
  records with runtime-synchronization records before bounded decoding;
- no CDP fallback, browser process, or third crate was introduced.

Suspended modal continuation, prompt response injection, popup/download
topology, child-frame ownership, request accounting, universal workflow
parity, and native production promotion remain issue #40 work.
