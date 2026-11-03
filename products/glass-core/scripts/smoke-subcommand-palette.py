#!/usr/bin/env python3
"""Exercise selectable slash actions, nested Escape and draft restoration in PTYs."""
import os, pathlib, sys, tempfile
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
root = pathlib.Path(__file__).resolve().parents[1]
source = (root / 'scripts/smoke-cli.py').read_text().split('with tempfile.TemporaryDirectory')[0]
exec(source.replace('ROOT = Path(__file__).resolve().parents[1]', f'ROOT = Path({str(root)!r})'))
with tempfile.TemporaryDirectory(prefix='gc-pal-') as d:
    p = pathlib.Path(d)
    cfg = p / 'config/glass-core'; cfg.mkdir(parents=True)
    (cfg / 'config.toml').write_text('provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:1/v1"\ntrust="always"\n')
    env = dict(os.environ, XDG_CONFIG_HOME=str(p/'config'), XDG_DATA_HOME=str(p/'data'), TERM='xterm-256color')
    for size in [(20,8), (40,24), (80,30)]:
        t = TTY(d, env, [], size)
        try:
            t.read(1)
            def open_menu(name):
                t.send('/'+name); t.read(.2); t.send('\r'); t.wait('Esc back')
            def close_menu(name):
                t.send('\x1b'); t.wait('Esc close')
                assert '/'+name in t.vt.text(), t.vt.text()
                t.send('\x1b'); t.read(.2)
            for name in ['agents', 'desktop', 'constraints', 'loop', 'prewalk']:
                open_menu(name)
                assert '/'+name in t.vt.text(), t.vt.text()
                t.send('\x1b[B'); t.send('\x1b[A'); t.read(.1)
                close_menu(name)
            # Selected viewer returns to the child list, then root, then original draft.
            t.send('keep my draft\x10'); t.wait('Esc close'); t.send('agents'); t.read(.2); t.send('\r'); t.wait('Esc back')
            t.send('\r'); t.read(.4); t.send('\x1b'); t.wait('Esc back')
            t.send('\x1b'); t.wait('Esc close'); t.send('\x1b'); t.read(.2)
            assert 'keep my draft' in t.vt.text(), t.vt.text()
            t.send('\x15'); t.read(.1)
            # A task-field cancellation returns to the interval selector.
            open_menu('loop'); t.send('\x1b[B\r'); t.wait('Schedule interval')
            t.send('\r'); t.wait('Scheduled task'); t.send('\x1b'); t.wait('Schedule interval')
            t.send('\x1b'); t.wait('Esc back'); close_menu('loop')
            # Adding a reminder is guided; cancellation leaves the menu usable.
            open_menu('constraints'); t.send('\x1b[B\r'); t.wait('Add reminder')
            t.send('\x1b'); t.wait('Esc back'); close_menu('constraints')
            # Status does not re-enter its own submenu; explicit Cancel stays in the menu.
            open_menu('prewalk'); t.send('status\r'); t.wait('Model handoff')
            t.send('\x1b'); t.wait('Esc back'); close_menu('prewalk')
            open_menu('constraints'); t.send('clear\r'); t.wait('Clear saved items')
            t.send('\r'); t.wait('Esc back'); close_menu('constraints')
            t.send('\x04'); t.wait('QUIT?'); t.send('\r'); t.finish()
            print(size, 'subcommands, viewer back, wizard back and draft restoration PASS')
        finally:
            t.close()
