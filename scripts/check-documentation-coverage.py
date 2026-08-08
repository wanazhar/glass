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
COMMAND_RE = re.compile(r"^  (?P<name>[a-z][a-z0-9-]*)\s{2,}", re.MULTILINE)
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


def commands(binary: pathlib.Path) -> list[str]:
    result = subprocess.run(
        [str(binary), "--help"],
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    match = COMMANDS_RE.search(result.stdout)
    if not match:
        raise ValueError(f"cannot parse Commands section from {binary}")
    return COMMAND_RE.findall(match.group("body"))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--glass", type=pathlib.Path, default=ROOT / "target/debug/glass")
    parser.add_argument(
        "--glass-browser", type=pathlib.Path, default=ROOT / "target/debug/glass-browser"
    )
    args = parser.parse_args()
    failures: list[str] = []

    cli_text = (ROOT / "docs/cli.md").read_text(encoding="utf-8")
    for binary in (args.glass, args.glass_browser):
        if not binary.is_file():
            failures.append(f"build documentation inventory binary first: {binary}")
            continue
        try:
            inventory = commands(binary)
        except (OSError, subprocess.CalledProcessError, ValueError) as error:
            failures.append(str(error))
            continue
        for command in inventory:
            documented = f"`{command}`" in cli_text or re.search(
                rf"^{re.escape(command)}(?:\s|$)", cli_text, re.MULTILINE
            )
            if command != "help" and not documented:
                failures.append(f"docs/cli.md omits {binary.name} command `{command}`")

    fixture = json.loads(
        (ROOT / "crates/glass-browser/tests/fixtures/client-conformance-v1.json").read_text(
            encoding="utf-8"
        )
    )
    mcp_text = (ROOT / "docs/mcp-tools.md").read_text(encoding="utf-8")
    for tool in fixture["tools"]:
        if f"`{tool}`" not in mcp_text:
            failures.append(f"docs/mcp-tools.md omits MCP tool `{tool}`")

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

    paths = markdown_files()
    failures.extend(check_links(paths))
    fail(failures)
    print(
        "documentation coverage validated: "
        f"{len(paths)} Markdown files, {len(fixture['tools'])} MCP tools, "
        f"{len(examples)} examples, {len(MODULE_RE.findall(lib_text))} public modules"
    )


if __name__ == "__main__":
    main()
