# Glass native engine browser slice 261: Request FormData parsing

Status: completed locally.

## Objective

Close the bounded `Request.formData()` gap for the two ordinary form-body
encodings already produced by the native Fetch bridge: URL-encoded fields and
multipart fields/files.

## Contract

- `Request.formData()` consumes the Request body once through the existing
  ownership gate and returns a native `FormData` object.
- `application/x-www-form-urlencoded` bodies decode UTF-8 fields, `+` spaces,
  percent escapes, and repeated names in order.
- `multipart/form-data` bodies require a bounded boundary, validate the
  `form-data` disposition, preserve field order, and return text fields plus
  byte-preserving `File` values with bounded filename/type metadata.
- Body sizes, field-name sizes, entry counts, and existing FormData/File
  limits remain authoritative. Unsupported media types and malformed
  multipart framing reject with typed errors.
- Caller-supplied streaming uploads, multipart edge-case conformance,
  complete Fetch Streams/Web IDL semantics, and unrestricted FormData parity
  remain open.

## Implementation

The native Request body helper now selects the declared or captured media
type, decodes URL-encoded bodies through the bounded URLSearchParams owner,
and parses multipart boundaries over a Latin-1 framing view while slicing
file bytes from the original payload. Parsed files are rebuilt through the
existing File owner and retain raw bytes for later Blob/File reads. Body
consumer transforms now convert synchronous parser/JSON failures into rejected
Promises.

The integration witness checks repeated URL-encoded fields, multipart text and
binary File values, filename/type/size preservation, and explicit rejection of
an unsupported raw body media type.

## Tradeoffs and follow-up

The parser intentionally covers the bounded encodings used by this native
bridge. It does not claim full MIME parameter decoding, arbitrary malformed
multipart recovery, streaming multipart parsing, or browser-complete
`FormData` Web IDL behavior. The byte-slice approach keeps binary uploads
lossless without making the content process retain a second unbounded copy.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_parses_request_form_data --locked -- --nocapture --exact` (1 passed)
- `git diff --check`

The evidence is local-only. Remote CI, push, release, registry publication,
and final native/CDP parity claims remain pending the wider issue #40 gates.
