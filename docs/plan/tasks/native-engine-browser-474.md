# Native engine browser-complete slice 474: rooted file subresources

- Status: complete locally
- Scope: `native-engine` / rooted `file:` scripts, stylesheets, and images
- Issue: #40
- Depends on: [native-engine-browser-473](native-engine-browser-473.md)

## Objective

Make an explicitly rooted local HTML document useful as an actual page by
admitting its file-backed JavaScript, CSS, and image subresources through the
same bounded native resource owners. A rooted page must never turn this
capability into broad filesystem or network-to-file access.

## Contract

- A file document may resolve relative or absolute `file:` script,
  stylesheet, and image URLs only when their canonical regular-file target is
  below one of the configured allowed roots.
- File text resources are bounded, UTF-8 decoded, and integrity checked when
  metadata is present. File scripts preserve the native page-script ordering
  and load/error event path; dynamic script attachment uses the same loader.
- File CSS is admitted before layout and retained in the document's external
  stylesheet state. File images use the existing bounded decoder and paint
  resource state for `<img>` and CSS background consumers.
- Network, `data:`, `blob:`, path traversal, symlink escape, missing files,
  directories, credentials, and unconfigured roots fail closed. No CDP or
  network fallback is added.

## Implementation

- Add bounded rooted file script, stylesheet, and image loader methods.
- Load static file CSS/images before the local navigation commit and execute
  static file scripts through the existing page-script scheduler.
- Extend local dynamic stylesheet/image/script handling to recognize rooted
  file targets while preserving Blob behavior and event ownership.
- Add integration and loader-level tests for successful resources and root
  denial, then synchronize the architecture and plan records.

## Tradeoffs and remaining scope

This slice deliberately does not claim file-backed fonts, workers, downloads,
CSS `url()` base-URL parity, module dependency graphs, complete file-origin
semantics, or unrestricted local networking. Those remain explicit issue #40
gates. Static file resource reads are synchronous in the inline local owner;
this keeps the existing local navigation contract but is not a general
filesystem task scheduler.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked` (passed)
- `cargo test --quiet -p glass-browser --test native_engine --locked file` (17
  matching file/resource tests passed)
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-474.json`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `git diff --check`
