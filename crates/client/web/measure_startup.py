#!/usr/bin/env python3
"""Measure the browser build's startup by phase (#1489).

Serves `crates/client/web`, loads a package in headless Chrome with `?measure`, and prints the
`startup ...` lines the client logs (`startup_profile.rs`). The phases are fetch, unpack,
parse/masks, sprite decode, script prepare, first room and the sprite image build/queue; the last
`startup total sprites ready` line is the time from the page's start to the last sprite image
queued. The GPU transfer is not a separate phase: the client builds Bevy images and the renderer
uploads them later, so no browser-side boundary isolates it.

Build the bundle first (docs/dev-setup.md, The client in a browser and web.md, Measuring startup):

    cargo build -p iwbtg-client --release --target wasm32-unknown-unknown
    wasm-bindgen --target web --no-typescript --out-dir crates/client/web/pkg <the .wasm>

Examples:

    # the packed showcase, as `docs/dev-setup.md` builds it
    python -I crates/client/web/measure_startup.py --package showcase.iwbz

    # 400 generated 256x256 sprites, the content `shell::tests::startup_phases` uses
    python -I crates/client/web/measure_startup.py --synthetic 400 --size 256

Chrome needs WebGL2; the SwiftShader flags make that work without a GPU. `wasm-opt` is not needed
for a measurement.
"""
import argparse
import functools
import http.server
import os
import pathlib
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse
import zlib

ROOT = pathlib.Path(__file__).resolve().parents[3]
CHROME_NAMES = ('google-chrome', 'google-chrome-stable', 'chromium', 'chromium-browser',
                'chrome', 'msedge')
PHASE = 'startup phase '
TOTAL = 'startup total '
COMPLETE = 'startup total sprites ready'
# Chrome prints a console line as: [pid:tid:ERROR:CONSOLE(n)] "startup phase fetch: 1.2 ms", source: ...
MEASURED = re.compile(r'"(startup (?:phase|total) [^"]*)"')


def chrome_path(explicit=None):
    """The Chrome/Chromium binary: `--chrome`, `$CHROME`, PATH, or a usual Windows install."""
    if explicit:
        return explicit
    env = os.environ.get('CHROME')
    if env:
        return env
    for name in CHROME_NAMES:
        found = shutil.which(name)
        if found:
            return found
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


def write_png(path, size):
    """A solid `size`x`size` RGBA PNG, the content `shell::tests::startup_phases` writes."""
    row = b'\x00' + bytes((0x80, 0x80, 0x80, 0x80)) * size
    raw = row * size

    def chunk(kind, data):
        return (struct.pack('>I', len(data)) + kind + data
                + struct.pack('>I', zlib.crc32(kind + data) & 0xffffffff))

    header = struct.pack('>IIBBBBB', size, size, 8, 6, 0, 0, 0)
    path.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', header)
                     + chunk(b'IDAT', zlib.compress(raw, 9)) + chunk(b'IEND', b''))


def copy_tree(source, target):
    shutil.copytree(source, target, dirs_exist_ok=True)


def synthetic_package(web_dir, count, size):
    """Copies the audio fixture, adds `count` solid sprites and packs it into `web_dir`.

    Returns the archive's path; it is regenerated on each run so the content is reproducible."""
    work = pathlib.Path(tempfile.mkdtemp(prefix='iwbtg-startup-pkg-'))
    folder = work / 'synthetic.pkg'
    copy_tree(ROOT / 'crates/client/tests/fixtures/audio.pkg', folder)
    sprites = folder / 'sprites'
    sprites.mkdir(parents=True, exist_ok=True)
    for i in range(count):
        (sprites / f'synthetic{i}.toml').write_text(
            f'frames = 1\nframe_size = [{size}, {size}]\norigin = [0, 0]\n'
            f'collision = "rect"\nbbox = [0, 0, {size - 1}, {size - 1}]\n', encoding='utf-8')
        write_png(sprites / f'synthetic{i}.png', size)
    archive = web_dir / f'synthetic-{count}x{size}.iwbz'
    print(f'packing {count} x {size}px sprites -> {archive}')
    proc = subprocess.run(
        ['cargo', 'run', '-q', '-p', 'iwbtg-tools', '--bin', 'iwbtg', '--locked', '--',
         'pkg', 'pack', str(folder), '-o', str(archive)],
        cwd=ROOT, capture_output=True, text=True, timeout=1800)
    if proc.returncode != 0:
        raise SystemExit(f'pkg pack failed:\n{proc.stdout}{proc.stderr}')
    shutil.rmtree(work, ignore_errors=True)
    print(f'archive: {archive.stat().st_size / (1 << 20):.1f} MiB, '
          f'{count * size * size * 4 / (1 << 20):.0f} MiB RGBA decoded')
    return archive


