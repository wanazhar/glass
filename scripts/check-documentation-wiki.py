#!/usr/bin/env python3
"""Validate the documentation catalog and report Markdown changes by section."""

from __future__ import annotations

import argparse
import difflib
import io
import json
import os
import pathlib
import re
import subprocess
import tarfile
import urllib.parse

ROOT = pathlib.Path(__file__).resolve().parent.parent
CATALOG = ROOT / "docs/documentation-catalog.json"
TOPIC_ROUTES = {
    "products": ("Glass and Core product boundaries", "docs/INDEX.md#product-and-package-boundaries"),
    "setup": ("Setup, providers, authentication, and models", "docs/INDEX.md#setup-authentication-providers-and-models"),
    "workspace": ("Everyday TUI, editor, Git, browser, and processes", "docs/INDEX.md#workspace-operations"),
    "orchestration": ("Orchestration, recovery, and review", "docs/INDEX.md#orchestration-recovery-and-review"),
    "api": ("API, runtime, and Rust", "docs/INDEX.md#interface-references"),
    "development": ("Development, testing, and release", "docs/INDEX.md#maintainer-and-release-guides"),
    "history": ("Historical, generated, and evidence records", "docs/INDEX.md#historical-generated-and-evidence-records"),
    "credits": ("Credits and project references", "docs/INDEX.md#credits-and-project-references"),
    "reference": ("Reference and support material", "docs/INDEX.md#glass-documentation"),
}


def markdown_paths(root: pathlib.Path = ROOT) -> list[str]:
    output = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "--", "*.md"],
        cwd=root, text=True,
    )
    return sorted(set(line for line in output.splitlines() if line))


def classify(path: str) -> str:
    lower = path.lower()
    parts = pathlib.PurePosixPath(lower).parts
    if any(part in parts for part in ("fixtures", "generated", "assets")):
        return "generated"
    if path in {"CHANGELOG.md", "docs/release-evidence.md"} or "releases" in parts or "release-checklist" in lower:
        return "record"
    if any(part in parts for part in ("plan", "migration", "design", "research", "archive", "reviews")):
        return "historical"
    return "current"


def topic_for(path: str) -> str:
    if path == "docs/INDEX.md":
        return "reference"
    if "credit" in path.lower() or "inspiration" in path.lower():
        return "credits"
    if classify(path) in {"historical", "record", "generated"}:
        return "history"
    if path.startswith(("products/", "crates/glass-core/")) or path == "docs/glass-core-product.md":
        return "products"
    if any(token in path.lower() for token in ("getting-started", "install", "provider", "auth", "model")):
        return "setup"
    if any(token in path.lower() for token in ("task", "thread", "queue", "recovery", "review", "attention", "agent")):
        return "orchestration"
    if path.startswith(("crates/", "clients/")) or any(token in path.lower() for token in ("sdk", "mcp", "protocol", "runtime", "api")):
        return "api"
    if path.startswith(("scripts/", ".github/")) or any(token in path.lower() for token in ("contribut", "maintain", "release", "test", "build", "develop")):
        return "development"
    if any(
        token in path.lower()
        for token in (
            "browser",
            "profile",
            "action",
            "semantic",
            "policy",
            "site-smoke",
            "bug_report",
            "bug_hunt_report",
        )
    ):
        return "workspace"
    return "reference"


def slug(text: str) -> str:
    text = re.sub(r"<[^>]+>", "", text).lower().strip()
    text = re.sub(r"[`*_~]", "", text)
    text = re.sub(r"[^\w\- ]", "", text, flags=re.UNICODE)
    return re.sub(r"\s+", "-", text)


def headings(text: str) -> list[dict[str, str]]:
    found: list[dict[str, str]] = []
    in_fence = False
    anchors: dict[str, int] = {}
    for line in text.splitlines():
        if re.match(r"^\s*(```|~~~)", line):
            in_fence = not in_fence
            continue
        match = re.match(r"^(#{1,6})\s+(.+?)\s*#*\s*$", line) if not in_fence else None
        if match:
            title = match.group(2).strip()
            base = slug(title)
            count = anchors.get(base, 0)
            anchors[base] = count + 1
            found.append({"level": str(len(match.group(1))), "title": title,
                          "anchor": base if count == 0 else f"{base}-{count}"})
    return found


