#!/usr/bin/env python3
"""Collect the comparable CLI and MCP contract from one packaged artifact."""

import hashlib
import json
import os
import pathlib
import platform
import subprocess
import sys


TARGETS = {
    "x86_64-unknown-linux-gnu": ("linux", "x86_64"),
    "aarch64-unknown-linux-gnu": ("linux", "aarch64"),
    "x86_64-apple-darwin": ("macos", "x86_64"),
    "aarch64-apple-darwin": ("macos", "aarch64"),
}


def fail(message: str) -> "NoReturn":
    raise SystemExit(f"contract evidence failed: {message}")


def command(*args: str) -> str:
    return subprocess.check_output(args, text=True).strip()


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: collect-contract-evidence.py BINARY OUTPUT.json")

    binary = pathlib.Path(sys.argv[1])
    output = pathlib.Path(sys.argv[2])
    if not binary.is_file() or not os.access(binary, os.X_OK):
        fail(f"packaged artifact is not executable: {binary}")

    target = os.environ.get("GLASS_PLATFORM_TARGET", "")
    if target not in TARGETS:
        fail(f"unsupported or missing GLASS_PLATFORM_TARGET: {target!r}")

    help_result = subprocess.run(
        [str(binary), "--help"],
        check=True,
        capture_output=True,
        text=True,
    )
    # Clap derives the program name from argv[0], which differs for each
    # target artifact. Normalize only that executable basename so the
    # cross-target contract compares behavior rather than packaging names.
    help_text = help_result.stdout.replace(binary.name, "glass")
    manifest = json.loads(command(str(binary), "capabilities"))
    manifest.pop("contextCost", None)
    if manifest.get("protocolVersion") != 1:
        fail("artifact reported an unsupported Glass protocol")

    child = subprocess.Popen(
        [str(binary), "--mcp"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
    )
    try:
        requests = [
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "glass": {
                        "protocolVersion": 1,
                        "schemas": {
                            "action": [1],
                            "observation": [1],
                            "workflow": [1],
                            "checkpoint": [1],
                        },
                    },
                },
            },
            {"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
        ]
        child.stdin.write("\n".join(json.dumps(request) for request in requests) + "\n")
        child.stdin.close()
        initialize = json.loads(child.stdout.readline())
        tools_response = json.loads(child.stdout.readline())
    finally:
        child.terminate()
        child.wait(timeout=5)

    if initialize.get("result", {}).get("glass") != manifest:
        fail("MCP and CLI capability manifests differ")
    tools = tools_response.get("result", {}).get("tools", [])
    names = [tool.get("name") for tool in tools]
    if not names or None in names or len(names) != len(set(names)):
        fail("artifact returned an invalid MCP tool inventory")

    artifact_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
    os_name, architecture = TARGETS[target]
    report = {
        "schema_version": 1,
        "type": "artifact_contract",
        "git_revision": command("git", "rev-parse", "HEAD"),
        "producer": {
            "name": "glass-release-artifact-contract",
            "version": manifest.get("glassVersion", "unknown"),
            "command": "scripts/collect-contract-evidence.py",
            "run_url": os.environ.get("GITHUB_RUN_URL", "local://glass-validation"),
        },
        "target": {
            "id": target,
            "os": os_name,
            "architecture": architecture,
            "runner_os": os.environ.get("RUNNER_OS", platform.system()),
            "runner_architecture": os.environ.get("RUNNER_ARCH", platform.machine()),
            "runner_image": os.environ.get("ImageOS", "not-recorded"),
        },
        "artifact": {
            "name": os.environ.get("GLASS_ARTIFACT_NAME", binary.name),
            "sha256": artifact_hash,
            "size_bytes": binary.stat().st_size,
        },
        "contract": {
            "cli_help": help_text,
            "cli_help_sha256": hashlib.sha256(help_text.encode()).hexdigest(),
            "capability_manifest": manifest,
            "mcp_tools": sorted(tools, key=lambda tool: tool["name"]),
        },
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"artifact contract collected: {binary} ({artifact_hash})")


if __name__ == "__main__":
    main()
