#!/usr/bin/env python3
"""Run the native journey corpus without retaining public page content."""

from __future__ import annotations

import argparse
import json
import os
import re
import signal
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
MAX_JOURNEY_OUTER_TIMEOUT_MS = 600_000


def journey_outer_timeout_ms(journey: dict) -> int:
    """Return the bounded process-group deadline, distinct from step budgets."""
    explicit = journey.get("runnerTimeoutMs")
    if explicit is None:
        explicit = int(journey.get("timeoutMs", 20_000)) + 10_000
    if isinstance(explicit, bool) or not isinstance(explicit, int):
        raise ValueError("runnerTimeoutMs must be an integer")
    if not 1_000 <= explicit <= MAX_JOURNEY_OUTER_TIMEOUT_MS:
        raise ValueError(
            f"runnerTimeoutMs must be between 1000 and {MAX_JOURNEY_OUTER_TIMEOUT_MS}"
        )
    return explicit


def classify(message: str, timed_out: bool = False) -> str:
    text = message.lower()
    if any(
        marker in text
        for marker in (
            "native action target was not found",
            "native action target matched",
            "observed target not found",
            "observed target is ambiguous",
            "no visible target matches",
            "multiple visible targets match",
            "fresh observation has no matching scroll target",
            "fresh observation has ambiguous scroll targets",
        )
    ):
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
    if "observed target not found" in text:
        return "native_target_not_found"
    if "observed target is ambiguous" in text:
        return "native_target_ambiguous"
    if "no visible target matches" in text:
        return "native_target_not_found"
    if "multiple visible targets match" in text:
        return "native_target_ambiguous"
    if "fresh observation has no matching scroll target" in text:
        return "native_scroll_target_not_found"
    if "fresh observation has ambiguous scroll targets" in text:
        return "native_scroll_target_ambiguous"
    if "native viewport scroll produced no movement" in text:
        return "native_viewport_scroll_unavailable"
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


def observed_click_step(step: dict) -> dict:
    if step.get("targetSource") != "fresh-observation":
        raise ValueError("clickObserved requires targetSource=fresh-observation")
    role = step.get("role")
    name_contains = step.get("nameContains")
    name_equals = step.get("nameEquals")
    if not isinstance(role, str) or not role:
        raise ValueError("clickObserved requires a nonempty role")
    if isinstance(name_contains, str) and name_contains and name_equals is None:
        return {"action": "clickObserved", "role": role, "nameContains": name_contains}
    if isinstance(name_equals, str) and name_equals and name_contains is None:
        return {"action": "clickObserved", "role": role, "nameEquals": name_equals}
    raise ValueError("clickObserved requires exactly one of nonempty nameContains or nameEquals")


def observed_scroll_step(step: dict) -> dict:
    if step.get("targetSource") != "fresh-observation":
        raise ValueError("scrollTargetObserved requires targetSource=fresh-observation")
    name_contains = step.get("nameContains")
    tag_name = step.get("tagName")
    if not (isinstance(name_contains, str) and name_contains.strip()) and not (
        isinstance(tag_name, str) and tag_name.strip()
    ):
        raise ValueError("scrollTargetObserved requires nonempty nameContains or tagName")
    result = {"action": "scrollTargetObserved", "dx": step.get("dx", 0), "dy": step.get("dy", 0)}
    if isinstance(name_contains, str) and name_contains.strip():
        result["nameContains"] = name_contains
    if isinstance(tag_name, str) and tag_name.strip():
        result["tagName"] = tag_name
    if isinstance(step.get("frameId"), str) and step["frameId"].strip():
        result["frameId"] = step["frameId"]
    return result


def observed_viewport_scroll_step(step: dict, previous_action: str | None) -> dict:
    if step.get("targetSource") != "observed-viewport":
        raise ValueError("scroll requires targetSource=observed-viewport")
    if previous_action != "observe":
        raise ValueError("scroll requires a fresh observe step immediately before it")
    result = {"action": "scroll", "dx": step.get("dx", 0), "dy": step.get("dy", 0)}
    if result["dx"] == 0 and result["dy"] == 0:
        raise ValueError("observed viewport scroll requires a nonzero delta")
    return result


def _signal_process_group(process: subprocess.Popen, sig: int) -> None:
    try:
        os.killpg(process.pid, sig)
    except ProcessLookupError:
        pass


def run_process_group(command: list[str], *, cwd: Path, env: dict, timeout: float) -> tuple[str, str, int | None, bool]:
    """Run an owned CLI session and kill/reap its entire process group on timeout."""
    process = subprocess.Popen(
        command,
        cwd=cwd,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
    )
    try:
        stdout, stderr = process.communicate(timeout=timeout)
        return stdout, stderr, process.returncode, False
    except subprocess.TimeoutExpired as timeout_error:
        partial_stdout = timeout_error.stdout or ""
        partial_stderr = timeout_error.stderr or ""
        _signal_process_group(process, signal.SIGTERM)
        try:
            stdout, stderr = process.communicate(timeout=1)
        except subprocess.TimeoutExpired:
            stdout = ""
            stderr = ""
        # The direct CLI may exit on SIGTERM while an owned worker still holds
        # the output pipes. The isolated process group lets us stop those
        # children too, then communicate() reaps the direct child.
        _signal_process_group(process, signal.SIGKILL)
        # Always drain and wait after killing the group. The direct child may
        # already have exited while an inherited pipe kept communicate() open.
        stdout, stderr = process.communicate()
        if not stdout:
            stdout = partial_stdout
        if not stderr:
            stderr = partial_stderr
        return stdout, stderr, process.returncode, True


