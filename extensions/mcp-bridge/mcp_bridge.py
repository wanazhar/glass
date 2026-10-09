#!/usr/bin/env python3
"""glass MCP bridge: make Model Context Protocol tools callable, out of process.

The core binary keeps its "no MCP" size argument — this bridge is an ordinary
glass extension. It reads `~/.config/glass-core/mcp.json` (or GC_MCP_CONFIG):

    {
      "servers": {
        "files": { "command": ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/tmp"] },
        "notes": { "command": ["python3", "/path/notes_server.py"], "env": { "NOTES_DIR": "~/notes" } },
        "remote": { "type": "streamable-http", "url": "https://mcp.example.com/mcp",
                    "headers": { "Authorization": "Bearer …" } }
      }
    }

Each server's tools are declared namespaced as `mcp__<server>__<tool>` with
their JSON Schema passed through. Beyond 40 tools the manifest switches to two
meta-tools, `mcp_search` and `mcp_call`, so a large server does not inflate
every prompt (the same "deferred tools" idea Claude Code's tool search uses).

Transports: stdio and Streamable HTTP. Version negotiation probes with
`server/discover` first (the stateless 2026-07-28 revision, where every
request carries `_meta` instead of a handshake) and falls back to the classic
`initialize` handshake for older servers, which remain the majority. The
legacy HTTP+SSE transport is deprecated in the spec and refused with a
pointer to `streamable-http`. A server that fails to start or dies is
reported on stderr and skipped; it never hangs a turn.
"""

import json
import ipaddress
import os
import select
import shutil
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

PROTOCOL = 1
CLIENT_INFO = {"name": "glass-core", "version": "1.0"}
# The stateless revision (2026-07-28) replaced the initialize handshake with
# per-request _meta and a required server/discover; the classic handshake is
# kept for older servers, which remain the majority.
STATELESS_VERSION = "2026-07-28"
LEGACY_VERSION = "2024-11-05"
INIT_TIMEOUT = 8.0
CALL_TIMEOUT = 120.0
DEFER_AT = 40  # switch to search/call meta-tools above this many tools
MAX_HTTP_RESPONSE = 8 * 1024 * 1024
# Agent Plugins 1.0 schema identifiers (agent-plugins.org).
PLUGIN_MCP_SCHEMA = "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json"


def stateless_meta(version):
    return {
        "io.modelcontextprotocol/protocolVersion": version,
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": CLIENT_INFO,
    }


def config_dir():
    xdg = os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config")
    return os.path.join(xdg, "glass-core")


def config_path():
    override = os.environ.get("GC_MCP_CONFIG")
    if override:
        return override
    return os.path.join(config_dir(), "mcp.json")


def load_servers():
    servers = {}
    try:
        with open(config_path(), "r", encoding="utf-8") as fh:
            data = json.load(fh)
        configured = data.get("servers") if isinstance(data, dict) else None
        if isinstance(configured, dict):
            servers.update(configured)
    except FileNotFoundError:
        pass
    except (OSError, ValueError) as error:
        print(f"mcp-bridge: cannot read {config_path()}: {error}", file=sys.stderr)
    for name, spec in plugin_servers().items():
        servers.setdefault(name, spec)
    return servers


def expand(text, root, data):
    """Single, non-recursive ${PLUGIN_ROOT}/${PLUGIN_DATA} replacement."""
    return text.replace("${PLUGIN_ROOT}", root).replace("${PLUGIN_DATA}", data)


