#!/usr/bin/env python3
"""glass oracle extension: /oracle — a second opinion from a different model.

Amp ships `oracle`; Claude ships `/advisor`. Both exist because self-review
is weak review. This is the same idea with no new core concepts: the current
diff goes to a pinned alternate model through the host's read-only subagent
path — inspection tools only, so the critique can look at everything and
change nothing — and prints inline without entering the conversation.

Config at `~/.config/glass-core/oracle.json` (or `GC_ORACLE_CONFIG`):

    {"model": "claude-opus-4-5", "prompt": "optional extra instruction"}

`GC_ORACLE_MODEL` overrides the model. `/oracle [question]` defaults the
question to reviewing the change.
"""

import json
import os
import subprocess
import sys

PROTOCOL = 1
DIFF_CAP = 60_000

DEFAULT_PROMPT = (
    "You are a second-opinion reviewer. You are given a repository diff and "
    "a question. Name the most important risks, mistakes, or simpler "
    "alternatives. Be specific and brief: file, line, and the concrete "
    "failure mode or improvement. Do not restate the diff, do not praise, "
    "and say plainly when the change looks sound."
)


def config_path():
    override = os.environ.get("GC_ORACLE_CONFIG")
    if override:
        return override
    xdg = os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config")
    return os.path.join(xdg, "glass-core", "oracle.json")


def config():
    try:
        with open(config_path(), "r", encoding="utf-8") as fh:
            data = json.load(fh)
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def cap_diff(text):
    if len(text) <= DIFF_CAP:
        return text
    return text[:DIFF_CAP] + "\n[diff truncated to save context]"


def git(workspace, args):
    try:
        p = subprocess.run(
            ["git", *args],
            cwd=workspace,
            capture_output=True,
            text=True,
            timeout=20,
        )
    except (OSError, subprocess.TimeoutExpired):
        return ""
    return p.stdout if p.returncode == 0 else ""


def gather_diff(workspace):
    """Working-tree diff plus a note about untracked files, or None."""
    diff = git(workspace, ["diff", "HEAD"])
    if not diff.strip():
        return None
    stat = git(workspace, ["diff", "HEAD", "--stat"]).strip()
    untracked = git(workspace, ["ls-files", "--others", "--exclude-standard"]).strip()
    parts = []
    if stat:
        parts.append(stat)
    if untracked:
        listed = ", ".join(untracked.splitlines()[:20])
        parts.append(f"[untracked files not shown: {listed}]")
    parts.append(cap_diff(diff))
    return "\n\n".join(parts)


def run_oracle(args, workspace):
    cfg = config()
    model = (os.environ.get("GC_ORACLE_MODEL") or "").strip() or str(cfg.get("model") or "").strip()
    if not model:
        return (
            "oracle: no model configured — set `model` in "
            "~/.config/glass-core/oracle.json or GC_ORACLE_MODEL"
        )
    question = args.strip() or "Review this change and name its most important risks."
    diff = gather_diff(workspace)
    if diff is None:
        return "oracle: no diff to review (clean tree, or not a git repository)"
    prompt = DEFAULT_PROMPT
    extra = cfg.get("prompt")
    if isinstance(extra, str) and extra.strip():
        prompt = prompt + "\n" + extra.strip()
    task = f"Question: {question}\n\nRepository diff:\n{diff}"
    send(
        {
            "type": "run_subagents",
            "tasks": [
                {
                    "task": task,
                    "prompt": prompt,
                    "model": model,
                    "read_only": True,
                }
            ],
        }
    )
    for _ in range(100000):
        reply = read_line()
        if reply is None:
            return "oracle: host closed the stream"
        if reply.get("type") == "subagents_result":
            if reply.get("error"):
                return "oracle: " + str(reply["error"])
            outputs = reply.get("outputs") or []
            if not outputs:
                return "oracle: the reviewer returned nothing"
            return outputs[0]
    return "oracle: host never answered the batch"


MANIFEST = {
    "name": "oracle",
    "version": "1.0.0",
    "system_prompt": "",
    "tools": [],
    "commands": [
        {"name": "oracle", "description": "Second opinion from an alternate model"},
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
    workspace = os.getcwd()
    while True:
        req = read_line()
        if req is None:
            break
        kind = req.get("type")
        if kind == "hello":
            workspace = req.get("workspace") or workspace
            reply = {"id": req.get("id"), "protocol": PROTOCOL}
            reply.update(MANIFEST)
            send(reply)
        elif kind == "system_prompt":
            send({"id": req.get("id"), "text": ""})
        elif kind == "authorize":
            send({"id": req.get("id"), "allow": True})
        elif kind == "command":
            if req.get("name") == "oracle":
                send(
                    {
                        "id": req.get("id"),
                        "output": run_oracle(req.get("args") or "", workspace),
                    }
                )
            else:
                send({"id": req.get("id"), "output": "oracle: unknown command"})
        elif kind in ("goodbye", "shutdown"):
            break


def selftest():
    assert cap_diff("x" * 70_000).endswith("[diff truncated to save context]")
    assert cap_diff("short") == "short"
    os.environ.pop("GC_ORACLE_MODEL", None)
    os.environ["GC_ORACLE_CONFIG"] = "/definitely/not/here.json"
    out = run_oracle("", os.getcwd())
    assert out.startswith("oracle: no model configured"), out
    os.environ["GC_ORACLE_MODEL"] = "some-model"
    out = run_oracle("", "/definitely/not/a/repo")
    assert out.startswith("oracle: no diff to review"), out
    print("PASS oracle extension selftest")


if __name__ == "__main__":
    if "--selftest" in sys.argv:
        selftest()
    else:
        main()
