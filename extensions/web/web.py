#!/usr/bin/env python3
"""glass web extension: read the web without turning bash into a browser.

Two tools:
- `web_fetch` gets one URL and returns readable text: HTML is stripped to
  prose with links kept inline, scripts and styles dropped, and the result
  capped so a giant page cannot swallow the context.
- `web_search` runs a query. The default backend is DuckDuckGo's HTML
  endpoint, which needs no key; a SearXNG instance can be configured instead.

Configuration lives in `~/.config/glass-core/web.json` (or GC_WEB_CONFIG):

    {
      "search": "ddg",                     // or "searxng"
      "ddg_url": "https://html.duckduckgo.com/html/",
      "searxng_url": "https://searx.example",
      "api_key": ""                        // only for backends that need one
    }

Keys belong in that file, not the environment: the host strips
API_KEY/TOKEN/SECRET/PASSWORD variables from extension processes, so an
env-based key could never work here.
"""

import http.server
import json
import os
import re
import sys
import threading
import urllib.error
import urllib.parse
import urllib.request
from html.parser import HTMLParser

PROTOCOL = 1
USER_AGENT = "glass-core-web/1.0 (text browser; like curl)"
FETCH_TIMEOUT = 15
FETCH_CAP = 512 * 1024
DEFAULT_CHARS = 20_000
MAX_CHARS = 60_000


def config():
    override = os.environ.get("GC_WEB_CONFIG")
    if override:
        path = override
    else:
        xdg = os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config")
        path = os.path.join(xdg, "glass-core", "web.json")
    try:
        with open(path, "r", encoding="utf-8") as fh:
            data = json.load(fh)
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


class TextExtractor(HTMLParser):
    """HTML to readable text: blocks on block tags, links inline, scripts out."""

    SKIP = {"script", "style", "noscript", "template", "svg", "head"}
    BLOCK = {
        "p", "div", "br", "li", "ul", "ol", "tr", "table", "section",
        "article", "header", "footer", "h1", "h2", "h3", "h4", "h5", "h6",
        "pre", "blockquote", "nav", "main", "aside", "form", "figure", "hr",
    }

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.parts = []
        self.skip = 0
        self.href = ""

    def handle_starttag(self, tag, attrs):
        if tag in self.SKIP:
            self.skip += 1
            return
        if self.skip:
            return
        if tag in self.BLOCK:
            self.parts.append("\n")
        if tag == "a":
            href = dict(attrs).get("href") or ""
            self.href = href if href.startswith(("http://", "https://")) else ""

    def handle_endtag(self, tag):
        if tag in self.SKIP:
            self.skip = max(0, self.skip - 1)
            return
        if self.skip:
            return
        if tag == "a":
            if self.href:
                self.parts.append(f" <{self.href}>")
            self.href = ""
        elif tag in self.BLOCK:
            self.parts.append("\n")

    def handle_data(self, data):
        if self.skip or not data.strip():
            return
        self.parts.append(data if data.endswith((" ", "\n")) else data + " ")

    def text(self):
        joined = "".join(self.parts)
        lines = [" ".join(line.split()) for line in joined.splitlines()]
        out = []
        for line in lines:
            if line or (out and out[-1]):
                out.append(line)
        return "\n".join(out).strip()

    def extract(self, body):
        self.feed(body)
        return self.text()