def audience_for(path: str) -> str:
    lower = path.lower()
    parts = pathlib.PurePosixPath(lower).parts
    if pathlib.PurePosixPath(path).name in {"AGENTS.md", "CLAUDE.md"}:
        return "agent-instructions"
    if any(part in parts for part in ("fixtures", "generated", "assets")):
        return "generated-or-fixture"
    if any(part in parts for part in ("plan", "design", "research", "archive", "reviews")):
        return "proposal-design-or-history"
    if path in {"CONTRIBUTING.md", "SECURITY.md"} or path.startswith((".github/", "scripts/", "docs/maintainers/")):
        return "contributor-maintainer"
    if path.startswith(("crates/", "clients/")):
        return "api-or-package"
    if path.startswith("products/"):
        return "product-user-or-developer"
    if classify(path) in {"historical", "record"}:
        return "historical-or-release-evidence"
    if path.startswith("docs/"):
        return "glass-user"
    return "repository-reference"


def status_for(path: str) -> str:
    classification = classify(path)
    parts = pathlib.PurePosixPath(path.lower()).parts
    if path in {"AGENTS.md", "CLAUDE.md"}:
        return "current-agent-instructions"
    if any(part in parts for part in ("plan", "design", "research", "archive", "reviews")):
        return "plan-or-evidence-status-is-recorded-in-page"
    return {
        "current": "current-source-guidance",
        "historical": "historical-record",
        "record": "dated-or-release-record",
        "generated": "generated-or-fixture-input",
    }[classification]


def recommendation_for(path: str) -> tuple[str, str, str | None]:
    classification = classify(path)
    if classification == "historical":
        return (
            "ARCHIVE_PUBLIC",
            "Keep historical evidence discoverable in its scoped record area; do not rewrite it as current guidance.",
            None,
        )
    if classification == "generated":
        return (
            "KEEP_PUBLIC",
            "Preserve generated or fixture inputs until their owning generator and consumers are verified.",
            None,
        )
    if classification == "record":
        return (
            "KEEP_PUBLIC",
            "Preserve dated release, changelog, and evidence records as an auditable source history.",
            None,
        )
    if path == "AGENTS.md":
        return (
            "KEEP_PUBLIC",
            "Repository agent instructions are an active source contract and contain no private category by path.",
            None,
        )
    if path.startswith("crates/") and path.endswith("/README.md"):
        return (
            "KEEP_PUBLIC",
            "Cargo package documentation is included by package metadata and routes users to the canonical product/API guide.",
            None,
        )
    return (
        "KEEP_PUBLIC",
        "Current source, contributor, or product documentation has a canonical repository route; consolidate claims at the owning guide.",
        None,
    )


def page_record(
    path: str,
    text: str,
    package_readmes: dict[str, list[str]],
    consumers: dict[str, list[str]],
    dependencies: dict[str, list[str]],
) -> dict[str, object]:
    topic = topic_for(path)
    sections = headings(text)
    route = f"{path}#{sections[0]['anchor']}" if sections else path
    recommendation, reason, successor = recommendation_for(path)
    return {"path": path, "class": classify(path), "topic": topic,
            "audience": audience_for(path), "status": status_for(path),
            "recommendation": recommendation, "recommendationReason": reason,
            "canonicalSuccessor": successor,
            "consumers": sorted(set(package_readmes.get(path, []) + consumers.get(path, []))),
            "dependencies": dependencies.get(path, []),
            "topicRoute": TOPIC_ROUTES[topic][1], "route": route,
            "sections": sections}


def source_texts(paths: list[str]) -> dict[str, str]:
    return {path: (ROOT / path).read_text(encoding="utf-8") for path in paths}


def unrouted_pages(paths: set[str]) -> list[str]:
    """New pages that only fall through to the generic reference route."""
    return sorted(path for path in paths if topic_for(path) == "reference")


