"""Browser-free Python client handshake smoke test."""

import os
from pathlib import Path

from glass_client import GlassClient


binary = os.environ.get(
    "GLASS_BINARY",
    str(Path(__file__).resolve().parents[2] / "target" / "debug" / "glass"),
)
client = GlassClient(command=binary)
try:
    manifest = client.initialize()
    assert manifest["protocolVersion"] == 1
    assert client.supports_capability("action")
    assert client.supports_schema("workflow", 1)
    client.require_capability("action")
finally:
    client.close()
