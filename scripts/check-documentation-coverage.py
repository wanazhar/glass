#!/usr/bin/env python3
"""Validate public documentation inventories and repository-local links."""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import subprocess
import urllib.parse


ROOT = pathlib.Path(__file__).resolve().parent.parent
LINK_RE = re.compile(r"(?<!!)\[[^\]]*\]\(([^)]+)\)")
COMMANDS_RE = re.compile(r"^Commands:\s*$\n(?P<body>.*?)(?:\n\n|\nArguments:|\nOptions:)", re.MULTILINE | re.DOTALL)
COMMAND_RE = re.compile(
    r"^  (?P<name>[a-z][a-z0-9-]*)(?:[ \t]{2,}.*)?$", re.MULTILINE
)
MODULE_RE = re.compile(r"^pub mod ([a-z][a-z0-9_]*);", re.MULTILINE)


def fail(failures: list[str]) -> None:
    if failures:
        detail = "\n".join(f"- {failure}" for failure in failures)
        raise SystemExit(f"documentation coverage check failed:\n{detail}")


def markdown_files() -> list[pathlib.Path]:
    output = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "*.md"],
        cwd=ROOT,
        text=True,
    )
    return sorted(ROOT / line for line in output.splitlines() if line)


def link_target(raw: str) -> str:
    raw = raw.strip()
    if raw.startswith("<") and ">" in raw:
        return raw[1 : raw.index(">")]
    # Markdown permits a title after an unbracketed destination.
    return raw.split(maxsplit=1)[0]


def check_links(paths: list[pathlib.Path]) -> list[str]:
    failures: list[str] = []
    for path in paths:
        text = path.read_text(encoding="utf-8")
        for raw in LINK_RE.findall(text):
            target = link_target(raw)
            if not target or target.startswith(("#", "http://", "https://", "mailto:")):
                continue
            decoded = urllib.parse.unquote(target.split("#", 1)[0].split("?", 1)[0])
            if not decoded:
                continue
            resolved = (path.parent / decoded).resolve()
            try:
                resolved.relative_to(ROOT)
            except ValueError:
                failures.append(f"{path.relative_to(ROOT)}: link escapes repository: {target}")
                continue
            if not resolved.exists():
                failures.append(f"{path.relative_to(ROOT)}: missing link target {target}")
    return failures


