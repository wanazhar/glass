# Native engine browser slice 201: namespace-qualified attributes

Status: completed locally.

## Objective

Carry namespace-qualified attribute identity through the native DOM owner,
content-worker wire, persistent JavaScript snapshots, mutation commands, and
same-origin frame projections. Close the gap left by slice 200 so SVG/XML
metadata and XLink attributes are observable and mutable through normal DOM
attribute APIs.

## Contract

- Element snapshots retain a bounded namespace URI for each qualified
  attribute, with legacy snapshots defaulting to the null namespace.
- Parsed `xlink:*`, `xml:*`, and `xmlns` attributes in supported SVG/MathML
  fragment contexts preserve XLink, XML, and XMLNS identity; ordinary HTML
  attributes remain namespace-less.
- `getAttributeNS`, `setAttributeNS`, and `removeAttributeNS` match by local
  name and namespace, validate the supported namespace set and reserved
  prefixes, and route mutations through the existing bounded command owner.
- `Attr` exposes `localName`, `prefix`, `namespaceURI`, value mutation, and
  `ownerElement` consistently for namespaced and namespace-less attributes.
- `getAttributeNode`, `getAttributeNodeNS`, `NamedNodeMap` indexed/iterable
  access, NS lookup, replacement, and removal preserve live object identity
  across local, detached, content-worker, and same-origin frame realms.
- `createAttributeNS`, namespace-aware `setAttributeNodeNS`, cloning, and
  detached `innerHTML` parsing preserve qualified attribute identity.
- Unsupported attribute namespaces, invalid qualified names, and reserved
  prefix mismatches fail with typed `NamespaceError` results; no namespace is
  silently downgraded to an ordinary attribute.

## Implementation

Rust `NativeElementState` now stores a bounded qualified-name-to-namespace
map. The map is validated on content-worker transfer and serialized in script
element snapshots. Set/remove script commands carry an optional namespace URI;
the native owner validates it and applies the mutation to the same authoritative
attribute store.

The JavaScript host now shares namespace normalization and qualified-name
validation across local elements, detached nodes, frame projections, and frame
detached nodes. The live `Attr`/`NamedNodeMap` surface keys identities by
namespace plus qualified name, and value setters dispatch to the corresponding
namespace-aware element method. Clone and fragment-parser paths retain SVG,
XML, XMLNS, and XLink metadata.

## Tradeoffs and follow-up

This slice intentionally supports the bounded HTML, SVG, MathML, XML, XMLNS,
and XLink URI set already accepted by the native profile. It does not add XML
document parsing, namespace-aware CSS selectors, SVG transforms/viewBox or
resource loading, complete Web IDL descriptors, arbitrary namespace registries,
or browser-wide standards conformance. The attribute map remains keyed by
qualified name for the existing bounded DOM store; local-name/namespace lookup
is provided by the host surface. These choices keep the owner single-source
and preserve the current command/snapshot protocol while leaving the full XML
attribute model for the issue #40 promotion work.

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_namespaced_attributes_preserve_attr_identity_and_commands -- --nocapture`
  (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_same_origin_frame_script_projection_matches_window_contract -- --nocapture`
  (1 passed, 0 failed)
- `python3 scripts/check-documentation-coverage.py` (851 Markdown files; 345
  full-product MCP tools; 100 browser-only tools; 17 examples; 22 public
  modules)
- `python3 scripts/check-documentation-depth.py` (93 current guides; 19
  substantive contracts)
- `python3 scripts/check-release-documentation.py --previous-version 0.3.13
  --require-previous-version` (851 documents; 61 previous-version hits; 1010
  semantic audit hits; 0 current-claim failures)

The focused witnesses cover parsed SVG XLink/XML attributes, detached SVG
fragment parsing, namespace-aware Attr ownership and value mutation,
NamedNodeMap identity, cloning, typed validation, content-worker command
commit, and same-origin frame projection.

Remote CI, push, release, registry publication, and browser-parity
certification remain pending. Native mode remains non-promoted until the
issue-level production gates pass.

Implementation checkpoint: `70b88d76`.
