# Native XHR ProgressEvent identity (422)

status: complete
scope: native-engine/xhr-progress-event
issue: 40
depends-on: [native-engine-browser-421]

## Objective

Give native XHR upload lifecycle notifications their declared event identity
in both page and worker realms. The existing bounded upload fields are useful,
but a normal consumer must also be able to recognize a `ProgressEvent` while
retaining target identity and the current event ordering.

## Contract

- Page and worker XHR upload `loadstart`, `progress`, terminal, and `loadend`
  events are `ProgressEvent` instances.
- Each event retains `lengthComputable`, non-negative `loaded`/`total`, upload
  target identity, and the existing stale/terminal ordering guarantees.
- The `ProgressEvent` constructor is available in both realms with the common
  event fields and bounded progress fields.
- This slice does not claim socket-level progress, complete event dispatch or
  Web IDL descriptor parity, or streaming upload semantics.

## Implementation path

- Add page and worker `ProgressEvent` constructors derived from each realm's
  existing Event owner.
- Construct ProgressEvents for XHR notifications carrying progress fields and
  restore their dispatch target/currentTarget.
- Extend the process-backed page/worker upload witness with constructor and
  instance checks.
- Synchronize architecture, plan, analysis, task, and issue #40.

## Tradeoffs

- Only events with progress fields use the specialized constructor; other XHR
  events retain the existing lightweight dispatch path.
- The fields remain bounded handoff measurements, not fabricated per-socket
  progress or complete browser event timing.

## Evidence

- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine xhr_upload_reports_buffered_progress --locked -- --nocapture` (2 passed)
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --nocapture` (9 passed)
- `cargo fmt --all`
- `git diff --check`
- documentation coverage, depth, and release-truth checks passed

Remote CI, push, release, tag, and registry publication are outside this
local checkpoint.
