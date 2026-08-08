#!/usr/bin/env python3
from __future__ import annotations

import os
import signal

from glass_client import GlassClient

stopped = False


def stop(*_: object) -> None:
    global stopped
    stopped = True


signal.signal(signal.SIGINT, stop)
glass = GlassClient(command=os.environ.get("GLASS_BINARY", "glass"))
try:
    project = glass.project_inspect()
    print(glass.project_session_status(project["root"]))
    glass.project_capsule_save(project["root"], {"mobileView": "home"})
    glass.on_attention_required(
        lambda item: print(f"needs you: {item['title']} — {item['detail']}"),
        project["root"],
        stop=lambda: stopped,
    )
finally:
    glass.close()