def plugin_servers():
    """MCP servers declared by installed Agent Plugins (`plugins/*/mcp.json`).

    Implements the spec's loading rules: fixed location, schema match,
    containment for `./command` and `cwd`, placeholder expansion in
    args/env/cwd, and per-entry failure isolation (a bad server never hides
    the others or any skills).
    """
    out = {}
    plugins = os.path.join(config_dir(), "plugins")
    if not os.path.isdir(plugins):
        return out
    for name in sorted(os.listdir(plugins)):
        pdir = os.path.join(plugins, name)
        if name == "data" or not os.path.isdir(pdir):
            continue
        mcp = os.path.join(pdir, "mcp.json")
        if not os.path.isfile(mcp):
            continue
        try:
            with open(mcp, "r", encoding="utf-8") as fh:
                data = json.load(fh)
        except (OSError, ValueError) as error:
            print(f"mcp-bridge: plugin {name}: mcp.json unreadable: {error}", file=sys.stderr)
            continue
        if not isinstance(data, dict) or data.get("$schema") != PLUGIN_MCP_SCHEMA:
            print(
                f"mcp-bridge: plugin {name}: unsupported mcp.json schema; MCP disabled for it",
                file=sys.stderr,
            )
            continue
        root = os.path.realpath(pdir)
        pdata = os.path.join(plugins, "data", name)
        try:
            os.makedirs(pdata, exist_ok=True)
        except OSError:
            pass
        for sname, spec in (data.get("mcpServers") or {}).items():
            if not isinstance(spec, dict):
                continue
            kind = spec.get("type", "stdio")
            key = f"{name}.{sname}"
            if kind == "streamable-http":
                url = spec.get("url")
                if not isinstance(url, str) or not url.startswith(("http://", "https://")):
                    print(
                        f"mcp-bridge: plugin {name}/{sname}: url must be absolute http(s); skipped",
                        file=sys.stderr,
                    )
                    continue
                headers = {
                    str(k): str(v) for k, v in (spec.get("headers") or {}).items()
                }
                # The spec forbids placeholder expansion in url/header values:
                # headers are visible package data, not a secret channel.
                out[key] = {"type": "streamable-http", "url": url, "headers": headers}
                continue
            if kind != "stdio":
                print(
                    f"mcp-bridge: plugin {name}/{sname}: transport `{kind}` not supported yet",
                    file=sys.stderr,
                )
                continue
            command = spec.get("command")
            if not isinstance(command, str) or not command:
                print(
                    f"mcp-bridge: plugin {name}/{sname}: command must be one executable token",
                    file=sys.stderr,
                )
                continue
            if command.startswith("./"):
                resolved = os.path.realpath(os.path.join(root, command))
                if not resolved.startswith(root + os.sep):
                    print(
                        f"mcp-bridge: plugin {name}/{sname}: command escapes the plugin root; skipped",
                        file=sys.stderr,
                    )
                    continue
                command = resolved
            args = [expand(str(a), root, pdata) for a in (spec.get("args") or [])]
            env = {
                str(k): expand(str(v), root, pdata)
                for k, v in (spec.get("env") or {}).items()
            }
            cwd = root
            configured_cwd = spec.get("cwd")
            if configured_cwd:
                candidate = os.path.realpath(expand(str(configured_cwd), root, pdata))
                inside = candidate == root or candidate.startswith(root + os.sep)
                in_data = candidate == pdata or candidate.startswith(pdata + os.sep)
                if not (inside or in_data):
                    print(
                        f"mcp-bridge: plugin {name}/{sname}: cwd escapes the plugin root; skipped",
                        file=sys.stderr,
                    )
                    continue
                cwd = candidate
            env["PLUGIN_ROOT"] = root
            env["PLUGIN_DATA"] = pdata
            out[f"{name}.{sname}"] = {"command": [command] + args, "env": env, "cwd": cwd}
    return out


