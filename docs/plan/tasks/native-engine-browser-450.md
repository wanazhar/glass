# Native engine browser-complete slice 450: page URL setter normalization

status: complete
scope: native-engine/page-url-setters
issue: 40
depends-on: [native-engine-browser-449]

## Objective

Remove the remaining page-side URL setter drift. Page URL pathname and
fragment writes must use the same bounded canonical URL owner as initial URL
construction and worker URL mutation.

## Normative reference

The setter serialization boundary follows the [WHATWG URL
Standard](https://url.spec.whatwg.org/). The page's existing URL object,
`URLSearchParams`, navigation, and Fetch/XHR owners retain their current
bounded Glass contracts.

## Contract

- Assigning `URL.pathname` escapes path delimiters that are data, preserves
  path separators, removes dot segments, and exposes the canonical path while
  retaining the current query and fragment.
- Assigning `URL.hash` canonicalizes fragment text, including spaces and other
  URL characters, without changing the path or query. Empty assignments clear
  the fragment.
- Setter normalization remains synchronous and bounded; a rejected canonical
  URL operation does not replace the prior URL state. The page URL's existing
  live `searchParams` synchronization remains intact.
- Page and worker URL objects now use the same Rust-backed initial and setter
  normalization boundary. This slice does not claim complete URL Web IDL
  descriptor or all setter edge-case parity.

## Implementation

- Removed the page-only hand-written pathname normalizer from the bootstrap.
- Routed page pathname and hash setters through the already installed Rust URL
  canonicalizer with an explicit undefined base for absolute setter inputs.
- Added a page witness that verifies dot-segment removal, path-space escaping,
  fragment-space escaping, and query preservation after mutation.

## Tradeoffs

- The page setter now follows the same canonical serialization as navigation,
  Fetch, XHR, and workers, at the cost of a small synchronous host parse for
  each pathname or hash mutation.
- The setter still lives in the page JavaScript projection so URL object state
  and `URLSearchParams` identity remain realm-local; a later Web IDL parity
  layer can consolidate the duplicated page/worker projection code.
- Complete descriptor identity, blob/file origins, unusual scheme setters, and
  the full browser URL scheme matrix remain issue #40 work.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_local_url_resolution_uses_canonical_url_parser --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine url --locked -- --test-threads=1 --nocapture` (8 passed, 0 failed)
- `git diff --check`
