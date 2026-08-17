#!/usr/bin/env python3
"""Verify release records while retaining explicit failed-candidate tags."""

from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor, as_completed
import os
import subprocess
import sys
import time


REPOSITORY = os.environ.get("GITHUB_REPOSITORY", "wanazhar/glass")
RELEASE_PROPAGATION_ATTEMPTS = 10
RELEASE_PROPAGATION_DELAY_SECONDS = 3
UNPUBLISHED_FAILED_TAGS = {"v0.3.6", "v0.3.7", "v0.3.8", "v0.3.10"}


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


def release_exists(tag: str) -> bool:
    result = subprocess.run(
        [
            "gh",
            "api",
            f"repos/{REPOSITORY}/releases/tags/{tag}",
            "--jq",
            ".tag_name",
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode == 0:
        return result.stdout.strip() == tag
    if "HTTP 404" in result.stderr:
        return False
    print(result.stderr.strip(), file=sys.stderr)
    raise SystemExit(result.returncode)


def published_release_names(tags: set[str]) -> set[str]:
    published: set[str] = set()
    with ThreadPoolExecutor(max_workers=min(8, max(1, len(tags)))) as pool:
        futures = {pool.submit(release_exists, tag): tag for tag in tags}
        for future in as_completed(futures):
            if future.result():
                published.add(futures[future])
    return published


def main() -> None:
    tags = {
        name
        for name in gh_names("tags?per_page=100", ".[] | .name")
        if name.startswith("v")
    }
    unknown_failed_tags = sorted(UNPUBLISHED_FAILED_TAGS - tags)
    if unknown_failed_tags:
        raise SystemExit(
            "GitHub release check failed; configured failed tags do not exist: "
            + ", ".join(unknown_failed_tags)
        )
    for attempt in range(RELEASE_PROPAGATION_ATTEMPTS):
        releases = published_release_names(tags)
        unexpected_failed_releases = sorted(UNPUBLISHED_FAILED_TAGS & releases)
        if unexpected_failed_releases:
            raise SystemExit(
                "GitHub release check failed; failed candidates have published "
                "release records: " + ", ".join(unexpected_failed_releases)
            )
        expected_releases = tags - UNPUBLISHED_FAILED_TAGS
        missing = sorted(expected_releases - releases)
        if not missing:
            print(
                "GitHub release records validated: "
                f"{len(expected_releases)} published tags, "
                f"{len(UNPUBLISHED_FAILED_TAGS)} retained failed candidates"
            )
            return
        if attempt + 1 < RELEASE_PROPAGATION_ATTEMPTS:
            time.sleep(RELEASE_PROPAGATION_DELAY_SECONDS)
    raise SystemExit(
        "GitHub release check failed; missing published releases for: "
        + ", ".join(missing)
    )


if __name__ == "__main__":
    main()