class BaseServer:
    """Shared call surface for both transports."""

    def __init__(self, name):
        self.name = name
        self.tools = []
        self.dead = None
        # CacheableResult hints (2026-07-28): how long this list may be
        # cached, and at what scope. Only the deferred index honours the
        # TTL; the non-deferred manifest is the session's declared tool
        # list and cannot change once the harness has frozen it.
        self.ttl_ms = None
        self.cache_scope = None
        self.listed_at = 0.0

    def list_tools(self):
        result = self.request("tools/list", {}, INIT_TIMEOUT)
        ttl = result.get("ttlMs")
        self.ttl_ms = ttl if isinstance(ttl, (int, float)) and ttl >= 0 else None
        scope = result.get("cacheScope")
        self.cache_scope = scope if isinstance(scope, str) else None
        self.listed_at = time.time()
        tools = result.get("tools") or []
        return [t for t in tools if isinstance(t, dict) and t.get("name")]

    def stale(self):
        """Whether the cached tools/list has outlived its TTL."""
        if self.ttl_ms is None:
            return False
        return (time.time() - self.listed_at) * 1000.0 > self.ttl_ms

    def call_tool(self, tool, arguments):
        if self.dead:
            return f"mcp({self.name}): server is not running ({self.dead})"
        try:
            result = self.request(
                "tools/call",
                {"name": tool, "arguments": arguments or {}},
                CALL_TIMEOUT,
            )
        except Exception as error:
            if isinstance(error, (EOFError, BrokenPipeError, OSError)):
                self.dead = "connection closed"
            return f"mcp({self.name}): {error}"
        if result.get("resultType") == "input_required":
            return (
                f"mcp({self.name}): the tool needs input from the client "
                "(multi round-trip requests are not supported yet)"
            )
        parts = []
        for item in result.get("content") or []:
            if isinstance(item, dict) and item.get("type") == "text":
                parts.append(str(item.get("text", "")))
            elif isinstance(item, dict):
                parts.append(f"[{item.get('type', 'non-text')} content omitted]")
        text = "\n".join(part for part in parts if part)
        if not text:
            if result.get("structuredContent") is not None:
                text = json.dumps(result["structuredContent"])
            elif result.get("isError"):
                text = "(tool reported an error with no message)"
            else:
                text = "(no content)"
        if result.get("isError"):
            text = f"error: {text}"
        return text


class Server(BaseServer):
    """One stdio MCP server: spawn, negotiate, call."""

    def __init__(self, name, spec):
        super().__init__(name)
        self.stateless = False
        self.version = LEGACY_VERSION
        command = spec.get("command") if isinstance(spec, dict) else None
        if not isinstance(command, list) or not command:
            raise ValueError("`command` must be a non-empty argument list")
        env = dict(os.environ)
        for key, value in (spec.get("env") or {}).items():
            env[str(key)] = os.path.expanduser(str(value))
        # Plugin servers carry a resolved cwd (plugin root or ${PLUGIN_DATA}).
        cwd = spec.get("cwd") if isinstance(spec.get("cwd"), str) else None
        self.proc = subprocess.Popen(
            [str(part) for part in command],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            env=env,
            cwd=cwd,
        )
        self.buffer = b""
        self.next_id = 0
        try:
            self.version = self.handshake()
        except Exception as error:
            raise RuntimeError(f"handshake failed: {error}") from error

    def handshake(self):
        """server/discover first (2026-07-28), classic initialize otherwise.

        The spec recommends exactly this probe on stdio for backward
        compatibility: a stateless server answers `server/discover`; an older
        one answers method-not-found and gets the classic handshake instead.
        """
        try:
            result = self.request(
                "server/discover",
                {"_meta": stateless_meta(STATELESS_VERSION)},
                INIT_TIMEOUT,
                pad=False,
            )
        except Exception:
            self.stateless = False
            self.request(
                "initialize",
                {
                    "protocolVersion": LEGACY_VERSION,
                    "capabilities": {},
                    "clientInfo": CLIENT_INFO,
                },
                INIT_TIMEOUT,
                pad=False,
            )
            self.notify("notifications/initialized")
            return LEGACY_VERSION
        self.stateless = True
        return str(result.get("protocolVersion") or STATELESS_VERSION)

    def send(self, message):
        self.proc.stdin.write((json.dumps(message) + "\n").encode("utf-8"))
        self.proc.stdin.flush()

    def notify(self, method, params=None):
        message = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            message["params"] = params
        self.send(message)

    def read_message(self, timeout):
        deadline = time.monotonic() + timeout
        while b"\n" not in self.buffer:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("no reply")
            readable, _, _ = select.select([self.proc.stdout], [], [], remaining)
            if not readable:
                raise TimeoutError("no reply")
            chunk = self.proc.stdout.read1(65536)
            if not chunk:
                raise EOFError("server closed the stream")
            self.buffer += chunk
        line, _, self.buffer = self.buffer.partition(b"\n")
        return json.loads(line.decode("utf-8", "replace"))

    def request(self, method, params, timeout, pad=True):
        self.next_id += 1
        message = {"jsonrpc": "2.0", "id": self.next_id, "method": method}
        if params is not None:
            message["params"] = dict(params)
        if pad and self.stateless and isinstance(message.get("params"), dict):
            message["params"].setdefault("_meta", stateless_meta(self.version))
        self.send(message)
        deadline = time.monotonic() + timeout
        while True:
            remaining = max(0.1, deadline - time.monotonic())
            reply = self.read_message(remaining)
            # Notifications and progress messages carry no matching id.
            if reply.get("id") != self.next_id:
                continue
            if "error" in reply:
                error = reply["error"]
                raise RuntimeError(error.get("message") or json.dumps(error))
            return reply.get("result") or {}

    def shutdown(self):
        try:
            self.proc.terminate()
            self.proc.wait(timeout=2)
        except Exception:
            try:
                self.proc.kill()
            except Exception:
                pass


