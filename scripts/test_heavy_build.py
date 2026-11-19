"""Queue, stale recovery and release for the heavy-build slot (#1029). No cargo is run."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

from heavy_build import acquire, owner, read_owner, release

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "heavy_build.py"


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
            holder = subprocess.Popen(
                [sys.executable, "-I", str(SCRIPT), "--lock", str(lock), "--",
                 sys.executable, "-I", "-c", "import time; time.sleep(1.5)"],
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            deadline = time.monotonic() + 10
            while not lock.exists() and time.monotonic() < deadline:
                time.sleep(0.05)
            self.assertTrue(lock.exists(), "the holder took the slot")
            started = time.monotonic()
            waiter = subprocess.run(
                [sys.executable, "-I", str(SCRIPT), "--lock", str(lock), "--wait", "30", "--",
                 sys.executable, "-I", "-c", "print('waiter-ran')"],
                capture_output=True, text=True, timeout=30)
            elapsed = time.monotonic() - started
            holder.wait(timeout=10)
            self.assertEqual(holder.returncode, 0)
            self.assertEqual(waiter.returncode, 0, waiter.stderr)
            self.assertIn("waiter-ran", waiter.stdout)
            self.assertIn("waiting for the slot", waiter.stdout)
            self.assertGreaterEqual(elapsed, 1.0)
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
