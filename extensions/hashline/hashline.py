#!/usr/bin/env python3
"""hashline — content-anchored edits for glass.

Every line you read comes back with a 2-3 character hash:

    1:a3|function hello() {
    2:f1|  return "world";
    3:0e|}

Edits then reference those anchors instead of re-typing content:

    {"ops": [{"op": "replace", "line": "2:f1", "text": "  return 42;"}]}

Why this exists. A frontier model asked to reproduce a block of text byte for
byte fails often enough to matter: one published benchmark measured patch
application failing 50.7% on one model and 46.2% on another, with a pass rate
moving 64.6 percentage points purely by changing the edit format. Anchoring
replaces recall with verification. The model never re-types whitespace or
reconstructs a string it has to have memorised; it names the line and the
harness fails loudly if that line has changed.

A stale anchor is rejected before anything is written. That is the point: a
silent edit against the wrong span is the failure mode this format exists to
remove.

Disabled by default. Enable in ~/.config/glass/config.toml:
  extensions = ["python3 /path/to/glass-core/extensions/hashline/hashline.py"]

Standard library only. `keyed` concurrency means edits to different files
still overlap, while two edits to one file stay ordered.
"""

import json
import os
import re
import sys

PROTOCOL = 1

# Anchor length. Two hex chars is 256 buckets, three is 4096. Three is used
# because at a few thousand lines a two-char anchor starts colliding often
# enough to be annoying.
HASH_LEN = 3


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def anchor(text, lineno):
    """A short stable hash of one line, prefixed by its line number.

    The line number is part of the payload so two identical lines get different
    anchors, which is what makes `replace 12:ab3` unambiguous. The returned
    string is `<lineno>:<hash>`.
    """
    h = 0
    for ch in f"{lineno}:{text}".encode("utf-8", "replace"):
        h = (h * 31 + ch) & 0xFFFFFFFF
    return f"{lineno}:{h:x}"


def annotate(text, start_line=1):
    """Prefix every line with its anchor."""
    out = []
    for i, line in enumerate(text.split("\n"), start=start_line):
        out.append(f"{anchor(line, i)}|{line}")
    return "\n".join(out)


def read_lines(path, offset=None, limit=None):
    with open(path, "r", encoding="utf-8", errors="replace") as f:
        lines = f.read().split("\n")
    start = max(0, (offset or 1) - 1)
    end = start + limit if limit else len(lines)
    return lines[start:end], start + 1


def apply_ops(path, ops):
    """Apply anchored edits. All-or-nothing: a bad anchor writes nothing."""
    with open(path, "r", encoding="utf-8", errors="replace") as f:
        original = f.read()
    lines = original.split("\n")

    # Validate every op against the pre-edit state first, so a batch cannot
    # half-apply because the third op had a stale anchor.
    resolved = []
    for op in ops:
        kind = op.get("op")
        if kind in ("replace", "delete"):
            lineno, want = split_anchor(op.get("line", ""))
            if lineno is None:
                raise ValueError(
                    f"malformed anchor {op.get('line')!r}; expected <line>:<hex>, e.g. 12:ab3"
                )
            if lineno < 1 or lineno > len(lines):
                raise ValueError(f"line {lineno} is out of range (file has {len(lines)} lines)")
            got = anchor(lines[lineno - 1], lineno)
            if want and got != want:
                raise ValueError(
                    f"stale anchor on line {lineno}: you said {want}, the file now says {got}.\n"
                    f"  line {lineno} is now: {lines[lineno - 1][:200]}\n"
                    "Re-read the file and use the current anchors. Nothing was written."
                )
            resolved.append((kind, lineno, op.get("text", "")))
        elif kind == "insert":
            after, want = split_anchor(op.get("after", ""))
            if after is None:
                raise ValueError(f"malformed anchor {op.get('after')!r}")
            if after < 0 or after > len(lines):
                raise ValueError(f"anchor line {after} is out of range (file has {len(lines)} lines)")
            if after >= 1:
                got = anchor(lines[after - 1], after)
                if want and got != want:
                    raise ValueError(
                        f"stale anchor after line {after}: you said {want}, the file now says {got}.\n"
                        "Re-read the file and use the current anchors. Nothing was written."
                    )
            resolved.append((kind, after, op.get("text", "")))
        else:
            raise ValueError(f"unknown op {kind!r}; use replace, insert, or delete")

    # Apply from the bottom up so earlier line numbers stay valid.
    for kind, lineno, text in sorted(resolved, key=lambda r: -r[1]):
        if kind == "replace":
            lines[lineno - 1] = text
        elif kind == "delete":
            del lines[lineno - 1]
        elif kind == "insert":
            for offset, l in enumerate(text.split("\n")):
                lines.insert(lineno + offset, l)

    return "\n".join(lines), original