class HttpServer(BaseServer):
    """Streamable HTTP MCP server: one POST per request, stateless."""

    def __init__(self, name, spec):
        super().__init__(name)
        self.url = str(spec.get("url") or "")
        parsed = urllib.parse.urlparse(self.url)
        if parsed.scheme not in ("http", "https") or not parsed.netloc:
            raise ValueError("url must be an absolute http(s) URL")
        host = (parsed.hostname or "").lower()
        try:
            loopback_ip = ipaddress.ip_address(host).is_loopback
        except ValueError:
            loopback_ip = False
        loopback = host == "localhost" or loopback_ip
        if parsed.scheme == "http" and not loopback:
            raise ValueError("non-loopback MCP endpoints must use HTTPS")
        self.headers = {
            str(key): str(value) for key, value in (spec.get("headers") or {}).items()
        }
        self.next_id = 0
        self.stateless = False
        self.version = LEGACY_VERSION
        self.session_id = None
        try:
            self.version = self.handshake()
        except Exception as error:
            raise RuntimeError(f"handshake failed: {error}") from error

    def handshake(self):
        try:
            result = self.request(
                "server/discover",
                {"_meta": stateless_meta(STATELESS_VERSION)},
                INIT_TIMEOUT,
                pad=False,
            )
        except Exception:
            self.stateless = False
            self.request(
                "initialize",
                {
                    "protocolVersion": LEGACY_VERSION,
                    "capabilities": {},
                    "clientInfo": CLIENT_INFO,
                },
                INIT_TIMEOUT,
                pad=False,
            )
            try:
                self.post(
                    {"jsonrpc": "2.0", "method": "notifications/initialized"},
                    INIT_TIMEOUT,
                )
            except Exception:
                pass
            return self.version
        self.stateless = True
        return str(result.get("protocolVersion") or STATELESS_VERSION)

    def post(self, payload, timeout):
        method = str(payload.get("method") or "")
        headers = {
            "Content-Type": "application/json",
            "Accept": "application/json, text/event-stream",
            "Mcp-Method": method,
            **self.headers,
        }
        if payload.get("method") != "initialize":
            headers["MCP-Protocol-Version"] = self.version
            if self.session_id:
                headers["MCP-Session-Id"] = self.session_id
        tool_name = (payload.get("params") or {}).get("name")
        if isinstance(tool_name, str) and tool_name:
            headers["Mcp-Name"] = tool_name
        request = urllib.request.Request(
            self.url,
            data=json.dumps(payload).encode("utf-8"),
            headers=headers,
            method="POST",
        )
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                content_type = response.headers.get_content_type()
                if payload.get("method") == "initialize":
                    self.session_id = response.headers.get("MCP-Session-Id")
                if response.status == 202:
                    if "id" in payload:
                        raise RuntimeError("server accepted request without returning a response")
                    return None
                if content_type == "text/event-stream":
                    return self._read_sse_response(response, payload, timeout)
                body = self._read_http_body(response, timeout)
        except urllib.error.HTTPError as error:
            if error.code == 401:
                raise RuntimeError(
                    "unauthorized (401): add a bearer token or headers to mcp.json"
                ) from error
            raise RuntimeError(f"HTTP {error.code}") from error
        text = body.decode("utf-8", "replace")
        if content_type == "text/event-stream" or text.lstrip().startswith("data:"):
            for message in self._sse_messages(text):
                if message.get("id") == payload.get("id"):
                    self._record_initialize_version(payload, message)
                    return message
            raise RuntimeError("no reply on the event stream")
        if not text.strip():
            return None
        reply = json.loads(text)
        self._record_initialize_version(payload, reply)
        return reply

    def _record_initialize_version(self, payload, reply):
        if payload.get("method") == "initialize":
            result = reply.get("result") or {}
            self.version = str(result.get("protocolVersion") or LEGACY_VERSION)

    @staticmethod
    def _sse_messages(text):
        data = []
        for line in text.splitlines():
            if line.startswith("data:"):
                data.append(line[5:].lstrip())
            elif not line and data:
                try:
                    yield json.loads("\n".join(data))
                except ValueError:
                    pass
                data = []

    def _read_http_body(self, response, timeout):
        length = response.headers.get("Content-Length")
        if length is not None and int(length) > MAX_HTTP_RESPONSE:
            raise RuntimeError("HTTP response exceeds 8 MiB limit")
        deadline = time.monotonic() + timeout
        body = bytearray()
        while True:
            chunk = self._read_http_chunk(response, deadline)
            if not chunk:
                return bytes(body)
            body.extend(chunk)
            if len(body) > MAX_HTTP_RESPONSE:
                raise RuntimeError("HTTP response exceeds 8 MiB limit")

    def _read_sse_response(self, response, payload, timeout):
        """Read complete SSE events through the matching reply under one deadline."""
        deadline = time.monotonic() + timeout
        pending = bytearray()
        event_data = []
        total = 0
        while True:
            chunk = self._read_http_chunk(response, deadline, 4096)
            if not chunk:
                raise RuntimeError("no reply on the event stream")
            total += len(chunk)
            if total > MAX_HTTP_RESPONSE:
                raise RuntimeError("HTTP response exceeds 8 MiB limit")
            pending.extend(chunk)
            while b"\n" in pending:
                raw_line, _, rest = pending.partition(b"\n")
                pending = bytearray(rest)
                line = raw_line.decode("utf-8", "strict").rstrip("\r")
                if line.startswith("data:"):
                    event_data.append(line[5:].lstrip())
                elif not line and event_data:
                    try:
                        message = json.loads("\n".join(event_data))
                    except ValueError:
                        event_data = []
                        continue
                    event_data = []
                    if message.get("id") == payload.get("id"):
                        self._record_initialize_version(payload, message)
                        return message

    @staticmethod
    def _read_http_chunk(response, deadline, size=65536):
        if response.fp is None:
            return b""
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("HTTP response timed out")
        try:
            # Reset the socket timeout to the remaining total deadline before
            # each read so many small chunks cannot extend the request forever.
            sock = response.fp.raw._sock
            sock.settimeout(remaining)
            return response.read1(size)
        except (socket.timeout, TimeoutError) as error:
            raise TimeoutError("HTTP response timed out") from error

    def request(self, method, params, timeout, pad=True):
        self.next_id += 1
        payload = {
            "jsonrpc": "2.0",
            "id": self.next_id,
            "method": method,
            "params": dict(params or {}),
        }
        if pad and self.stateless:
            payload["params"].setdefault("_meta", stateless_meta(self.version))
        reply = self.post(payload, timeout)
        if not isinstance(reply, dict):
            raise RuntimeError("server returned an invalid JSON-RPC response")
        if "error" in reply:
            raise RuntimeError(
                reply["error"].get("message") or json.dumps(reply["error"])
            )
        return reply.get("result") or {}


