# Browser verification add-on

Enable `browser` in GC's `/extensions` picker. Optional dependency: Python `playwright`.
Set `GC_BROWSER_CDP=http://127.0.0.1:9222` for a browser you explicitly expose.
GC connects to that local endpoint, creates a fresh isolated context, and closes
only that context. It never installs dependencies or starts/closes your browser.

`browser_verify` accepts `url` and optional `expected_text`. It reports HTTP status,
expected text presence at DOMContentLoaded, bounded console/page errors, a viewport
PNG, and `passed`. Dynamic pages may need another check after application startup.
Screenshots/reports are private workspace `.gc-artifacts/browser-*` files. Register
returned paths with `gc workspace artifact add LABEL PATH` to add them to the shelf.
The default timeout budgets are 10s connection, 20s navigation and 10s screenshot.
No background polling or requests occur when this add-on is disabled.

Use a dedicated browser profile. Local CDP grants browser control and is not an
authentication boundary against other local programs. Refer to
[GC workflows](../../docs/workflows.md) for policies, artifacts and verification.

Tests: `python3 scripts/smoke-browser.py`. To exercise actual Chromium, set
`GC_BROWSER_TEST_EXECUTABLE` to a Chromium executable. The test launches and closes
its own temporary browser, never an existing user process.
