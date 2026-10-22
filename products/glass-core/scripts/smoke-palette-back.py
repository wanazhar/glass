#!/usr/bin/env python3
"""Verify nested slash menu Esc navigation in release PTYs."""
import sys, os, tempfile, pathlib
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
root = pathlib.Path(__file__).resolve().parents[1]
source=(root/'scripts/smoke-cli.py').read_text().split('with tempfile.TemporaryDirectory')[0]
exec(source.replace("ROOT = Path(__file__).resolve().parents[1]", f"ROOT = Path({str(root)!r})"))
with tempfile.TemporaryDirectory() as d:
 p=pathlib.Path(d); c=p/'config/glass-core';c.mkdir(parents=True)
 (c/'config.toml').write_text('provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:1/v1"\ntrust="always"\n')
 env=dict(os.environ,XDG_CONFIG_HOME=str(p/'config'),XDG_DATA_HOME=str(p/'data'),TERM='xterm-256color')
 for size in [(20,8),(40,24),(80,30)]:
  t=TTY(d,env,[],size)
  try:
   t.read(1);t.send('/');t.wait('Esc close');t.send('settings');t.read(.2);t.send('\r');t.wait('Settings');t.send('theme');t.read(.2);t.send('\r');t.read(.4)
   assert 'Settings' not in t.vt.text(),t.vt.text()
   t.send('\x1b');t.wait('Settings');t.send('\x1b');t.wait('Esc close');assert '/settings' in t.vt.text(),t.vt.text()
   t.send('\x1b');t.read(.3);assert 'Esc close' not in t.vt.text(),t.vt.text()
   t.send('saved draft\x10');t.wait('Esc close');t.send('settings');t.read(.2);t.send('\r');t.wait('Settings');t.send('\x1b');t.wait('Esc close');t.send('\x1b');t.read(.3);assert 'saved draft' in t.vt.text(),t.vt.text()
   t.send('\x03');t.read(.2);t.send('\x04');t.wait('Exit glass-core?');t.send('\r');t.finish();print(size,'nested theme → settings → palette → composer PASS')
  finally:t.close()
