# Native engine browser slice 214: URL-reflected element properties

Status: completed locally.

## Objective

Expose the URL-reflected DOM properties used by ordinary pages and keep their
setters connected to the native attribute mutation path.

## Contract

- Anchors, areas, bases, and links expose `href`; images, scripts, frames,
  embeds, sources, tracks, audio, and video expose `src`; forms expose
  `action`.
- Relative values resolve against the active document URL, including the
  current same-origin frame URL. Absolute values and supported non-network
  schemes remain intact; missing/empty values retain the empty-string IDL
  result.
- Assigning a reflected URL property stringifies the value, updates the
  content attribute, and emits the existing typed `setAttribute` command.
  Image `src` assignment therefore enters the same native resource discovery
  path as `setAttribute("src", ...)`.
- The content attribute remains the serialized author value, while the IDL
  property returns the resolved URL. The shared implementation is installed
  for local, content-worker, and same-origin frame element projections.

## Implementation

`document_bootstrap` now installs one URL-reflection helper on the common
element surface and passes the frame's active URL for child projections. The
helper reuses the native JavaScript URL implementation for resolution and the
existing attribute command bridge for writes; no second URL or DOM state is
introduced.

## Tradeoffs and follow-up

This slice keeps URL reflection focused on the high-use URL attributes and
does not add a separate live URL object or browser-specific defaulting for
every specialized element. The returned property is a resolved string, which
matches the ordinary IDL read contract while avoiding mutable object identity
that could drift from the content attribute. Complete URL reflection for
remaining HTML/SVG attributes, base-element document-base semantics, and the
broader navigation/resource surface remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_url_attributes_resolve_and_persist_through_dom_commands -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_script_exposes_web_idl_identity -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture` (1 passed, 0 failed)

Implementation checkpoint: `f74efdce`.
