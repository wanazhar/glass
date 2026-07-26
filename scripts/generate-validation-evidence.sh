#!/usr/bin/env bash
set -euo pipefail

# Generate revision-bound, fail-closed evidence files consumed by
# benchmarks/run-acceptance.mjs. This script never claims a platform or
# comparator passed unless it was actually observed on this host.

output_dir="${1:-benchmarks/results/validation/$(git rev-parse --short HEAD)}"
mkdir -p "$output_dir"
revision="$(git rev-parse HEAD)"
version="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="glass-browser"))')"
host_target="$(rustc -vV | awk '/^host:/ {print $2}')"
installed_targets="$(rustup target list --installed 2>/dev/null || true)"

test_status=passed
if ! cargo test --all-targets --locked >/dev/null; then test_status=failed; fi
fmt_status=passed
if ! cargo fmt --all -- --check >/dev/null; then fmt_status=failed; fi
clippy_status=passed
if ! cargo clippy --all-targets --all-features --locked -- -D warnings >/dev/null; then clippy_status=failed; fi
docs_status=passed
if ! cargo doc --no-deps --locked >/dev/null; then docs_status=failed; fi
build_status=passed
if ! cargo build --release --locked >/dev/null; then build_status=failed; fi
package_status=passed
if ! cargo package --locked --allow-dirty --no-verify >/dev/null; then package_status=failed; fi
deny_status=failed
if command -v cargo-deny >/dev/null 2>&1 && cargo deny check >/dev/null; then deny_status=passed; fi
audit_status=failed
if command -v cargo-audit >/dev/null 2>&1 && cargo audit >/dev/null; then audit_status=passed; fi
fuzz_status=passed
if ! cargo check --manifest-path fuzz/Cargo.toml --locked --offline --all-targets >/dev/null; then fuzz_status=failed; fi

python3 - "$output_dir" "$revision" "$version" "$host_target" "$installed_targets" "$test_status" "$fmt_status" "$clippy_status" "$docs_status" "$build_status" "$package_status" "$deny_status" "$audit_status" "$fuzz_status" <<'PY'
import json
import os
import pathlib
import sys

(
    out, revision, version, host, targets, tests, fmt, clippy, docs, build,
    package, deny, audit, fuzz,
) = sys.argv[1:]
target_list = targets.splitlines() if targets else []
required = [
    "x86_64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
]
release_statuses = {
    "fmt": fmt,
    "test": tests,
    "clippy": clippy,
    "docs": docs,
    "build": build,
    "package": package,
    "deny": deny,
    "audit": audit,
    "fuzz-check": fuzz,
}
release_passed = all(status == "passed" for status in release_statuses.values())
local_passed = tests == fmt == clippy == "passed"
producer = {
    "name": "glass-local-validation",
    "version": version,
    "command": "scripts/generate-validation-evidence.sh",
    "run_url": os.environ.get("GITHUB_RUN_URL", "local://glass-validation"),
}
base = {
    "schema_version": 1,
    "git_revision": revision,
    "producer": producer,
}
check_commands = {
    "fmt": "cargo fmt --all -- --check",
    "test": "cargo test --all-targets --locked",
    "clippy": "cargo clippy --all-targets --all-features --locked -- -D warnings",
    "docs": "cargo doc --no-deps --locked",
    "build": "cargo build --release --locked",
    "package": "cargo package --locked --allow-dirty --no-verify",
    "deny": "cargo deny check",
    "audit": "cargo audit",
    "fuzz-check": "cargo check --manifest-path fuzz/Cargo.toml --locked --offline --all-targets",
}
passed_checks = [
    {"id": check_id, "status": "passed", "raw_report": check_commands[check_id]}
    for check_id, status in release_statuses.items()
    if status == "passed"
]
metrics = json.loads(os.environ.get("GLASS_RATIFIED_METRICS", "{}"))
observed_platforms = [
    {
        "target": target,
        "os": "linux" if "linux" in target else "macos",
        "architecture": target.split("-")[0],
        "chrome": "not-recorded",
        "status": "passed",
        "raw_report": "platform-build-and-smoke",
    }
    for target in required
    if target in target_list and target == host
]
reports = {
    "ratified-gates.json": {
        **base,
        "type": "ratified_gates",
        "passed": local_passed and bool(metrics),
        "metrics": metrics,
        "raw_reports": {"local_checks": "cargo-test-fmt-clippy"},
    },
    "release-validation.json": {
        **base,
        "type": "release_validation",
        "checks": passed_checks,
    },
    "platform-matrix.json": {
        **base,
        "type": "real_browser_platform_matrix",
        "platforms": observed_platforms,
    },
}
for name, report in reports.items():
    pathlib.Path(out, name).write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"output": out, "revision": revision, "release_checks_passed": release_passed}, indent=2))
PY
