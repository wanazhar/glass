#!/usr/bin/env python3
"""Generate a substantive, exact-tag GitHub Release body."""

import argparse
import os
import pathlib
import re


REQUIRED_HEADINGS = (
    "Major features",
    "Breaking changes",
    "Security",
    "Installation and migration",
    "Known limitations",
    "Validation evidence",
)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--run-url", required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()

    if args.tag != f"v{args.version}":
        raise SystemExit(f"tag {args.tag} does not match version {args.version}")
    if not re.fullmatch(r"[0-9a-f]{40}", args.commit):
        raise SystemExit("release commit must be a full 40-character SHA")
    source = pathlib.Path(f"docs/releases/{args.version}.md")
    if not source.is_file():
        raise SystemExit(f"missing substantive release source {source}")
    body = source.read_text(encoding="utf-8").strip()
    title = f"# Glass {args.version}"
    if body.startswith(title + "\n"):
        body = body.removeprefix(title + "\n").lstrip()
    for heading in REQUIRED_HEADINGS:
        if f"## {heading}" not in body:
            raise SystemExit(f"{source} is missing required heading: {heading}")
    forbidden = ("local release candidate", "not published", "Full Changelog")
    for phrase in forbidden:
        if phrase.casefold() in body.casefold():
            raise SystemExit(f"{source} contains pre-publication wording: {phrase}")

    publication = f"""

## Publication record

- Tag: `{args.tag}`
- Commit: `{args.commit}`
- Release workflow: {args.run_url}
- crates.io: [`glass-browser {args.version}`](https://crates.io/crates/glass-browser/{args.version}) and [`glass-dev {args.version}`](https://crates.io/crates/glass-dev/{args.version})
- Distribution: crates.io/source-only GitHub Release; no native binary assets

This body was generated from the tagged repository source. Publication and
verification status are checked by the release workflow; this text does not
claim a signature is verified unless GitHub reports it as verified.
""".strip()
    args.output.write_text(f"{title}\n\n{body}\n\n{publication}\n", encoding="utf-8")
    print(f"generated substantive release notes: {args.output}")


if __name__ == "__main__":
    main()
