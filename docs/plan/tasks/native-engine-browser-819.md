---
id: native-engine-browser-819
scope: glass-browser/page-response-cookie-live-sharedworker-probe
status: done
depends-on: [native-engine-browser-818]
---

# Glass native-engine browser slice 819: verify page-cookie changes in a live SharedWorker

## Objective

Isolate whether a page-owned HTTP response cookie update is observable by an
already-running same-backend SharedWorker's next request, independently from
the already-verified page/frame fan-out and profile persistence in Slice 818.

## Context

- [Issue #40](https://github.com/wanazhar/glass/issues/40) is the source of
  truth for native-browser completion.
- [Slice 818](native-engine-browser-818.md) returns accepted ordinary
  page-response cookie deltas to the backend and applies them to the bounded
  SharedWorker cookie override journal and request loader.
- Existing process-backed tests cover SharedWorker-originated updates and
  bidirectional page/SharedWorker message ports, but did not directly observe
  a page-originated update through a live SharedWorker request.

## Contract

- Keep the probe process-backed and use local HTTP only.
- First establish the SharedWorker connection and verify its ready message.
- After the page's ordinary `fetch()` response accepts cookies, send a distinct
  message to the already-running module worker; have it dynamically import a
  local HTTP module and report completion or failure to the page.
- Verify the worker request carries the latest accepted cookie values,
  including HttpOnly, and excludes the deleted cookie. Keep the existing peer,
  frame, separate-profile, and profile-reload assertions.
- Use an idle timeout per expected HTTP request so sequential native process
  startup time does not consume the server's entire test lifetime.
- If the regression fails, isolate message delivery, worker module loading,
  and cookie synchronization as separate stages; do not attribute an
  ambiguous timeout to cookie synchronization.

## Verification

- Run `cargo check -p glass-browser --test native_engine --locked --quiet`
  before the focused process-backed regression.
- Run the exact Slice 819 regression and the existing Slice 817 companion if
  implementation code changes.
- Run format/diff validation and all four maintainer documentation gates.
- Commit this bounded checkpoint locally; do not push or run remote CI.

## Results

The process-backed module-SharedWorker regression passes (1 passed, 869
filtered; 86.02 seconds). It separates page-to-worker message delivery from a
second message that triggers a dynamic module import. The imported module
completes in the already-running worker, and the HTTP server confirms that its
request carries the latest ordinary cookie and HttpOnly cookie but not the
deleted cookie. The same test retains Slice 818's peer/frame fan-out,
script-visibility, separate-profile isolation, and fresh-engine profile-reload
assertions. Per-request idle deadlines and accepted-path diagnostics keep
startup time from obscuring a missing request.

`cargo check -p glass-browser --test native_engine --locked --quiet` passed
with existing dead-code warnings from the superseded HTML parser in `dom.rs`;
formatting and diff validation passed. All four maintainer documentation gates
passed after updating this record: 1,447 Markdown documents with zero
current-claim failures; 93 guides/19 contracts; 15 shortcut keys/63 markers;
346 MCP tools (101 browser-only), 17 examples, and 22 public modules. This
verifies the live SharedWorker module-request path; direct Fetch API requests
from a SharedWorker remain a separate unverified behavior. Issue #40 remains
open.
