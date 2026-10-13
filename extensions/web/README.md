# Web extension

Fetch readable page text and search results through an optional Python extension.
Enable `web` in `/extensions`.

## Tools

`web_fetch` accepts `url` and optional `max_chars`. It retrieves an HTTP(S) URL,
removes HTML scripts/styles, and returns readable text with links and source
metadata. Response bodies are capped at 512 KiB. Text defaults to 20,000 characters,
with a maximum of 60,000. It does not execute JavaScript or provide an interactive
browser. Use other tools for raw headers, non-GET requests, or browser automation.

`web_search` accepts `query` and optional `count`, returning titles, URLs, and
snippets. The default DuckDuckGo HTML backend requires no key. A SearXNG backend
can be configured instead.

## Configuration

Use `~/.config/glass-core/web.json`, or `GC_WEB_CONFIG`:

```json
{
  "search":"ddg",
  "ddg_url":"https://html.duckduckgo.com/html/",
  "searxng_url":"https://searx.example",
  "api_key":""
}
```

Store backend credentials in private configuration. Common credential environment
variables are filtered from extension processes; do not rely on their inheritance.
The extension reports transport errors, rate limits, and unparsed search pages.
Third-party page formats can change, so an unsuccessful parse does not establish
that no matching information exists.

Checks: `python3 web.py --selftest` from this directory, or
`python3 scripts/smoke-web.py` from the repository root.