class DuckResults(HTMLParser):
    """Best-effort parse of the DuckDuckGo HTML endpoint's result list."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.results = []
        self.grab = None

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        cls = attrs.get("class") or ""
        if tag == "a" and "result__a" in cls:
            self.results.append(
                {"title": "", "url": unwrap_ddg(attrs.get("href") or ""), "snippet": ""}
            )
            self.grab = "title"
        elif "result__snippet" in cls and self.results:
            self.grab = "snippet"

    def handle_endtag(self, tag):
        if tag in ("a", "div", "td", "span") and self.grab:
            self.grab = None

    def handle_data(self, data):
        if self.grab and self.results:
            self.results[-1][self.grab] += data


def unwrap_ddg(href):
    """DuckDuckGo wraps results in /l/?uddg=<encoded>."""
    if "uddg=" in href:
        query = urllib.parse.urlparse(href).query
        target = urllib.parse.parse_qs(query).get("uddg", [""])[0]
        if target:
            return target
    if href.startswith("//"):
        return "https:" + href
    return href


def get(url, accept="text/html,application/xhtml+xml;q=0.9,*/*;q=0.5"):
    """GET a URL with the fetch caps applied. Returns (status, ctype, body)."""
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT, "Accept": accept})
    with urllib.request.urlopen(request, timeout=FETCH_TIMEOUT) as response:
        raw = response.read(FETCH_CAP + 1)
        ctype = response.headers.get("Content-Type", "")
        status = response.status
    capped = len(raw) > FETCH_CAP
    raw = raw[:FETCH_CAP]
    charset = "utf-8"
    match = re.search(r"charset=([\w.-]+)", ctype, re.IGNORECASE)
    if match:
        charset = match.group(1)
    body = raw.decode(charset, errors="replace")
    return status, ctype, body, capped


def extract(body, ctype):
    if "html" in ctype.lower() or body.lstrip()[:1] == "<":
        return TextExtractor().extract(body)
    return body


def web_fetch(url, max_chars):
    if not url.startswith(("http://", "https://")):
        return "web_fetch: url must be http(s)"
    max_chars = max(1_000, min(int(max_chars or DEFAULT_CHARS), MAX_CHARS))
    try:
        status, ctype, body, capped = get(url)
    except urllib.error.HTTPError as error:
        return f"web_fetch: HTTP {error.code} for {url}"
    except Exception as error:  # timeouts, DNS, TLS, bad URLs
        return f"web_fetch: {error}"
    text = TextExtractor().extract(body) if "html" in ctype.lower() else body
    tail = []
    if len(text) > max_chars:
        text = text[:max_chars]
        tail.append(f"[truncated at {max_chars} chars; use bash+curl for the rest]")
    if capped:
        tail.append(f"[response body capped at {FETCH_CAP} bytes]")
    suffix = ("\n\n" + "\n".join(tail)) if tail else ""
    return f"{text}{suffix}\n\n[fetched {url} · HTTP {status} · {ctype or 'unknown type'}]"


def web_search(query, count):
    query = (query or "").strip()
    if not query:
        return "web_search: query is empty"
    count = max(1, min(int(count or 5), 10))
    cfg = config()
    backend = str(cfg.get("search") or "ddg").lower()
    try:
        if backend == "searxng":
            base = str(cfg.get("searxng_url") or "").rstrip("/")
            if not base:
                return "web_search: set \"searxng_url\" in ~/.config/glass-core/web.json"
            url = f"{base}/search?q={urllib.parse.quote(query)}&format=json"
            status, _, body, _ = get(url, accept="application/json")
            data = json.loads(body)
            rows = data.get("results", [])[:count]
            if not rows:
                return "web_search: no results"
            out = []
            for i, row in enumerate(rows, 1):
                out.append(
                    f"{i}. {row.get('title', '')}\n   {row.get('url', '')}\n"
                    f"   {row.get('content', '')}".rstrip()
                )
            return "\n".join(out)
        # Default: DuckDuckGo's HTML endpoint.
        base = str(cfg.get("ddg_url") or "https://html.duckduckgo.com/html/")
        sep = "&" if "?" in base else "?"
        url = f"{base}{sep}q={urllib.parse.quote(query)}"
        status, _, body, _ = get(url)
        parser = DuckResults()
        parser.feed(body)
        rows = [r for r in parser.results if r["url"]][:count]
        if not rows:
            return (
                "web_search: no results parsed (the endpoint may be rate-limiting; "
                "consider a searxng_url in ~/.config/glass-core/web.json)"
            )
        out = []
        for i, row in enumerate(rows, 1):
            snippet = " ".join(row["snippet"].split())
            line = f"{i}. {' '.join(row['title'].split())}\n   {row['url']}"
            if snippet:
                line += f"\n   {snippet}"
            out.append(line)
        return "\n".join(out)
    except urllib.error.HTTPError as error:
        return f"web_search: HTTP {error.code} from the {backend} backend"
    except Exception as error:
        return f"web_search: {error}"


MANIFEST = {
    "name": "web",
    "version": "1.0.0",
    "system_prompt": (
        "You can read the web with `web_fetch` (one URL as readable text) and "
        "`web_search` (find current pages). Use them instead of guessing about "
        "library versions, changelogs, or error messages; use bash+curl when you "
        "need the raw bytes, headers, or a POST."
    ),
    "tools": [
        {
            "name": "web_fetch",
            "description": (
                "Fetch one URL and return readable text. HTML is stripped to "
                "prose with links inline; scripts and styles are dropped; the "
                "result is capped. Use for documentation, changelogs, and issue "
                "pages. Raw bytes and non-GET requests: use curl in bash."
            ),
            "schema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "The http(s) URL to read."},
                    "max_chars": {
                        "type": "integer",
                        "description": "Cap the returned text (default 20000, max 60000).",
                    },
                },
                "required": ["url"],
            },
            "concurrency": "parallel",
        },
        {
            "name": "web_search",
            "description": (
                "Search the web and return titles, URLs, and snippets. Defaults "
                "to DuckDuckGo; a SearXNG instance can be configured in "
                "~/.config/glass-core/web.json."
            ),
            "schema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "The search query."},
                    "count": {
                        "type": "integer",
                        "description": "How many results (default 5, max 10).",
                    },
                },
                "required": ["query"],
            },
            "concurrency": "parallel",
        },
    ],
}


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def read_line():
    line = sys.stdin.readline()
    if not line:
        return None
    line = line.strip()
    return json.loads(line) if line else None


def handle_run_tool(req):
    try:
        args = json.loads(req.get("arguments") or "{}")
    except ValueError:
        return "arguments are not valid JSON"
    if req.get("name") == "web_fetch":
        return web_fetch(str(args.get("url") or ""), args.get("max_chars"))
    if req.get("name") == "web_search":
        return web_search(str(args.get("query") or ""), args.get("count"))
    return "unknown tool"


def serve():
    """NDJSON loop, the extension's normal life."""
    while True:
        req = read_line()
        if req is None:
            break
        kind = req.get("type")
        if kind == "hello":
            reply = {"id": req.get("id"), "protocol": PROTOCOL}
            reply.update(MANIFEST)
            send(reply)
        elif kind == "system_prompt":
            send({"id": req.get("id"), "text": MANIFEST.get("system_prompt", "")})
        elif kind == "authorize":
            send({"id": req.get("id"), "allow": True})
        elif kind == "run_tool":
            send({"id": req.get("id"), "output": handle_run_tool(req)})
        elif kind in ("goodbye", "shutdown"):
            break


