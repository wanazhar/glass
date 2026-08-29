# Maintainer handbook

Status: Current repository process. This page is the versioned maintainer wiki
for documentation, source-of-truth decisions, validation, review, and release
evidence.

## Start here

| Task | Authority | Required result |
|---|---|---|
| Change user-facing behavior | [Contributing guide](../../CONTRIBUTING.md) | Code, tests, and current guides agree |
| Add or revise documentation | [Documentation style](../documentation-style.md) and this handbook | The document is classified, routed, and audited |
| Change a public interface | [Feature reference](../features.md) and [ownership guide](../ownership.md) | Every supported interface and owner stays discoverable |
| Prepare a release | [Release checklist](../release-checklist.md) | Exact source, CI, package, registry, and release evidence is recorded |
| Investigate a failure | [Release evidence](../release-evidence.md) and the relevant plan record | Historical facts remain immutable and current guidance is repaired |
| Report a security issue | [Security policy](../../SECURITY.md) | Sensitive details use the private reporting path |

This handbook is part of the repository contract. A proposal, pull request,
or release is incomplete when its documentation does not follow this page.

## Documentation contract

Every non-ignored Markdown file is part of the documentation inventory. The
release-truth audit and the link/depth checks discover the inventory from Git;
they do not use a manually maintained list of filenames.

Use one of these classes:

- `current`: describes the behavior, interface, installation, support, or
  process in the current source checkout. New Markdown defaults to this class.
- `historical`: preserves an old design, benchmark result, migration, release
  note, or implementation plan. Keep its version, date, and evidence explicit.
- `record`: records a changelog, release checklist, or release evidence. Do not
  rewrite a completed record to make a later source state look older or newer.
- `generated`: is an embedded asset or fixture consumed by a program. Change
  the generator or fixture contract when one exists; do not present it as a
  current user guide.

The checker derives these classes from repository paths and reserved record
files. If a new document does not have a historical or generated path, it is
audited as current. This fail-closed default prevents a new file from silently
escaping the current-claim audit.

## Add or change a document

1. Choose one canonical owner for each claim. Link to that owner instead of
   copying a version, shortcut, command list, or support statement into many
   pages.
2. Put current guides under `docs/`, package guidance beside its package, and
   historical material under its historical path. Put generated material under
   its generated path.
3. Add every current guide to [the documentation index](../INDEX.md). Add its
   purpose and route to the documentation-depth audit when it is a new guide.
4. Update code-derived references in the same change. This includes CLI
   inventories, MCP tools, examples, Rust modules, package links, and shortcut
   inventories.
5. Use stable links and exact names. Do not write a current-release claim from
   a remembered version. Do not call a local result published, supported, or
   verified unless the required evidence exists.
6. Preserve historical context. A previous version in a migration, release,
   benchmark, or evidence record is useful when its scope is explicit; it is
   not a current default.

## Validation

Run the static documentation checks while editing:

```console
python3 scripts/check-release-documentation.py \
  --require-previous-version \
  --report /tmp/glass-release-documentation.json
python3 scripts/check-documentation-depth.py
python3 scripts/check-tui-shortcuts.py
git diff --check
```

After the debug binaries are built, run the inventory and link check:

```console
python3 scripts/check-documentation-coverage.py
```

Run the common source certification before requesting merge or release:

```console
scripts/release-certify.sh
```

The release-truth report records all discovered documents, classifications,
previous-version references, and semantic audit lines. A current claim that
uses the previous published version fails the check. Historical and record
references remain visible in the report for review.

The coverage check resolves repository-local links and compares live command,
MCP, example, and Rust-module inventories with their documentation. The depth
check requires every current `docs/` guide to be routed from the index and
accounted for by the depth audit. CI runs these checks before it issues an
exact-source release certification.

## Review and ownership

Reviewers must ask four questions:

1. Is this claim current, historical, recorded, or generated?
2. Which file is the canonical source of the claim?
3. Does the change alter a code-derived inventory, link, shortcut, package,
   support, or release statement elsewhere?
4. Does the validation output prove the claim, or does it only prove that the
   code compiled?

The author owns the claim until the pull request is merged. The release owner
owns the exact source and publication record. A failed check is fixed at its
source; it is not hidden by weakening a stale marker or relabeling current
guidance as historical.

## Failure and recovery

If a check finds a stale current claim, correct the canonical guide and search
all discovered Markdown files for the old wording. If a historical record is
wrong, add a dated correction that preserves the original evidence boundary.
If a link or inventory is missing, update the owning index or generator in the
same change. Re-run the affected check after the last documentation edit.

If a release has already published, do not move its tag or rewrite its record.
Add a follow-up source fix, record the post-release discovery, and verify the
public surface again before announcing closure.