def unlinked_current_pages(paths: set[str], index_text: str) -> list[str]:
    """Current additions need a direct route from the human documentation index."""
    routed: set[str] = set()
    for raw in LINK.findall(index_text):
        target = raw.strip().split(maxsplit=1)[0].strip("<>")
        if target.startswith(("https://", "http://", "mailto:", "#")):
            continue
        decoded = urllib.parse.unquote(target).split("#", 1)[0].split("?", 1)[0]
        if decoded:
            resolved = ((ROOT / "docs/INDEX.md").parent / decoded).resolve()
            try:
                routed.add(resolved.relative_to(ROOT).as_posix())
            except ValueError:
                continue
    return sorted(
        path for path in paths
        if classify(path) == "current" and path != "docs/INDEX.md" and path not in routed
    )


def link_relationships(paths: list[str], texts: dict[str, str]) -> tuple[dict[str, list[str]], dict[str, list[str]]]:
    known = set(paths)
    consumers: dict[str, list[str]] = {}
    dependencies: dict[str, list[str]] = {}
    for source in paths:
        for raw in LINK.findall(texts[source]):
            target = raw.strip().split(maxsplit=1)[0].strip("<>")
            if target.startswith(("https://", "http://", "mailto:", "#")):
                continue
            decoded = urllib.parse.unquote(target).split("#", 1)[0].split("?", 1)[0]
            if not decoded:
                continue
            resolved = ((ROOT / source).parent / decoded).resolve()
            try:
                relative = resolved.relative_to(ROOT).as_posix()
            except ValueError:
                continue
            if relative in known and relative != source:
                dependencies.setdefault(source, []).append(relative)
                consumers.setdefault(relative, []).append(source)
    return (
        {path: sorted(set(values)) for path, values in consumers.items()},
        {path: sorted(set(values)) for path, values in dependencies.items()},
    )


def expected_catalog(paths: list[str], package_readmes: dict[str, list[str]]) -> dict[str, object]:
    pages = source_texts(paths)
    consumers, dependencies = link_relationships(paths, pages)
    return {"schemaVersion": 2, "pages": [
        page_record(path, pages[path], package_readmes, consumers, dependencies)
        for path in paths
    ]}


def validate_routes(catalog: dict[str, object]) -> list[str]:
    failures: list[str] = []
    for page in catalog["pages"]:
        path = page["path"]
        if page["topic"] not in TOPIC_ROUTES:
            failures.append(f"{path}: unknown topic {page['topic']!r}")
        if not page["route"] or not page["topicRoute"]:
            failures.append(f"{path}: missing canonical route or topic route")
            continue
        route_path, _, route_anchor = page["route"].partition("#")
        route_file = ROOT / route_path
        if not route_file.is_file():
            failures.append(f"{path}: canonical route target is missing: {page['route']}")
        elif route_anchor and route_anchor not in {h["anchor"] for h in headings(route_file.read_text(encoding="utf-8"))}:
            failures.append(f"{path}: canonical route anchor is missing: {page['route']}")
        topic_path, _, topic_anchor = page["topicRoute"].partition("#")
        topic_file = (ROOT / topic_path).resolve()
        if not topic_file.is_file():
            failures.append(f"{path}: topic route target is missing: {page['topicRoute']}")
        elif topic_anchor and topic_anchor not in {h["anchor"] for h in headings(topic_file.read_text(encoding="utf-8"))}:
            failures.append(f"{path}: topic route anchor is missing: {page['topicRoute']}")
    return failures


def package_readme_inventory() -> tuple[dict[str, list[str]], list[str]]:
    """Keep Cargo's package metadata and the Markdown catalog connected."""
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"],
        cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    if result.returncode:
        return {}, [f"cargo metadata failed while checking package readmes: {result.stderr.strip()}"]
    metadata = json.loads(result.stdout)
    failures: list[str] = []
    consumers: dict[str, list[str]] = {}
    for package in metadata.get("packages", []):
        readme = package.get("readme")
        if not readme:
            failures.append(f"Cargo package {package['name']} has no package README")
            continue
        path = pathlib.Path(readme)
        if not path.is_absolute():
            path = pathlib.Path(package["manifest_path"]).parent / path
        if not path.is_file():
            failures.append(f"Cargo package {package['name']} README is missing: {path}")
        try:
            relative = path.resolve().relative_to(ROOT).as_posix()
        except ValueError:
            failures.append(f"Cargo package {package['name']} README escapes the workspace: {path}")
            continue
        consumers.setdefault(relative, []).append(package["name"])
    return consumers, failures


