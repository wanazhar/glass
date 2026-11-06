#!/usr/bin/env python3
"""Audit the executable serving each Core conversation; never open or stop a turn."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import stat
import struct
import sys


def revision(executable):
    try:
        manifest = executable.parent.parent.parent / 'manifest.json'
        if manifest.stat().st_size > 65536:
            return None
        value = json.loads(manifest.read_text())['components']['core']
        if isinstance(value, str) and 7 <= len(value) <= 64 and all(c in '0123456789abcdefABCDEF' for c in value):
            return value
    except (OSError, KeyError, ValueError, TypeError):
        pass
    return None


def audit(binary, data):
    expected = binary.resolve(strict=True)
    identity = expected.stat()
    workers = []
    fingerprints = {}
    def fingerprint(path):
        identity = path.stat()
        key = (identity.st_dev, identity.st_ino)
        if key not in fingerprints:
            with path.open("rb") as source:
                fingerprints[key] = hashlib.file_digest(source, "sha256").digest()
        return fingerprints[key]
    for path in sorted(data.glob('serve-*.sock'))[:256]:
        try:
            if not stat.S_ISSOCK(path.lstat().st_mode):
                continue
        except OSError:
            continue
        entry = {'status': 'unavailable'}
        try:
            with socket.socket(socket.AF_UNIX) as peer:
                peer.settimeout(1)
                peer.connect(str(path))
                pid, uid, _ = struct.unpack('3i', peer.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, struct.calcsize('3i')))
                if uid != os.getuid():
                    entry['status'] = 'different_user'
                else:
                    running = Path(f'/proc/{pid}/exe')
                    actual = running.stat()
                    entry = {'pid': pid, 'status': 'current' if (actual.st_dev, actual.st_ino) == (identity.st_dev, identity.st_ino) or fingerprint(running) == fingerprint(expected) else 'outdated', 'revision': revision(running.resolve())}
        except (OSError, ValueError):
            pass
        workers.append(entry)
    return {'expectedRevision': revision(expected), 'workers': workers, 'readOnly': True, 'promptsSent': 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path(shutil.which('gc') or shutil.which('glass-core') or 'glass-core'))
    parser.add_argument('--data-dir', type=Path, default=Path(os.environ.get('XDG_DATA_HOME', str(Path.home() / '.local/share'))) / 'glass-core')
    parser.add_argument('--require-pid', type=int, help='Check one known conversation worker instead of all workers')
    args = parser.parse_args()
    if not sys.platform.startswith('linux'):
        parser.error('Executable identity checks currently require Linux')
    report = audit(args.binary, args.data_dir)
    selected = report['workers'] if args.require_pid is None else [worker for worker in report['workers'] if worker.get('pid') == args.require_pid]
    report['checkedWorkers'] = len(selected)
    report['passed'] = (args.require_pid is None or bool(selected)) and all(worker['status'] == 'current' for worker in selected)
    print(json.dumps(report, indent=2))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