def commands(binary: pathlib.Path, path: tuple[str, ...] = ()) -> list[str]:
    result = subprocess.run(
        [str(binary), *path, "--help"],
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    match = COMMANDS_RE.search(result.stdout)
    if not match:
        raise ValueError(f"cannot parse Commands section from {binary}")
    return [command for command in COMMAND_RE.findall(match.group("body")) if command != "help"]


def mcp_schema_metrics(binary: pathlib.Path) -> tuple[list[str], int]:
    process = subprocess.Popen(
        [str(binary), "--mcp"],
        cwd=ROOT,
        text=True,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert process.stdin is not None
    assert process.stdout is not None
    try:
        initialize = {
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05", "capabilities": {},
                "clientInfo": {"name": "documentation-coverage", "version": "1"},
            },
        }
        process.stdin.write(json.dumps(initialize, separators=(",", ":")) + "\n")
        process.stdin.flush()
        initialized = json.loads(process.stdout.readline())
        if initialized.get("id") != 1 or "error" in initialized:
            raise ValueError(f"MCP initialization failed for {binary}: {initialized}")
        process.stdin.write(
            '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}\n'
        )
        process.stdin.write(
            '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}\n'
        )
        process.stdin.flush()
        listed = json.loads(process.stdout.readline())
        if listed.get("id") != 2 or "error" in listed:
            raise ValueError(f"MCP tools/list failed for {binary}: {listed}")
        tools = listed.get("result", {}).get("tools", [])

        # Match JavaScript JSON.stringify, which renders integral JSON numbers
        # as integers (`1`, not Python's `1.0`). The public scoreboard uses
        # JSON.stringify and defines the release measurement.
        def javascript_numbers(value: object) -> object:
            if isinstance(value, float) and value.is_integer():
                return int(value)
            if isinstance(value, list):
                return [javascript_numbers(item) for item in value]
            if isinstance(value, dict):
                return {key: javascript_numbers(item) for key, item in value.items()}
            return value

        encoded = json.dumps(
            javascript_numbers(tools), ensure_ascii=False, separators=(",", ":")
        ).encode()
        names = sorted(tool["name"] for tool in tools)
        return names, len(encoded)
    finally:
        process.terminate()
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--glass", type=pathlib.Path, default=ROOT / "target/debug/glass")
    parser.add_argument(
        "--glass-browser", type=pathlib.Path, default=ROOT / "target/debug/glass-browser"
    )
    args = parser.parse_args()
    failures: list[str] = []

    cli_text = (ROOT / "docs/cli.md").read_text(encoding="utf-8")
    cli_inventory = json.loads(
        (ROOT / "docs/cli-inventory.json").read_text(encoding="utf-8")
    )
    if cli_inventory.get("schemaVersion") != 1:
        failures.append("docs/cli-inventory.json schemaVersion must be 1")
    for binary in (args.glass, args.glass_browser):
        if not binary.is_file():
            failures.append(f"build documentation inventory binary first: {binary}")
            continue
        try:
            inventory = commands(binary)
        except (OSError, subprocess.CalledProcessError, ValueError) as error:
            failures.append(str(error))
            continue
        expected_inventory = cli_inventory.get("binaries", {}).get(binary.name)
        if inventory != expected_inventory:
            failures.append(
                f"docs/cli-inventory.json {binary.name} inventory differs from live --help: "
                f"expected {expected_inventory!r}, got {inventory!r}"
            )
        for command in inventory:
            documented = f"`{command}`" in cli_text or re.search(
                rf"^{re.escape(command)}(?:\s|$)", cli_text, re.MULTILINE
            )
            if command != "help" and not documented:
                failures.append(f"docs/cli.md omits {binary.name} command `{command}`")

    if args.glass.is_file():
        for raw_path, expected_inventory in cli_inventory.get("nested", {}).items():
            path = tuple(raw_path.split())
            try:
                inventory = commands(args.glass, path)
            except (OSError, subprocess.CalledProcessError, ValueError) as error:
                failures.append(str(error))
                continue
            if inventory != expected_inventory:
                failures.append(
                    f"docs/cli-inventory.json `{raw_path}` differs from live --help: "
                    f"expected {expected_inventory!r}, got {inventory!r}"
                )
            if f"`{raw_path}`" not in cli_text and f"`{path[0]}`" not in cli_text:
                failures.append(f"docs/cli.md omits nested family `{raw_path}`")
            for command in inventory:
                if f"`{command}`" not in cli_text:
                    failures.append(
                        f"docs/cli.md omits `{raw_path}` subcommand `{command}`"
                    )

    browser_fixture = json.loads(
        (ROOT / "crates/glass-browser/tests/fixtures/client-conformance-v1.json").read_text(
            encoding="utf-8"
        )
    )
    dev_fixture = json.loads(
        (ROOT / "crates/glass-dev/tests/fixtures/client-conformance-v1.json").read_text(
            encoding="utf-8"
        )
    )
    mcp_text = (ROOT / "docs/mcp-tools.md").read_text(encoding="utf-8")
    for tool in dev_fixture["tools"]:
        if f"`{tool}`" not in mcp_text:
            failures.append(f"docs/mcp-tools.md omits MCP tool `{tool}`")

    schema_budget_text = (ROOT / "docs/mcp-schema-budget.md").read_text(
        encoding="utf-8"
    )
    if args.glass.is_file():
        try:
            tool_names, schema_bytes = mcp_schema_metrics(args.glass)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            failures.append(str(error))
        else:
            if tool_names != dev_fixture["tools"]:
                failures.append(
                    "live glass MCP tools differ from the development client "
                    "conformance fixture"
                )
            tool_count = len(tool_names)
            markers = (
                f"| Negotiated tools | {tool_count} |",
                f"| Serialized `tools` array | {schema_bytes:,} UTF-8 bytes |",
            )
            for marker in markers:
                if marker not in schema_budget_text:
                    failures.append(
                        f"docs/mcp-schema-budget.md omits live measurement `{marker}`"
                    )

    if args.glass_browser.is_file():
        try:
            browser_tool_names, _ = mcp_schema_metrics(args.glass_browser)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            failures.append(str(error))
        else:
            if browser_tool_names != browser_fixture["tools"]:
                failures.append(
                    "live glass-browser MCP tools differ from the browser client "
                    "conformance fixture"
                )

    examples_text = (ROOT / "docs/examples.md").read_text(encoding="utf-8")
    examples = sorted((ROOT / "crates/glass-browser/examples").glob("*.rs"))
    for example in examples:
        if f"`{example.stem}`" not in examples_text:
            failures.append(f"docs/examples.md omits example `{example.stem}`")

    lib_text = (ROOT / "crates/glass-browser/src/lib.rs").read_text(encoding="utf-8")
    sdk_text = (ROOT / "docs/rust-sdk.md").read_text(encoding="utf-8")
    for module in MODULE_RE.findall(lib_text):
        if f"[`{module}`]" not in lib_text:
            failures.append(f"crate rustdoc module map omits `{module}`")
        if f"`{module}`" not in sdk_text:
            failures.append(f"docs/rust-sdk.md omits public module `{module}`")

    for relative in ("docs/getting-started.md", "docs/features.md", "docs/rust-sdk.md", "docs/examples.md", "docs/mcp-tools.md"):
        if f"({relative.removeprefix('docs/')})" not in (ROOT / "docs/INDEX.md").read_text(encoding="utf-8"):
            failures.append(f"docs/INDEX.md does not route to {relative}")

    for relative in ("crates/glass-browser/README.md", "crates/glass-dev/README.md"):
        text = (ROOT / relative).read_text(encoding="utf-8")
        if "../../docs/" in text:
            failures.append(f"{relative} contains a package-broken relative docs link")
        if "https://github.com/wanazhar/glass/" not in text:
            failures.append(f"{relative} lacks permanent repository documentation links")

    docs_rs_markers = {
        "crates/glass-browser/Cargo.toml": 'documentation = "https://docs.rs/glass-browser"',
        "crates/glass-dev/Cargo.toml": 'documentation = "https://docs.rs/glass-dev"',
        "crates/glass-browser/README.md": "https://docs.rs/glass-browser",
        "crates/glass-dev/README.md": "https://docs.rs/glass-dev",
        "crates/glass-browser/src/lib.rs": "//! # Choose an entry point",
        "crates/glass-dev/src/lib.rs": "//! ## Public API",
        "docs/rust-sdk.md": "https://docs.rs/glass-dev",
    }
    for relative, marker in docs_rs_markers.items():
        if marker not in (ROOT / relative).read_text(encoding="utf-8"):
            failures.append(f"{relative} omits docs.rs marker `{marker}`")


    paths = markdown_files()
    failures.extend(check_links(paths))
    fail(failures)
    print(
        "documentation coverage validated: "
        f"{len(paths)} Markdown files, {len(dev_fixture['tools'])} full-product "
        f"MCP tools ({len(browser_fixture['tools'])} browser-only), "
        f"{len(examples)} examples, {len(MODULE_RE.findall(lib_text))} public modules"
    )


if __name__ == "__main__":
    main()
