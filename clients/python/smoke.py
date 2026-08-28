"""Browser-free Python client handshake smoke test."""

import os
import json
from pathlib import Path

from glass_client import GlassClient, GlassError


binary = os.environ.get(
    "GLASS_BINARY",
    str(Path(__file__).resolve().parents[2] / "target" / "debug" / "glass"),
)
client = GlassClient(command=binary)
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
    trust = client.call("glass.workspace.trust.status")
    assert trust["trust"] in {"untrusted", "trustedOnce", "trustedProject"}
    authority = client.call("glass.workspace.trust.inspect")
    assert isinstance(authority["items"], list)
    tree = client.call("glass.file.list")
    assert isinstance(tree["entries"], list)
    browser = client.call("glass.browser.state")
    assert isinstance(browser["connected"], bool)
    assert isinstance(client.call("glass.task.list"), list)
    assert isinstance(client.call("glass.replay.list"), list)
    if trust["trust"] == "untrusted":
        try:
            client.call("glass.test.discover")
        except GlassError as error:
            assert "trusted" in str(error).lower() or "blocked" in str(error).lower()
        else:
            raise AssertionError("untrusted executable project discovery was not denied")
finally:
    client.close()
