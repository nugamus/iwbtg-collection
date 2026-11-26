#!/usr/bin/env python3
"""Headless-browser smoke test for the browser build (#1078, #1127).

Packs a fixture package with the native `iwbtg pkg pack`, serves `crates/client/web` on a local
port, and loads `?package=<file>` in headless Chrome. The page fetches the `.iwbz`, unpacks it
into `PackageFiles` and logs `package <id> loaded: gameplay hash <hex>`
(docs/architecture/client/web.md); this script asserts that hash equals the native one and that
the page did not panic. Chrome needs WebGL2; the SwiftShader flags make that work without a GPU.

It then plays a second package that starts a looping tune, with the same browser driven over the
DevTools protocol: it holds Right and taps Shift for a sustained session and fails if the page
stops answering, panics or crashes (#1127). That session is what a load-only check missed: the
web build deadlocked inside music sync the moment the first key unlocked the audio. Pass
`--no-sustained` to keep the old load-only check, or `--session-seconds` to lengthen it.

Run after `cargo build --release --target wasm32-unknown-unknown` and `wasm-bindgen` have written
`crates/client/web/pkg/` (docs/dev-setup.md, The client in a browser). `--chrome` or `$CHROME`
picks a browser; otherwise common names and install paths are tried.
"""
import argparse
import base64
import functools
import http.server
import json
import os
import pathlib
import re
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
HASH_LINE = re.compile(r'package \S+ loaded: gameplay hash ([0-9a-f]{64})')
PACK_HASH = re.compile(r'^gameplay_hash: ([0-9a-f]{64})$', re.MULTILINE)
CHROME_NAMES = ('google-chrome', 'google-chrome-stable', 'chromium', 'chromium-browser',
                'chrome', 'msedge')
SWIFTSHADER = ('--enable-unsafe-swiftshader', '--use-angle=swiftshader')


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
             '--no-first-run', f'--user-data-dir={profile}', *SWIFTSHADER,
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


class Cdp:
    """A minimal Chrome DevTools Protocol client over a raw WebSocket (stdlib only).

    The sustained session needs trusted key events (`Input.dispatchKeyEvent`), which a page
    script cannot fake: only a trusted gesture opens the browser's audio lock.
    """

    def __init__(self, ws_url, timeout=10.0):
        parsed = urllib.parse.urlparse(ws_url)
        self.sock = socket.create_connection((parsed.hostname, parsed.port), timeout=timeout)
        key = base64.b64encode(os.urandom(16)).decode()
        request = (f'GET {parsed.path or "/"} HTTP/1.1\r\n'
                   f'Host: {parsed.hostname}:{parsed.port}\r\n'
                   'Upgrade: websocket\r\nConnection: Upgrade\r\n'
                   f'Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n')
        self.sock.sendall(request.encode())
        self.buffer = b''
        head = b''
        while b'\r\n\r\n' not in head:
            chunk = self.sock.recv(4096)
            if not chunk:
                raise SystemExit('the DevTools websocket closed during the handshake')
            head += chunk
        header, self.buffer = head.split(b'\r\n\r\n', 1)
        if b' 101 ' not in header.split(b'\r\n', 1)[0]:
            raise SystemExit(f'DevTools websocket handshake failed: {header[:200]!r}')
        self.next_id = 1
        self.events = []

    def _read(self, count, deadline):
        while len(self.buffer) < count:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError
            self.sock.settimeout(min(remaining, 1.0))
            try:
                chunk = self.sock.recv(65536)
            except socket.timeout:
                continue
            if not chunk:
                raise EOFError('the DevTools websocket closed')
            self.buffer += chunk
        data, self.buffer = self.buffer[:count], self.buffer[count:]
        return data

    def _frame(self, deadline):
        first, second = self._read(2, deadline)
        opcode = first & 0x0F
        length = second & 0x7F
        if length == 126:
            length = struct.unpack('>H', self._read(2, deadline))[0]
        elif length == 127:
            length = struct.unpack('>Q', self._read(8, deadline))[0]
        mask = self._read(4, deadline) if second & 0x80 else None
        payload = self._read(length, deadline)
        if mask:
            payload = bytes(b ^ mask[i & 3] for i, b in enumerate(payload))
        return opcode, payload

    def _send(self, opcode, payload):
        mask = os.urandom(4)
        head = bytearray([0x80 | opcode])
        size = len(payload)
        if size < 126:
            head.append(0x80 | size)
        elif size < 65536:
            head.append(0x80 | 126)
            head += struct.pack('>H', size)
        else:
            head.append(0x80 | 127)
            head += struct.pack('>Q', size)
        masked = bytes(b ^ mask[i & 3] for i, b in enumerate(payload))
        self.sock.sendall(bytes(head) + mask + masked)

    def _message(self, deadline):
        while True:
            opcode, payload = self._frame(deadline)
            if opcode == 0x8:
                raise EOFError('the DevTools websocket closed')
            if opcode == 0x9:  # ping
                self._send(0xA, payload)
                continue
            if opcode == 0xA:  # pong
                continue
            if opcode in (0x1, 0x2):
                return json.loads(payload)

    def poll(self, timeout=1.0):
        """One protocol message, or None; method messages are also kept in `events`."""
        try:
            message = self._message(time.monotonic() + timeout)
        except TimeoutError:
            return None
        except EOFError:
            # The target went away: record it as a crash so the caller's checks see it.
            self.events.append({'method': 'Inspector.targetCrashed'})
            return None
        if 'method' in message:
            self.events.append(message)
        return message

    def command(self, method, params=None, timeout=30.0):
        ident = self.next_id
        self.next_id += 1
        self._send(0x1, json.dumps({'id': ident, 'method': method,
                                    'params': params or {}}).encode())
        deadline = time.monotonic() + timeout
        while True:
            try:
                message = self._message(deadline)
            except TimeoutError:
                raise TimeoutError(f'{method} got no answer within {timeout:g}s')
            except EOFError as error:
                raise SystemExit(f'{method}: {error}')
            if message.get('id') == ident:
                if 'error' in message:
                    raise SystemExit(f'{method}: {message["error"]}')
                return message.get('result', {})
            if 'method' in message:
                self.events.append(message)

    def close(self):
        try:
            self._send(0x8, b'')
        except OSError:
            pass
        self.sock.close()

    def log(self):
        """The page's console, errors and crashes as plain lines, for the checks below."""
        lines = []
        for event in self.events:
            method = event.get('method')
            params = event.get('params', {})
            if method == 'Runtime.consoleAPICalled':
                args = params.get('args', [])
                lines.append(' '.join(
                    str(arg.get('value', arg.get('description', arg.get('type'))))
                    for arg in args))
            elif method == 'Runtime.exceptionThrown':
                details = params.get('exceptionDetails', {})
                exception = details.get('exception', {})
                lines.append(f"EXCEPTION {details.get('text', '')} "
                             f"{exception.get('description', exception.get('value', ''))}")
            elif method == 'Log.entryAdded':
                entry = params.get('entry', {})
                lines.append(f"{entry.get('source')} {entry.get('level')}: {entry.get('text')}")
            elif method == 'Inspector.targetCrashed':
                lines.append('TARGET CRASHED')
        return '\n'.join(lines)


