#!/usr/bin/env python3
"""glass subagent extension.

Adds a `subagent` tool that delegates one or more self-contained tasks to
fresh agents, each with its own context. Sub-agents run with the core tools
(bash, state) and report back; their intermediate steps never enter the
parent's conversation, so delegation is cheap in context.

The extension is a thin proxy: it declares the tool and forwards a batch to
the host, which runs the nested agents. Only the host has the provider
credentials and the tools, so the nested loop lives there. Fan-out happens
host-side because one extension is one serial pipe: a batch arrives as a
single request, the host runs up to four tasks at once, and the reports come
back in request order.

Agent definitions live in `~/.config/glass-core/agents/*.md` as frontmatter
(`name`, `description`, optional `model`) plus a body that becomes the
sub-agent's prompt.
"""

import json
import os
import sys

# The wire version this extension speaks. It must match
# what glass expects, and it goes at the top level of the
# handshake reply, not nested inside the manifest.
PROTOCOL = 1

# How many tasks one call may delegate. The host runs four at a time; this
# bound keeps a runaway batch from queueing for the rest of the turn.
MAX_TASKS = 8


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def read_line():
    line = sys.stdin.readline()
    if not line:
        return None
    line = line.strip()
    return json.loads(line) if line else None


def config_dir():
    xdg = os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config")
    return os.path.join(xdg, "glass-core")


def parse_frontmatter(text):
    """Split `---` frontmatter and a body into (fields, body)."""
    if not text.startswith("---"):
        return {}, text
    end = text.find("\n---", 3)
    if end < 0:
        return {}, text
    header = text[3:end]
    body = text[end + 4 :].lstrip("\n")
    fields = {}
    for line in header.splitlines():
        if ":" in line:
            key, _, value = line.partition(":")
            fields[key.strip().lower()] = value.strip().strip('"').strip("'")
    return fields, body


def load_agents():
    """Load agent definitions; a broken file is skipped, never fatal."""
    agents = {}
    directory = os.path.join(config_dir(), "agents")
    try:
        names = sorted(os.listdir(directory))
    except OSError:
        return agents
    for name in names:
        if not name.endswith(".md"):
            continue
        path = os.path.join(directory, name)
        try:
            with open(path, "r", encoding="utf-8", errors="replace") as fh:
                fields, body = parse_frontmatter(fh.read())
        except OSError:
            continue
        agent = {
            "name": fields.get("name") or name[:-3],
            "description": fields.get("description", ""),
            "model": fields.get("model") or None,
            "prompt": body.strip(),
        }
        if agent["prompt"]:
            agents[agent["name"]] = agent
    return agents


AGENTS = load_agents()


def tool_schema():
    item = {
        "type": "object",
        "properties": {
            "task": {
                "type": "string",
                "description": (
                    "The task to delegate, written so someone with no prior "
                    "context can do it: goal, paths, and the expected result."
                ),
            },
            "worktree": {
                "type": "boolean",
                "description": (
                    "Run in an isolated detached git worktree at HEAD, so "
                    "edits cannot collide with the parent's working tree. "
                    "The report includes the path for inspection."
                ),
            },
        },
        "required": ["task"],
    }
    if AGENTS:
        item["properties"]["agent"] = {
            "type": "string",
            "enum": sorted(AGENTS),
            "description": (
                "Named agent definition to run this task with (its prompt and "
                "optional model). Defaults to the built-in subagent framing."
            ),
        }
    names = ", ".join(sorted(AGENTS)) if AGENTS else "none configured"
    return {
        "name": "subagent",
        "description": (
            "Delegate independent tasks to sub-agents and get their reports "
            "back. Each task runs in its own context with the bash and state "
            "tools; up to four run concurrently and the reports return in "
            "order. Use for research, exploration, review, or bulk work that "
            "does not need the current conversation. Every task must be fully "
            "self-contained. Named agent definitions: " + names + "."
        ),
        "schema": {
            "type": "object",
            "properties": {
                "tasks": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_TASKS,
                    "items": item,
                    "description": (
                        "One to eight self-contained tasks. Independent tasks "
                        "in one call run in parallel."
                    ),
                }
            },
            "required": ["tasks"],
        },
        "concurrency": "serial",
    }


