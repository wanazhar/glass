#!/usr/bin/env python3
"""Reproducible, offline CLI/PTY performance smoke with generous budgets.

Run after building a release binary. GLASS_CORE_TEST_BINARY selects it.
Limits can be tuned by CI hardware with GC_PERF_*_MS environment variables.
The script writes a JSON artifact to GC_PERF_REPORT (default perf-report.json).
"""
import json
import os
import fcntl
import pty
import re
import select
import signal
import statistics
import struct
import subprocess
import tempfile
import termios
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BIN = Path(os.environ.get('GLASS_CORE_TEST_BINARY', ROOT / 'target/release/glass-core'))


def limit(name, default):
    return float(os.environ.get(name, default))


def elapsed_ms(call):
    start = time.perf_counter()
    call()
    return (time.perf_counter() - start) * 1000


def pty_key_latency(env):
    master, slave = pty.openpty()
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 40, 0, 0))
    proc = subprocess.Popen([str(BIN)], cwd=env['GC_PERF_WORKSPACE'], env=env, stdin=slave, stdout=slave, stderr=slave, close_fds=True)
    os.close(slave)
    try:
        ready = bytearray()
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline and proc.poll() is None:
            readable, _, _ = select.select([master], [], [], 0.1)
            if readable:
                try: ready.extend(os.read(master, 65536))
                except OSError: break
                if b'glass-core' in ready and (b'>' in ready or b'Ctrl-D' in ready): break
        if proc.poll() is not None:
            raise RuntimeError('interactive process exited before prompt')
        os.write(master, b'z')
        start = time.perf_counter()
        received = bytearray()
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            readable, _, _ = select.select([master], [], [], 0.05)
            if readable:
                try: received.extend(os.read(master, 65536))
                except OSError: break
                visible = re.sub(rb'\x1b\[[0-?]*[ -/]*[@-~]', b'', received)
                visible = re.sub(rb'\x1b\][^\x07]*(?:\x07|\x1b\\)', b'', visible)
                if b'z' in visible:
                    # Stop at the first printable emission, before sending a
                    # second key so this is input-to-render latency.
                    key_ms = (time.perf_counter() - start) * 1000
                    # Let the key frame drain, then force a same-size terminal
                    # resize. Time the first bytes of the resulting redraw.
                    time.sleep(0.1)
                    while select.select([master], [], [], 0)[0]:
                        try: os.read(master, 65536)
                        except OSError: break
                    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 40, 0, 0))
                    start = time.perf_counter()
                    os.kill(proc.pid, signal.SIGWINCH)
                    if not select.select([master], [], [], 3)[0]:
                        raise RuntimeError('PTY did not redraw after resize')
                    try: os.read(master, 65536)
                    except OSError as exc: raise RuntimeError('PTY closed during redraw') from exc
                    return key_ms, (time.perf_counter() - start) * 1000
        raise RuntimeError('PTY did not repaint after a key')
    finally:
        if proc.poll() is None:
            proc.terminate()
            try: proc.wait(timeout=2)
            except subprocess.TimeoutExpired: proc.kill(); proc.wait()
        os.close(master)


def main():
    if not BIN.is_file():
        raise SystemExit(f'missing release binary: {BIN}; build glass-core first')
    with tempfile.TemporaryDirectory(prefix='gc-perf-') as td:
        root = Path(td)
        workspace = root / 'workspace'
        workspace.mkdir()
        config = root / 'config' / 'glass-core'
        config.mkdir(parents=True)
        (config / 'config.toml').write_text('api_key_env = ""\n')
        sessions = root / 'data' / 'glass-core'
        sessions.mkdir(parents=True)
        # Deterministic large history fixture; search exercises the real log
        # scanner without network access, credentials, or provider startup.
        history = sessions / 'perf-fixture.jsonl'
        with history.open('w') as f:
            for i in range(12000):
                f.write(json.dumps({'k': 'user' if i % 2 == 0 else 'assistant', 't': f'fixture record {i} stable-search-target'}) + '\n')
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'), TERM='xterm-256color', GC_PERF_WORKSPACE=str(workspace))
        starts = [elapsed_ms(lambda: subprocess.run([BIN, '--help'], cwd=ROOT, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)) for _ in range(7)]
        latencies = [pty_key_latency(env) for _ in range(3)]
        history_ms = elapsed_ms(lambda: subprocess.run([BIN, 'search', 'stable-search-target'], cwd=workspace, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True))
        # GNU time reports the peak RSS for this one child, in KiB.
        rss_file = root / 'rss-kib.txt'
        subprocess.run(['/usr/bin/time', '-f', '%M', '-o', str(rss_file), str(BIN), 'search', 'stable-search-target'], cwd=workspace, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
        rss = int(rss_file.read_text().strip())
        report = {'binary': str(BIN), 'fixture_records': 12000, 'startup_help_median_ms': statistics.median(starts), 'key_repaint_median_ms': statistics.median(x[0] for x in latencies), 'frame_redraw_median_ms': statistics.median(x[1] for x in latencies), 'long_history_search_ms': history_ms, 'long_history_child_peak_rss_kib': rss}
        Path(os.environ.get('GC_PERF_REPORT', 'perf-report.json')).write_text(json.dumps(report, indent=2) + '\n')
        budgets = {'startup_help_median_ms': limit('GC_PERF_STARTUP_BUDGET_MS', 250), 'key_repaint_median_ms': limit('GC_PERF_KEY_BUDGET_MS', 200), 'frame_redraw_median_ms': limit('GC_PERF_FRAME_BUDGET_MS', 200), 'long_history_search_ms': limit('GC_PERF_HISTORY_BUDGET_MS', 10000), 'long_history_child_peak_rss_kib': limit('GC_PERF_RSS_BUDGET_KIB', 128 * 1024)}
        failures = [f'{key}={report[key]:.1f} exceeds {budget:.1f}' for key, budget in budgets.items() if report[key] > budget]
        print(json.dumps({'measurements': report, 'budgets': budgets, 'failures': failures}, indent=2))
        if failures: raise SystemExit('\n'.join(failures))


if __name__ == '__main__':
    main()
