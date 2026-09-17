# Native engine browser-complete slice 473: allowed file URLs and media

- Status: complete locally
- Scope: `native-engine` / explicitly rooted `file:` documents and media
- Issue: #40
- Depends on: [native-engine-browser-472](native-engine-browser-472.md)

## Objective

Give the native engine a useful local-file browser path while preserving the
content-process security boundary. A caller must explicitly configure one or
more allowed directory roots; top-level HTML and media files are bounded,
canonicalized, and rejected when they escape those roots.

## Contract

- `NativeEngineConfig::with_allowed_file_root` adds an absolute, UTF-8,
  existing directory root. The root list is bounded and transferred to the
  isolated content process as policy data.
- `file:` URLs accept an empty host or `localhost`, reject credentials, and
  resolve only to regular files whose canonical path is below a configured
  root. Missing roots, directories, symlink escapes, and unconfigured files
  fail closed.
- Top-level file documents use the existing bounded HTML decoder and document
  byte limit. Relative file navigation retains the file URL base.
- File media uses the existing 16 MiB metadata/sniffing and WAV-duration
  owner. Static and dynamic media on file documents share the existing
  readiness, event, and playback timeline state.
- A network document cannot use this slice to read a local file. The native
  loader does not add a filesystem or CDP fallback for any denied target.

## Implementation

- Added explicit root configuration, validation, IPC transfer, and Linux/macOS
  sandbox read-only bindings.
- Added bounded file document loading and rooted file media admission to the
  inline loader and content-process loader.
- Added relative file navigation and static/dynamic file-media integration
  tests, plus a loader-level outside-root denial witness.

## Tradeoffs and remaining scope

This slice intentionally does not claim file-backed script, stylesheet, image,
font, worker, download, or complete file-origin/Web IDL parity. It also keeps
file documents on the native opaque-origin model until the origin/security
workstream defines the complete file-origin contract. No broad host filesystem
access is implied by enabling the feature.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked` (passed)
- `cargo test --quiet -p glass-browser --test native_engine --locked file` (3
  file document/media tests passed)
- `cargo test --quiet -p glass-browser --lib --locked rooted_file_loader` (1
  loader contract test passed)
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-473.json` (current-claim failures=0)
- `python3 scripts/check-documentation-depth.py` (93 current guides routed/audited)
- `python3 scripts/check-tui-shortcuts.py` (15 implementation help keys)
- `git diff --check`
