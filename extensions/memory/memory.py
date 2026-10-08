#!/usr/bin/env python3
"""glass memory extension: durable, cross-session project memory.

Sessions start from zero; the same project quirks get rediscovered every
week. This extension gives glass-core the file convention the big harnesses
converged on: a per-project directory with a size-bounded `MEMORY.md` index
that is injected into the system prompt, plus topic files the model reads on
demand.

Layout: `~/.config/glass-core/memory/<slug>/` (or `GC_MEMORY_DIR`):

    MEMORY.md          the index, one line per memory, injected every session
    build.md           a topic file, read with `memory read`

User-level only by design: a repository cannot add memories to its own
instructions, so cloning an untrusted repo changes nothing here.
"""

import json
import os
import re
import sys
import time

PROTOCOL = 1
INDEX_CAP_LINES = 200
INDEX_CAP_BYTES = 25_000
READ_CAP = 8_000
NAME_RE = re.compile(r"^[a-z0-9][a-z0-9._-]{0,63}$")


def memory_root():
    override = os.environ.get("GC_MEMORY_DIR")
    if override:
        return override
    xdg = os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config")
    return os.path.join(xdg, "glass-core", "memory")


def slug(workspace):
    canonical = os.path.realpath(workspace)
    text = canonical.strip(os.sep).replace(os.sep, "-")
    return (text[:96] or "root")


def today():
    return time.strftime("%Y-%m-%d")


class Memory:
    def __init__(self, workspace):
        self.dir = os.path.join(memory_root(), slug(workspace))
        os.makedirs(self.dir, exist_ok=True)
        self.index_path = os.path.join(self.dir, "MEMORY.md")

    def index_lines(self):
        try:
            with open(self.index_path, "r", encoding="utf-8") as fh:
                return [line.rstrip("\n") for line in fh if line.strip()]
        except OSError:
            return []

    def write_index(self, lines):
        with open(self.index_path, "w", encoding="utf-8") as fh:
            fh.write("\n".join(lines) + ("\n" if lines else ""))

    def index_text(self):
        lines = self.index_lines()
        truncated = False
        if len(lines) > INDEX_CAP_LINES:
            lines = lines[:INDEX_CAP_LINES]
            truncated = True
        text = "\n".join(lines)
        if len(text) > INDEX_CAP_BYTES:
            text = text[:INDEX_CAP_BYTES]
            truncated = True
        if truncated:
            text += "\n[index truncated — prune with the memory tool]"
        return text

    def path_for(self, name):
        return os.path.join(self.dir, name + ".md")

    def file_body(self, name):
        try:
            with open(self.path_for(name), "r", encoding="utf-8") as fh:
                text = fh.read()
        except OSError:
            return None
        # Strip the frontmatter; the model cares about the body.
        if text.startswith("---"):
            end = text.find("\n---", 3)
            if end >= 0:
                text = text[end + 4 :].lstrip("\n")
        return text

    def add(self, name, text):
        if not NAME_RE.match(name or ""):
            return "memory: name must be lowercase [a-z0-9._-], 1-64 chars"
        text = (text or "").strip()
        if not text:
            return "memory: `text` is empty"
        existing = self.file_body(name)
        created = today()
        if existing is not None:
            # Keep the original creation date across updates.
            try:
                with open(self.path_for(name), "r", encoding="utf-8") as fh:
                    head = fh.read(200)
                found = re.search(r"created:\s*([0-9-]+)", head)
                if found:
                    created = found.group(1)
            except OSError:
                pass
        body = f"---\ncreated: {created}\nmodified: {today()}\n---\n\n{text}\n"
        with open(self.path_for(name), "w", encoding="utf-8") as fh:
            fh.write(body)
        hook = " ".join(text.splitlines()[0].split())[:100]
        line = f"- [{name}]({name}.md) — {hook}"
        lines = [l for l in self.index_lines() if not l.startswith(f"- [{name}]")]
        lines.append(line)
        self.write_index(sorted(lines))
        verb = "updated" if existing is not None else "saved"
        return f"memory {verb}: {name} ({len(text)} chars) in {self.dir}"

    def remove(self, name):
        path = self.path_for(name)
        if not os.path.exists(path):
            return f"memory: no memory named `{name}` (use list)"
        os.remove(path)
        lines = [l for l in self.index_lines() if not l.startswith(f"- [{name}]")]
        self.write_index(lines)
        return f"memory removed: {name}"

    def read(self, name):
        if not name:
            return "memory: `name` is required for read"
        body = self.file_body(name)
        if body is None:
            return f"memory: no memory named `{name}` (use list)"
        if len(body) > READ_CAP:
            return body[:READ_CAP] + "\n[truncated — the file is in " + self.dir + "]"
        return body

    def run(self, action, name, text):
        if action == "add":
            if not name:
                return "memory: add needs `name` and `text`"
            return self.add(name, text)
        if action == "read":
            return self.read(name)
        if action == "remove":
            if not name:
                return "memory: remove needs `name`"
            return self.remove(name)
        if action == "list":
            index = self.index_text()
            return index or "memory: empty"
        return "memory: action must be add, read, list, or remove"


