#!/usr/bin/env python3
"""Real-model tool-selection eval for Glass Core browser and computer-use tools.

Each task is a realistic request that never names a tool. The runner drives
`glass-core run --json` against the configured provider, then reads the saved
session to score:

* right_first: the first tool call belongs to the task's tool family
  (browser or desktop), or is an explicitly allowed alternative;
* ideal: the specialised tool for the capability was used at some point;
* success: an outcome check on fixture-server state, the desktop, or the answer;
* steps: number of tool calls in the run.

Usage:
  run_eval.py --glass-core target/debug/glass-core --glass-browser target/debug/glass-browser \
      --label before --out results/before.json [--only browser|desktop] [--task ID] [--repeat N]
"""

import argparse
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import fixture_server  # noqa: E402


def matches(call, matcher):
    if not re.search(matcher["name"], call["name"]):
        return False
    return "args" not in matcher or re.search(matcher["args"], call["args"], re.I) is not None


def any_match(call, matchers):
    return any(matches(call, m) for m in matchers)


def read_calls(data_dir: Path, session: str):
    found = list(data_dir.rglob(f"{session}.jsonl"))
    if not found:
        return []
    calls = []
    for line in found[0].read_text(errors="replace").splitlines():
        try:
            record = json.loads(line)
        except json.JSONDecodeError:
            continue
        if record.get("k") == "call":
            calls.append({"name": record.get("name", ""), "args": record.get("args", "")})
    return calls


def fixture_state(base):
    with urllib.request.urlopen(f"{base}/api/state", timeout=5) as response:
        return json.load(response)


def reset_fixture(base):
    request = urllib.request.Request(f"{base}/api/reset", data=b"{}", method="POST")
    urllib.request.urlopen(request, timeout=5).read()


def subset(expected, actual):
    return all(actual.get(k) == v for k, v in expected.items())


def managed_display():
    """Find the managed Xvfb display and its authority file from /proc."""
    for proc in Path("/proc").iterdir():
        if not proc.name.isdigit():
            continue
        try:
            argv = (proc / "cmdline").read_bytes().split(b"\0")
        except OSError:
            continue
        if not argv or not argv[0].endswith(b"Xvfb"):
            continue
        args = [a.decode() for a in argv if a]
        display = next((a for a in args if a.startswith(":")), None)
        auth = args[args.index("-auth") + 1] if "-auth" in args else None
        if display and display != os.environ.get("DISPLAY"):
            return display, auth
    return None, None


def desktop_env():
    display, auth = managed_display()
    if not display:
        return None
    env = dict(os.environ, DISPLAY=display)
    if auth:
        env["XAUTHORITY"] = auth
    return env


def check(task, answer, calls, base, workspace=None, existing_files=()):
    spec = task["check"]
    results = []
    if "answer_contains" in spec:
        hits = [s.lower() in answer.lower() for s in spec["answer_contains"]]
        results.append(any(hits) if spec.get("any") else all(hits))
    if "state" in spec or "state_absent" in spec:
        state = fixture_state(base)
        for event, expected in spec.get("state", {}).items():
            results.append(any(subset(expected, e) for e in state.get(event, [])))
        for event, unexpected in spec.get("state_absent", {}).items():
            results.append(not any(subset(unexpected, e) for e in state.get(event, [])))
    if "calls" in spec:
        results.append(any(re.search(spec["calls"], c["name"]) for c in calls))
    if "desktop_clipboard" in spec:
        env = desktop_env()
        text = ""
        if env:
            text = subprocess.run(["xclip", "-o", "-selection", "clipboard"], env=env,
                                  capture_output=True, text=True, timeout=5).stdout
        results.append(text.strip() == spec["desktop_clipboard"])
    if "desktop_window" in spec:
        env = desktop_env()
        names = ""
        if env:
            names = subprocess.run(["xdotool", "search", "--name", spec["desktop_window"]],
                                   env=env, capture_output=True, text=True, timeout=5).stdout
        results.append(bool(names.strip()))
    if "workspace_file" in spec:
        want = spec["workspace_file"]
        found = sorted(p for p in Path(workspace).glob(want["glob"])
                       if p.is_file() and p not in existing_files) if workspace else []
        data = found[-1].read_bytes() if found else b""
        ok = bool(found)
        if "starts_with" in want:
            ok = ok and data.startswith(want["starts_with"].encode())
        if "contains" in want:
            ok = ok and want["contains"].encode() in data
        results.append(ok)
    return bool(results) and all(results)


