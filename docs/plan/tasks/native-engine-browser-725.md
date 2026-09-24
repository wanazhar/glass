---
id: native-engine-browser-725
scope: glass-browser/security/rooted-file-runtime-csp-meta
status: done
depends-on: [native-engine-browser-724]
---

# Glass native-engine browser slice 725: rooted-file runtime CSP meta insertion

## Objective

Apply runtime-inserted CSP meta policies to rooted-file Documents before
subsequent inline script execution and resource processing, including a meta
insertion and later script within one JavaScript evaluation.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-722.md`
- `docs/plan/tasks/native-engine-browser-723.md`
- `docs/plan/tasks/native-engine-browser-724.md`
- [CSP Level 3: the `<meta>` element](https://www.w3.org/TR/CSP/#meta-element)
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- On a configured-root file Document, process each non-empty
  `Content-Security-Policy` meta value captured when its element becomes
  connected under the current `head`. Append it to the Document's enforced
  policy container before later dynamic scripts and resources are checked.
- Classic inline scripts run synchronously inside the JavaScript host view.
  The runtime policy callback must capture a connected head policy before a
  later inline script can execute in the same evaluation; the Document ledger
  synchronizes the captured policy to the resource loader before later
  external resources are processed.
- The policy is additive and conjunctive with parser policies. The processed
  value is immutable: later `content` edits and element removal cannot relax
  it. A head policy captured before removal in the same DOM command batch must
  still be applied.
- A CSP meta element outside `head` does not install policy. Meta-delivered
  report-only policy remains unsupported and ignored.
- Preserve policy-count limits, configured file-root admission, the existing
  HTTP(S) process-backed path, and same-URL navigation policy replacement.
- Do not make CSP apply retroactively to already processed resources; this
  slice gates subsequent dynamic script/resource checks.

## Path

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-725.md`

## Verification

- Add configured-root file engine tests proving a connected head policy
  blocks a later dynamic inline script and external script in the same
  evaluation, content edits/removal do not relax it, and a body-level CSP meta
  does not affect later scripts.
- Preserve current rooted-file parser CSP and inline-style CSP regressions and
  the HTTP(S) dynamically inserted meta CSP regression.
- Run one scoped `glass-browser` check before focused tests, then formatting,
  whitespace, and maintainer documentation gates. Do not run workspace-wide
  tests or remote CI for this slice.

## Results

The JavaScript host view now sends connected head CSP meta values to the
runtime's append-only inline policy before it executes a later classic inline
script, including when the meta is edited or removed later in that same
evaluation. The direct engine synchronizes the Document's captured policy
ledger to the resource loader before processing later dynamic resources and
script batches. The per-node ledger and policy-count limit remain in force;
body-level meta remains inert.

`cargo check -p glass-browser --lib --test native_engine --locked --quiet`
passed. The focused same-turn rooted-file CSP test, rooted-file parser CSP,
rooted-file inline-style CSP, and process-backed HTTP(S) dynamic-meta CSP tests
each passed. `cargo fmt --all -- --check`, `git diff --check`, release-document
truth (1,353 Markdown files, zero current-claim failures), documentation depth
(93 guides/19 contracts), and shortcut inventory (15 keys/63 markers) passed.
Full documentation link/CLI inventory was not run because `target/debug/glass`
is absent; no unrelated development binary was built. Remote CI was not run,
and issue #40 remains open.
