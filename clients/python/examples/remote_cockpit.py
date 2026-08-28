#!/usr/bin/env python3
from __future__ import annotations

import os
import signal
import time

from glass_client import GlassClient

stopped = False


def stop(*_: object) -> None:
    global stopped
    stopped = True


signal.signal(signal.SIGINT, stop)
glass = GlassClient(command=os.environ.get("GLASS_BINARY", "glass"))
try:
    trust = glass.call("glass.workspace.trust.status")
    runtime = glass.call("glass.runtime.inspect")
    browser = glass.call("glass.browser.state")
    project = runtime.get("project") if isinstance(runtime, dict) else {}
    print(
        {
            "trust": trust.get("trust") if isinstance(trust, dict) else trust,
            "root": project.get("root") if isinstance(project, dict) else None,
            "browserConnected": browser.get("connected") if isinstance(browser, dict) else None,
        }
    )
    while not stopped:
        print(glass.call("glass.workspace.trust.inspect"))
        time.sleep(2)
except KeyboardInterrupt:
    pass
finally:
    glass.close()