def output_identity(path):
    metadata = path.lstat()
    if not stat.S_ISREG(metadata.st_mode):
        return None
    return [metadata.st_dev, metadata.st_ino, metadata.st_size, metadata.st_mtime_ns]


def write_eval_outputs(workspace, paths):
    records = [{"path": str(path.relative_to(workspace)), "identity": output_identity(path)}
               for path in paths]
    descriptor, temporary = tempfile.mkstemp(prefix=".browser-cu-outputs-", dir=workspace)
    try:
        with os.fdopen(descriptor, "w") as output:
            json.dump(records, output)
        os.replace(temporary, workspace / ".browser-cu-outputs.json")
    finally:
        Path(temporary).unlink(missing_ok=True)


def clear_eval_outputs(workspace):
    manifest = workspace / ".browser-cu-outputs.json"
    try:
        descriptor = os.open(manifest, os.O_RDONLY | os.O_NOFOLLOW)
        with os.fdopen(descriptor) as source:
            records = json.load(source)
    except (OSError, ValueError):
        return
    if not isinstance(records, list):
        return
    for record in records:
        if not isinstance(record, dict) or not isinstance(record.get("path"), str):
            continue
        path = workspace / record["path"]
        if Path(record["path"]).is_absolute() or not path.resolve().is_relative_to(workspace.resolve()):
            continue
        try:
            identity = output_identity(path)
            if identity is not None and identity == record.get("identity"):
                path.unlink()
        except FileNotFoundError:
            pass


def run_task(task, families, args, base, data_dir, workspace, bin_dir):
    reset_fixture(base)
    clear_eval_outputs(workspace)
    pattern = task["check"].get("workspace_file", {}).get("glob")
    existing_files = set(workspace.glob(pattern)) if pattern else set()
    if task["id"] == "upload-receipt":
        with (workspace / "receipt-0921.txt").open("xb") as receipt:
            receipt.write((HERE / "fixtures" / "receipt-0921.txt").read_bytes())
    prompt = task["prompt"].format(base=base)
    env = dict(os.environ, XDG_DATA_HOME=str(data_dir), PATH=f"{bin_dir}:{os.environ['PATH']}")
    command = [args.glass_core, "--yolo", "run", "--json"]
    if args.model:
        command += ["--model", args.model]
    started = time.monotonic()
    try:
        output = subprocess.run(command + [prompt], cwd=workspace, env=env, capture_output=True,
                                text=True, timeout=args.timeout)
        stdout, stderr, code = output.stdout, output.stderr, output.returncode
    except subprocess.TimeoutExpired as error:
        stdout, stderr, code = error.stdout or "", error.stderr or "", "timeout"
        if isinstance(stdout, bytes):
            stdout = stdout.decode(errors="replace")
        if isinstance(stderr, bytes):
            stderr = stderr.decode(errors="replace")
    elapsed = time.monotonic() - started
    try:
        result = json.loads(stdout.strip().splitlines()[-1]) if stdout.strip() else {}
    except json.JSONDecodeError:
        result = {}
    session = result.get("session", "")
    calls = read_calls(data_dir, session) if session else []
    answer = result.get("text") or ""
    family = families[task["family"]]
    first_ok = bool(calls) and any_match(calls[0], family + task.get("allow_first", []))
    used_family = any(any_match(c, family) for c in calls)
    ideal = task.get("ideal")
    ideal_hit = None if not ideal else any(any_match(c, ideal) for c in calls)
    outputs = [p for p in workspace.glob(pattern)
               if p.is_file() and p not in existing_files] if pattern else []
    write_eval_outputs(workspace, outputs)
    infrastructure_error = None
    try:
        success = check(task, answer, calls, base, workspace, existing_files)
    except (OSError, subprocess.SubprocessError) as error:
        success = False
        infrastructure_error = f"Verification failed: {error}"
    return {
        "task": task["id"], "family": task["family"], "capability": task["capability"],
        "session": session, "exit": code, "status": result.get("status"), "error": result.get("error"),
        "cost_usd": result.get("cost_usd"), "seconds": round(elapsed, 1),
        "right_first": first_ok, "used_family": used_family, "ideal": ideal_hit,
        "success": success, "steps": len(calls),
        "infrastructure_error": infrastructure_error,
        "tools": [c["name"] if c["name"] != "bash" else "bash:" + c["args"][:120] for c in calls],
        "answer": answer[-600:], "stderr_tail": stderr[-400:],
    }


