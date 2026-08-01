#!/usr/bin/env python3
"""Verify every version tag has a published GitHub Release record."""

from __future__ import annotations

import os
import subprocess
import sys


REPOSITORY = os.environ.get("GITHUB_REPOSITORY", "wanazhar/glass")


def gh_names(endpoint: str, query: str) -> set[str]:
    result = subprocess.run(
        [
            "gh",
            "api",
            "--paginate",
            f"repos/{REPOSITORY}/{endpoint}",
            "--jq",
            query,
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        print(result.stderr.strip(), file=sys.stderr)
        raise SystemExit(result.returncode)
    return {line.strip() for line in result.stdout.splitlines() if line.strip()}


def main() -> None:
    tags = {
        name
        for name in gh_names("tags?per_page=100", ".[] | .name")
        if name.startswith("v")
    }
    releases = gh_names(
        "releases?per_page=100",
        ".[] | select(.draft == false and .prerelease == false) | .tag_name",
    )
    missing = sorted(tags - releases)
    if missing:
        raise SystemExit(
            "GitHub release check failed; missing published releases for: "
            + ", ".join(missing)
        )
    print(f"GitHub release records validated: {len(tags)} version tags")


if __name__ == "__main__":
    main()
