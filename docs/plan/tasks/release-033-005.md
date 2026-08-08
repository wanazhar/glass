id: release-033-005
scope: Glass v0.3.3 distribution and release candidate
status: completed
depends-on: [runtime-033-004]

## objective

Ship coherent 0.3.3 package surfaces, including both user-facing binaries from
`glass-dev`; complete three-OS/tool CI, scenarios A–K, release truth, curated
notes, limitations and every issue gate before a local final candidate.

## context

- `docs/plan/analysis/release-033.md`
- `docs/release-checklist.md`
- `docs/release-evidence.md`
- issue #33 and its authoritative amendment

## path

- workspace/package manifests and `glass-dev` launchers
- CI/release workflows and validation scripts
- clean-install/black-box smoke scripts
- all public/release documentation

## verification

- clean `CARGO_HOME` core/full transition matrix
- Linux/macOS/Windows browser-free CI definitions and tool jobs
- full local test/lint/rustdoc/audit/fuzz/package/publish-dry-run suite
- browser-backed scenarios where supported
- 53/53 release-check evidence matrix

## result

Completed locally on 2026-08-08. Both 0.3.3 crates package and pass publication
dry runs; isolated core/full installs and ownership transitions pass; source,
client, documentation, security, fuzz, PTY, tool and native Chromium gates are
recorded in `docs/release-evidence.md`. Publication remains an explicit
maintainer action.
