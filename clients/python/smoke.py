"""Browser-free Python client handshake smoke test."""

import os
import json
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
    fixture = json.loads(
        (
            Path(__file__).resolve().parents[2]
            / "crates"
            / "glass-browser"
            / "tests"
            / "fixtures"
            / "client-conformance-v1.json"
        ).read_text()
    )
    for schema, versions in fixture["requiredSchemas"].items():
        for version in versions:
            assert client.supports_schema(schema, version)
    for capability in fixture["requiredCapabilities"]:
        client.require_capability(capability)
    tool_names = sorted(tool["name"] for tool in client.list_tools())
    assert tool_names == fixture["tools"]
    project_root = str(Path(__file__).resolve().parents[2])
    project = client.project_inspect(project_root)
    assert project["schemaVersion"] == "glass.development.v1"
    events = client.project_events(project_root, limit=8)
    assert len(events["events"]) <= 8
    client.project_inspect(project_root)
    stopped = False
    subscription = client.watch_project_events(
        project_root,
        after_id=events.get("cursor"),
        limit=8,
        poll_interval=0.05,
        stop=lambda: stopped,
    )
    page = next(subscription)
    stopped = True
    subscription.close()
    assert isinstance(page["events"], list)
finally:
    client.close()
