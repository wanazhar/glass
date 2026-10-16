# GC Connect release review

## Scope

Native optional gateway, Tailscale Serve lifecycle, shared session ownership,
durable follow-ups, interactive approvals and questions, and a responsive PWA
with rich previews. The core executable has no new Rust dependencies. Web assets
are installed separately and need no Node process at runtime.

## Evidence

- Workspace tests: 1,001 passed; two existing tests ignored.
- Strict workspace Clippy: passed.
- Release browser checks: desktop and iPhone viewport layouts, Markdown, syntax,
  KaTeX, Mermaid labels, JSON, PDF, sandboxed HTML, file previews, tools,
  questions with custom answers, approval denial, busy steering, reconnect,
  PWA assets, and queued prompt recovery after a worker crash.
- Release lifecycle checks: owner identity, host and origin rejection, inherited
  restriction policy, route preservation, gateway reuse, changed-route protection,
  and stale-route recovery.
- Served session regression and installer checks passed.
- Installed release verified through real Tailscale HTTPS: page and authenticated
  API returned HTTP 200. Existing port 443 route was preserved; occupied local
  port 8443 was skipped.
- Production npm dependency audit: no reported vulnerabilities.
- Performance fixture: 12,000 records; help startup median 1.50 ms; worst
  interactive startup median 8.08 ms; redraw median below 0.34 ms; history
  reader frame median below 124 ms; child peak RSS 4,136 KiB. These are local
  fixture measurements, not browser or provider latency guarantees.
- Initial JavaScript is approximately 23 KB gzip. Heavy renderers load on demand.
  Installed web assets occupy approximately 12 MiB.

## Independent review

One GPT-6 Luna reviewer checked lifecycle, authorization, bounds, performance,
and browser interaction. Findings addressed: durable queue acknowledgment,
receipt wording, visible steering corrections, and separate port probes with
IPv6-unavailable handling. Live host verification additionally found a local
listener collision on port 8443; selection now checks local listeners as well
as Tailscale routes.

## Limits

Physical iPhone Safari testing remains a user check; mobile rendering was checked
in Chromium at an iPhone viewport. Existing standalone TUI writers require a
handoff before web access. Shared served clients have explicit input ownership.
HTML previews cannot execute scripts or submit forms. Large previews are bounded;
PDFs render one page at a time. Tailnet owner identity is required; tagged nodes
are not supported in this mode. Existing unrelated Tailscale routes remain intact.
