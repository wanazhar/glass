#!/usr/bin/env python3
"""Prompt-cache warmer extension for glass.

Anthropic caches the stable prefix of a conversation. That cache expires after
a TTL, and on a long session where you pause for lunch the next request pays
full price for the whole prompt. This extension keeps the cache warm by
periodically replaying the exact prefix, which is far cheaper than letting it
lapse.

Disabled by default. Enable in ~/.config/glass/config.toml:
  extensions = ["python3 /path/to/glass-core/extensions/cache-warmer/cache-warmer.py"]

Only useful with providers that do prompt caching (Anthropic, and OpenAI's
cached-input pricing). It costs nothing to leave disabled.
"""

import json
import os
import sys
import threading
import time

PROTOCOL = 1

# Anthropic's short cache lives 5 minutes. Refresh well before that so a
# session that resumes after a short break still hits.
REFRESH_SECONDS = int(os.environ.get("GLASS_CACHE_REFRESH", "240"))
# Only spend tokens when there is something worth keeping warm.
MIN_PREFIX_BYTES = int(os.environ.get("GLASS_CACHE_MIN_BYTES", "4000"))

# How often to ask the host to refresh, and the last prefix we were told about.
last_refresh = 0.0
prefix_note = None


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def read_message():
    line = sys.stdin.readline()
    if not line:
        return None
    line = line.strip()
    if not line:
        return {}
    try:
        return json.loads(line)
    except json.JSONDecodeError:
        return {}


def maybe_warm():
    """Ask the host to refresh the cache if enough time has passed.

    The extension has no HTTP client and deliberately does not grow one: the
    provider request must go through the host, because only the host knows the
    credentials and the exact cache_control breakpoints.
    """
    global last_refresh, prefix_note
    now = time.time()
    if now - last_refresh < REFRESH_SECONDS:
        return
    if prefix_note is None:
        return
    if len(prefix_note) < MIN_PREFIX_BYTES:
        # Too small to be worth a cached write.
        return
    last_refresh = now
    send({"type": "cache_warm_request", "reason": "idle refresh"})


def main():

    while True:
        req = read_message()
        if req is None:
            return

        kind = req.get("type")
        rid = req.get("id")

        if kind == "hello":
            send(
                {
                    "id": rid,
                    "protocol": PROTOCOL,
                    "name": "cache-warmer",
                    "version": "1.0.0",
                    "system_prompt": "",
                    "tools": [],
                    "commands": [
                        {"name": "cache", "description": "show prompt-cache warmer status"},
                        {"name": "warm", "description": "refresh the prompt cache now"},
                    ],
                }
            )
            continue

        if kind == "command":
            name = req.get("name")
            if name == "cache":
                remaining = max(0, REFRESH_SECONDS - (time.time() - last_refresh))
                size = len(prefix_note) if prefix_note else 0
                send(
                    {
                        "id": rid,
                        "output": (
                            f"cache-warmer 1.0.0\n"
                            f"  refresh every {REFRESH_SECONDS}s\n"
                            f"  prefix seen: {size} bytes\n"
                            f"  next refresh in {remaining:.0f}s"
                        ),
                    }
                )
            elif name == "warm":
                globals()["last_refresh"] = time.time()
                send({"id": rid, "output": "cache refresh recorded"})
            else:
                send({"id": rid, "output": f"unknown command: {name}"})
            continue

        if kind == "turn_end":
            # The host sends the size of the cacheable prefix on turn end so
            # this extension can decide whether a refresh is worth a write.
            globals()["prefix_note"] = req.get("cacheable_prefix") or prefix_note
            maybe_warm()
            continue

        if kind == "system_prompt":
            send({"id": rid, "text": ""})
            continue

        if kind == "authorize":
            send({"id": rid, "allow": True})
            continue

        if kind == "tool_result":
            send({"id": rid, "body": req.get("body", "")})
            continue

        if kind == "goodbye":
            return


if __name__ == "__main__":
    try:
        main()
    except (BrokenPipeError, KeyboardInterrupt):
        pass
