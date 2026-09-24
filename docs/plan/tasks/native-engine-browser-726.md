id: native-engine-browser-726
scope: glass-browser/security/rooted-file-subresource-csp
status: done
depends-on: [native-engine-browser-725]
---

# Glass native-engine browser slice 726: rooted-file image, font, and media CSP

## Objective

Enforce configured-root file-document CSP for image, font, and media resources
before their bytes are read, using the applicable directive fallback and the
same configured-root semantics already used by rooted-file script and style
loads.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-722.md` through
  `docs/plan/tasks/native-engine-browser-725.md`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- For a configured-root file Document, `img-src`, `font-src`, and `media-src`
  govern matching `file:` subresources; each directive falls back to
  `default-src` when absent.
- `'self'` authorizes resources admitted by the Document's most-specific
  configured file root, not an unrelated configured root. Explicit file
  sources remain bounded by normal configured-root admission.
- Every enforced policy is conjunctive. A later runtime-inserted head policy
  reaches dynamic resource checks through the append-only Document policy
  ledger.
- Reject disallowed resource URLs before reading their file bytes. Preserve
  current HTTP(S) CSP behavior and resource size/MIME limits.
- This slice covers rooted `file:` image/font/media resources. File-origin
  data/blob handling, other CSP resource classes, report-only file policy, and
  full CSP conformance remain separate issue #40 work; they are not completion
  claims for the engine.

## Path

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-726.md`

## Verification

- Add loader coverage for same-root admission, cross-root rejection, each
  resource directive, and `default-src` fallback.
- Add a rooted-file engine regression in which a head CSP meta policy is
  inserted and an image is attached in the same JavaScript evaluation; prove
  the disallowed image is not rendered/read.
- Run one locked `glass-browser` check before the focused loader and engine
  tests, then formatting, whitespace, and maintainer documentation gates.
- Do not run workspace-wide tests or remote CI for this slice.

## Results

The shared CSP policy path now applies rooted-file `img-src`, `font-src`, and
`media-src` fallback chains to file-backed resources before reading their
bytes. `'self'` is limited to the Document's most-specific configured root;
explicit `file:` sources cannot bypass configured-root admission, and
policies remain conjunctive. A process-backed regression confirms that a head
policy and image inserted in one evaluation results in an image error and no
image display command.

`cargo check -p glass-browser --lib --test native_engine --locked --quiet`
passed. The focused loader matrix and same-evaluation engine test each passed
1/1. `cargo fmt --all -- --check` and `git diff --check` passed. The release-
documentation truth, documentation depth, and shortcut inventory checks
passed. The live coverage/link check was not run because `target/debug/glass`
is absent. Remote CI was not run. Issue #40 remains open; file-origin
data/blob handling, other CSP resource classes, report-only file policy, and
full CSP conformance remain unfinished.
