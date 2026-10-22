# Browser command parity verification

The browser now obtains command names, aliases, and form fields from the native
registry. Session mutations run in the served-session actor. Inspectors and
browser display controls do not contact a model. Model commands allocate a
worker only when invoked and support cooperative cancellation.

A Luna medium reviewer checked dispatch, observers, cancellation, queues,
handoffs, and credential forms. The review led to corrections for saved queue
delivery and editing, queue clearing, heavy-command inspectors, handoff routing,
observer file access, prompt-history scope, and hidden credential fields.

## Verification

- Workspace tests: 1,008 passed; two environment-specific tests remain ignored.
- Workspace Clippy, all targets: passed with warnings treated as errors.
- Optimized binary: release build completed successfully.
- Release browser smoke: desktop and iPhone layouts, native command forms,
  rich previews, custom question answers, denied approvals, reasoning, reload,
  and PWA checks passed.
- Release protocol checks: local commands, duplicate prevention, observer
  restrictions, saved queue delivery, heavy-command inspection, cancellation,
  and crash recovery passed.
- Explicit idle worker upgrade preserved transcript bytes.
- Gateway lifecycle checks passed, including route preservation, identity and
  origin checks, HTTP operation, and stale-route recovery.
- Live mobile audit: 390 by 844 viewport, scale 1, no browser errors.

The offline performance check passed without browser-test contention: CLI help
startup median 1.23 ms, redraw median 0.29 ms, and worst long-history reader
median 121.10 ms for 12,000 records. Long-history child peak RSS was 4,132 KiB.
The release binary is 7,345,640 bytes. The main browser script is 32,396 bytes
gzipped. These measurements are specific to this host.

## Operational limits

Existing workers are preserved during gateway updates. Older workers require
an explicit idle restart before accepting browser command frames. Such a restart
stops their owned background jobs and teammates; the browser explains this
before requesting confirmation.

Browser workflows use forms and result panels instead of terminal controls.
Credential and provider network flows are not exhaustively tested against every
provider. The release smoke uses an isolated local provider. Large command
results are clipped for display; transcript export uses a separate download.
