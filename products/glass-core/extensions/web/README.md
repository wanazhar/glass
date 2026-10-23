# Web extension (retired)

The optional Python web extension was retired when web search and page reading
moved into native Glass Core tools. Do not enable this extension: its tool names
are now built in and cannot be registered a second time.

Use [`web_search` and `web_fetch`](../../docs/web-tools.md) instead. They use
Core's runtime and permission ownership, work without Python, and expose the
same tools when Core is resident inside Glass Dev.