def devtools_page(profile, timeout):
    """The page target's websocket URL, waiting for Chrome to open DevTools."""
    active = pathlib.Path(profile) / 'DevToolsActivePort'
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if active.is_file():
            try:
                port = int(active.read_text().splitlines()[0])
                with urllib.request.urlopen(f'http://127.0.0.1:{port}/json', timeout=2) as body:
                    targets = json.load(body)
                for target in targets:
                    if target.get('type') == 'page' and \
                            target.get('url', '').startswith('http://127.0.0.1'):
                        return target['webSocketDebuggerUrl']
            except (OSError, ValueError, IndexError):
                pass
        time.sleep(0.2)
    raise SystemExit('headless Chrome did not expose a DevTools page')


def sustained(chrome, web_dir, package_dir, seconds, timeout):
    """Plays a package with a looping tune under sustained input and asserts it stays live.

    The package must start a looping tune in its first room (`audio.pkg` does): that is the path
    where the browser build deadlocked (#1127). The first trusted key unlocks audio; the session
    then holds Right and taps Shift, checking the page answers between taps.
    """
    package_dir = package_dir.resolve()
    archive = web_dir / (package_dir.stem + '.iwbz')
    print(f'sustained session: packing {package_dir} -> {archive}')
    pack(package_dir, archive)

    server, port = serve(web_dir)
    profile = tempfile.mkdtemp(prefix='iwbtg-web-sustained-')
    url = f'http://127.0.0.1:{port}/?package={urllib.parse.quote(archive.name)}'
    print(f'sustained session: loading {url} in {chrome}')
    proc = subprocess.Popen(
        [chrome, '--headless=new', '--no-sandbox', '--disable-dev-shm-usage',
         '--no-first-run', '--no-default-browser-check', f'--user-data-dir={profile}',
         '--remote-debugging-port=0', *SWIFTSHADER,
         '--enable-logging=stderr', '--v=0', url],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        start_new_session=(os.name == 'posix'))
    # Drain stderr so a chatty Chrome cannot fill the pipe and block.
    chrome_err = []
    threading.Thread(target=lambda: chrome_err.extend(proc.stderr), daemon=True).start()
    cdp = None
    try:
        ws_url = devtools_page(profile, min(timeout, 60))
        cdp = Cdp(ws_url)
        cdp.command('Runtime.enable')
        cdp.command('Log.enable')
        cdp.command('Inspector.enable')

        def panic_or_crash():
            log = cdp.log()
            if 'panicked at' in log:
                raise SystemExit(f'the page panicked:\n{tail(log)}')
            if any(event.get('method') == 'Inspector.targetCrashed' for event in cdp.events):
                raise SystemExit('the page crashed')

        def responsive(wait):
            """True if a trivial evaluate answers; False when the page stops responding."""
            try:
                cdp.command('Runtime.evaluate', {'expression': '1 + 1', 'returnByValue': True},
                            timeout=wait)
                return True
            except TimeoutError:
                return False

        def hang(where):
            panic_or_crash()
            raise SystemExit(
                f'the page stopped responding {where}. A main-thread deadlock in the wasm client '
                f'(#1127 was music sync blocking on a seek) looks like this. Last page output:'
                f'\n{tail(cdp.log())}')

        def key(kind, code, name, vk):
            try:
                cdp.command('Input.dispatchKeyEvent',
                            {'type': kind, 'key': name, 'code': code,
                             'windowsVirtualKeyCode': vk, 'nativeVirtualKeyCode': vk},
                            timeout=10)
            except TimeoutError:
                hang('while dispatching a key event')

        # Wait for the package line, then for the client to answer. The wasm compile and the
        # first pipeline build can stall the page for a while; that is not the deadlock.
        loaded = False
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            cdp.poll(0.5)
            panic_or_crash()
            if HASH_LINE.search(cdp.log()):
                loaded = True
                break
        if not loaded:
            raise SystemExit(f'no gameplay-hash line before the session:\n{tail(cdp.log())}')

        ready = False
        ready_deadline = time.monotonic() + timeout
        while time.monotonic() < ready_deadline:
            if responsive(20):
                ready = True
                break
            cdp.poll(0.5)
        if not ready:
            hang('before the first input')

        # The first trusted key is the browser gesture that opens the audio lock. If the lock
        # never opens, the session never reaches the tune and would pass for the wrong reason.
        unlocked = False
        for attempt in range(3):
            if attempt:
                # A gesture that landed while the page was busy may not count: press again.
                key('keyUp', 'ArrowRight', 'ArrowRight', 39)
                key('rawKeyDown', 'ArrowRight', 'ArrowRight', 39)
            else:
                key('rawKeyDown', 'ArrowRight', 'ArrowRight', 39)
            attempt_deadline = time.monotonic() + 8
            while time.monotonic() < attempt_deadline:
                cdp.poll(0.3)
                try:
                    result = cdp.command(
                        'Runtime.evaluate',
                        {'expression': 'window.iwbtgAudioUnlocked === true',
                         'returnByValue': True},
                        timeout=8)
                except TimeoutError:
                    hang('while waiting for the audio unlock')
                if result.get('result', {}).get('value') is True:
                    unlocked = True
                    break
            if unlocked:
                break
        if not unlocked:
            raise SystemExit('audio did not unlock after the first key; the session never reached '
                             'the tune playing path')

        print(f'sustained session: audio unlocked, playing {seconds:g}s with input')
        start = time.monotonic()
        next_tap = start
        while time.monotonic() - start < seconds:
            cdp.poll(0.2)
            panic_or_crash()
            now = time.monotonic()
            if now >= next_tap:
                next_tap = now + 0.8
                key('rawKeyDown', 'ShiftLeft', 'Shift', 16)
                key('keyUp', 'ShiftLeft', 'Shift', 16)
            if not responsive(8):
                cdp.poll(0.5)
                if not responsive(8):
                    hang('during play')
        key('keyUp', 'ArrowRight', 'ArrowRight', 39)
        panic_or_crash()
        if not responsive(8):
            hang('after the session')
        print(f'sustained session ok: {seconds:g}s of play, page alive, audio unlocked')
    finally:
        if cdp is not None:
            cdp.close()
        if os.name == 'posix':
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except OSError:
                pass
        else:
            proc.kill()
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            pass
        if proc.returncode not in (0, -9, -15, 143, None):
            print(f'headless Chrome exited {proc.returncode}:\n'
                  f'{tail("".join(chrome_err))}', file=sys.stderr)
        server.shutdown()
        server.server_close()
        shutil.rmtree(profile, ignore_errors=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--web-dir', type=pathlib.Path,
                        default=ROOT / 'crates/client/web',
                        help='folder with index.html and pkg/ (default crates/client/web)')
    parser.add_argument('--package-dir', type=pathlib.Path,
                        default=ROOT / 'crates/client/tests/fixtures/clear.pkg',
                        help='package folder to pack and play (default the clear fixture)')
    parser.add_argument('--sustained-package-dir', type=pathlib.Path,
                        default=ROOT / 'crates/client/tests/fixtures/audio.pkg',
                        help='package with a looping tune for the sustained session (#1127)')
    parser.add_argument('--session-seconds', type=float, default=10.0,
                        help='sustained input seconds after the audio unlocks (default 10)')
    parser.add_argument('--no-sustained', action='store_true',
                        help='run only the load/hash check')
    parser.add_argument('--chrome', help='Chrome/Chromium binary (default: $CHROME or PATH)')
    parser.add_argument('--timeout', type=int, default=180,
                        help='headless Chrome timeout in seconds (default 180)')
    parser.add_argument('--keep', action='store_true',
                        help='keep the packed .iwbz files in the web folder')
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

    if not args.no_sustained:
        try:
            sustained(chrome, web_dir, args.sustained_package_dir, args.session_seconds,
                      args.timeout)
        finally:
            if not args.keep:
                (web_dir / (args.sustained_package_dir.resolve().stem + '.iwbz')).unlink(
                    missing_ok=True)
    return 0


if __name__ == '__main__':
    sys.exit(main())
