# web

Read the web without turning `bash` into a browser.

- **`web_fetch {url, max_chars?}`** returns one URL as readable text. HTML is
  stripped to prose with links kept inline as ` <https://…>`; scripts, styles,
  and head content are dropped. The response body is capped at 512 KB and the
  returned text at 20,000 characters by default (max 60,000, per call). It is
  a text browser — raw bytes, headers, and non-GET requests remain `curl`'s
  job, and the model is told so.
- **`web_search {query, count?}`** returns titles, URLs, and snippets.
  Default backend: DuckDuckGo's HTML endpoint, no key required. A
  [SearXNG](https://docs.searxng.org) instance can be configured instead.

Configuration, optional, at `~/.config/glass-core/web.json`:

```json
{
  "search": "ddg",
  "ddg_url": "https://html.duckduckgo.com/html/",
  "searxng_url": "https://searx.example",
  "api_key": ""
}
```

Keys belong in this file, never in the environment: the host strips
names containing `TOKEN`, `SECRET`, `PASSWORD`, or `API_KEY` from extension
processes by design, so an env-based key could not work here.

Both tools are best-effort about third-party HTML: a rate-limited or
redesigned search page degrades into an explicit "no results parsed" message
with a pointer at the searxng option, never a silent empty success. Run the
offline check with `python3 web.py --selftest` (or `scripts/smoke-web.py`).
