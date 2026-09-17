# Native engine browser-complete slice 479: rooted file downloads

- Status: complete
- Scope: `native-engine` / download links in rooted file documents
- Issue: #40
- Depends on: [native-engine-browser-478](native-engine-browser-478.md)

## Objective

Make the existing native download queue complete for rooted file documents by
serving download-link bytes through the configured local file capability.

## Contract

- A download link owned by a rooted file document queues through the existing
  download ID/cancel/wait API and writes bounded bytes to the caller-authorized
  destination using the existing collision-safe writer and digest reporting.
- The source path is canonicalized through the configured allowed roots, so
  missing files, directories, symlink escapes, and out-of-root targets fail
  closed. The file download is bounded by the native download-byte limit.
- HTTP(S) download behavior remains fetch/navigation-owned and unchanged; no
  file-to-network fallback or page-script response exposure is introduced.

## Implementation

- Add a rooted-file branch to the download response owner before the HTTP(S)
  fetch path.
- Reuse `local_file_subresource_path`, bounded file reads, and the current
  parent-owned download writer.
- Add an end-to-end file-link download test and update issue documentation.

## Tradeoffs and remaining scope

This slice covers ordinary file download links only. Content-Disposition and
HTTP filename precedence, resumable/background downloads, file-origin service
workers, and complete download/Web IDL parity remain issue #40 gates.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_file_download_link` (1 passed)
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-479.json`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `git diff --check`
