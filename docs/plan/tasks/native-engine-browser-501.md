# Native engine browser slice 501: discover installed fonts

Status: completed locally on the current source line.

## Objective

Make the native font owner resolve ordinary fonts installed on the host, not
only the small static candidate table. This advances CSS `@font-face` local
selection and script-created `FontFace` local sources toward the browser
contract while retaining bounded resource use and deterministic selection.

## Contract

- Search platform font roots for `.ttf`, `.otf`, `.ttc`, and `.otc` files.
- Prefer the current user's font directories before shared platform roots.
- Read family and typographic-family names, weight, and italic/oblique style
  from each collection face with `ttf-parser`.
- Map the bounded metadata into the existing native family, generic-family,
  weight, and style selection owners.
- Preserve the static built-in candidates and let them win before discovered
  duplicates.
- Admit at most 512 font files, 64 MiB of discovered file bytes, 64 faces,
  32 faces per collection, and the existing 4 MiB per-face byte limit.
- Ignore unreadable, malformed, empty, oversized, symlinked, and unsupported
  paths without making system-font loading fail globally.
- Keep discovery deterministic by sorting paths and preserving stable first
  admission for duplicate family/weight/style tuples.

## Implementation

`NativeFontBook` now loads the static candidates and then walks bounded,
platform-specific roots. Collection faces are identified with `ttf-parser`;
the existing `fontdue` and HarfRust owners remain responsible for raster and
shaping admission. User roots are checked first on Linux, macOS, and Windows.
The discovery test selects an admissible non-static installed face and proves
that its bytes and metadata enter the book.

## Tradeoffs and explicit limits

Discovery is eager at first native font-book initialization, which adds
startup I/O and retains admitted font bytes for the process lifetime. The
bounds make that cost predictable and protect the browser from enormous font
trees, but a large installed face can remain unavailable when it exceeds the
existing 4 MiB admission limit. Metadata mapping is intentionally conservative:
weight is normal/bold, style is normal/italic, and generic-family inference
uses bounded family-name hints. Variable axes, color tables, CSS font-display
timing, WOFF/WOFF2, full font enumeration, and complete FontFace/Web IDL
parity remain separate issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib --locked font -- --nocapture --test-threads=1`
- `cargo test --quiet -p glass-browser --lib --locked content_process_font_destination_uses_the_native_font_loader -- --nocapture --test-threads=1`
- `python3 scripts/check-release-documentation.py --require-previous-version`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`

