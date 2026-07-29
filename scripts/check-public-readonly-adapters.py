#!/usr/bin/env python3
"""Validate the versioned public/read-only adapter contract inventory."""

import json
import pathlib
import subprocess
import sys
from urllib.parse import urlparse


ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST_PATH = ROOT / "docs/public-readonly-adapters.json"
EXPECTED_IDS = {
    "documentation-site",
    "public-git-hosting",
    "accessibility-reference",
    "modern-spa",
    "server-rendered-site",
}
ALLOWED = {"navigate", "observe", "text", "verify", "wait"}
REQUIRED_FORBIDDEN = {"click", "type", "submit", "purchase", "delete", "upload", "download"}
DRIFT_CLASSIFICATIONS = {"externalInstability", "siteDrift", "glassRegression"}


def fail(message: str) -> None:
    raise SystemExit(f"public read-only adapter validation failed: {message}")


def main() -> None:
    if len(sys.argv) > 2:
        raise SystemExit("usage: check-public-readonly-adapters.py [OUTPUT.json]")
    try:
        manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read adapter manifest: {error}")
    if manifest.get("schemaVersion") != 1 or manifest.get("id") != "public-readonly-adapter-suite-v1":
        fail("manifest identity is not version 1")
    if manifest.get("runtimeCertification") != "not_run":
        fail("checked-in manifest must not claim runtime certification")
    adapters = manifest.get("adapters")
    if not isinstance(adapters, list) or {adapter.get("id") for adapter in adapters} != EXPECTED_IDS:
        fail("adapter IDs do not match the initial five-site set")
    if len(adapters) != len(EXPECTED_IDS):
        fail("adapter IDs must be unique")

    rows = []
    for adapter in adapters:
        adapter_id = adapter["id"]
        hosts = set(adapter.get("approvedHosts", []))
        parsed = urlparse(adapter.get("url", ""))
        if parsed.scheme != "https" or parsed.hostname not in hosts:
            fail(f"{adapter_id} URL is not HTTPS or is outside its host allowlist")
        allowed = set(adapter.get("allowedActionKinds", []))
        if allowed != ALLOWED:
            fail(f"{adapter_id} allowed actions are not read-only")
        forbidden = set(adapter.get("forbiddenActionKinds", []))
        if not REQUIRED_FORBIDDEN.issubset(forbidden):
            fail(f"{adapter_id} is missing required destructive-action denials")
        if not adapter.get("forbiddenTargetPatterns"):
            fail(f"{adapter_id} has no forbidden target patterns")
        limits = adapter.get("limits", {})
        if not (
            1 <= limits.get("maxRequests", 0) <= 32
            and 1 <= limits.get("maxBrowserActions", 0) <= 32
            and 1 <= limits.get("maxDurationMs", 0) <= 120000
        ):
            fail(f"{adapter_id} has invalid bounded limits")
        expected = adapter.get("expected", {})
        if not all(expected.get(field) for field in ("pageType", "regionTypes", "terminalCondition")):
            fail(f"{adapter_id} has incomplete expected read-only evidence")
        drift = adapter.get("drift", {})
        if not drift.get("signature") or set(drift.get("classifications", [])) != DRIFT_CLASSIFICATIONS:
            fail(f"{adapter_id} has incomplete external-drift classification")
        if adapter.get("credentials") != "forbidden" or adapter.get("mutations") != "forbidden":
            fail(f"{adapter_id} does not forbid credentials and mutations")
        rows.append({
            "id": adapter_id,
            "host": parsed.hostname,
            "page_type": expected["pageType"],
            "max_requests": limits["maxRequests"],
            "max_browser_actions": limits["maxBrowserActions"],
            "max_duration_ms": limits["maxDurationMs"],
            "runtime_certification": "not_run",
        })

    report = {
        "schema_version": 1,
        "type": "public_readonly_adapter_inventory",
        "source_revision": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip(),
        "adapter_count": len(rows),
        "adapters": sorted(rows, key=lambda row: row["id"]),
        "runtime_certification": "not_run",
    }
    if len(sys.argv) == 2:
        output = pathlib.Path(sys.argv[1])
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(
        f"public read-only adapter inventory validated: {len(rows)} adapters; "
        "runtime certification not claimed"
    )


if __name__ == "__main__":
    main()
