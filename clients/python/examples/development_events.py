"""Print bounded Glass Development Runtime state until interrupted."""

from __future__ import annotations

import os
import sys
import time

from glass_client import GlassClient


root = sys.argv[1] if len(sys.argv) > 1 else os.getcwd()
glass = GlassClient(command=os.environ.get("GLASS_BINARY", "glass"), cwd=root)
try:
    trust = glass.call("glass.workspace.trust.status")
    runtime = glass.call("glass.runtime.inspect")
    project = runtime.get("project") if isinstance(runtime, dict) else {}
    watched = project.get("root", root) if isinstance(project, dict) else root
    print(f"watching {watched} trust={trust.get('trust') if isinstance(trust, dict) else trust}")
    seen = set()
    while True:
        entries = glass.call("glass.replay.list", {"since": 0, "limit": 128})
        if not isinstance(entries, list):
            entries = []
        for entry in entries:
            record = entry if isinstance(entry, dict) else {}
            identity = str(record.get("sequence", record.get("id", entry)))
            if identity in seen:
                continue
            seen.add(identity)
            print(identity, record.get("kind") or "replay", record.get("actor") or "")
        time.sleep(0.5)
except KeyboardInterrupt:
    pass
finally:
    glass.close()
