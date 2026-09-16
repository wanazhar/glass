# Native XHR JSON response type (421)

status: complete
scope: native-engine/xhr-json-response
issue: 40
depends-on: [native-engine-browser-420]

## Objective

Expose the bounded JSON response type through the native page and worker
`XMLHttpRequest` bridges. The existing Fetch response owner already performs
bounded JSON decoding; XHR should be able to select that representation in
both realms without falling back to text or another backend.

## Contract

- Page and worker XHR accept `responseType = "json"` for asynchronous requests.
- A successful JSON response exposes the parsed value through `xhr.response`
  and leaves the bounded `responseText` projection empty.
- Existing response metadata, ready-state, load/error, timeout, abort, upload,
  and stale-continuation ownership remains unchanged.
- JSON parsing remains bounded and uses the existing native Response decoder;
  synchronous XHR, XML documents, streaming JSON, and complete XHR/Web IDL
  parity remain outside this slice.

## Implementation path

- Admit `json` in the page and worker response-type gates.
- Select `response.json()` in each existing buffered response continuation.
- Add one process-backed HTTP witness covering page and worker JSON XHR
  identity, parsed values, and the empty text projection.
- Synchronize architecture, plan, analysis, task, and issue #40.

## Tradeoffs

- JSON parsing is still buffered and bounded, matching the current native
  Response contract and avoiding a second parser or transport path.
- Parse failures retain the existing XHR terminal error owner; detailed JSON
  error and Web IDL parity remain later work.

## Evidence

- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_supports_json_response_type_in_page_and_worker --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --nocapture` (9 passed)
- `cargo fmt --all`
- `git diff --check`
- documentation coverage, depth, and release-truth checks passed

Remote CI, push, release, tag, and registry publication are outside this
local checkpoint.
