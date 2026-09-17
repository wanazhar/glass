# Native engine browser-complete slice 482: network stylesheet URL bases

- Status: complete
- Scope: `native-engine` / direct HTTP(S) stylesheet-relative CSS URLs
- Issue: #40
- Depends on: [native-engine-browser-481](native-engine-browser-481.md)

## Objective

Make CSS resource discovery use the loaded stylesheet URL as the base for
direct HTTP(S) stylesheets, matching the already-established rooted-file
stylesheet behavior.

## Contract

- Relative CSS `url(...)` tokens in a direct HTTP(S) stylesheet resolve against
  that stylesheet's URL, not the owning document URL.
- Absolute HTTP(S) CSS URLs remain network-owned; rooted file stylesheets keep
  file-owned resolution and credential-bearing/cross-scheme owners fail closed.
- Data, blob, fixture, and unsupported stylesheet owners are not rewritten by
  this helper and retain their existing resource-specific handling.
- The canonicalized source feeds the existing CSS cascade, background-image
  identity, policy, cache, decoder, and paint owners without changing their
  limits or event semantics.

## Implementation

- Generalize the stylesheet URL canonicalizer from file-only to paired file and
  HTTP(S) stylesheet owners.
- Preserve quote/escape/comment handling and reject credential-bearing or
  cross-scheme targets before rewriting.
- Route initial network stylesheet parsing, rooted-file parsing, dynamic
  stylesheet rebuilds, and recursive file stylesheet expansion through the
  shared helper.
- Add direct unit coverage for network/file/embedded-owner behavior and an
  HTTP content-process regression fixture whose image path proves the request
  used the nested stylesheet directory.

## Tradeoffs and remaining scope

The loader still returns only stylesheet text to the document owner, so a
stylesheet redirect's final URL cannot yet become the CSS base; direct
non-redirected links are covered and redirect tracking remains a separate
transport/state slice. Network CSS `@import` fetching, file fonts and other CSS
resource types, complete file-origin semantics, and full Web IDL parity remain
issue #40 work. Keeping embedded and unsupported schemes unchanged avoids
turning this canonicalizer into a second blob/data policy owner.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --lib --locked stylesheet_urls_use_the_loaded`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_content_process_resolves_network_css_urls_against_stylesheet`
- documentation truth/depth/shortcut/coverage audits
- `git diff --check`
