#!/usr/bin/env python3
"""Run the native journey corpus without retaining public page content."""

from __future__ import annotations

import argparse
import json
import os
import re
from pathlib import Path
import subprocess
import shutil
import tempfile
import time
import uuid
import tomllib
from datetime import datetime, timezone


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CORPUS = ROOT / "crates/glass-browser/tests/fixtures/native-journeys-v1.json"


def classify(message: str, timed_out: bool = False) -> str:
    text = message.lower()
    if "native action target was not found" in text or "native action target matched" in text:
        return "product_bug"
    if "content-process document snapshot" in text and "exceeds limit" in text:
        return "resource_limit"
    if any(word in text for word in ("unsupported", "not supported", "not implemented")):
        return "unsupported"
    if any(word in text for word in ("403", "forbidden", "captcha", "challenge", "access denied")):
        return "denied"
    if "native navigation exceeded" in text or "host view" in text or "layout" in text:
        return "product_bug"
    if any(word in text for word in ("dns", "connection reset", "resource fetch failed", "remote host closed")):
        return "external_flaky"
    if timed_out:
        return "inconclusive"
    return "product_bug"


def error_kind(message: str, timed_out: bool = False) -> str:
    text = message.lower()
    if "native action target was not found" in text:
        return "native_target_not_found"
    if "native action target matched" in text:
        return "native_target_ambiguous"
    if "content-process document snapshot" in text and "exceeds limit" in text:
        return "native_snapshot_budget_exceeded"
    if ("host view" in text or "javascript host snapshot" in text) and (
        "interrupted" in text or "deadline" in text
    ):
        return "native_host_view_deadline"
    if "out of memory" in text:
        return "quickjs_out_of_memory"
    if "null\"null\"" in text:
        return "quickjs_null_exception"
    if "native navigation exceeded" in text:
        return "native_navigation_deadline"
    if "content process operation exceeded" in text:
        return "content_process_deadline"
    if any(word in text for word in ("403", "forbidden", "captcha", "challenge", "access denied")):
        return "remote_access_denied"
    if any(word in text for word in ("dns", "connection reset", "resource fetch failed", "remote host closed")):
        return "external_network_error"
    if timed_out:
        return "runner_deadline"
    return "journey_step_error"


