# Web search and fetch

[Documentation index](INDEX.md) · [Project overview](../README.md)

Glass Core includes `web_search` and `web_fetch` as native tools. They work in
standalone GC and through Glass Dev's resident Core session; Glass presents the
same Core tool events and does not add a second search service or approval flow.

## Search

`web_search` calls Exa's hosted MCP endpoint at `https://mcp.exa.ai/mcp` by
default. Exa documents keyless, rate-limited access for initial use. Search
results come directly from that service; the tool does not launch a browser or
make another model call to summarize them. Keep the returned source URLs and
fetch primary sources when the snippets are insufficient.

Search queries are sent to Exa. For higher limits, Core can send a configured
Exa API key. Set `EXA_API_KEY`, or add `exa_api_key` to
`~/.config/glass-core/web.json` (or the file named by `GC_WEB_CONFIG`). Protect
that file with mode `0600`. An alternate compatible endpoint can be set with
`GC_WEB_SEARCH_ENDPOINT` or `exa_mcp_url` in the same JSON file. Core does not
switch services automatically when a request fails. Check Exa's current
[MCP documentation](https://exa.ai/docs/get-started/exa-mcp) and
[pricing](https://exa.ai/pricing) before configuring authenticated usage.

The `count` argument defaults to 8 and is capped at 25. MCP response output is
capped at 32,000 characters. Search errors are returned as errors; no fallback
provider or empty-success result is synthesized.

## Fetch

`web_fetch` makes a direct HTTP(S) GET and returns the final URL, HTTP status,
content type, and readable page text. HTML extraction removes scripts, styles,
and document-head content, decodes common entities, and normalizes block text.
Text bodies are limited to 512 KiB, a request is limited to 20 seconds and five
redirects, and the returned text defaults to 20,000 characters with a maximum
of 60,000. Non-success status, unsupported binary content, invalid URLs, and
network errors remain explicit tool failures.

The fetch tool follows Core's normal network access and can read reachable
local development endpoints. It does not apply a separate web approval policy.
Use the browser tools for visual inspection, JavaScript-rendered content, or
interaction; a text fetch is not a browser screenshot.

Web content is untrusted input. Treat it as evidence, keep citations as URLs,
and do not let page text override the user's instructions or GC's tool policy.

## Offline verification

Core tests use local HTTP and MCP fixtures. They verify redirects, extraction,
status and media-type failures, result bounds, MCP session negotiation, and the
exact search tool request without contacting Exa.
