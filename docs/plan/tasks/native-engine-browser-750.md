---
id: native-engine-browser-750
scope: glass-browser/xhr-html-document-charset
status: complete
depends-on: [native-engine-browser-749]
---

# Glass native-engine browser slice 750: standards-based XHR HTML decoding

## Context

- [Issue #40](https://github.com/wanazhar/glass/issues/40) remains the
  authority for native browser completion.
- [GCWP-0.1 XHR document responses](../native-engine-browser-profile.md#xhr-document-responses)
  requires an HTML response document to be parsed from bytes using the XHR
  final encoding, then HTML's bounded encoding prescan when that encoding is
  unavailable.
- The [XHR Standard](https://xhr.spec.whatwg.org/#document-response) selects
  response/override MIME charset labels, prescans the first 1,024 response
  bytes if the selected label is absent or invalid, and otherwise falls back
  to UTF-8.
- The [HTML Standard prescan](https://html.spec.whatwg.org/multipage/parsing.html#prescan-a-byte-stream-to-determine-its-encoding)
  defines `meta charset`, pragma-gated `content` metadata, UTF-16 XML prefixes,
  and XML declaration fallback. `encoding_rs` implements WHATWG label lookup
  and replacement decoding.
- [Native-engine architecture](../../architecture/native-engine.md) owns the
  bounded byte-to-parser boundary. XML and text XHR paths remain separate.

## Objective

Replace the page-realm XHR HTML document's handwritten charset subset with a
bounded Rust decoder that applies WHATWG encoding labels and the HTML
first-1,024-byte prescan before calling the existing detached HTML parser.

## Contract

- A valid response MIME `charset` label is selected first; a present override
  MIME `charset` label replaces it. If the resulting label is absent or
  unsupported, use the HTML byte prescan. If that finds no supported encoding,
  use UTF-8.
- A byte-order mark overrides the selected fallback encoding and is removed
  from decoded text. Malformed input is replacement-decoded.
- The prescan is limited to the first 1,024 response bytes. It honors
  first-duplicate-attribute behavior, direct `charset`, pragma-gated `content`
  encoding, UTF-16 XML prefixes, XML declaration fallback, the HTML-specific
  UTF-16-to-UTF-8 adjustment, and `x-user-defined` to Windows-1252 adjustment.
- Use the WHATWG-supported encoding label table; prohibited encodings remain
  unsupported. Preserve the existing raw response byte cap and bound decoded
  text to at most three times that cap before parsing.
- Apply decoding only to Window `responseType="document"` HTML responses.
  Preserve XHR override behavior, XML parsing, text response decoding,
  detached/read-only response documents, inert scripts/resources, and URL/MIME
  metadata.
- The new decoder borrows the existing JavaScript `Uint8Array` input; the
  standards encoding crate is optional and enabled only with `native-engine`.
  This adds a bounded dependency/build cost in exchange for deleting a
  hand-maintained and incomplete codec table.
- This slice does not close HTML parsing, XHR, encoding WPT, cross-platform, or
  issue #40 completion gates.

## Paths

- `crates/glass-browser/Cargo.toml`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/src/browser/native_engine/html_encoding.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `Cargo.lock`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-750.md`

## Verification

- Run `cargo fmt --all -- --check` and `git diff --check`.
- After updating `Cargo.lock`, run one
  `cargo check -p glass-browser --tests --locked --quiet` to type-check the
  changed test target. Then run focused
  `cargo test -p glass-browser --test native_engine --locked --quiet
  native_content_process_xhr_exposes_bounded -- --nocapture`.
- Cover a valid non-Western header label, invalid-header fallback to both
  direct meta charset and pragma-gated `content`, HTTP/override precedence,
  BOM precedence, first duplicate attribute, first-1,024-byte boundary,
  XML response preservation, and existing detached/inert-document assertions.
- Run release-documentation truth, depth, shortcut, and inventory/coverage
  checks. Do not report full encoding conformance or issue #40 completion.

## Evidence

Verification passed:

- `cargo fmt --all -- --check` and `git diff --check`.
- `cargo check -p glass-browser --tests --locked --quiet` (pre-existing
  dead-code warnings remain for the retired handwritten parser helpers).
- `cargo test -p glass-browser --lib --locked --quiet html_encoding::tests::`
  passed 9 decoder unit tests.
- `cargo test -p glass-browser --test native_engine --locked --quiet
  native_content_process_xhr_exposes_bounded -- --nocapture` passed both
  process-backed XML and HTML XHR document tests. Coverage includes valid
  response/override charsets, UTF-16 BOM override, Shift_JIS response and meta
  labels, GBK pragma metadata, header-over-meta priority, malformed recovery,
  read-only/detached results, and inert resources/scripts.
- Release truth passed for 1,378 Markdown files, 83 current documents, and
  zero current-claim failures (63 previous-version hits, 1,546 semantic audit
  hits). Documentation depth passed for 93 guides and 19 contracts; shortcut
  inventory passed for 15 implementation keys and 63 markers; coverage
  passed for 346 full-product MCP tools (101 browser-only), 17 examples, and
  22 public modules. Remote CI, full Encoding/XHR WPT, cross-platform
  certification, and issue #40 completion remain open.
