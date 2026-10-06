#!/usr/bin/env python3
"""Cache-warmer extension for glass. Disabled by default.

This extension declares a *policy*; the host owns the *mechanism*.
That split is not an accident. A subprocess cannot warm the cache
itself: it holds no API key, and the host is not reading its stdout
between turns, which is exactly when a refresh has to happen. So the
extension says how long to wait and how big the prefix must be, and
the host does the refreshing in a background thread.

The economics, stated plainly: a cache *write* costs 1.25x the
prefix while a cache *miss* costs 1.0x, so each refresh is a net
spend of about 0.35x the prefix. The only thing it buys is latency
-- the first turn after an idle gap starts from a warm cache instead
of a cold one. Enable it if that latency is worth the spend; leave it
off otherwise. It is off by default.

To enable, add it to the extensions list in your glass config:

    extensions = ["python3 /path/to/cache-warmer.py"]

 knobs (both optional):
    refresh_after_s   seconds after a turn to refresh (default 240)
    min_prefix_bytes  smallest prefix worth warming (default 4000)

The provider expires the cache five minutes after the last write, so
240s refreshes just before expiry. Lower min_prefix_bytes means more
refreshes on small prompts, which is rarely worth it.
"""
import json
import os
import sys

# Provider TTL is 300s; refresh just before it. Both knobs can be
# overridden in the environment, which is how the harness is tested
# without waiting four minutes, and how an operator tunes it without
# editing the file.
REFRESH_AFTER_S = int(
    os.environ.get("GLASS_CACHE_WARMER_REFRESH_S", "240") or "240"
)
MIN_PREFIX_BYTES = int(
    os.environ.get("GLASS_CACHE_WARMER_MIN_PREFIX_BYTES", "4000") or "4000"
)


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def reply(req_id, **fields):
    obj = {"id": req_id}
    obj.update(fields)
    send(obj)


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue

        kind = req.get("type")
        rid = req.get("id")

        if kind == "hello":
            reply(
                rid,
                protocol=1,
                name="cache-warmer",
                version="1.0.0",
                cache_warmer={
                    "refresh_after_s": REFRESH_AFTER_S,
                    "min_prefix_bytes": MIN_PREFIX_BYTES,
                },
            )

        elif kind == "system_prompt":
            # No prompt contribution: the warmer is policy, not
            # instructions, and every token here is paid every turn.
            reply(rid, text="")

        elif kind == "command":
            # Answer even for commands we do not own, so the host
            # never waits out a timeout on us.
            reply(rid, output="")

        elif kind == "authorize":
            reply(rid, allow=True)

        elif kind == "tool_result":
            reply(rid, body=req.get("body", ""))

        elif kind == "turn_end":
            reply(rid)

        elif kind == "run_tool":
            # This extension contributes no tools.
            reply(rid, output="")

        elif kind == "call_tool_result":
            pass

        elif kind == "goodbye":
            break


if __name__ == "__main__":
    main()
