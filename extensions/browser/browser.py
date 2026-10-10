#!/usr/bin/env python3
"""Opt-in browser checks. Connects to an explicitly exposed local CDP browser.
Requires Python playwright; never installs dependencies or launches a browser.
"""
import ipaddress
import json
import os
from pathlib import Path
import secrets
import sys
from urllib.parse import urlparse


def endpoint(value):
    parsed = urlparse(value)
    try:
        local = ipaddress.ip_address(parsed.hostname or '').is_loopback
    except ValueError:
        local = parsed.hostname == 'localhost'
    if parsed.scheme != 'http' or not local or parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise ValueError('CDP endpoint must be a local http://127.0.0.1:PORT URL')
    return value


def verify(workspace, args):
    if not isinstance(args, dict):
        raise ValueError('arguments must be an object')
    url = args.get('url', '')
    if not isinstance(url, str) or len(url) > 4096 or urlparse(url).scheme not in ('http', 'https'):
        raise ValueError('url must be an HTTP or HTTPS URL')
    expected = args.get('expected_text', '')
    if not isinstance(expected, str) or len(expected) > 4096:
        raise ValueError('expected_text must be at most 4096 characters')
    cdp = endpoint(os.environ.get('GC_BROWSER_CDP', 'http://127.0.0.1:9222'))
    try:
        from playwright.sync_api import sync_playwright
    except ImportError:
        raise ValueError('Optional dependency missing: install Python playwright and expose a local browser CDP port; GC does not launch or install browsers')
    root = Path(workspace).resolve()
    base = root / '.gc-artifacts'
    base.mkdir(mode=0o700, exist_ok=True)
    if base.is_symlink() or base.resolve().parent != root:
        raise ValueError('artifact directory must be a real workspace directory')
    os.chmod(base, 0o700)
    directory = base / ('browser-' + secrets.token_hex(8))
    directory.mkdir(mode=0o700)
    events, errors = [], []
    context = None
    with sync_playwright() as driver:
        browser = driver.chromium.connect_over_cdp(cdp, timeout=10000)
        try:
            context = browser.new_context(viewport={'width': 1280, 'height': 800})
            page = context.new_page()
            page.on('console', lambda event: events.append({'type': event.type, 'text': event.text[:1000]}) if len(events) < 50 else None)
            page.on('pageerror', lambda error: errors.append(str(error)[:1000]) if len(errors) < 50 else None)
            response = page.goto(url, wait_until='domcontentloaded', timeout=20000)
            # Locator matching avoids downloading/serializing a complete DOM.
            found = page.get_by_text(expected, exact=False).first.count() > 0 if expected else None
            screenshot = directory / 'viewport.png'
            image = page.screenshot(full_page=False, timeout=10000)
            if len(image) > 8 * 1024 * 1024:
                raise ValueError('screenshot exceeded the 8 MiB artifact cap')
            with os.fdopen(os.open(screenshot, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'wb') as output:
                output.write(image)
            if screenshot.stat().st_size > 8 * 1024 * 1024:
                screenshot.unlink()
                raise ValueError('screenshot exceeded the 8 MiB artifact cap')
            result = {'url': page.url, 'status': response.status if response else None,
                      'expected_text_found': found, 'console': events, 'page_errors': errors,
                      'screenshot': str(screenshot), 'report': str(directory / 'report.json'),
                      'passed': (response is not None and response.status < 400 and not errors and found is not False)}
            with os.fdopen(os.open(directory / 'report.json', os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'w', encoding='utf-8') as output:
                json.dump(result, output, indent=2)
            return json.dumps(result, indent=2)
        finally:
            if context is not None:
                context.close()
            # Do not browser.close(): the existing user's browser stays alive.


MANIFEST = {'name': 'browser', 'version': '1.0.0',
 'system_prompt': 'Optional browser verification is available. browser_verify uses an isolated context in a local CDP browser and returns viewport screenshot, console errors and a report. Use gc workspace artifact add LABEL PATH to register returned artifacts. Never claim a successful check when passed is false.',
 'tools': [{'name': 'browser_verify', 'description': 'Verify a web page in an explicitly exposed local browser; capture screenshot, HTTP status, expected text and console/page errors.',
 'schema': {'type': 'object', 'properties': {'url': {'type': 'string'}, 'expected_text': {'type': 'string'}}, 'required': ['url']}, 'concurrency': 'serial'}]}


def main():
    workspace = os.getcwd()
    for line in sys.stdin:
        try:
            req = json.loads(line)
            kind = req.get('type')
            reply = {'id': req.get('id')}
            if kind == 'hello':
                workspace = req.get('workspace') or workspace
                reply.update(protocol=1, **MANIFEST)
            elif kind == 'system_prompt':
                reply['text'] = MANIFEST['system_prompt']
            elif kind == 'authorize':
                reply['allow'] = True
            elif kind == 'run_tool':
                try:
                    reply['output'] = verify(workspace, json.loads(req.get('arguments') or '{}'))
                except Exception as error:
                    reply['output'] = 'browser verification failed: ' + str(error)[:2000]
            elif kind in ('goodbye', 'shutdown'):
                break
            else:
                continue
            print(json.dumps(reply), flush=True)
        except ValueError:
            print(json.dumps({'error': 'invalid protocol JSON'}), flush=True)


if __name__ == '__main__':
    main()