class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, _format, *_args):
        pass


def serve(web_dir):
    handler = functools.partial(Quiet, directory=str(web_dir))
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, server.server_address[1]


def run_chrome(chrome, url, timeout):
    """Loads `url` in headless Chrome and returns its stderr lines, stopping after the total."""
    profile = tempfile.mkdtemp(prefix='iwbtg-startup-chrome-')
    proc = subprocess.Popen(
        [chrome, '--headless=new', '--no-sandbox', '--disable-dev-shm-usage', '--no-first-run',
         f'--user-data-dir={profile}', '--enable-unsafe-swiftshader', '--use-angle=swiftshader',
         '--disable-background-timer-throttling', '--disable-renderer-backgrounding',
         '--enable-logging=stderr', '--v=0', url],
        stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True, errors='replace')
    lines, done = [], threading.Event()

    def read():
        for line in proc.stderr:
            lines.append(line.rstrip())
            if COMPLETE in line:
                done.set()
        done.set()

    threading.Thread(target=read, daemon=True).start()
    deadline = time.time() + timeout
    while time.time() < deadline and not done.is_set() and proc.poll() is None:
        time.sleep(0.2)
    if proc.poll() is None:
        proc.kill()
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        pass
    shutil.rmtree(profile, ignore_errors=True)
    return lines


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--web-dir', type=pathlib.Path, default=ROOT / 'crates/client/web',
                        help='folder with index.html and pkg/ (default crates/client/web)')
    parser.add_argument('--package', default='showcase.iwbz',
                        help='package in --web-dir to play (default showcase.iwbz)')
    parser.add_argument('--synthetic', type=int, metavar='N',
                        help='instead pack N generated sprites and measure those')
    parser.add_argument('--size', type=int, default=256, help='synthetic sprite size (default 256)')
    parser.add_argument('--chrome', help='Chrome/Chromium binary (default: $CHROME or PATH)')
    parser.add_argument('--timeout', type=int, default=300,
                        help='seconds to wait for the measurement (default 300)')
    parser.add_argument('--keep', action='store_true', help='keep the packed synthetic archive')
    args = parser.parse_args()

    web_dir = args.web_dir.resolve()
    if not (web_dir / 'pkg' / 'iwbtg-client.js').is_file():
        raise SystemExit(f'{web_dir}/pkg is missing; build the bundle first '
                         '(docs/dev-setup.md, The client in a browser)')
    chrome = chrome_path(args.chrome)
    version = subprocess.run([chrome, '--version'], capture_output=True, text=True,
                             timeout=30).stdout.strip()
    print(f'browser: {version or chrome}')

    archive = None
    if args.synthetic:
        archive = synthetic_package(web_dir, args.synthetic, args.size)
        package = archive.name
        if archive.stat().st_size > 512 << 20:
            raise SystemExit(f'the archive is over the client limit: {archive}')
    else:
        package = args.package
        path = web_dir / package
        if not path.is_file():
            raise SystemExit(f'{path} is missing; pack it first '
                             '(docs/architecture/client/web.md, Measuring startup)')
        print(f'package: {path.name} ({path.stat().st_size / (1 << 20):.1f} MiB)')

    server, port = serve(web_dir)
    url = f'http://127.0.0.1:{port}/?package={urllib.parse.quote(package)}&measure'
    try:
        print(f'loading {url}')
        lines = run_chrome(chrome, url, args.timeout)
    finally:
        server.shutdown()
        server.server_close()
        if archive is not None and not args.keep:
            archive.unlink(missing_ok=True)

    log = '\n'.join(lines)
    if 'panicked at' in log:
        raise SystemExit(f'the page panicked:\n' + '\n'.join(lines[-30:]))
    measured = []
    for line in lines:
        match = MEASURED.search(line)
        if match:
            measured.append(match.group(1))
        elif PHASE in line:
            measured.append(line[line.find(PHASE):])
        elif TOTAL in line:
            measured.append(line[line.find(TOTAL):])
    if not measured:
        raise SystemExit('no startup lines in the browser console; last lines:\n'
                         + '\n'.join(lines[-30:]))
    print('\n'.join(measured))
    if not any(COMPLETE in line for line in lines):
        raise SystemExit('the measurement did not reach the last sprite image')
    return 0


if __name__ == '__main__':
    sys.exit(main())
