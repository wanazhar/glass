#!/usr/bin/env python3
"""glass MCP bridge: make Model Context Protocol tools callable, out of process.

The core binary keeps its "no MCP" size argument — this bridge is an ordinary
glass extension. It reads `~/.config/glass-core/mcp.json` (or GC_MCP_CONFIG):

    {
      "servers": {
        "files": { "command": ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/tmp"] },
        "notes": { "command": ["python3", "/path/notes_server.py"], "env": { "NOTES_DIR": "~/notes" } }
      }
    }

Each server's tools are declared namespaced as `mcp__<server>__<tool>` with
their JSON Schema passed through. Beyond 40 tools the manifest switches to two
meta-tools, `mcp_search` and `mcp_call`, so a large server does not inflate
every prompt (the same "deferred tools" idea Claude Code's tool search uses).

Transports: stdio only. HTTP/SSE MCP servers are deliberately out of scope.
A server that fails to start or dies is reported on stderr and skipped; it
never hangs a turn.
"""

import json
import os
import select
import subprocess
import sys
import time

PROTOCOL = 1
CLIENT_INFO = {"name": "glass-core", "version": "1.0"}
PROTOCOL_VERSION = "2024-11-05"
INIT_TIMEOUT = 8.0
CALL_TIMEOUT = 120.0
DEFER_AT = 40  # switch to search/call meta-tools above this many tools
# Agent Plugins 1.0 schema identifiers (agent-plugins.org).
PLUGIN_MCP_SCHEMA = "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json"


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


class Server:
    """One stdio MCP server: spawn, initialize, call."""

    def __init__(self, name, spec):
        self.name = name
        self.tools = []
        self.dead = None
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
            self.request(
                "initialize",
                {
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": CLIENT_INFO,
                },
                INIT_TIMEOUT,
            )
            self.notify("notifications/initialized")
        except Exception as error:
            raise RuntimeError(f"initialize failed: {error}") from error

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

    def request(self, method, params, timeout):
        self.next_id += 1
        message = {"jsonrpc": "2.0", "id": self.next_id, "method": method}
        if params is not None:
            message["params"] = params
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

    def list_tools(self):
        result = self.request("tools/list", {}, INIT_TIMEOUT)
        tools = result.get("tools") or []
        return [t for t in tools if isinstance(t, dict) and t.get("name")]

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

    def shutdown(self):
        try:
            self.proc.terminate()
            self.proc.wait(timeout=2)
        except Exception:
            try:
                self.proc.kill()
            except Exception:
                pass


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
                server = Server(str(name), spec)
                server.tools = server.list_tools()
                self.servers[server.name] = server
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
            except Exception as error:
                print(f"mcp-bridge: server `{name}` skipped: {error}", file=sys.stderr)
        self.deferred = len(self.entries) > DEFER_AT

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
        for full, (server, tool, description) in self.entries.items():
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
