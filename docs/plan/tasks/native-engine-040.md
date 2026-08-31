---
id: native-engine-040
scope: glass-browser/native-engine/bounded-relative-local-links
status: done
depends-on: [native-engine-039]
---

# Native bounded fixture-relative link resolution

## Objective

Extend the existing semantic local-link default action to resolve bounded
relative references against the current registered `fixture://` resource,
without turning the native engine into a network, filesystem, or general URL
loader.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/config.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

For a semantic `<a href>` click, empty hrefs remain click-only and fragment-
only hrefs retain the existing same-resource behavior. Absolute URLs continue
to use the existing local resource-loader boundary. A non-absolute relative
reference is resolved with the existing `url` parser against the current
resource URL only when that resource is a registered `fixture://` document.
The resolved URL must retain the `fixture` scheme and the current host; path,
query, and raw fragment resolution may use the parser's bounded normalized
result. Scheme-relative host changes, malformed references, relative links from
`about:blank` or `data:` documents, remote schemes, and filesystem or other
unsupported destinations fail before click, document, URL, revision, scroll,
or history mutation. Errors do not echo the href or resolved destination.

The existing loader still decides whether the resolved fixture URL is
registered and whether its body parses. Same-resource references reuse the
current document and exact fragment-target scroll rule; different-resource
references parse before commit and apply the existing bounded fragment target
and history-scroll behavior. Link activation remains semantic, local, and
dispatcher-reachable; no transport-level navigation operation is added.

## Tradeoffs

- Resolving only registered fixture bases gives useful relative-link coverage
  without inventing a general origin/base-URL policy for opaque data documents.
- `url::Url` normalization makes dot-segment handling deterministic, but this
  slice does not add percent-decoding policy, redirects, credentials, ports,
  network access, or browser URL-parsing parity.
- Same-host enforcement prevents a relative reference from becoming a hidden
  cross-origin fixture lookup; absolute local URLs retain their existing
  explicit-loader behavior.
- Preflight preserves the existing failure-atomicity guarantee, at the cost of
  loading/validating the destination before the source link's click mutation.

## Path

- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/config.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, feature, SDK, README, and
  plan docs

## Verification

- fragment-only and absolute local link behavior remains green;
- fixture-relative path, dot-segment, query, and fragment links resolve through
  the real dispatcher action path;
- relative links from opaque/non-fixture resources, host-changing references,
  missing fixtures, malformed references, and remote schemes fail before
  mutation without destination echo;
- same-resource anchor scrolling and different-resource parse-before-commit
  history/scroll restoration remain green;
- native integration and unit suites;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature and workspace checks;
- native-feature doctests and documentation coverage, depth, release-truth,
  version-sync, and feature-parity validators; and
- `cargo fmt --all -- --check` and `git diff --check`.

## Completion evidence

The implementation and synchronized docs are complete. The focused dispatcher
and direct-engine coverage passed for registered fixture-relative path,
dot-segment, query, and fragment references; missing fixtures, host-changing
references, malformed authorities, and relative links from a data URL remain
failure-atomic without destination echo. Fragment-only, absolute local, and
empty-href behavior remain green.

The native-engine module unit suite passed 38/38 tests (including 2 new
URL-policy tests); the native integration suite passed 52/52 tests. Strict
Clippy passed for `--all-features` and
`--no-default-features`. Native-feature doctests passed 4/4. The locked
all-target/all-feature `glass-browser` matrix passed 821 library tests (820
passed, 1 ignored), 52 native integration tests, all browser-smoke, daemon,
protocol, public-API, reliability, workspace-contract, TUI, and example
targets. The stack-guarded workspace check passed with
`RUST_MIN_STACK=8388608`: 821 `glass-browser` unit tests, 365 `glass-dev`
unit tests, 4 development integration tests, and 15 PTY tests.

Documentation coverage reports 454 Markdown files, 345 full-product MCP
tools, 17 examples, and 22 public modules; documentation depth reports 93
current guides and 19 substantive contracts; release truth reports zero
current-claim failures; version sync is 0.3.14; and feature parity reports 14
capabilities across 4 targets. `cargo fmt --all -- --check` and
`git diff --check` passed. Issue #40 is the authoritative remote checkpoint
for the commit and remaining epic work.
