# Native XHR response type state (424)

status: complete
scope: native-engine/xhr-response-type-state
issue: 40
depends-on: [native-engine-browser-423]

## Objective

Make XHR response-type selection a typed, state-aware property in the page
and worker native realms. Invalid values must fail when selected, supported
values must have one canonical representation, and a response type must not
change after response loading has begun.

## Contract

- Page XHR accepts the bounded `""`, `"text"`, `"json"`, `"arraybuffer"`,
  `"blob"`, and `"document"` response types; worker XHR accepts the existing
  worker set without promoting Window-only XML documents.
- Assignment is case-insensitive and reads back the canonical lower-case
  value; unsupported values fail with `TypeError` before `send()`.
- Assignment while the request is `LOADING` or `DONE` fails with a bounded
  `InvalidStateError` and cannot replace the selected response projection.
- Existing response projections, XML `responseXML`, lifecycle ordering,
  upload events, timeout, abort, and stale-continuation behavior remain
  unchanged.
- This slice does not claim synchronous XHR, response streaming, complete
  `responseText`/`response` Web IDL getter parity, legacy browser-specific
  response types, or complete XHR Web IDL parity.

## Implementation path

- Give page and worker XHR a private canonical response-type slot with realm-
  appropriate error construction and bounded enum admission.
- Preserve the current response selection and completion owners while routing
  all response-type reads through the canonical slot.
- Add a real process-backed witness for canonicalization, invalid assignment,
  and mutation rejection during `LOADING`/`DONE`; retain the XHR regression
  group.
- Synchronize architecture, plan, analysis, task, and issue #40.

## Tradeoffs

- Early assignment errors make caller mistakes visible sooner, while the
  existing buffered response projections remain intact for compatibility with
  the current bounded profile.
- The worker remains intentionally narrower than Window XHR because
  `responseXML`/document response selection is not promoted into worker
  globals by this task.

## Evidence

- Local implementation checkpoint: the focused Conventional Commit and issue
  #40 checkpoint comment record this task.
- `cargo fmt --all` passed.
- `cargo check --quiet -p glass-browser --tests --locked` passed.
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_response_type_is_canonical_and_state_aware --locked -- --nocapture` passed.
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --nocapture` passed: 11 tests.
- The real HTTP witness covers page and worker canonicalization, assignment-time
  type rejection, and mutation rejection during `LOADING` and `DONE`.

Remote CI, push, release, tag, and registry publication are outside this
local checkpoint.
