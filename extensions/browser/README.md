# Browser verification extension

Enable `browser` in `/extensions`. The extension requires Python Playwright and
an explicitly exposed local CDP connection:

```sh
export GC_BROWSER_CDP=http://127.0.0.1:9222
```

It creates an isolated context in that browser and closes only its own context.
It does not install dependencies, launch a browser, or close an existing browser.

## Interface

`browser_verify` accepts `url` and optional `expected_text`. Results include HTTP
status, expected-text presence at DOMContentLoaded, bounded console/page errors,
a viewport PNG, and `passed`. Dynamic pages may need a later check after startup.

Screenshots and reports are private `.gc-artifacts/browser-*` files in the
workspace. Register a returned path with `gc workspace artifact add LABEL PATH`
to expose it through the artifact shelf. Default timeouts are 10 seconds for
connection, 20 for navigation, and 10 for screenshots.

Use a dedicated browser profile. CDP grants browser control and does not
isolate credentials from other local programs. No polling or browser requests
occur while the extension is disabled. See [Workflows](../../docs/workflows.md).

Check: `python3 scripts/smoke-browser.py`. Setting `GC_BROWSER_TEST_EXECUTABLE`
allows the suite to launch its own temporary Chromium instance. It does not
terminate an existing user's browser.
