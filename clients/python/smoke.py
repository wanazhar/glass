"""Browser-free Python client handshake smoke test."""

import os
import json
from pathlib import Path

from glass_client import GlassClient


binary = os.environ.get(
    "GLASS_BINARY",
    str(Path(__file__).resolve().parents[2] / "target" / "debug" / "glass"),
)
repository_root = Path(__file__).resolve().parents[2]
client = GlassClient(command=binary, cwd=str(repository_root))
try:
    manifest = client.initialize()
    assert manifest["protocolVersion"] == 1
    assert client.supports_capability("action")
    browser_fixture = json.loads(
        (
            Path(__file__).resolve().parents[2]
            / "crates"
            / "glass-browser"
            / "tests"
            / "fixtures"
            / "client-conformance-v1.json"
        ).read_text()
    )
    development_fixture = json.loads(
        (
            Path(__file__).resolve().parents[2]
            / "crates"
            / "glass-dev"
            / "tests"
            / "fixtures"
            / "client-conformance-v1.json"
        ).read_text()
    )
    for schema, versions in browser_fixture["requiredSchemas"].items():
        for version in versions:
            assert client.supports_schema(schema, version)
    for capability in browser_fixture["requiredCapabilities"]:
        client.require_capability(capability)
    tool_names = sorted(tool["name"] for tool in client.list_tools())
    assert tool_names == development_fixture["tools"]
    runtime = client.call("glass.runtime.inspect")
    assert isinstance(runtime, dict) and isinstance(runtime.get("project"), dict)
    tree = client.call("glass.file.list", {"path": "clients/python", "limit": 10})
    assert isinstance(tree["entries"], list)
    browser = client.call("glass.browser.state")
    assert isinstance(browser["connected"], bool)
    assert isinstance(client.call("glass.task.list"), list)
    assert isinstance(client.call("glass.replay.list"), list)
finally:
    client.close()