LINK = re.compile(r"(?<!!)\[[^\]]*\]\(([^)]+)\)")


def check_links(paths: list[str]) -> list[str]:
    failures: list[str] = []
    for path in paths:
        text = (ROOT / path).read_text(encoding="utf-8")
        for raw in LINK.findall(text):
            target = raw.strip().split(maxsplit=1)[0].strip("<>")
            if target.startswith(("https://", "http://", "mailto:", "#")):
                if target.startswith("#") and target[1:] not in {h["anchor"] for h in headings(text)}:
                    failures.append(f"{path}: broken local anchor #{target[1:]}")
                continue
            decoded = urllib.parse.unquote(target)
            file_part, _, anchor = decoded.partition("#")
            file_part = file_part.split("?", 1)[0]
            resolved = ((ROOT / path).parent / file_part).resolve()
            try:
                resolved.relative_to(ROOT)
            except ValueError:
                failures.append(f"{path}: link escapes repository: {target}")
                continue
            if not resolved.exists():
                failures.append(f"{path}: missing link target {target}")
            elif anchor and resolved.suffix.lower() == ".md":
                target_text = resolved.read_text(encoding="utf-8", errors="replace") if resolved.is_file() else ""
                if anchor not in {h["anchor"] for h in headings(target_text)}:
                    failures.append(f"{path}: broken anchor in {target}")
    return failures


def base_texts(base: str) -> dict[str, str]:
    archive = subprocess.check_output(["git", "archive", "--format=tar", base], cwd=ROOT)
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as bundle:
        result: dict[str, str] = {}
        for member in bundle.getmembers():
            if member.isfile() and member.name.lower().endswith(".md"):
                stream = bundle.extractfile(member)
                if stream is not None:
                    result[member.name] = stream.read().decode("utf-8", errors="replace")
        return result