def split_anchor(spec):
    """`12:ab3` -> (12, "12:ab3"). `12` -> (12, "").

    The hash returned includes the line number because that is what `anchor`
    produced, so the comparison in `apply_ops` is a straight equality.
    """
    m = re.match(r"^\s*(\d+)(?::([0-9a-zA-Z]+))?\s*$", str(spec))
    if not m:
        return None, None
    lineno = int(m.group(1))
    if not m.group(2):
        return lineno, ""
    return lineno, f"{lineno}:{m.group(2)}"


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue

        rid = req.get("id")
        kind = req.get("type")

        if kind == "hello":
            send({
                "id": rid,
                "protocol": PROTOCOL,
                "name": "hashline",
                "version": "1.0.0",
                "system_prompt": (
                    "A `hl_read` / `hl_edit` pair is available for editing files. Every line "
                    "hl_read returns carries a short hash anchor, and hl_edit refers to lines by "
                    "that anchor instead of re-typing their content. Prefer it over sed or a "
                    "heredoc when editing an existing file: it cannot apply to the wrong span, and "
                    "a stale anchor is rejected instead of silently corrupting the file. Use bash "
                    "for everything else."
                ),
                "tools": [
                    {
                        "name": "hl_read",
                        "description": (
                            "Read a file with every line prefixed by a short anchor, e.g. "
                            "`12:ab3|  return x;`. Pass those anchors back to hl_edit."
                        ),
                        "schema": {
                            "type": "object",
                            "properties": {
                                "path": {"type": "string"},
                                "offset": {"type": "integer", "description": "1-based first line"},
                                "limit": {"type": "integer"},
                            },
                            "required": ["path"],
                        },
                        "concurrency": "parallel",
                    },
                    {
                        "name": "hl_edit",
                        "description": (
                            "Apply anchored edits, all or nothing. Ops: "
                            '{"op":"replace","line":"12:ab3","text":"..."}, '
                            '{"op":"delete","line":"12:ab3"}, '
                            '{"op":"insert","after":"12:ab3","text":"..."}. '
                            "Omit the hash to address a line by number only. A stale anchor is "
                            "rejected and nothing is written."
                        ),
                        "schema": {
                            "type": "object",
                            "properties": {
                                "path": {"type": "string"},
                                "ops": {"type": "array", "items": {"type": "object"}},
                            },
                            "required": ["path", "ops"],
                        },
                        # Keyed: edits to different files overlap, two edits to
                        # one file stay in order.
                        "concurrency": "keyed",
                    },
                ],
                "commands": [{"name": "hashline", "description": "hashline status"}],
            })
            continue

        if kind == "command":
            send({"id": rid, "output": f"hashline 1.0.0 (anchors: {HASH_LEN} hex chars)"})
            continue

        if kind == "run_tool":
            name = req.get("name")
            try:
                args = json.loads(req.get("arguments") or "{}")
            except json.JSONDecodeError:
                args = {}
            try:
                if name == "hl_read":
                    lines, first = read_lines(args.get("path", ""), args.get("offset"), args.get("limit"))
                    total = sum(1 for _ in open(args["path"], "r", encoding="utf-8", errors="replace"))
                    send({
                        "id": rid,
                        "output": (
                            f"{args['path']} ({total} lines)\n"
                            + annotate("\n".join(lines), start_line=first)
                        ),
                    })
                elif name == "hl_edit":
                    new, old = apply_ops(args["path"], args.get("ops") or [])
                    # os.replace is atomic, so a reader never sees a half-written
                    # file, and a crash mid-write leaves the original intact.
                    tmp = args["path"] + ".hl.tmp"
                    with open(tmp, "w", encoding="utf-8") as f:
                        f.write(new)
                    os.replace(tmp, args["path"])
                    before = len(old.split("\n"))
                    after = len(new.split("\n"))
                    send({
                        "id": rid,
                        "output": (
                            f"edited {args['path']}: {before} -> {after} lines "
                            f"({len(args.get('ops') or [])} ops applied)"
                            + annotate_diff(old, new)
                        ),
                    })
                else:
                    send({"id": rid, "output": f"unknown tool {name}"})
            except FileNotFoundError as e:
                send({"id": rid, "output": f"hashline: no such file ({e.filename})"})
            except ValueError as e:
                send({"id": rid, "output": f"hashline: {e}"})
            except OSError as e:
                send({"id": rid, "output": f"hashline: {e}"})
            continue

        if kind in ("system_prompt", "authorize"):
            send({"id": rid, "allow": True, "text": ""})
            continue
        if kind == "tool_result":
            send({"id": rid, "body": req.get("body", "")})
            continue
        if kind == "turn_end":
            continue
        if kind == "goodbye":
            return


def annotate_diff(old, new):
    """Show just the changed region, anchored, so the model can verify."""
    o = old.split("\n")
    n = new.split("\n")
    # find the first and last differing lines
    start = 0
    while start < min(len(o), len(n)) and o[start] == n[start]:
        start += 1
    if start >= len(o) and start >= len(n):
        return "\n(no textual change)"
    end = 0
    while end < min(len(o), len(n)) - start and o[len(o) - 1 - end] == n[len(n) - 1 - end]:
        end += 1
    hi = len(n) - end

    # Anchors are computed from the NEW line numbers, which is what the model
    # will use next. Annotating the slice directly (rather than renumbering
    # afterwards) keeps them pointing at the right lines.
    body = "\n".join(
        f"{anchor(n[i], i + 1)}|{n[i]}" for i in range(start, hi)
    )
    return "\nchanged region:\n" + body


if __name__ == "__main__":
    try:
        main()
    except (BrokenPipeError, KeyboardInterrupt):
        pass
