"""Print bounded Glass Development Runtime events until interrupted."""

from __future__ import annotations

import os
import sys

from glass_client import GlassClient


root = sys.argv[1] if len(sys.argv) > 1 else os.getcwd()
glass = GlassClient(command=os.environ.get("GLASS_BINARY", "glass"))
try:
    project = glass.project_inspect(root)
    print(f"watching {project['root']}")
    for page in glass.watch_project_events(project["root"]):
        if page.get("cursorExpired"):
            print("event cursor expired; resumed at oldest retained event", file=sys.stderr)
        for event in page["events"]:
            print(event["id"], event["kind"], event["actor"]["id"])
except KeyboardInterrupt:
    pass
finally:
    glass.close()