def make_server(name, spec):
    if not isinstance(spec, dict):
        raise ValueError("server configuration must be an object")
    kind = spec.get("type") or "stdio"
    if kind == "stdio":
        return Server(name, spec)
    if kind == "streamable-http":
        return HttpServer(name, spec)
    if kind == "sse":
        raise ValueError("the legacy HTTP+SSE transport is deprecated; use streamable-http")
    raise ValueError(f"unknown transport `{kind}`")


def namespaced(server, tool):
    return f"mcp__{server}__{tool}"


def schema_of(tool):
    schema = tool.get("inputSchema")
    if not isinstance(schema, dict):
        schema = {"type": "object"}
    schema.setdefault("type", "object")
    return schema


class Bridge:
    def __init__(self):
        self.servers = {}
        self.entries = {}  # namespaced name -> (server name, tool name, description)
        for name, spec in load_servers().items():
            try:
                server = make_server(str(name), spec)
                server.tools = server.list_tools()
                self.servers[server.name] = server
                self.index(server)
            except Exception as error:
                print(f"mcp-bridge: server `{name}` skipped: {error}", file=sys.stderr)
        self.deferred = len(self.entries) > DEFER_AT

    def index(self, server):
        """Add one server's tools to the addressable catalogue."""
        for tool in server.tools:
            full = namespaced(server.name, tool["name"])
            if full in self.entries:
                print(f"mcp-bridge: duplicate tool {full}, keeping the first", file=sys.stderr)
                continue
            self.entries[full] = (
                server.name,
                tool["name"],
                str(tool.get("description") or ""),
            )

    def refresh(self):
        """Re-fetch servers whose tools/list TTL has expired.

        Only the deferred index is refreshed: it is reached through
        `mcp_search`, so replacing it changes no declared tool. The
        non-deferred manifest is the session's declared tool list, and
        the harness freezes that list for prompt-cache stability, so a
        re-fetch there would not reach the model anyway.
        """
        for server in self.servers.values():
            if not server.stale():
                continue
            try:
                server.tools = server.list_tools()
            except Exception as error:
                print(
                    f"mcp-bridge: refresh of `{server.name}` failed: {error}",
                    file=sys.stderr,
                )
                continue
            for full in [
                full
                for full, (owner, _, _) in self.entries.items()
                if owner == server.name
            ]:
                del self.entries[full]
            self.index(server)

    def manifest_tools(self):
        if not self.entries:
            return []
        if self.deferred:
            return [
                {
                    "name": "mcp_search",
                    "description": (
                        f"Search the {len(self.entries)} tools from configured MCP "
                        "servers and return matching names with descriptions."
                    ),
                    "schema": {
                        "type": "object",
                        "properties": {
                            "query": {"type": "string", "description": "Substring to match."}
                        },
                        "required": ["query"],
                    },
                    "concurrency": "parallel",
                },
                {
                    "name": "mcp_call",
                    "description": (
                        "Call one MCP tool by its `mcp__server__tool` name. "
                        "`arguments` is a JSON object string for that tool."
                    ),
                    "schema": {
                        "type": "object",
                        "properties": {
                            "tool": {
                                "type": "string",
                                "description": "Namespaced tool name, e.g. mcp__files__read.",
                            },
                            "arguments": {
                                "type": "string",
                                "description": "JSON object with the tool's arguments.",
                            },
                        },
                        "required": ["tool"],
                    },
                    "concurrency": "parallel",
                },
            ]
        tools = []
        # Sorted by namespaced name: the spec recommends deterministic
        # tools/list ordering, and a stable order is what keeps the
        # declared prefix byte-identical across sessions.
        for full, (server, tool, description) in sorted(self.entries.items()):
            tools.append(
                {
                    "name": full,
                    "description": f"[{server}] {description}".strip(),
                    "schema": schema_of(
                        next(
                            t
                            for t in self.servers[server].tools
                            if t["name"] == tool
                        )
                    ),
                    "concurrency": "parallel",
                }
            )
        return tools

    def system_prompt(self):
        if not self.entries:
            return ""
        if self.deferred:
            return (
                f"{len(self.entries)} tools from MCP servers are available through "
                "`mcp_search` and `mcp_call` (names look like `mcp__server__tool`)."
            )
        return (
            "Tools from MCP servers are exposed as `mcp__<server>__<tool>`; "
            "their descriptions carry the server name."
        )

    def run_tool(self, name, arguments):
        if self.deferred:
            if name == "mcp_search":
                # The index is a cache: honour each server's TTL
                # before answering a search over it.
                self.refresh()
                query = str((arguments or {}).get("query") or "").lower()
                hits = [
                    f"{full} — {description}"
                    for full, (_, _, description) in sorted(self.entries.items())
                    if query in full.lower() or query in description.lower()
                ]
                if not hits:
                    return f"mcp_search: no tools match `{query}`"
                return "\n".join(hits[:40])
            if name == "mcp_call":
                target = str((arguments or {}).get("tool") or "")
                raw = (arguments or {}).get("arguments")
                if isinstance(raw, str):
                    try:
                        raw = json.loads(raw or "{}")
                    except ValueError as error:
                        return f"mcp_call: arguments is not valid JSON ({error})"
                if not isinstance(raw, dict):
                    raw = {}
                return self.call(target, raw)
            return f"unknown tool {name}"
        if name not in self.entries:
            return f"unknown tool {name}"
        return self.call(name, arguments or {})

    def call(self, full, arguments):
        entry = self.entries.get(full)
        if entry is None:
            return f"mcp: unknown tool {full}; use mcp_search for the catalogue"
        server, tool, _ = entry
        return self.servers[server].call_tool(tool, arguments)

    def shutdown(self):
        for server in self.servers.values():
            server.shutdown()


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def read_line():
    line = sys.stdin.readline()
    if not line:
        return None
    line = line.strip()
    return json.loads(line) if line else None


def serve():
    bridge = Bridge()
    tools = bridge.manifest_tools()
    manifest = {
        "name": "mcp-bridge",
        "version": "1.0.0",
        "system_prompt": bridge.system_prompt(),
        "tools": tools,
    }
    try:
        while True:
            req = read_line()
            if req is None:
                break
            kind = req.get("type")
            if kind == "hello":
                reply = {"id": req.get("id"), "protocol": PROTOCOL}
                reply.update(manifest)
                send(reply)
            elif kind == "system_prompt":
                send({"id": req.get("id"), "text": manifest["system_prompt"]})
            elif kind == "authorize":
                send({"id": req.get("id"), "allow": True})
            elif kind == "run_tool":
                try:
                    arguments = json.loads(req.get("arguments") or "{}")
                except ValueError:
                    output = "arguments are not valid JSON"
                else:
                    output = bridge.run_tool(req.get("name") or "", arguments)
                send({"id": req.get("id"), "output": output})
            elif kind in ("goodbye", "shutdown"):
                break
    finally:
        bridge.shutdown()


if __name__ == "__main__":
    serve()
