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
    assert client.project_session_status(project_root)["resident"]
    client.project_attach("python-smoke", project_root)
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
    client.project_attach("python-smoke-wait", project_root)
    joined = client.wait_for_event(
        lambda event: event["kind"] == "actorJoined"
        and event["actor"]["name"] == "python-smoke-wait",
        project_root,
        after_id=page["cursor"],
        timeout=2.0,
        poll_interval=0.05,
    )
    assert joined["kind"] == "actorJoined"
    healthy = client.run_until_healthy(
        "python-smoke",
        "printf 'ready\\n'; sleep 5",
        project_root,
        timeout=2.0,
        poll_interval=0.05,
    )
    assert healthy["health"] == "healthy"
    client.project_process_stop("python-smoke", project_root)
    card = client.project_verification_card("Python smoke", project_root)
    assert card["visualStatus"] == "not-captured"
    client.project_capsule_save(
        project_root, {"eventCursor": page["cursor"], "mobileView": "app"}
    )
    assert client.project_capsule_show(project_root)["capsule"] is not None
    client.project_capsule_clear(confirmed=True, root=project_root)
    assert isinstance(client.project_inbox(project_root), list)
finally:
    client.close()
