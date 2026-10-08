---
id: web-extension-001
scope: extensions/web, addons catalogue
status: done
depends-on: []
---

## Objective

Bundled `web` extension: `web_fetch` (one URL as readable text) and
`web_search` (DuckDuckGo HTML by default, SearXNG configurable). Ported from
the first-class web tools in Claude Code, Codex, and opencode
(research/harness-features-2026.md), shipped out of process per the
extensions ethos — the core binary does not grow an HTTP client.

## Context

- HTML is stripped with the stdlib parser: prose in block order, links kept
  inline as ` <https://…>`, scripts/styles/head dropped. The response body
  caps at 512 KB, the returned text at 20,000 chars by default.
- Keys, when a backend needs one, live in `~/.config/glass-core/web.json`;
  the host strips `*API_KEY*`/`*TOKEN*` env vars from extensions by design.
- `GC_WEB_CONFIG` overrides the config path for tests and unusual setups.
- Search is explicitly best-effort: a rate-limited page degrades into an
  actionable message pointing at the SearXNG option.

## Paths

- `extensions/web/web.py` (new): tools, HTML-to-text, DuckDuckGo result
  parse with `uddg=` unwrapping, selftest mode.
- `extensions/web/README.md`, `crates/glass/src/addons.rs` (bundle entry).
- `scripts/smoke-web.py`: protocol-level check against a local fixture.

## Verification

- `python3 extensions/web/web.py --selftest` and `scripts/smoke-web.py`:
  handshake manifest, prose extraction without scripts, link retention,
  search parse and URL unwrapping, explicit HTTP 404 handling.
- `cargo test -p glass-core` (catalogue entry) and Clippy.
- Real-network behavior is deliberately left to the operator; the suite is
  offline.
