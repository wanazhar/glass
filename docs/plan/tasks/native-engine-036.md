---
id: native-engine-036
scope: glass-browser/native-engine/base64-data-url
status: done
depends-on: [native-engine-035]
---

# Native bounded base64 `data:text/html` loading

## Objective

Extend the existing local-only native resource loader with bounded standard
base64 `data:text/html` payloads, preserving the real navigation, scheduler,
document-revision, semantic-evidence, and dispatcher call chain.

## Contract

The loader accepts a `data:` URL whose media type is exactly `text/html`
(case-insensitive) and whose metadata contains the `base64` flag. The
payload uses the existing base64 crate's standard RFC 4648 alphabet and
canonical padding rules; it must decode to valid UTF-8 HTML. The decoded body
must not exceed `NativeEngineLimits::max_document_bytes`, and the encoded
payload is rejected against a derived pre-decode bound so an oversized input
cannot allocate an unbounded decoded buffer.

Successful base64 data URLs retain their original URL for navigation evidence,
use the same opaque local origin as percent-encoded data URLs, and enter the
existing `NativeEngine::navigate` and dispatcher path. They do not add network,
filesystem, script, storage, or external resource behavior. Existing
percent-decoded non-base64 `data:text/html` URLs remain unchanged.

Non-HTML media types, missing payload commas, invalid base64, invalid UTF-8,
percent-encoded base64 payloads, arbitrary metadata/data URL modes, and
non-local URL schemes remain rejected with bounded typed errors. Decode errors
must not echo payload contents or secrets.

## Tradeoffs

- Base64 data URLs make self-contained local fixtures easier to transport and
  exercise the real navigation path without adding a network dependency.
- The standard padded alphabet is deliberately narrower than every browser
  data-URL interpretation; URL-safe alphabets, whitespace-tolerant decoding,
  percent-encoded base64, and alternate charset conversion are excluded to
  keep the contract deterministic.
- A derived encoded-length bound plus a decoded-length check limits allocation,
  but the loader still performs a synchronous bounded decode; streaming and
  cancellation remain later resource-loading work.
- The opaque origin and no-subresource policy preserve the existing local
  security boundary; this does not make arbitrary remote content safe.

## Path

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

- successful base64 data navigation through the real backend dispatcher;
- invalid base64, invalid UTF-8, non-HTML, percent-encoded-payload, and
  decoded/encoded size-limit rejection;
- native integration and unit suites;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- native-feature doctests and documentation coverage, depth, release-truth,
  version-sync, and feature-parity validators; and
- `cargo fmt --all -- --check`, `git diff --check`, and one focused
  Conventional Commit before the next slice.

## Completion evidence

Implemented in the existing local resource loader and public engine navigation
path. Standard padded base64 `data:text/html` is bounded before decode and
after decode, valid UTF-8 is required, the original URL and opaque origin are
retained, and errors are typed without raw payload echo.

Verified locally on 2026-08-31:

- `cargo fmt --all -- --check` and `git diff --check` passed;
- focused base64 data navigation/limits test: 1 passed;
- native integration suite: 48 passed;
- native unit suite: 36 passed;
- strict Clippy for default/no-default and `native-engine` feature targets
  passed with warnings denied;
- locked all-target/all-feature `glass-browser` matrix: 819 library tests ran,
  with 818 passed and 1 ignored, and all integration and example targets
  green;
- native-feature Rust doctests: 4 passed; and
- documentation coverage, depth, release-truth, version-sync, and feature
  parity validators passed: 450 Markdown documents, 83 current documents,
  345 full-product MCP tools, 17 examples, 22 public modules, 19 substantive
  contracts, 536 semantic-audit hits, and 0 current-claim failures.