def selftest():
    """Offline check against a local fixture server; no real network."""
    page = b"""<html><head><style>p{color:red}</style><script>EVIL()</script></head>
<body><h1>Fixture title</h1><p>First paragraph with <a href="https://example.com/doc">a link</a>.</p>
<ul><li>one</li><li>two</li></ul></body></html>"""
    search = b"""<html><body>
<a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fone">Example One</a>
<a class="result__snippet">The first snippet.</a>
<a class="result__a" href="https://example.com/two">Example Two</a>
<a class="result__snippet">The second snippet.</a>
</body></html>"""

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            if self.path.startswith("/search"):
                body = search
            elif self.path.startswith("/page"):
                body = page
            else:
                self.send_response(404)
                self.send_header("Content-Length", "0")
                self.end_headers()
                return
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    import tempfile

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    port = server.server_port
    with tempfile.TemporaryDirectory() as tmp:
        cfg = os.path.join(tmp, "web.json")
        with open(cfg, "w", encoding="utf-8") as fh:
            json.dump(
                {
                    "search": "ddg",
                    "ddg_url": f"http://127.0.0.1:{port}/search",
                },
                fh,
            )
        os.environ["GC_WEB_CONFIG"] = cfg
        text = web_fetch(f"http://127.0.0.1:{port}/page", None)
        assert "Fixture title" in text, text
        assert "First paragraph" in text, text
        assert "<https://example.com/doc>" in text, text
        assert "EVIL" not in text and "color:red" not in text, text
        assert "one" in text and "two" in text, text
        results = web_search("fixture", None)
        assert "Example One" in results and "https://example.com/one" in results, results
        assert "The first snippet." in results, results
        assert "Example Two" in results and "https://example.com/two" in results, results
        missing = web_fetch(f"http://127.0.0.1:{port}/missing", None)
        assert "HTTP 404" in missing, missing
        assert web_fetch("ftp://example.com", None).startswith("web_fetch: url must be http(s)")
    server.shutdown()
    print("PASS web extension selftest")


if __name__ == "__main__":
    if "--selftest" in sys.argv:
        selftest()
    else:
        serve()
