# Native engine browser-complete slice 476: rooted file CSS URL bases

- Status: complete
- Scope: `native-engine` / relative `url(...)` tokens in rooted file CSS
- Issue: #40
- Depends on: [native-engine-browser-475](native-engine-browser-475.md)

## Objective

Make a loaded rooted file stylesheet behave as a stylesheet resource rather
than as document text: relative CSS resource URLs must resolve against that
stylesheet's own canonical file URL for initial and dynamic attachments.

## Contract

- Relative CSS `url(...)` tokens in rooted file stylesheets resolve against
  the stylesheet URL, including URLs used by supported background-image
  declarations and the existing native paint/resource path.
- Absolute `file:` URLs retain canonical file syntax and continue through the
  configured allowed-file-root, credential, symlink, MIME, byte, and decode
  checks. Network/data/blob and non-file stylesheet behavior remain on their
  existing owners.
- Initial and dynamically attached rooted file stylesheets use the same
  canonicalization. CSS comments and quoted non-URL text are not rewritten;
  unresolved or unsupported schemes remain fail-closed at their resource
  owner.

## Implementation

- Add a bounded CSS URL-token canonicalization pass for loaded rooted file
  stylesheets before the existing CSS parser creates source identities.
- Apply it to initial navigation and dynamic stylesheet rebuilds in the inline
  engine and content-process path.
- Add nested-directory integration coverage for static and dynamic stylesheet
  background images, then update architecture and plan records.

## Tradeoffs and remaining scope

This is a focused URL-base owner, not a complete CSS loader: it preserves the
existing bounded parser and only canonicalizes URL functions needed by the
current CSS/resource surface. CSS `@import`, file fonts, other CSS resource
types, URL escape grammar, network stylesheet URL-base parity, complete
file-origin semantics, and remaining Web IDL parity stay open under issue #40.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_file_`
  (6 passed)
- `python3 scripts/check-release-documentation.py --require-previous-version
  --report /tmp/glass-release-documentation-476.json`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `git diff --check`