def system_prompt(memory):
    header = (
        "Project memory lives in "
        f"{memory.dir}. Record durable facts — conventions, gotchas, user "
        "preferences — with the `memory` tool (add/read/list/remove); keep "
        "the index tidy and remove what turned out wrong."
    )
    index = memory.index_text()
    if not index:
        return header
    return header + "\n\n# Project memory\n" + index


MANIFEST = {
    "name": "memory",
    "version": "1.0.0",
    "tools": [
        {
            "name": "memory",
            "description": (
                "Durable project memory shared across sessions. add writes or "
                "replaces a named memory (name is lowercase [a-z0-9._-]); "
                "read returns one; list shows the index; remove deletes. "
                "Record conventions, gotchas, and user preferences worth "
                "keeping — not transient task state (that is `state`)."
            ),
            "schema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["add", "read", "list", "remove"],
                    },
                    "name": {"type": "string", "description": "Memory name."},
                    "text": {
                        "type": "string",
                        "description": "For add: the memory content.",
                    },
                },
                "required": ["action"],
            },
            "concurrency": "serial",
        }
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


def main():
    memory = None
    while True:
        req = read_line()
        if req is None:
            break
        kind = req.get("type")
        if kind == "hello":
            memory = Memory(req.get("workspace") or os.getcwd())
            reply = {"id": req.get("id"), "protocol": PROTOCOL}
            reply.update(MANIFEST)
            reply["system_prompt"] = system_prompt(memory)
            send(reply)
        elif kind == "system_prompt":
            send({"id": req.get("id"), "text": system_prompt(memory) if memory else ""})
        elif kind == "authorize":
            send({"id": req.get("id"), "allow": True})
        elif kind == "run_tool":
            try:
                args = json.loads(req.get("arguments") or "{}")
            except ValueError:
                output = "memory: arguments are not valid JSON"
            else:
                output = memory.run(
                    str(args.get("action") or ""),
                    str(args.get("name") or ""),
                    args.get("text"),
                )
            send({"id": req.get("id"), "output": output})
        elif kind == "command":
            if req.get("name") == "memory" and memory is not None:
                index = memory.index_text()
                output = f"memory dir: {memory.dir}\n" + (index or "(empty)")
            else:
                output = "memory: unknown command"
            send({"id": req.get("id"), "output": output})
        elif kind in ("goodbye", "shutdown"):
            break


def selftest():
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        os.environ["GC_MEMORY_DIR"] = tmp
        memory = Memory(tmp + "/ws")
        assert "empty" in memory.run("list", "", None)
        out = memory.run("add", "build", "Use cargo test --locked.\nMore detail.")
        assert out.startswith("memory saved"), out
        assert os.path.isfile(os.path.join(memory.dir, "build.md"))
        listing = memory.run("list", "", None)
        assert "build.md" in listing, listing
        body = memory.run("read", "build", None)
        assert "cargo test --locked" in body, body
        # Replace keeps one index line.
        memory.run("add", "build", "Now cargo check first.")
        assert memory.run("list", "", None).count("build.md") == 1
        assert "cargo check first" in memory.run("read", "build", None)
        # Names are constrained; read of a missing one is a clear message.
        assert "lowercase" in memory.run("add", "../evil", "x")
        assert "no memory named" in memory.run("read", "ghost", None)
        assert memory.run("remove", "build", None).startswith("memory removed")
        assert "empty" in memory.run("list", "", None)
        # Index caps are enforced and marked.
        memory.write_index([f"- [m{i}](m{i}.md) — hook {i}" for i in range(300)])
        capped = memory.index_text()
        assert "truncated" in capped and len(capped.splitlines()) <= INDEX_CAP_LINES + 1
        # Injection carries the index into the system prompt.
        injected = system_prompt(memory)
        assert "Project memory" in injected and "m0.md" in injected
    print("PASS memory extension selftest")


if __name__ == "__main__":
    if "--selftest" in sys.argv:
        selftest()
    else:
        main()
