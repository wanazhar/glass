---
id: native-engine-browser-832
scope: glass-browser/native-engine/module-dynamic-import-referrer-policy
status: in-progress
depends-on: [native-engine-browser-831]
---

# Glass native-engine browser slice 832: module dynamic-import referrer policy

## Objective

Carry each page module's effective fetch policy into `import()` requests and
the recursively loaded dynamic module graph, including response-policy
overrides on dynamically fetched modules.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-831.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [HTML Standard: `script` element](https://html.spec.whatwg.org/multipage/scripting.html#the-script-element)
- [HTML Standard: script fetch options](https://html.spec.whatwg.org/multipage/webappapis.html#script-fetch-options)
- [HTML Standard: `HostLoadImportedModule`](https://html.spec.whatwg.org/multipage/webappapis.html#hostloadimportedmodule-referrer-modulerequest-loadstate-payload)
- [Referrer Policy: redirect processing](https://w3c.github.io/webappsec-referrer-policy/#integration-with-fetch)

## Contract

- For page-realm module scripts, `import()` uses the active referencing
  module's response URL as its referrer and that module's effective script
  fetch referrer policy. The policy originates from the module element or the
  owning Document default, and a recognized module response
  `Referrer-Policy` header overrides it for that module's static and dynamic
  dependencies.
- A dynamically fetched module carries its effective response policy through
  its static graph and into later `import()` calls. Redirect response policies
  update the next request hop only; the final module response policy updates
  the module's inherited fetch options. Missing or unrecognized response
  values retain the policy supplied by the referencing module.
- Preserve module identity/deduplication, import-map resolution, source and
  response URLs, CORS/credentials, integrity, CSP, cookies, cache behavior,
  redirect and byte limits, module linking, and dynamic-import promise/error
  semantics. Do not apply this policy to ordinary page `fetch()` calls.
- A process-backed two-origin regression verifies actual headers for
  `import()` from inline and external page modules, the response-policy
  override, and a nested dynamic/static dependency. Pure tests cover policy
  propagation through module source metadata and transformed import commands.
- Module workers, SharedWorkers, service workers, worklets, classic-script
  dynamic imports, preload/modulepreload, full Referrer Policy/Fetch or WPT
  conformance, remote CI, and cross-platform certification remain separate
  requirements; this slice must not claim complete dynamic-import conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-832.md`

## Verification

- `cargo check -p glass-browser --features native-engine --lib --tests --locked -q`
  passed. Output contains only existing dead-code warnings from the superseded
  HTML parser.
- `cargo test -p glass-browser --lib --features native-engine runtime_dynamic_import_rewrite_preserves_literal_and_computed_import_calls --locked --quiet`
  passed (1 passed, 1,645 filtered).
- `cargo test -p glass-browser --lib --features native-engine module_source_maps_preserve_referrer_policy_per_module_identity --locked --quiet`
  passed (1 passed, 1,646 filtered).
- The focused process-backed batch reached all five dynamic-module requests,
  and its actual `Referer` header assertions passed. The final runtime
  settlement assertion failed: the inline root's imported module and import
  promise remained `null`; the external root's dynamic and nested imports
  settled. The slice remains in progress until inline-module dynamic import
  execution and settlement pass.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Maintainer documentation gates passed: release-documentation truth (1,460
  Markdown documents, zero current-claim failures), documentation depth (93
  routed guides and 19 substantive contracts), TUI shortcut inventory (15
  implementation keys and 63 documentation markers), and documentation
  coverage (1,460 Markdown files, 346 MCP tools, 17 examples, 22 public
  modules).
- Remote CI, WPT conformance, and cross-platform certification are not part of
  local results and must not be claimed without evidence.
