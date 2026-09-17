# Native engine browser-complete slice 484: network CSS import graphs

- Status: complete
- Scope: `native-engine` / direct HTTP(S) CSS `@import` dependencies
- Issue: #40
- Depends on: [native-engine-browser-483](native-engine-browser-483.md)

## Objective

Load bounded recursive network CSS imports so direct HTTP(S) stylesheets can
compose real multi-file style graphs before native cascade and resource
discovery.

## Contract

- Literal quoted and `url(...)` network `@import` targets load recursively at
  their source positions for initial and dynamic stylesheet attachment.
- Supported viewport/media, Supports, and layer conditions are evaluated
  before resolving or requesting a dependency; inactive imports remain inert.
- Each imported response uses the existing HTTP(S) redirect, CSP, mixed-content,
  integrity, MIME, cache, cookie, timeout, and byte-stream owners.
- Every admitted dependency keeps its final stylesheet URL as the base for
  nested imports and CSS `url(...)` resources.
- Duplicate and cyclic targets are suppressed within one stylesheet graph, and
  graph entries/raw bytes stay within the existing bounded stylesheet limits.

## Implementation

- Add a boxed asynchronous recursive network stylesheet graph owner in the
  content process, reusing `static_css_imports`, `css_import_matches`, and the
  HTTP stylesheet loader.
- Decode CSS URL escapes, resolve each target against its owning stylesheet
  URL, preserve redirect-final URLs, and splice active dependencies with named
  or anonymous layer wrappers.
- Carry expanded network stylesheet state through initial and dynamic rebuilds;
  preserve raw link `href` identity separately from the resolved URL.
- Add a nested HTTP fixture covering active layered/recursive imports, a
  stylesheet-relative image, and a false `print` import that must not request.

## Tradeoffs and remaining scope

The graph is loaded serially to keep one policy/cache owner and deterministic
import order; parallel fetch scheduling is not introduced as a second network
owner. Limits are bounded per graph and across the current content-process
stylesheet set. Blob stylesheets remain on their existing object-URL path
without network import expansion. File fonts and other CSS resource types,
complete file-origin semantics, and full Web IDL parity remain issue #40 work.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_content_process_loads_network_css_import_graph`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_content_process_revalidates_stylesheet_and_script_subresources`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_file_`
- documentation truth/depth/shortcut/coverage audits
- `git diff --check`
