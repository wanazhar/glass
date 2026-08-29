#!/usr/bin/env python3
"""Validate the TUI shortcut inventory and its required public references."""

from __future__ import annotations

import json
import pathlib


ROOT = pathlib.Path(__file__).resolve().parent.parent
INVENTORY_PATH = ROOT / "crates/glass-dev/src/tui/shortcuts.json"
BINDINGS_PATH = ROOT / "crates/glass-dev/src/tui/bindings.rs"


def fail(message: str) -> None:
    raise SystemExit(f"TUI shortcut check failed: {message}")


def main() -> None:
    try:
        inventory = json.loads(INVENTORY_PATH.read_text(encoding="utf-8"))
        bindings = BINDINGS_PATH.read_text(encoding="utf-8")
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read inventory or implementation: {error}")

    if inventory.get("schema_version") != 1:
        fail("shortcut inventory schema_version must be 1")
    if 'include_str!("shortcuts.json")' not in bindings:
        fail("bindings.rs must consume shortcuts.json")
    if "documented_shortcut_help()" not in bindings:
        fail("bindings.rs must render help from the shortcut inventory")

    keys: list[str] = []
    for index, line in enumerate(inventory.get("help_lines", []), start=1):
        line_keys = line.get("keys")
        text = line.get("text")
        if not isinstance(line_keys, list) or not line_keys:
            fail(f"help_lines[{index}] must contain at least one key")
        if not isinstance(text, str) or not text:
            fail(f"help_lines[{index}] must contain help text")
        for key in line_keys:
            if not isinstance(key, str) or not key:
                fail(f"help_lines[{index}] contains an invalid key")
            if key in keys:
                fail(f"shortcut {key!r} is listed more than once")
            if key not in text:
                fail(f"help_lines[{index}] help text omits {key!r}")
            keys.append(key)

    documentation = inventory.get("documentation", [])
    if not documentation:
        fail("documentation requirements are empty")
    checked_markers = 0
    for entry in documentation:
        relative = entry.get("path")
        markers = entry.get("markers")
        if not isinstance(relative, str) or not isinstance(markers, list):
            fail("each documentation entry needs a path and marker list")
        path = ROOT / relative
        if not path.is_file():
            fail(f"documentation path does not exist: {relative}")
        text = path.read_text(encoding="utf-8")
        for marker in markers:
            if not isinstance(marker, str) or not marker:
                fail(f"{relative} contains an invalid marker")
            if marker not in text:
                fail(f"{relative} is missing shortcut marker {marker!r}")
            checked_markers += 1

    print(
        f"TUI shortcut inventory validated: {len(keys)} implementation help keys; "
        f"{checked_markers} documentation markers"
    )


if __name__ == "__main__":
    main()
