---
id: native-engine-browser-712
scope: glass-browser/javascript/module-prefetch-discovery
status: done
depends-on: [native-engine-browser-711]
---

# Objective

Keep the bounded dynamic-module prefetch pass aligned with actual JavaScript
`import()` calls. A property method named `import`, including optional-chain
access and intervening comments/whitespace, must not be treated as an
`ImportCall` or cause a module request. Preserve static-string folding,
ordering, and the existing graph/resource limits.

Correct the current roadmap's worker-import-map item: the HTML Standard
processes import maps in a `Document`; worker module graphs use their worker
module map and URL resolution, not a `Document` import map. Do not add
non-standard worker import-map behavior to the Glass Core Web Profile.

This does not implement runtime-valued dynamic imports or complete module
scheduling. Those remain issue #40 work and must use runtime information rather
than expanding the static prefetch scanner into a guessed JavaScript evaluator.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-711.md`
- HTML Standard: [Import maps](https://html.spec.whatwg.org/multipage/webappapis.html#import-maps)
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- `module.import("./x.js")`, `module?.import("./x.js")`, and member access
  separated from `import` by comments/whitespace do not add `./x.js` to the
  prefetched graph.
- A real `import("./x.js")` retains existing discovery behavior.
- The scanner remains a bounded prefetch-discovery pass, not a replacement for
  the JavaScript parser and not a runtime module loader.
- Worker module graphs retain URL/referrer/base identity. Document import maps
  are not a worker capability in the declared standards profile.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-712.md`

## Verification

- `cargo check -p glass-browser --lib --tests --locked --quiet` passed.
- `cargo test -p glass-browser --lib --locked --quiet
  native_static_dynamic_import_tests` passed 3/3 tests, covering static
  concatenation, refusal to partially accept runtime expressions, and
  rejection of ordinary/optional property methods named `import` with trivia
  around member access.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- All four maintainer documentation gates passed: release truth covered 1,340
  Markdown files with zero current-claim failures; depth covered 93 guides and
  19 contracts; shortcut inventory covered 15 keys and 63 markers; coverage
  validated 346 MCP tools (101 browser-only), 17 examples, and 22 public
  modules.
- Remote CI was not run; this is local-only evidence.
