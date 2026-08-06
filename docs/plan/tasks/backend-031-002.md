---
id: backend-031-002
scope: CDP BrowserBackend adapter integration
status: pending
depends-on: [backend-031-001]
---

# Objective

Make the existing CDP-backed `BrowserSession` the first implementation of the transport-neutral Browser Capability Interface. Keep CDP as the production backend while isolating connection, contexts, extraction, actions, navigation, runtime, storage, prompts, downloads, capture, and error translation behind stable Glass contracts.

# Context

- `docs/plan/analysis/release-031.md`
- `docs/architecture/browser.md`
- Issue #31 Pillar III and Gate 3
- `src/browser_backend.rs` from `backend-031-001`
- `src/browser/cdp.rs`
- `src/browser/session/`
- `src/task_compiler.rs`

# Path

Backend contract, CDP adapter/session modules, stable error translation, capability declarations, and focused conformance tests. Do not add a second browser runtime.

# Verification

Run the common backend contract tests against the real CDP implementation plus browser-free capability/translation tests. Prove compiler/runtime rejection for omitted capabilities and verify no CDP types cross stable protocol/public contracts.
