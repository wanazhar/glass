# Native CSP report-only violation events (399)

status: done
scope: native-engine/csp-reporting
issue: 40

## Objective

Carry report-only CSP declarations through the existing native resource-policy
owner and deliver bounded `securitypolicyviolation` events to the page realm.
The enforced policy must remain unchanged: a report-only violation can be
observed, but it never authorizes or blocks a request.

## Contract

- Parse `Content-Security-Policy-Report-Only` response headers alongside
  enforced headers, preserving each original policy string.
- Record violations for the already implemented URL and inline policy checks,
  including the effective directive, blocked URL/sample, original policy, and
  report disposition.
- Deliver records as structured page events and expose the standard event
  fields plus the historical `violatedDirective` alias. Synchronously inserted
  classic inline scripts use the persistent QuickJS bridge so their event is
  observed at insertion time.
- Bound loader/report batches, validate all cross-owner payloads, and keep
  report-only records out of ordinary logs and request authorization.
- Cover local runtime and content-process HTTP(S) paths with focused tests.

## Tradeoffs and non-goals

This slice does not add network `report-uri`/`report-to` POST delivery,
strict-dynamic trust propagation, dynamic policy-container mutation, or the
complete CSP source-expression grammar. Those need their own request-lifecycle
and grammar work. Initial resource-discovery violations are delivered at the
next page host turn after the content-process load; synchronously inserted
classic inline scripts report through the existing page bridge. The bounded
report record is serialized only for that bridge call; no report data is
written to logs.

## Delivered

- Preserved each bounded `Content-Security-Policy-Report-Only` response header
  declaration and kept it separate from enforced and parser-time meta policy.
- Added report-only URL checks for the existing EventSource, Fetch, stylesheet,
  image, script, and worker resource owners, with redirect-aware blocked URLs.
- Added report-only inline script/style checks and a native
  `SecurityPolicyViolationEvent` surface with bounded metadata and
  `violatedDirective` compatibility.
- Added the dynamic inline-script QuickJS bridge so report-only violations are
  dispatched before the inserted script executes while the enforced decision
  remains unchanged.
- Added HTTP(S) content-process coverage for initial and dynamic script/style
  reporting.

## Verification

- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_delivers_report_only_csp_violation_events --locked -- --nocapture`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_delivers_report_only_events_for_dynamic_scripts --locked -- --nocapture`
- `cargo fmt --all -- --check`
- `git diff --check`

Remote CI, push, release, tag, and registry publication remain outside this
local-only checkpoint.
