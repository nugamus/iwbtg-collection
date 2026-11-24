"""One machine-wide slot for a cold or heavy client build (#1029).

Measured on the development host: three concurrent cold `cargo check -p iwbtg-client` runs took
665 s, against 359 s when the same three ran one after another; warm client checks took about
6 s at any concurrency. The slot therefore serializes only the work an agent chooses to mark as
heavy. Editing, searches and light checks never wait for it.

Usage, from a client checkout or worktree:

    python -I scripts/heavy_build.py -- cargo build -p iwbtg-client --locked
    python -I scripts/heavy_build.py --status

A holder updates the lock's mtime every few seconds; a crashed holder's lock is reclaimed after
`--stale` seconds (default 90). Waiters print progress and give up after `--wait` seconds (default
900) instead of queueing forever. The lock lives in the system temp folder, so it is shared by
every worktree of the same checkout and never enters a repository.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import tempfile
import threading
import time

HEARTBEAT_SECONDS = 5
STALE_SECONDS = 90
WAIT_LIMIT_SECONDS = 900


def default_lock():
    """A stable name for the main checkout shared by all of its worktrees."""
    root = Path(__file__).resolve().parents[1]
    try:
        common = subprocess.check_output(["git", "-C", str(root), "rev-parse", "--git-common-dir"],
                                         text=True, stderr=subprocess.DEVNULL).strip()
        key = str(Path(common).resolve().parent)
    except (OSError, subprocess.CalledProcessError):
        key = str(root)
    return Path(tempfile.gettempdir()) / ("iwbtg-heavy-build-" + hashlib.sha256(key.encode()).hexdigest()[:16] + ".lock")


def owner(lock, command):
    return {
        "pid": os.getpid(),
        "host": platform.node(),
        "started": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "command": command,
        "worktree": os.getcwd(),
    }


def read_owner(lock):
    try:
        return json.loads(lock.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}


def acquire(lock, info, wait=WAIT_LIMIT_SECONDS, stale=STALE_SECONDS):
    """Take the slot, reclaim a stale one, or wait. False when `wait` is exhausted."""
    lock.parent.mkdir(parents=True, exist_ok=True)
    deadline = time.monotonic() + wait
    last_notice = 0.0
    while True:
        try:
            with os.fdopen(os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY), "w",
                           encoding="utf-8") as file:
                json.dump(info, file)
            return True
        except FileExistsError:
            try:
                age = time.time() - lock.stat().st_mtime
            except OSError:
                age = 0.0
            if age > stale:
                try:
                    lock.unlink()
                except OSError:
                    pass
                print(f"heavy build: reclaimed a stale slot ({age:.0f}s without a heartbeat)", flush=True)
                continue
            if time.monotonic() >= deadline:
                holder = read_owner(lock)
                print(f"heavy build: no slot after {wait}s; holder pid {holder.get('pid')} "
                      f"({holder.get('command')})", file=sys.stderr, flush=True)
                return False
            if time.monotonic() - last_notice >= 10:
                holder = read_owner(lock)
                print(f"heavy build: waiting for the slot held by pid {holder.get('pid')} "
                      f"since {holder.get('started')}; light work can continue", flush=True)
                last_notice = time.monotonic()
            time.sleep(1)


def heartbeat(lock, stop):
    while not stop.wait(HEARTBEAT_SECONDS):
        try:
            os.utime(lock, None)
        except OSError:
            return


def release(lock):
    try:
        lock.unlink()
    except OSError:
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--status", action="store_true", help="show the current holder and exit")
    parser.add_argument("--lock", type=Path, help="lock path (tests; default is a temp-folder lock)")
    parser.add_argument("--wait", type=int, default=WAIT_LIMIT_SECONDS)
    parser.add_argument("--stale", type=int, default=STALE_SECONDS)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    lock = args.lock or default_lock()
    if args.status:
        if not lock.exists():
            print("heavy build: slot is free")
        else:
            holder = read_owner(lock)
            print(json.dumps(holder) if holder else "heavy build: slot is held (unreadable owner)")
        return 0
    command = args.command
    if command[:1] == ["--"]:
        command = command[1:]
    if not command:
        parser.error("give the heavy command after `--`")
    info = owner(lock, " ".join(command))
    if not acquire(lock, info, args.wait, args.stale):
        return 3
    stop = threading.Event()
    threading.Thread(target=heartbeat, args=(lock, stop), daemon=True).start()

    def interrupted(_signum, _frame):
        raise KeyboardInterrupt

    signal.signal(signal.SIGTERM, interrupted)
    try:
        return subprocess.call(command)
    except KeyboardInterrupt:
        return 130
    finally:
        stop.set()
        release(lock)


if __name__ == "__main__":
    sys.exit(main())
