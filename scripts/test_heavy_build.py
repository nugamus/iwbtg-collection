"""Queue, stale recovery and release for the heavy-build slot (#1029). No cargo is run."""
import os
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import threading
import time
import unittest

from heavy_build import acquire, owner, read_owner, release

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "heavy_build.py"
OUTPUT_TIMEOUT_SECONDS = 30


def read_lines(stream, lines):
    """Collect a child's output on a thread, so the test can wait for one line."""
    try:
        for line in stream:
            lines.put(line)
    finally:
        stream.close()


class SlotTests(unittest.TestCase):
    def test_slot_is_exclusive_and_released(self):
        with tempfile.TemporaryDirectory() as folder:
            lock = Path(folder) / "slot.lock"
            self.assertTrue(acquire(lock, owner(lock, "first"), wait=0))
            self.assertTrue(lock.exists())
            self.assertFalse(acquire(lock, owner(lock, "second"), wait=0))
            release(lock)
            self.assertFalse(lock.exists())
            self.assertTrue(acquire(lock, owner(lock, "third"), wait=0))
            release(lock)

    def test_a_crashed_holder_is_reclaimed(self):
        with tempfile.TemporaryDirectory() as folder:
            lock = Path(folder) / "slot.lock"
            lock.write_text("not json", encoding="utf-8")
            old = time.time() - 120
            os.utime(lock, (old, old))
            self.assertTrue(acquire(lock, owner(lock, "after-crash"), wait=0, stale=90))
            self.assertEqual(read_owner(lock)["command"], "after-crash")
            release(lock)

    def test_a_waiter_runs_after_the_holder_and_reports_progress(self):
        with tempfile.TemporaryDirectory() as folder:
            lock = Path(folder) / "slot.lock"
            # The holder runs until the test writes a line to its stdin, so the waiter is
            # still blocked when it reports progress. A fixed holder sleep made a slow start
            # flaky: the waiter could reach the lock after the holder had already released it.
            holder = subprocess.Popen(
                [sys.executable, "-I", str(SCRIPT), "--lock", str(lock), "--",
                 sys.executable, "-I", "-c", "import sys; sys.stdin.readline()"],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            waiter = None
            try:
                deadline = time.monotonic() + 10
                while not lock.exists() and time.monotonic() < deadline:
                    time.sleep(0.05)
                self.assertTrue(lock.exists(), "the holder took the slot")
                waiter = subprocess.Popen(
                    [sys.executable, "-I", str(SCRIPT), "--lock", str(lock), "--wait", "30", "--",
                     sys.executable, "-I", "-c", "print('waiter-ran')"],
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
                lines = queue.Queue()
                reader = threading.Thread(target=read_lines, args=(waiter.stdout, lines), daemon=True)
                reader.start()
                output = []
                deadline = time.monotonic() + OUTPUT_TIMEOUT_SECONDS
                while time.monotonic() < deadline:
                    try:
                        line = lines.get(timeout=0.1)
                    except queue.Empty:
                        continue
                    output.append(line)
                    if "waiting for the slot" in line:
                        break
                else:
                    self.fail(f"the waiter did not report waiting for the slot within "
                              f"{OUTPUT_TIMEOUT_SECONDS}s; output was {''.join(output)!r}")
                self.assertIsNone(waiter.poll(), "the waiter was blocked while it reported waiting")
                holder.stdin.write("go\n")
                holder.stdin.flush()
                waiter_code = waiter.wait(timeout=30)
                reader.join(timeout=5)
                self.assertFalse(reader.is_alive(), "the waiter's output ended")
                while True:
                    try:
                        output.append(lines.get_nowait())
                    except queue.Empty:
                        break
            finally:
                if waiter is not None and waiter.poll() is None:
                    waiter.kill()
                    waiter.wait()
                try:
                    holder.stdin.close()
                except OSError:
                    pass
                try:
                    holder.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    holder.kill()
                    holder.wait()
                waiter_stderr = waiter.stderr.read() if waiter is not None else ""
                holder_stderr = holder.stderr.read()
                for stream in (waiter.stderr if waiter is not None else None,
                               holder.stdout, holder.stderr):
                    if stream is not None:
                        try:
                            stream.close()
                        except OSError:
                            pass
            self.assertEqual(waiter_code, 0, waiter_stderr)
            self.assertEqual(holder.returncode, 0, holder_stderr)
            self.assertIn("waiter-ran", "".join(output))
            self.assertIn("waiting for the slot", "".join(output))
            self.assertFalse(lock.exists())

    def test_status_is_available_when_free(self):
        with tempfile.TemporaryDirectory() as folder:
            lock = Path(folder) / "slot.lock"
            result = subprocess.run(
                [sys.executable, "-I", str(SCRIPT), "--lock", str(lock), "--status"],
                capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 0)
            self.assertIn("slot is free", result.stdout)


if __name__ == "__main__":
    unittest.main()
