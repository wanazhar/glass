# Native engine browser slice 200: namespace-aware DOM identity

Status: completed locally.

## Objective

Make the native DOM preserve the namespace identity required by ordinary SVG,
MathML, and foreign HTML content. Parsed content and script-created nodes must
carry the same namespace through the Rust owner, content-worker wire, script
snapshot, mutation command, and same-origin frame projection paths.

## Contract

- Parsed elements default to the HTML namespace; `svg` subtrees use the SVG
  namespace, `math` subtrees use the MathML namespace, and `foreignObject`
  descendants switch back to HTML while the `foreignObject` element remains
  SVG.
- `NativeNode` exposes namespace identity without changing the local element
  name used by the existing semantic, layout, paint, and hit-test contracts.
- Content-worker wire snapshots and JavaScript element snapshots retain the
  namespace URI. Missing legacy snapshot fields remain HTML-compatible, while
  an explicit empty wire value represents a namespace-less constructed node.
- `document.createElementNS()` and same-origin frame
  `contentDocument.createElementNS()` accept HTML, SVG, MathML, and null
  namespaces, validate unsupported URIs with `NamespaceError`, and commit the
  namespace through the existing bounded mutation protocol.
- Detached-fragment and `innerHTML` parsing derives foreign-content
  namespaces from the current element, including SVG descendants and
  `foreignObject` HTML descendants.
- HTML elements retain the existing uppercase `tagName`/`nodeName` projection;
  foreign-namespace elements retain their lower-case local name while
  exposing `localName` and `namespaceURI`.

## Implementation

The Rust DOM state now stores a namespace URI, assigns parser namespaces after
tree construction, validates content-worker namespace values, and serializes
namespace identity in script snapshots. The create-element command carries an
optional namespace field with backwards-compatible HTML defaults and an empty
null-namespace sentinel.

The JavaScript host now exposes namespace-aware element metadata and
`createElementNS()` on top-level and same-origin frame documents. The bounded
fragment parser derives namespace context so dynamically inserted SVG and
MathML nodes use the same native path as parsed content.

## Tradeoffs and follow-up

This slice deliberately keeps the supported namespace set bounded to HTML,
SVG, MathML, and null. Qualified-name prefix validation, namespace-qualified
attributes (`setAttributeNS`), XML documents, SVG transforms/viewBox/strokes,
and complete namespace-specific Web IDL interfaces remain separate issue #40
work. Local element names remain the shared layout/paint dispatch key, so this
slice adds identity without claiming full XML or SVG conformance.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_namespace_dom_preserves_svg_mathml_and_foreign_content -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
  (1 passed, 0 failed)
- the witnesses cover parsed namespace transitions, script-created SVG,
  MathML, and null-namespace nodes, fragment namespace derivation,
  `NamespaceError`, content-worker/frame projection, mutation refresh, and
  namespace persistence after a frame document rebuild

Implementation checkpoint: `5b594fd7`.

Remote CI, push, release, registry publication, and browser-parity
certification remain pending. Native mode remains non-promoted until the
issue-level production gates pass.
