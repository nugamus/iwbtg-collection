#!/usr/bin/env python3
"""Headless-browser smoke test for the browser build (#1078).

Packs a fixture package with the native `iwbtg pkg pack`, serves `crates/client/web` on a local
port, and loads `?package=<file>` in headless Chrome. The page fetches the `.iwbz`, unpacks it
into `PackageFiles` and logs `package <id> loaded: gameplay hash <hex>`
(docs/architecture/client/web.md); this script asserts that hash equals the native one and that
the page did not panic. Chrome needs WebGL2; the SwiftShader flags make that work without a GPU.

Run after `cargo build --release --target wasm32-unknown-unknown` and `wasm-bindgen` have written
`crates/client/web/pkg/` (docs/dev-setup.md, The client in a browser). `--chrome` or `$CHROME`
picks a browser; otherwise common names and install paths are tried.
"""
import argparse
import functools
import http.server
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import urllib.parse

ROOT = pathlib.Path(__file__).resolve().parents[1]
HASH_LINE = re.compile(r'package \S+ loaded: gameplay hash ([0-9a-f]{64})')
PACK_HASH = re.compile(r'^gameplay_hash: ([0-9a-f]{64})$', re.MULTILINE)
CHROME_NAMES = ('google-chrome', 'google-chrome-stable', 'chromium', 'chromium-browser',
                'chrome', 'msedge')


def chrome_path(explicit=None):
    """The Chrome/Chromium binary to drive: `--chrome`, `$CHROME`, PATH, or a usual install."""
    if explicit:
        return explicit
    env = os.environ.get('CHROME')
    if env:
        return env
    for name in CHROME_NAMES:
        found = shutil.which(name)
        if found:
            return found
    # Windows installs are not on PATH.
    for base in (os.environ.get('PROGRAMFILES'), os.environ.get('PROGRAMFILES(X86)'),
                 os.environ.get('LOCALAPPDATA')):
        if not base:
            continue
        for rel in ('Google/Chrome/Application/chrome.exe',
                    'Microsoft/Edge/Application/msedge.exe'):
            path = pathlib.Path(base) / rel
            if path.is_file():
                return str(path)
    raise SystemExit('no Chrome/Chromium found; pass --chrome or set CHROME')


def pack(package_dir, out):
    """Packs `package_dir` and prints/returns the gameplay hash native `pkg pack` reports."""
    proc = subprocess.run(
        ['cargo', 'run', '-q', '-p', 'iwbtg-tools', '--locked', '--', 'pkg', 'pack',
         str(package_dir), '-o', str(out)],
        cwd=ROOT, capture_output=True, text=True, timeout=600)
    if proc.returncode != 0:
        raise SystemExit(f'pkg pack failed:\n{proc.stdout}{proc.stderr}')
    match = PACK_HASH.search(proc.stdout)
    if not match:
        raise SystemExit(f'pkg pack printed no gameplay hash:\n{proc.stdout}{proc.stderr}')
    return match.group(1)


class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, format, *args):
        pass


def serve(web_dir):
    """Serves `web_dir` on a free local port; returns the server and its port."""
    handler = functools.partial(Quiet, directory=str(web_dir))
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, server.server_address[1]


def headless(chrome, url, timeout):
    """Loads `url` in headless Chrome and returns stdout and stderr."""
    profile = tempfile.mkdtemp(prefix='iwbtg-web-smoke-')
    try:
        proc = subprocess.run(
            [chrome, '--headless=new', '--no-sandbox', '--disable-dev-shm-usage',
             '--no-first-run', f'--user-data-dir={profile}',
             '--enable-unsafe-swiftshader', '--use-angle=swiftshader',
             '--enable-logging=stderr', '--v=0', '--virtual-time-budget=30000',
             '--dump-dom', url],
            capture_output=True, text=True, timeout=timeout)
        return proc.stdout, proc.stderr
    except subprocess.TimeoutExpired:
        raise SystemExit(f'headless Chrome did not finish within {timeout}s')
    finally:
        shutil.rmtree(profile, ignore_errors=True)


def tail(text, lines=30):
    return '\n'.join(text.splitlines()[-lines:])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--web-dir', type=pathlib.Path,
                        default=ROOT / 'crates/client/web',
                        help='folder with index.html and pkg/ (default crates/client/web)')
    parser.add_argument('--package-dir', type=pathlib.Path,
                        default=ROOT / 'crates/client/tests/fixtures/clear.pkg',
                        help='package folder to pack and play (default the clear fixture)')
    parser.add_argument('--chrome', help='Chrome/Chromium binary (default: $CHROME or PATH)')
    parser.add_argument('--timeout', type=int, default=180,
                        help='headless Chrome timeout in seconds (default 180)')
    parser.add_argument('--keep', action='store_true',
                        help='keep the packed .iwbz in the web folder')
    args = parser.parse_args()

    web_dir = args.web_dir.resolve()
    if not (web_dir / 'pkg' / 'iwbtg-client.js').is_file():
        raise SystemExit(f'{web_dir}/pkg is missing; build the bundle first '
                         '(docs/dev-setup.md, The client in a browser)')
    chrome = chrome_path(args.chrome)

    archive = web_dir / (args.package_dir.resolve().stem + '.iwbz')
    print(f'packing {args.package_dir} -> {archive}')
    expected = pack(args.package_dir.resolve(), archive)
    print(f'native gameplay hash: {expected}')

    server, port = serve(web_dir)
    url = f'http://127.0.0.1:{port}/?package={urllib.parse.quote(archive.name)}'
    try:
        print(f'loading {url} in {chrome}')
        out, err = headless(chrome, url, args.timeout)
    finally:
        server.shutdown()
        server.server_close()
        if not args.keep:
            archive.unlink(missing_ok=True)

    log = out + '\n' + err
    if 'panicked at' in log:
        raise SystemExit(f'the page panicked:\n{tail(log)}')
    match = HASH_LINE.search(log)
    if not match:
        raise SystemExit(f'no "package ... loaded: gameplay hash" line in the browser console; '
                         f'last lines:\n{tail(log)}')
    found = match.group(1)
    if found != expected:
        raise SystemExit(f'gameplay hash mismatch: browser {found}, native {expected}')
    print(f'web smoke ok: package loaded, gameplay hash {found}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
