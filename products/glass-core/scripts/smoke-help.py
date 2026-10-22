#!/usr/bin/env python3
"""Exercise release CLI help through pipes and real terminals."""
import errno
import fcntl
import os
from pathlib import Path
import pty
import select
import struct
import subprocess
import tempfile
import termios
import time
from core_binary import core_binary_path

binary = str(Path(os.environ.get('GLASS_CORE_TEST_BINARY', str(core_binary_path('release')))).resolve())
with tempfile.TemporaryDirectory(prefix='gc-help-') as directory:
    env = dict(os.environ, XDG_CONFIG_HOME=directory+'/config', XDG_DATA_HOME=directory+'/data', TERM='xterm-256color')
    def pipe(*args):
        return subprocess.run([binary, *args], cwd=directory, env=env, capture_output=True, text=True, timeout=10)
    base = pipe('help')
    assert base.returncode == 0 and '\x1b' not in base.stdout
    assert 'Start here' in base.stdout and 'gc help commands' in base.stdout
    for alias in ('--help', '-h'):
        assert pipe(alias).stdout == base.stdout
    for topic in ('commands', 'run', 'keys', 'all', 'agent', 'file'):
        result = pipe('help', topic)
        assert result.returncode == 0 and result.stdout, (topic, result.stderr)
    assert pipe('help', 'unknown').returncode == 2
    assert pipe('help', 'run', 'extra').returncode == 2
    alias = Path(binary).with_name("gc")
    if alias.is_symlink():
        assert subprocess.check_output([str(alias), "help"], cwd=directory, env=env, text=True) == base.stdout
    from pty_vt import VT
    for width, height in ((80,30), (40,24), (20,8)):
        for plain in (False, True):
            master, slave = pty.openpty()
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
            child_env = dict(env)
            child_env.pop('NO_COLOR', None)
            if plain:
                child_env['NO_COLOR'] = '1'
            process = subprocess.Popen([binary, 'help'], cwd=directory, env=child_env, stdin=slave, stdout=slave, stderr=slave)
            os.close(slave)
            output = b''
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if select.select([master], [], [], .1)[0]:
                    try:
                        chunk = os.read(master, 65536)
                        if not chunk:
                            break
                        output += chunk
                    except OSError as error:
                        if error.errno == errno.EIO:
                            break
                        raise
            process.wait(timeout=2)
            os.close(master)
            assert process.returncode == 0
            text = output.decode()
            assert ('\x1b[' in text) != plain, (width, plain)
            assert '?1049' not in text and '?25' not in text
            vt = VT(width, height)
            vt.feed(text)
            assert 'Inside GC:' in text
            # Strip SGR only: help must not move the cursor or enter screen modes.
            import re
            rows = re.sub(r'\x1b\[[0-9;]*m', '', text).splitlines()
            assert all(len(row) <= width for row in rows), (width, rows)
            print(f'PASS help PTY {width}x{height} color={not plain}')
    assert not list(Path(directory).iterdir()), 'Help must not initialize config or sessions'
print('PASS help aliases, focused pages, errors, pipes and non-Git startup')
