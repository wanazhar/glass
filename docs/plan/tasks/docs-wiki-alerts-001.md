# Documentation wiki and change alerts

Status: implementation in progress.

## Goal

Provide one versioned documentation home that routes readers by product area,
topic, and section while keeping existing guides canonical. Track every
repository Markdown file with a class, topic, canonical route, and heading
inventory. Report actionable documentation changes in local checks and CI.

## Scope

- Extend the existing documentation index as the topic hub and add a
  machine-readable page catalog; do not copy guide prose or create a separate
  GitHub wiki.
- Discover Markdown files from Git and classify current, historical,
  generated, and record material using explicit rules.
- Report added, removed, renamed, and edited pages, changed headings/anchors,
  and body-only edits relative to a base SHA.
- Validate local links and anchors, catalog route coverage, and catalog freshness.
- Exercise report behavior with isolated fixtures and document the authoring
  workflow for maintainers and agents.

## Acceptance

- Every tracked or untracked non-ignored Markdown file has a class, topic,
  canonical route, and heading inventory; missing or unrouted entries fail.
- A topic hub provides routes for product boundaries, setup/auth/models,
  everyday workspace/browser/process use, orchestration/recovery/review,
  APIs/runtime/Rust, development/release, and historical/generated records.
- The change-aware report names page and section changes and distinguishes
  body-only edits without claiming semantic freshness detection.
- CI writes the report to the job summary and existing documentation checks
  remain enabled.
- Fixtures cover heading rename, broken anchor, new unrouted page, body-only
  edit, and historical classification.
- Documentation coverage, depth, release-truth, link checks, formatting, and
  tests pass; commit locally with a Conventional Commit.

## Verification

Run `python3 scripts/check-documentation-wiki.py --base <sha>`, its fixture
tests, `scripts/release-certify.sh`, and `git diff --check`.
