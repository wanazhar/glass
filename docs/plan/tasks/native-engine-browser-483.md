# Native engine browser-complete slice 483: redirected stylesheet URL bases

- Status: complete
- Scope: `native-engine` / HTTP(S) stylesheet redirect URL ownership
- Issue: #40
- Depends on: [native-engine-browser-482](native-engine-browser-482.md)

## Objective

Preserve the final HTTP(S) URL of a redirected stylesheet so relative CSS
resources use the response stylesheet's actual URL as their base.

## Contract

- A stylesheet loaded through one or more HTTP(S) redirects resolves relative
  CSS `url(...)` tokens against the final response URL.
- Fresh cache hits and 304 revalidation preserve the same final stylesheet URL
  used by the original response.
- The raw link `href` remains the identity used to detect dynamic link changes;
  the resolved stylesheet URL is separate resource-base state.
- Existing redirect, mixed-content, CSP, integrity, MIME, byte-limit, cache,
  decoder, paint, and event owners remain authoritative.

## Implementation

- Return a bounded `{ url, body }` stylesheet resource from the HTTP(S) loader.
- Populate the resolved stylesheet URL from the final redirect hop, cache entry,
  304 path, or blob target without changing the body owner.
- Store the resolved URL separately from raw `href` in `NativeDocument` external
  stylesheet state and use it during initial and dynamic CSS rebuilds.
- Add an HTTP regression fixture with a redirected nested stylesheet and a
  relative image whose request path proves final-URL resolution.

## Tradeoffs and remaining scope

The resolved URL is carried inside the content process, where stylesheet body
and dynamic mutation state already live; no new IPC field is required. This
slice does not fetch network CSS `@import` dependencies, and blob-relative CSS
resource loading remains on its existing object-URL path. File fonts and other
CSS resource types, complete file-origin semantics, and full Web IDL parity
remain issue #40 work.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_content_process_uses_redirected_network_stylesheet_url_as_css_base`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_file_`
- documentation truth/depth/shortcut/coverage audits
- `git diff --check`