def system_prompt():
    agents = ""
    if AGENTS:
        agents = (
            " Named agents are available: "
            + "; ".join(
                f"{a['name']} ({a['description'] or 'no description'})"
                for a in (AGENTS[k] for k in sorted(AGENTS))
            )
            + "."
        )
    return (
        "You can delegate self-contained tasks to sub-agents with the "
        "`subagent` tool, one to eight per call. Independent tasks in one "
        "call run in parallel, each in its own context with the bash and "
        "state tools, so their steps never enter your conversation; only "
        "their reports do. Delegate tasks that are independent of your "
        "current working context — research, exploration, review, bulk file "
        "operations. Spell out the goal, the paths involved, and what a "
        "finished result looks like, because the sub-agent cannot see your "
        "conversation." + agents
    )


MANIFEST = {
    "name": "subagent",
    "version": "2.0.0",
    "system_prompt": system_prompt(),
    "tools": [tool_schema()],
    "commands": [
        {"name": "agents", "description": "list subagent definitions"},
    ],
}


def label_for(item):
    if item.get("agent"):
        return item["agent"]
    task = (item.get("task") or "").strip().replace("\n", " ")
    return (task[:60] + "…") if len(task) > 60 else task


def handle_run_tool(req):
    try:
        args = json.loads(req.get("arguments") or "{}")
    except ValueError:
        return "subagent: arguments are not valid JSON"
    tasks = args.get("tasks")
    if not isinstance(tasks, list) or not tasks:
        return "subagent: provide a non-empty `tasks` array"
    if len(tasks) > MAX_TASKS:
        return f"subagent: at most {MAX_TASKS} tasks per call"
    payload = []
    labels = []
    for i, item in enumerate(tasks):
        if not isinstance(item, dict) or not (item.get("task") or "").strip():
            return f"subagent: task {i + 1} needs non-empty `task` text"
        entry = {"task": item["task"].strip()}
        agent = AGENTS.get(item.get("agent"))
        if agent:
            entry["prompt"] = agent["prompt"]
            if agent["model"]:
                entry["model"] = agent["model"]
        if item.get("model"):
            entry["model"] = item["model"]
        if item.get("worktree"):
            entry["worktree"] = True
        payload.append(entry)
        labels.append(label_for(item))
    # Ask the host to run the batch. This blocks until every task finishes:
    # the host runs them concurrently and writes one report per task back,
    # so there is nothing to poll.
    send({"type": "run_subagents", "tasks": payload})
    outputs = None
    for _ in range(1000):
        reply = read_line()
        if reply is None:
            return "subagent: host closed the stream"
        if reply.get("type") == "subagents_result":
            if reply.get("error"):
                return "subagent: " + str(reply["error"])
            outputs = reply.get("outputs") or []
            break
    if outputs is None:
        return "subagent: host never answered the batch"
    if len(outputs) != len(payload):
        return (
            f"subagent: expected {len(payload)} reports, got {len(outputs)}; "
            "re-run any task whose report is missing"
        )
    parts = []
    for i, out in enumerate(outputs):
        parts.append(f"## Task {i + 1} — {labels[i]}\n{out}")
    return "\n\n".join(parts)


def main():
    while True:
        req = read_line()
        if req is None:
            break
        kind = req.get("type")
        if kind == "hello":
            # The manifest fields live at the top level of
            # the reply; glass reads the protocol version
            # from there.
            reply = {"id": req.get("id"), "protocol": PROTOCOL}
            reply.update(MANIFEST)
            send(reply)
        elif kind == "system_prompt":
            send({"id": req.get("id"), "text": MANIFEST.get("system_prompt", "")})
        elif kind == "authorize":
            send({"id": req.get("id"), "allow": True})
        elif kind == "run_tool":
            output = handle_run_tool(req)
            send({"id": req.get("id"), "output": output})
        elif kind == "command":
            if req.get("name") == "agents":
                if AGENTS:
                    lines = [
                        f"{a['name']}"
                        + (f" · model {a['model']}" if a["model"] else "")
                        + (f" · {a['description']}" if a["description"] else "")
                        for a in (AGENTS[k] for k in sorted(AGENTS))
                    ]
                    output = "\n".join(lines)
                else:
                    output = (
                        "No agent definitions. Add markdown files with "
                        "frontmatter to ~/.config/glass-core/agents/."
                    )
            else:
                output = "subagent: unknown command"
            send({"id": req.get("id"), "output": output})
        elif kind in ("goodbye", "shutdown"):
            break
        # turn_end and anything else need no reply.


if __name__ == "__main__":
    main()
