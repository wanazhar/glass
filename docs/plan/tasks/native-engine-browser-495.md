# Native engine browser-complete slice 495: dedicated FontFace URL loading

- Status: complete
- Scope: page-realm script-created `FontFace` network source policy
- Issue: #40
- Depends on: [native-engine-browser-494](native-engine-browser-494.md)

## Objective

Make a script-created `FontFace` URL use the native font resource policy. A
dynamic face must not accidentally consume the generic page Fetch
`connect-src` contract when the document already has a dedicated bounded
`font-src` loader.

## Contract

- The page realm emits a private `font` destination only for the URL branch of
  `FontFace.load()`; ordinary page, worker, and Service Worker Fetch commands
  remain destination-less.
- The content owner accepts only the exact FontFace transport shape: GET,
  bodyless, no content type, no request headers, same-origin credentials, CORS
  mode, follow redirects, default cache mode, and no upload stream.
- A valid dynamic font request calls `NativeResourceLoader::load_font_async`,
  preserving document `font-src`, report-only diagnostics, mixed-content,
  redirect, CORS, cookies, cache, MIME, and 4 MiB byte-limit checks.
- A successful bounded byte result is projected as a synthetic successful Fetch
  response and continues through the existing page resolver, where the page
  emits the parser-validated `FontFaceInstall` command.
- Blocked or unavailable font resources reject the page FontFace promise and
  do not mutate the document font book.

## Implementation

- Add an optional, serde-defaulted `destination` field to the internal Fetch
  command and pass `font` through the private JavaScript FontFace call only.
- Reject non-page destinations in worker and Service Worker Fetch owners.
- Decode and validate the destination in the content-process page Fetch queue,
  then route `font` requests to `load_font_async` rather than the generic
  `open_fetch_response_stream_async`/`connect-src` path.
- Project the bounded result through the existing `__glassResolveFetch`
  response shape so the established FontFace source and install logic remains
  the single page-facing admission boundary.
- Add command-shape and runtime witnesses for the destination.

## Tradeoffs and remaining scope

This slice deliberately reuses the existing CSS font loader, so it restores
the correct font policy without duplicating redirect/CORS/cache/cookie code.
The synthetic response does not yet expose the redirect-final URL or original
font response headers, and this path does not yet run Service Worker
interception. Host acknowledgement after `FontFaceInstall` admission remains
necessary to prevent a page promise from settling before native parser and
aggregate-resource validation. ArrayBuffer sources, source lists,
`format()` descriptors, variation ranges, installed-font discovery,
font-display timing, variable/color tables, cross-realm projection, and full
text/Web IDL parity remain issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib --locked native_font_face_tests -- --nocapture`
- `cargo test --quiet -p glass-browser --lib --locked font -- --nocapture`
- `cargo test --quiet -p glass-browser --lib --locked fetch_commands -- --nocapture`
- `python3 scripts/check-release-documentation.py --require-previous-version`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`

The implementation remains local-only until the slice is committed and
explicitly authorized for remote publication.

Observed results: the scoped library check passed; the focused JavaScript
FontFace group passed 5 tests; the focused destination group passed 1 test;
the file-backed content-process FontFace loop passed 1 test; the broader font
group passed 30 tests; formatting, diff, release-documentation, depth,
shortcut, and coverage gates passed. Scoped Clippy remains red on the
repository's existing 55-warning `-D warnings` baseline outside this slice;
no new warning was attributed to the destination implementation. This slice
is committed locally only and has not been pushed, run in remote CI, released,
tagged, or published.