def summarize(rows):
    def rate(values):
        values = [v for v in values if v is not None]
        return round(100 * sum(values) / len(values), 1) if values else None

    out = {}
    for name in ["all", "browser", "desktop"]:
        group = [r for r in rows if name == "all" or r["family"] == name]
        if not group:
            continue
        ok_steps = [r["steps"] for r in group if r["success"]]
        out[name] = {
            "runs": len(group),
            "right_first_pct": rate([r["right_first"] for r in group]),
            "ideal_tool_pct": rate([r["ideal"] for r in group]),
            "success_pct": rate([r["success"] for r in group]),
            "mean_steps": round(sum(r["steps"] for r in group) / len(group), 1),
            "mean_steps_successful": round(sum(ok_steps) / len(ok_steps), 1) if ok_steps else None,
        }
    return out


def write_browser_wrapper(wrapper, browser, runtime):
    wrapper.write_text(f'''#!{sys.executable}
import os
import sys
arguments = []
index = 1
while index < len(sys.argv):
    argument = sys.argv[index]
    if argument == "--":
        arguments.extend(sys.argv[index:])
        break
    if argument == "--browser-runtime":
        index += 2
        continue
    if not argument.startswith("--browser-runtime="):
        arguments.append(argument)
    index += 1
os.execv({str(browser)!r}, [{str(browser)!r}, "--browser-runtime", {runtime!r}, *arguments])
''')
    wrapper.chmod(0o755)


def eval_data_dir(override, tasks, root):
    if override:
        return Path(override).resolve()
    if any(task["family"] == "desktop" for task in tasks):
        return Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share").resolve()
    return root / "data"


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--glass-core", required=True)
    parser.add_argument("--glass-browser", required=True)
    parser.add_argument("--label", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--only", choices=["browser", "desktop"])
    parser.add_argument("--task", action="append")
    parser.add_argument("--repeat", type=int, default=1)
    parser.add_argument("--model")
    parser.add_argument("--browser-runtime", help="force a glass-browser runtime (e.g. chromium) for every CLI call")
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--timeout", type=int, default=600)
    parser.add_argument("--workspace", help="persistent workspace; desktop tasks need the one the desktop manager serves")
    parser.add_argument("--data-dir", help="XDG_DATA_HOME for glass-core; desktop runs default to inherited XDG_DATA_HOME or ~/.local/share")
    args = parser.parse_args()

    spec = json.loads((HERE / "tasks.json").read_text())
    tasks = [t for t in spec["tasks"] if (not args.only or t["family"] == args.only)
             and (not args.task or t["id"] in args.task)]
    for key, executable in [("desktop_clipboard", "xclip"), ("desktop_window", "xdotool")]:
        if any(key in task["check"] for task in tasks) and not shutil.which(executable):
            parser.error(f"Selected tasks require the {executable} verifier; install it before running")
    root = Path(tempfile.mkdtemp(prefix="glass-cu-eval-"))
    workspace = Path(args.workspace or root / "workspace").resolve()
    data_dir = eval_data_dir(args.data_dir, tasks, root)
    workspace.mkdir(parents=True, exist_ok=True)
    data_dir.mkdir(parents=True, exist_ok=True)
    bin_dir = root / "bin"
    bin_dir.mkdir()
    browser = Path(args.glass_browser).resolve()
    if args.browser_runtime:
        wrapper = bin_dir / "glass-browser"
        write_browser_wrapper(wrapper, browser, args.browser_runtime)
    else:
        (bin_dir / "glass-browser").symlink_to(browser)
    args.glass_core = str(Path(args.glass_core).resolve())

    server = fixture_server.serve(args.port)
    base = f"http://127.0.0.1:{args.port}"
    rows = []
    try:
        for round_index in range(args.repeat):
            for task in tasks:
                row = run_task(task, spec["families"], args, base, data_dir, workspace, bin_dir)
                row["round"] = round_index
                rows.append(row)
                print(f"[{args.label}] {task['id']}: success={row['success']} right_first={row['right_first']} "
                      f"ideal={row['ideal']} steps={row['steps']} tools={row['tools'][:6]}", flush=True)
    finally:
        server.shutdown()
    report = {"label": args.label, "summary": summarize(rows), "runs": rows}
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out).write_text(json.dumps(report, indent=2))
    print(json.dumps(report["summary"], indent=2))


if __name__ == "__main__":
    main()
