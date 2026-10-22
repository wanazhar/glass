#!/usr/bin/env python3
"""Smoke for the codemode output-item protocol (task six-wave-e-001 follow-up).

Drives extensions/codemode/codemode.js directly over its stdin/stdout
protocol — no provider, no network. Paths:

  1. hello handshake (protocol 2, docs mention the contract)
  2. single returned value → no `==> text N/M <==` header
  3. two text() items + return → each item preceded by `==> text N/M <==`
  4. console.log/error → one trailing <console_output> block
  5. // @options overrides → timeout_ms honored (tiny timeout kills sleep),
     unknown key warns
  6. overflow budget → bounded preview with [Full output: <path>] note whose
     file exists and holds the full text
"""

import json
import os
import re
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXT = os.path.join(ROOT, "extensions", "codemode", "codemode.js")


class Codemode:
    def __init__(self):
        self.proc = subprocess.Popen(
            ["node", EXT], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, text=True, cwd="/tmp"
        )
        self.seq = 0

    def roundtrip(self, req):
        self.seq += 1
        req["id"] = f"r{self.seq}"
        self.proc.stdin.write(json.dumps(req) + "\n")
        self.proc.stdin.flush()
        line = self.proc.stdout.readline()
        assert line, "codemode closed the connection"
        return json.loads(line)

    def run_tool(self, code):
        return self.roundtrip({
            "type": "run_tool", "name": "codemode",
            "arguments": json.dumps({"code": code}),
        })["output"]

    def close(self):
        self.proc.stdin.close()
        self.proc.wait(timeout=5)


def main() -> int:
    cm = Codemode()
    failures = []

    def check(name, cond, detail=""):
        if cond:
            print(f"PASS {name}")
        else:
            failures.append(name)
            print(f"FAIL {name}: {detail}")

    hello = cm.roundtrip({"type": "hello"})
    check("hello protocol 2", hello.get("protocol") == 2, str(hello.get("protocol")))

    # 2. single value: no items header.
    out = cm.run_tool("return 42")
    check("single value plain", out.strip() == "42", out)

    # 3. multiple items each get the header.
    out = cm.run_tool(
        "tools.text('first'); tools.text('second'); return 'third'"
    )
    matches = re.findall(r"==> text (\d)/(\d) <==", out)
    check("item headers 3 total", matches == [("1", "3"), ("2", "3"), ("3", "3")], str(matches))
    order = [m for m in re.finditer(r"==> text \d/3 <==\n(.*?)(?=\n==>|\Z)", out, re.S)]
    check("items in order", [m.group(1).strip() for m in order] == ["first", "second", "third"], out)

    # 4. console goes to one trailing block.
    out = cm.run_tool("console.log('a line'); console.error('bad line'); return 'value'")
    check("console block present", "<console_output>" in out and "a line" in out and "ERROR: bad line" in out, out)
    check("console after value", out.index("value") < out.index("<console_output>"), out)

    # 5. @options: tiny timeout kills a busy loop; unknown key warns (the
    # warning prefixes whatever the script produced — checked on a
    # completing script, since a timeout result reports only the timeout).
    out = cm.run_tool("// @options: {\"timeout_ms\": 100}\nfor(;;){}")
    check("timeout enforced", "timed out after 100ms" in out, out)
    out = cm.run_tool("// @options: {\"typo_key\": 1}\nreturn 42")
    check("unknown options key warned", 'typo_key' in out, out)

    # 6. overflow: budget 1 token (4 chars) but head+tail preview keeps edges.
    # Note format matches the bash spill note the model already knows.
    out = cm.run_tool("// @options: {\"max_output_tokens\": 1}\nreturn 'x'.repeat(100)")
    m = re.search(r"\[full output saved to (\S+) —", out)
    check("spill path named", m is not None, out)
    if m:
        path = m.group(1)
        exists = os.path.isfile(path)
        full = open(path).read() if exists else ""
        try:
            os.unlink(path)
        except OSError:
            pass
        check("spill file holds full text", exists and len(full) == 100, f"exists={exists}")
        check("preview bounded under 4x budget", len(out) < 400, str(len(out)))
        check("warning states budget", "budget: 1" in out, out)

    cm.close()
    if failures:
        print(f"\n{len(failures)} failure(s): {', '.join(failures)}")
        return 1
    print("\nsmoke-codemode: all paths pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
