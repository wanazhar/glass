# Native engine browser slice 195: HTML recovery and implied end tags

Status: completed locally.

## Objective

Advance the HTML document tree builder from happy-path tokenization toward
browser-compatible recovery. Malformed markup must produce a recoverable
document without synthetic partial elements, and detached `innerHTML` parsing
must make the same bounded decisions.

## Contract

- Unterminated comments, bogus declarations, and EOF-terminated tags recover
  to a bounded Comment or Text node instead of failing or publishing a
  partial element.
- Unknown `<!...>` and `<?...>` declarations use the HTML bogus-comment path;
  valid doctypes remain document metadata.
- Duplicate HTML attributes are ASCII-case-insensitive and keep the first
  occurrence, matching tokenizer precedence.
- A parsed document accepts at most one doctype, and ignores a doctype that
  arrives after the document element has started.
- Common implied end-tag cases cover paragraphs, list/description items,
  options, ruby annotations, and table rows/cells/sections in the bounded
  tree builder.
- The local JavaScript detached-fragment parser mirrors comment/declaration
  recovery, first-attribute precedence, and the same implied end-tag family.

## Implementation

- Made the Rust tokenizer recover unterminated comments, declarations, and
  tags while preserving bounded source content.
- Added root-level doctype uniqueness/order validation and first-duplicate
  attribute retention.
- Expanded the shared implied-end-tag table used by document and fragment
  construction.
- Added matching JavaScript fragment-parser recovery and auto-close rules.
- Added Rust unit witnesses and a local persistent-realm integration witness
  covering malformed recovery, duplicate attributes, bogus declarations,
  and list/option closures.

## Tradeoffs and follow-up

This closes a high-value recovery subset without claiming the complete
WHATWG insertion-mode/tree-construction algorithm. Foreign content, adoption
agency behavior, table foster parenting, full character-reference coverage,
and complete Web IDL/conformance remain separate issue #40 work. The recovery
path is failure-tolerant but still bounded by the existing document, token,
node, depth, and script-value limits. No CDP or fallback path changed.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine native_engine::dom::tests:: -- --nocapture`
  (all filtered native DOM unit tests passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_document_fragments_preserve_tree_ownership_and_helpers -- --nocapture`
  (1 passed, 0 failed)

Implementation checkpoint: `4c7622e4`.

