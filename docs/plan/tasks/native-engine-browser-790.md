---
id: native-engine-browser-790
scope: native-engine/worker-csp-request-destinations
status: planned
depends-on: [native-engine-browser-789]
---

# Glass native-engine browser slice 790: worker CSP request destinations

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- [CSP Level 3 `worker-src`](https://www.w3.org/TR/CSP/#directive-worker-src)
  governs Worker, SharedWorker, and ServiceWorker script creation. Its fallback
  chain is `worker-src`, `child-src`, `script-src`, then `default-src`.
- [CSP Level 3 `script-src`](https://www.w3.org/TR/CSP/#directive-script-src)
  governs script requests. HTML's
  [`importScripts()` algorithm](https://html.spec.whatwg.org/multipage/workers.html#dom-workerglobalscope-importscripts)
  fetches classic worker-imported scripts through that script-request path.
- Slices [283](native-engine-browser-283.md),
  [285](native-engine-browser-285.md), and [401](native-engine-browser-401.md)
  establish worker loading, bounded import graphs, and worker response-policy
  ownership. The loader currently classifies both root workers and imported
  scripts as `NativeSubresourceKind::Worker`, conflating these CSP checks.

## Objective

Apply CSP to worker creation and worker-loaded scripts according to their
distinct request destinations, while preserving policy ownership, report-only
behavior, and existing loader bounds.

## Contract

- Keep root `Worker`, `SharedWorker`, and `ServiceWorker` creation requests on
  the worker destination. Enforced and report-only checks use the fallback
  chain `worker-src` → `child-src` → `script-src` → `default-src`; violation
  metadata names the directive that supplied the effective source list.
- Fetch classic `importScripts()` dependencies and dedicated/shared/service
  worker module-graph dependencies as script requests. Use the existing script
  URL-policy chain (`script-src-elem`, then `script-src`, then `default-src`)
  against the active worker policy. `worker-src` must not authorize or block
  these script dependencies.
- Preserve page policy ownership for worker creation and worker-response
  policy ownership for each worker's imports. Do not attach an imported
  script's response policy to the already-running worker global.
- Report-only declarations emit the correctly attributed bounded violation,
  but never change whether the worker or imported script loads.
- Keep redirects, MIME checks, integrity behavior, cookies, source bounds,
  structured errors, and rooted-file admission unchanged.
- Cover worker-src precedence and each fallback edge, plus a real HTTP(S)
  worker whose response policy allows `worker-src` but denies a script import
  through `script-src`; add report-only evidence that observes but does not
  block the same import.

## Boundaries and tradeoffs

- This corrects directive selection only; it does not claim complete CSP
  source expressions, strict-dynamic, nonce/hash semantics for worker imports,
  dynamic policy-container mutation, blob/data worker inheritance, or complete
  CSP/WPT conformance.
- Sites that relied on the engine's previous use of `worker-src` to authorize
  `importScripts()` or module dependencies may now see those imports correctly
  blocked by their script policy. This is an intentional standards-alignment
  and security correction, not a compatibility fallback.
- The in-process native engine is still not a security boundary for hostile
  remote content; issue #40's isolation and production gates remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-790.md`

## Verification plan

- Run one scoped `cargo check -p glass-browser --test native_engine --locked
  --quiet` after the complete implementation batch.
- Run the focused worker CSP regressions, then relevant existing worker import
  and service-worker module-graph tests against the built integration binary.
- Run `rustfmt --edition 2024 --check` on changed Rust files and `git diff
  --check`.
- Run the four maintainer documentation gates after synchronizing the current
  contract and evidence.
- Record local-only evidence precisely. Remote CI, cross-platform
  certification, full WPT conformance, and issue #40 promotion gates remain
  open.