def diff_report(base: str, current: dict[str, str]) -> tuple[str, list[dict[str, object]]]:
    before = base_texts(base)
    removed, added = set(before) - set(current), set(current) - set(before)
    entries: list[dict[str, object]] = []
    # Pair highly similar removals/additions as renames, including edited moves.
    for old in sorted(list(removed)):
        candidates = sorted(
            ((difflib.SequenceMatcher(None, before[old], current[new]).ratio(), new) for new in added),
            reverse=True,
        )
        match = candidates[0][1] if candidates and candidates[0][0] >= 0.55 else None
        if match:
            old_h, new_h = headings(before[old]), headings(current[match])
            entries.append({"kind": "renamed", "from": old, "to": match,
                            "sectionsAdded": [h["title"] for h in new_h if h not in old_h],
                            "sectionsRemoved": [h["title"] for h in old_h if h not in new_h]})
            removed.remove(old); added.remove(match)
    for path in sorted(added):
        entries.append({"kind": "added", "path": path,
                        "sectionsAdded": [h["title"] for h in headings(current[path])]})
    for path in sorted(removed):
        entries.append({"kind": "removed", "path": path,
                        "sectionsRemoved": [h["title"] for h in headings(before[path])]})
    for path in sorted(set(before) & set(current)):
        if before[path] == current[path]:
            continue
        old_h, new_h = headings(before[path]), headings(current[path])
        old_titles, new_titles = [h["title"] for h in old_h], [h["title"] for h in new_h]
        entry: dict[str, object] = {"kind": "modified", "path": path}
        if old_h != new_h:
            entry["sectionsAdded"] = [h["title"] for h in new_h if h not in old_h]
            entry["sectionsRemoved"] = [h["title"] for h in old_h if h not in new_h]
            entry["anchorsChanged"] = [{"title": h["title"], "from": h["anchor"], "to": next((x["anchor"] for x in new_h if x["title"] == h["title"]), None)} for h in old_h if next((x for x in new_h if x["title"] == h["title"]), None) and h["anchor"] != next(x["anchor"] for x in new_h if x["title"] == h["title"])]
            entry["kind"] = "sections-changed"
        else:
            entry["kind"] = "body-only"
        entries.append(entry)
    report = [f"Documentation change report (base `{base}`)"]
    if not entries:
        report.append("- No Markdown changes.")
    for item in entries:
        kind = item["kind"]
        if kind == "renamed":
            report.append(f"- Renamed ({topic_for(item['to'])}): `{item['from']}` → `{item['to']}`")
            if item.get("sectionsAdded"): report.append("  - Sections added/renamed: " + ", ".join(f"`{v}`" for v in item["sectionsAdded"]))
            if item.get("sectionsRemoved"): report.append("  - Sections removed/renamed: " + ", ".join(f"`{v}`" for v in item["sectionsRemoved"]))
        elif kind in {"added", "removed"}:
            report.append(f"- {kind.title()} ({topic_for(item['path'])}): `{item['path']}`")
            for key, label in (("sectionsAdded", "sections added"), ("sectionsRemoved", "sections removed")):
                if item.get(key): report.append(f"  - {label}: " + ", ".join(f"`{v}`" for v in item[key]))
        elif kind == "body-only":
            report.append(f"- Body-only edit ({topic_for(item['path'])}; headings unchanged): `{item['path']}`")
        else:
            report.append(f"- Sections changed ({topic_for(item['path'])}): `{item['path']}`")
            if item.get("sectionsAdded"): report.append("  - Added: " + ", ".join(f"`{v}`" for v in item["sectionsAdded"]))
            if item.get("sectionsRemoved"): report.append("  - Removed: " + ", ".join(f"`{v}`" for v in item["sectionsRemoved"]))
            for change in item.get("anchorsChanged", []):
                report.append(f"  - Anchor changed: `{change['from']}` → `{change['to']}` ({change['title']})")
    report.append("Body-only means heading structure was unchanged; it does not assess semantic freshness.")
    return "\n".join(report) + "\n", entries


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", help="base Git SHA for a Markdown change report")
    parser.add_argument("--write", action="store_true", help="regenerate docs/documentation-catalog.json")
    parser.add_argument("--summary", action="store_true", help="append report to GITHUB_STEP_SUMMARY")
    args = parser.parse_args()
    paths = markdown_paths()
    package_readmes, failures = package_readme_inventory()
    catalog = expected_catalog(paths, package_readmes)
    if args.write:
        CATALOG.parent.mkdir(parents=True, exist_ok=True)
        CATALOG.write_text(json.dumps(catalog, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    failures.extend(validate_routes(catalog))
    if not CATALOG.exists():
        failures.append("docs/documentation-catalog.json is missing; run with --write")
    else:
        actual = json.loads(CATALOG.read_text(encoding="utf-8"))
        if actual != catalog:
            failures.append("docs/documentation-catalog.json is stale; run scripts/check-documentation-wiki.py --write")
    failures.extend(check_links(paths))
    report = ""
    if args.base:
        report, entries = diff_report(args.base, source_texts(paths))
        print(report, end="")
        added_paths = {item["path"] for item in entries if item["kind"] == "added"}
        added_paths.update(item["to"] for item in entries if item["kind"] == "renamed")
        unrouted = unrouted_pages(added_paths)
        if unrouted:
            failures.extend(f"{path}: assign an explicit wiki topic before adding this Markdown page" for path in unrouted)
            alert = "\nUnrouted new pages (assign a topic before merge):\n" + "".join(f"- `{path}`\n" for path in unrouted)
            report += alert
            print(alert, end="")
        unlinked = unlinked_current_pages(added_paths, source_texts(paths).get("docs/INDEX.md", ""))
        if unlinked:
            failures.extend(f"{path}: add a canonical page link to docs/INDEX.md before merge" for path in unlinked)
            alert = "\nUnlinked current pages (add a canonical link to docs/INDEX.md):\n" + "".join(f"- `{path}`\n" for path in unlinked)
            report += alert
            print(alert, end="")
        if args.summary:
            summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
            if summary_path:
                summary = pathlib.Path(summary_path)
                with summary.open("a", encoding="utf-8") as handle:
                    handle.write("\n" + report)
    if failures:
        raise SystemExit("documentation wiki check failed:\n" + "\n".join(f"- {f}" for f in failures))
    print(f"documentation wiki validated: {len(paths)} Markdown files, {len(catalog['pages'])} catalog entries")


if __name__ == "__main__":
    main()
