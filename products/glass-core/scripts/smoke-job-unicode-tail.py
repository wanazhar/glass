#!/usr/bin/env python3
"""A completion inspector can open when its tail cutoff intersects UTF-8."""
import os
from pathlib import Path
import tempfile
from pty_vt import Terminal

for size in ((80, 30), (40, 24), (20, 8)):
    with tempfile.TemporaryDirectory(prefix='gc-job-unicode-tail-') as tmp:
        root = Path(tmp)
        config = root / 'config/glass-core'
        config.mkdir(parents=True)
        (config/'config.toml').write_text('provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:1/v1"\ntrust="always"\n')
        # 30,000 captured bytes: the old cutoff 5,424 was inside the euro sign.
        payload = b'x'*5423 + '€'.encode()
        marker = b'\nUNICODE_TAIL_OK\n'
        payload += b'z'*(30000-len(payload)-len(marker)) + marker
        (root/'payload.txt').write_bytes(payload)
        env = dict(os.environ, XDG_CONFIG_HOME=str(root/'config'), XDG_DATA_HOME=str(root/'data'))
        terminal = Terminal(root, env, size)
        try:
            terminal.idle()
            terminal.send('/bg cat payload.txt\r')
            terminal.wait('job 1 pid')
            terminal.collect(.3)
            terminal.send('/attention\r')
            terminal.visible('Attention')
            terminal.send('\r')
            terminal.visible('live output' if size[0] >= 24 else 'o/s')
            terminal.send('o')
            terminal.visible('Job 1')
            terminal.visible('UNICODE_TAIL_OK')
            assert 'UTF-8 character' not in terminal.vt.text()
            terminal.send('\x1b')
            terminal.collect(.2)
            terminal.visible('Attention')
            terminal.send('\x1b')
            terminal.collect(.2)
            terminal.send('\x1b')
            terminal.collect(.2)
            terminal.idle()
            print('PASS UTF-8 job tail inspector and return:', size, flush=True)
        finally:
            terminal.close()