def run_public_journey(binary: str, journey: dict, artifact_dir: Path) -> dict:
    steps = []
    for step in journey["steps"]:
        action = step["action"]
        if action == "navigate":
            steps.append({"action": "navigate", "url": step["url"], "timeout_ms": step.get("timeoutMs", journey["timeoutMs"])})
        elif action == "observe":
            steps.append({"action": "observe"})
        elif action == "click":
            steps.append({"action": "click", "target": step["target"]})
        elif action == "assert":
            condition = step["condition"] if "condition" in step else f"js={step['expression']}"
            steps.append({"action": "wait", "condition": condition, "timeout_ms": step.get("timeoutMs", 5000)})
        elif action == "capability":
            # Explicit negative capability assertion, evaluated in-page without
            # attempting playback or fetching additional media.
            steps.append({"action": "wait", "condition": "js=document.createElement('video').canPlayType('video/mp4') === ''", "timeout_ms": 1000})
        else:
            return {"id": journey["id"], "classification": "unsupported", "message": f"runner does not implement step action {action}"}

    started = time.monotonic()
    with tempfile.TemporaryDirectory(prefix="glass-native-journey-") as temp_dir:
        manifest = Path(temp_dir) / "batch.json"
        manifest.write_text(json.dumps({"steps": steps}), encoding="utf-8")
        command = [binary, "--browser-runtime", "native", "--incognito", "--policy", "polite", "batch", "--mode", "unguarded", str(manifest)]
        trace_base = Path(temp_dir) / "native-trace"
        environment = os.environ.copy()
        environment["GLASS_RUNTIME_DIAGNOSTIC_TRACE"] = str(trace_base)
        try:
            completed = subprocess.run(command, cwd=ROOT, env=environment, text=True, capture_output=True, timeout=(journey.get("timeoutMs", 20000) / 1000) + 10, check=False)
            timed_out = False
            stdout = completed.stdout
            stderr = completed.stderr[:512]
            return_code = completed.returncode
        except subprocess.TimeoutExpired as error:
            timed_out = True
            stdout = error.stdout.decode(errors="replace") if isinstance(error.stdout, bytes) else (error.stdout or "")
            stderr = "native journey process exceeded its outer deadline"
            return_code = None

        trace_source = trace_base.with_suffix(".native_load.jsonl")
        trace_target = artifact_dir / f"{journey['id']}.native_load.jsonl"
        stage_time_us: dict[str, int] = {}
        failure_match = None
        if trace_source.exists():
            for line in trace_source.read_text(encoding="utf-8", errors="replace").splitlines():
                try:
                    event = json.loads(line)
                except json.JSONDecodeError:
                    continue
                stage = event.get("stage")
                elapsed = event.get("elapsed_us")
                if isinstance(stage, str) and isinstance(elapsed, int):
                    stage_time_us[stage] = stage_time_us.get(stage, 0) + elapsed
            failure_match = re.search(
                r"native host view element construction failed.*element_index=(\d+).*element_metadata=\"([^\"]*)\"",
                stderr,
            )
            shutil.copyfile(trace_source, trace_target)
            trace_target.chmod(0o600)

    elapsed_ms = round((time.monotonic() - started) * 1000)
    try:
        result = json.loads(stdout)
    except json.JSONDecodeError:
        result = {}
    passed = result.get("success") is True and return_code == 0
    step_results = []
    for item in result.get("steps", []):
        message_text = str(item.get("message", ""))[:256]
        step_results.append({
            "index": item.get("index"),
            "action": item.get("action"),
            "status": item.get("status"),
            "elapsedMs": item.get("elapsedMs"),
            "errorKind": error_kind(message_text) if item.get("status") == "error" else None,
            "message": message_text,
        })
    failed_steps = [step for step in step_results if step.get("status") not in ("ok", "success", "completed")]
    message = failed_steps[0].get("message", "") if failed_steps else stderr
    return {
        "id": journey["id"],
        "backend": "native",
        "classification": "pass" if passed else classify(message, timed_out),
        "errorKind": None if passed else error_kind(message, timed_out),
        "elapsedMs": elapsed_ms,
        "returnCode": return_code,
        "finalRevision": result.get("finalRevision"),
        "finalUrl": result.get("finalUrl"),
        "completedSteps": result.get("completed"),
        "totalSteps": result.get("total"),
        "steps": step_results,
        "failedSteps": failed_steps,
        "diagnosticResultId": (result.get("detailAvailability") or {}).get("resultId"),
        "diagnosticsArtifact": str(trace_target) if trace_target.exists() else None,
        "diagnosticStageTotalsMs": {stage: round(value / 1000, 2) for stage, value in stage_time_us.items()},
        "hostViewFailureStructure": (
            {"elementIndex": int(structure_match.group(1)), "tag": structure_match.group(2),
             "attributeCount": int(structure_match.group(3)), "textLength": int(structure_match.group(4)),
             "mode": structure_match.group(5)}
            if (structure_match := re.search(r"host_view_element=(\d+):([^:]*):(\d+):(\d+):([^:]+)", message)) else
            ({"elementIndex": int(failure_match.group(1)), "tagLength": len(failure_match.group(2).split(":", 1)[0]),
              "attributeCount": int(failure_match.group(2).split(":")[1]),
              "textLength": int(failure_match.group(2).split(":")[2])}
             if failure_match and len(failure_match.group(2).split(":")) == 3 else None)
        ),
        "message": message[:256],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", type=Path, default=DEFAULT_CORPUS)
    parser.add_argument("--browser", default=os.environ.get("GLASS_BROWSER_BIN", str(ROOT / "target/debug/glass-browser")))
    parser.add_argument("--public", action="store_true", help="run only bounded public read-only journeys")
    parser.add_argument("--journey", help="select one journey id")
    parser.add_argument("--output", type=Path, help="write the redacted machine-readable report")
    args = parser.parse_args()

    corpus = json.loads(args.corpus.read_text(encoding="utf-8"))
    if corpus.get("schemaVersion") != 1:
        parser.error("unsupported journey corpus schemaVersion")
    journeys = corpus.get("journeys", [])
    if args.public:
        journeys = [journey for journey in journeys if journey.get("scope") == "public-read-only"]
    if args.journey:
        journeys = [journey for journey in journeys if journey.get("id") == args.journey]
    if not args.public:
        parser.error("local journeys run in the native integration suite; pass --public to run network journeys")
    if not journeys:
        parser.error("no journey matched the selected filters")

    output_path = args.output or Path(tempfile.gettempdir()) / f"glass-native-journey-{uuid.uuid4().hex}.json"
    artifact_dir = output_path.parent / f"{output_path.stem}.artifacts"
    artifact_dir.mkdir(parents=True, exist_ok=True)
    try:
        artifact_dir.chmod(0o700)
    except OSError:
        pass
    results = [run_public_journey(args.browser, journey, artifact_dir) for journey in journeys]
    package_version = tomllib.loads((ROOT / "crates/glass-browser/Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    if isinstance(package_version, dict) and package_version.get("workspace"):
        package_version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    report = {
        "schemaVersion": 1,
        "corpus": corpus["corpus"],
        "version": package_version,
        "backend": "native",
        "startedAt": datetime.now(timezone.utc).isoformat(),
        "commit": subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True, capture_output=True, check=False).stdout.strip(),
        "workingTreeDirty": bool(subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, text=True, capture_output=True, check=False).stdout.strip()),
        "contentPolicy": "no page dumps, cookies, or personal data retained",
        "journeys": results,
    }
    report["reportPath"] = str(output_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    try:
        output_path.chmod(0o600)
    except OSError:
        pass
    print(json.dumps(report, separators=(",", ":")))
    return 0 if all(item["classification"] == "pass" for item in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