def progress_step_results(progress_path: Path, total: int) -> list[dict]:
    """Recover completed batch steps and the step interrupted by the runner deadline."""
    events = []
    try:
        lines = progress_path.read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError:
        return []
    for line in lines:
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(event, dict):
            events.append(event)
    starts = {
        event["index"]: event for event in events
        if event.get("event") == "step_started" and isinstance(event.get("index"), int)
    }
    finished = {
        event["index"]: event for event in events
        if event.get("event") == "step_finished" and isinstance(event.get("index"), int)
    }
    results = []
    for index in range(total):
        event = finished.get(index)
        if event is not None:
            status = event.get("status")
            results.append({
                "index": index,
                "action": event.get("action"),
                "status": status,
                "elapsedMs": event.get("elapsedMs"),
                "errorKind": "batch_step_error" if status == "error" else None,
                "message": "completed step reported an error" if status == "error" else "",
            })
        elif index in starts:
            results.append({
                "index": index,
                "action": starts[index].get("action"),
                "status": "interrupted",
                "elapsedMs": None,
                "errorKind": "runner_deadline",
                "message": "runner deadline expired during this step",
            })
            break
        else:
            break
    if not results and any(event.get("event") == "batch_started" for event in events):
        results.append({
            "index": None,
            "action": "batch_setup",
            "status": "interrupted",
            "elapsedMs": None,
            "errorKind": "runner_deadline",
            "message": "runner deadline expired before the first step started",
        })
    return results


def run_public_journey(binary: str, journey: dict, artifact_dir: Path) -> dict:
    try:
        outer_timeout_ms = journey_outer_timeout_ms(journey)
    except ValueError as error:
        return {"id": journey["id"], "classification": "product_bug", "message": str(error)}
    steps = []
    previous_action = None
    for step in journey["steps"]:
        action = step["action"]
        if action == "navigate":
            steps.append({"action": "navigate", "url": step["url"], "timeout_ms": step.get("timeoutMs", journey["timeoutMs"])})
        elif action == "observe":
            steps.append({"action": "observe"})
        elif action == "click":
            steps.append({"action": "click", "target": step["target"]})
        elif action == "clickObserved":
            try:
                steps.append(observed_click_step(step))
            except ValueError as error:
                return {"id": journey["id"], "classification": "product_bug", "message": str(error)}
        elif action == "scrollTargetObserved":
            try:
                steps.append(observed_scroll_step(step))
            except ValueError as error:
                return {"id": journey["id"], "classification": "product_bug", "message": str(error)}
        elif action == "scroll":
            try:
                steps.append(observed_viewport_scroll_step(step, previous_action))
            except ValueError as error:
                return {"id": journey["id"], "classification": "product_bug", "message": str(error)}
        elif action == "assert":
            condition = step["condition"] if "condition" in step else f"js={step['expression']}"
            steps.append({"action": "wait", "condition": condition, "timeout_ms": step.get("timeoutMs", 5000)})
        elif action == "capability":
            # Explicit negative capability assertion, evaluated in-page without
            # attempting playback or fetching additional media.
            steps.append({"action": "wait", "condition": "js=document.createElement('video').canPlayType('video/mp4') === ''", "timeout_ms": 1000})
        else:
            return {"id": journey["id"], "classification": "unsupported", "message": f"runner does not implement step action {action}"}
        previous_action = action

    started = time.monotonic()
    with tempfile.TemporaryDirectory(prefix="glass-native-journey-") as temp_dir:
        manifest = Path(temp_dir) / "batch.json"
        manifest.write_text(json.dumps({"steps": steps}), encoding="utf-8")
        progress_source = Path(temp_dir) / "progress.jsonl"
        progress_source.touch(mode=0o600)
        command = [binary, "--browser-runtime", "native", "--incognito", "--policy", "polite", "batch", "--mode", "unguarded", str(manifest)]
        trace_base = Path(temp_dir) / "native-trace"
        environment = os.environ.copy()
        environment["GLASS_RUNTIME_DIAGNOSTIC_TRACE"] = str(trace_base)
        environment["GLASS_BROWSER_BATCH_PROGRESS_PATH"] = str(progress_source)
        stdout, stderr, return_code, timed_out = run_process_group(
            command,
            cwd=ROOT,
            env=environment,
            timeout=outer_timeout_ms / 1000,
        )
        progress_results = progress_step_results(progress_source, len(steps))
        stderr = stderr[:512] if not timed_out else "native journey process exceeded its outer deadline"

        trace_source = trace_base.with_suffix(".native_load.jsonl")
        trace_target = artifact_dir / f"{journey['id']}.native_load.jsonl"
        progress_target = artifact_dir / f"{journey['id']}.progress.jsonl"
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
        if progress_source.exists() and progress_source.stat().st_size:
            shutil.copyfile(progress_source, progress_target)
            progress_target.chmod(0o600)

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
    if timed_out and not step_results:
        step_results = progress_results
    failed_steps = [step for step in step_results if step.get("status") not in ("ok", "success", "completed")]
    message = failed_steps[0].get("message", "") if failed_steps else stderr
    return {
        "id": journey["id"],
        "backend": "native",
        "classification": "pass" if passed else classify(message, timed_out),
        "errorKind": None if passed else error_kind(message, timed_out),
        "elapsedMs": elapsed_ms,
        "outerTimeoutMs": outer_timeout_ms,
        "returnCode": return_code,
        "finalRevision": result.get("finalRevision"),
        "finalUrl": result.get("finalUrl"),
        "completedSteps": result.get("completed"),
        "totalSteps": result.get("total"),
        "steps": step_results,
        "failedSteps": failed_steps,
        "diagnosticResultId": (result.get("detailAvailability") or {}).get("resultId"),
        "diagnosticsArtifact": str(trace_target) if trace_target.exists() else None,
        "progressArtifact": str(progress_target) if progress_target.exists() else None,
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
