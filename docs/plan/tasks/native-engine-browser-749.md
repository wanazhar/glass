---
id: native-engine-browser-749
scope: glass-browser/xhr-html-document-charset
status: complete
depends_on: [native-engine-browser-748]
---

# Glass native-engine browser slice 749: XHR HTML document byte decoding

## Context

- [Issue #40](https://github.com/wanazhar/glass/issues/40) remains the
  authority for native browser completion.
- [GCWP-0.1 XHR document responses](../native-engine-browser-profile.md#xhr-document-responses)
  requires HTML response documents to be parsed from bytes using a definite
  encoding, with scripting disabled.
- [XHR response-document algorithm](https://xhr.spec.whatwg.org/#document-response)
  defines response/override MIME charset precedence and requires HTML byte
  parsing without loading referenced resources.
- [Native-engine architecture](../../architecture/native-engine.md) owns the
  bounded parser and detached response-document boundary.

## Objective

Decode common explicitly selected HTML XHR document encodings before passing
the string to the existing bounded html5ever document parser. Keep this slice
limited to page-realm `responseType="document"` HTML responses; do not change
XML parsing, text `responseText`, Fetch, worker XHR, or navigation decoding.

## Contract

- Read the charset parameter from the response `Content-Type`; a charset in
  `overrideMimeType()` takes precedence over the response header. A recognized
  UTF-8 or UTF-16 byte-order mark selects its indicated encoding.
- Decode UTF-8 aliases, UTF-16LE/BE, and the common Windows-1252 label family,
  including the WHATWG Windows-1252 aliases `iso-8859-1`, `latin1`, and
  `us-ascii`. Decode malformed byte sequences with replacement characters.
- An absent or unsupported charset falls back to UTF-8 for this bounded slice.
  The existing response-byte limit remains authoritative; do not create an
  unbounded intermediate buffer.
- Apply the decoded text only to HTML `responseType="document"`. Preserve the
  detached, read-only document, scripting-disabled parse, response URL,
  resource inertness, XML route, and other XHR response-type behavior.
- This is not full HTML encoding conformance: HTML prescan/meta charset
  detection and the remaining WHATWG encoding labels stay open for issue #40.

## Tradeoffs

The implementation fixes common Western and Unicode HTML document responses
without adding a Rust dependency or moving the byte ownership boundary. The
small decoder duplicates a bounded subset of the navigation decoder; keeping
it local avoids broad transport changes, but a future shared byte-decoding
boundary should remove that duplication. UTF-8 fallback for unrecognized
labels and the absent HTML meta prescan are known parity gaps.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-749.md`

## Verification

- Run formatting and whitespace checks.
- Run `cargo check -p glass-browser --tests --locked --quiet` before tests.
- Run the focused process-backed HTML XHR document test. It must exercise
  response-header Windows-1252, `overrideMimeType()` overriding a conflicting
  response charset, and a UTF-16LE BOM overriding the response charset.
- Preserve the existing default HTML document, malformed-document recovery,
  XML response, and inert-resource/script assertions.
- Run current documentation truth/depth/shortcut/coverage gates when their
  required binaries are available. Do not claim issue #40 completion or full
  HTML encoding parity.

## Evidence

- `cargo fmt --all -- --check` and `git diff --check` passed.
- `cargo check -p glass-browser --tests --locked --quiet` passed.
- `cargo test -p glass-browser --test native_engine --locked --quiet
  native_content_process_xhr_exposes_bounded -- --nocapture` passed (2 tests),
  preserving process-backed XML behavior and covering HTML XHR documents with
  response-header Windows-1252, `overrideMimeType()` precedence over a
  conflicting UTF-8 header, and a UTF-16LE BOM overriding a Windows-1252
  header.
- The same HTML regression retains the default response, malformed recovery,
  detached/read-only document, response URL, inert script/resource, and parser
  behavior assertions.
- HTML meta prescan, remaining WHATWG encoding labels, full template
  `DocumentFragment` exposure, full HTML/XHR conformance, remote CI, and
  cross-platform certification remain open; issue #40 remains open.
- Current documentation gates pass: release truth covers 1,377 Markdown files
  with zero current-claim failures; depth covers 93 current guides/19
  contracts; shortcuts cover 15 implementation keys/63 markers; coverage
  includes 346 full-product MCP tools (101 browser-only), 17 examples, and
  22 public modules.
