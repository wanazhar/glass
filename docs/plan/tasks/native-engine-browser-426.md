# Native XHR responseText state contract (426)

status: complete
scope: native-engine/xhr-response-text
issue: 40
depends-on: [native-engine-browser-425]

## Objective

Complete the bounded page and worker XHR `responseText` projection contract.
Text reads must remain available for the empty and `text` response types, while
binary, JSON, and document response types must reject access with the correct
state error. Opening, aborting, timing out, and completing a request must not
leak the previous text projection into the next request.

## Contract

- Page and worker `responseText` is a read-only projection backed by internal
  response text state rather than an own mutable field.
- `responseText` returns the bounded text during `OPENED`/`LOADING`/`DONE` for
  response types `""` and `"text"`; other response types throw
  `InvalidStateError`.
- `open()`, `abort()`, timeout, and terminal error clear the internal text
  projection; a reopened XHR starts with an empty projection.
- Existing `response`, `responseXML`, response-type canonicalization, binary/
  JSON/document decoding, lifecycle ordering, and worker isolation remain
  unchanged.
- No synchronous XHR, streaming body delivery, or complete XHR/Web IDL parity
  is claimed by this slice.

## Implementation path

- Replace page and worker constructor-owned `responseText` values with private
  bounded fields and install guarded prototype accessors.
- Update every lifecycle reset and response completion path to use the private
  field, preserving the existing response projections and error names.
- Add a real HTTP witness covering page and worker invalid-type guards, valid
  text access, reopening reset, and abort/terminal clearing; retain the full
  XHR regression group.
- Synchronize architecture, plan, analysis, task, and issue #40.

## Tradeoffs

- A prototype accessor adds a small amount of binding code but prevents callers
  from bypassing response-type semantics through an enumerable own field.
- The projection remains buffered because the native XHR owner still exposes
  one bounded body completion; transport streaming remains a separate gate.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_guards_response_text_by_type_and_resets_it --locked -- --nocapture`
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --nocapture` (13 passed)
- `git diff --check`

All scoped checks and the complete XHR regression group passed locally. Remote
CI, push, release, tag, and registry publication are outside this local
checkpoint.

Remote CI, push, release, tag, and registry publication are outside this
local checkpoint.
