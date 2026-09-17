# Native engine browser-complete slice 489: bounded data @font-face loading

- Status: complete
- Scope: `native-engine` / document-owned data URL font faces
- Issue: #40
- Depends on: [native-engine-browser-488](native-engine-browser-488.md)

## Objective

Make a bounded, observable subset of CSS `@font-face` useful to the native
renderer by admitting valid embedded font bytes into the document's existing
HarfRust/fontdue font book. Keep unsupported source kinds explicit so a page
falls back deterministically instead of treating unimplemented file or network
font loading as successful.

## Contract

- The CSS parser recognizes an exact `@font-face` at-rule with one named
  `font-family`, a `src` containing a supported `url(data:...)` source, and the
  existing `normal|bold` weight and `normal|italic` style values. Generic and
  fallback family names are rejected for document faces.
- `local(...)` and unsupported file, network, or blob sources may be skipped
  while a later valid data URL is considered; a rule with no admitted data
  source produces a diagnostic and no resource.
- Data font media types are limited to the admitted TrueType, OpenType, WOFF,
  WOFF2, and legacy font MIME aliases. Standard base64 and bounded percent
  decoding are accepted; empty, unsupported, malformed, control-containing,
  or oversized payloads fail closed.
- At most 16 face rules are considered, each decoded resource is at most 4 MiB,
  and the aggregate document payload is at most 8 MiB. Invalid font bytes are
  ignored by the font parser and the page retains system-font fallback.
- Document font loads pass through the existing document `font-src` policy,
  including report-only diagnostics, and are performed for initial documents
  and rebuilt external stylesheets in both the inline and content-process
  owners.
- Admitted resources are document-owned and precede system faces for matching
  named families. Existing metrics, shaped runs, missing-glyph fallback,
  display-list, clipping, decoration, hit geometry, and PNG replay consumers
  remain the owners of layout and paint behavior.
- The content-process document wire transfers validated font metadata and
  base64 bytes under protocol version 13. The receiving side rechecks family
  hashes, encoded and decoded size bounds, face count, and aggregate bytes
  before building its local font book.

## Implementation

- Extend `NativeStylesheet` and its bounded parser context with
  `NativeFontFaceRule` descriptors without placing at-rules in the ordinary
  style-rule cascade.
- Add data-font URL decoding and policy admission to
  `NativeResourceLoader`, reusing the existing URL, CSP, diagnostic, and
  resource-owner boundaries.
- Add `NativeFontFaceResource` and `NativeFontBook::from_resources`, keeping
  custom faces document-local and system candidates cached and deterministic.
- Carry resources in `NativeDocumentWire`, rebuild them after stylesheet
  changes, and use the document font book from native text metrics.
- Load the resources at initial navigation and external stylesheet rebuild
  boundaries in both native owners.

## Tradeoffs and remaining scope

This slice deliberately covers embedded `data:` fonts only. File, network,
and blob font sources, `local()` lookup, `FontFace`/`FontFaceSet` loading
events, font-display timing, variable-font instances, color fonts, language or
script-specific matching, mixed bidi/writing modes, and complete browser text
and Web IDL parity remain issue #40 gates. The bounded base64 document wire is
simple and deterministic but adds transfer and memory overhead; the 8 MiB
aggregate cap and parser admission are the resource-safety boundary. Invalid
font bytes do not abort navigation and instead retain the existing system
fallback.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib --locked font_` (14 passed)
- `cargo test --quiet -p glass-browser --lib --locked content_wire_round_trips_document_font_resources` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine --locked native_real_font_metrics_feed_layout_and_glyph_paint_when_available -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine --locked native_css_direction_reaches_real_font_raster_coordinates -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine --locked native_display_list_is_revisioned_deterministic_and_visibility_aware -- --nocapture` (1 passed)
- process/listener audit found no stale Glass, Cargo, rustc, Chromium,
  Firefox, or native-content-worker targets to terminate
- documentation truth/depth/shortcut/coverage audits (pending final docs gate)
- `git diff --check` (pending final docs gate)

The implementation is local-only at this checkpoint: it is not pushed, run in
remote CI, released, tagged, or published.
