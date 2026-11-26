"""Pin the dashboard's Script API counts to their generated sources.

The block used to count every census row, including a group whose status is
already "Registered"; this test parses both documents independently and
compares them with the generator's loader. The two expected values are pinned
on purpose: when an API group lands, update this test with the refreshed
counts in the same PR so the dashboard cannot silently drift.
"""

import importlib.util
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
GENERATE = ROOT / "progress" / "generate.py"
SCRIPT_API = ROOT / "docs" / "format" / "script-api.md"
SCRIPTS = ROOT / "docs" / "architecture" / "sim" / "scripts.md"

# Pinned 2026-11-25: path_start/path_end landed in #1530; paths remains in the
# census table for the record but is no longer an unregistered group.
EXPECTED_FUNCTIONS = 79
EXPECTED_GROUPS = 6


def load_generate():
    spec = importlib.util.spec_from_file_location("progress_generate", GENERATE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def registered_function_names():
    text = SCRIPT_API.read_text(encoding="utf-8")
    section = text.split("\n## Functions\n", 1)[1].split("\n## ", 1)[0]
    return {
        hit.group(1)
        for hit in re.finditer(r"^- `([A-Za-z_][A-Za-z0-9_$]*)\s*\(", section, re.M)
    }


def unregistered_groups():
    text = SCRIPTS.read_text(encoding="utf-8")
    section = text.split("### Census groups not yet registered", 1)[1].split("\n### ", 1)[0]
    groups = []
    for line in section.splitlines():
        if not line.startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) < 3 or cells[0].startswith("Census") or set(cells[0]) <= {"-", ":", " "}:
            continue
        status = re.sub(r"[*_`]", "", cells[2]).strip()
        if status.lower().startswith("registered"):
            continue
        groups.append(cells[0])
    return groups


class ScriptApiCountsTest(unittest.TestCase):
    def test_loader_matches_the_generated_sources(self):
        info = load_generate().load_script_api()
        self.assertEqual(info["registered_functions"], len(registered_function_names()))
        self.assertEqual(info["planned_count"], len(unregistered_groups()))
        self.assertEqual(info["registered_functions"], EXPECTED_FUNCTIONS)
        self.assertEqual(info["planned_count"], EXPECTED_GROUPS)
        self.assertNotIn("paths (14)", [group["name"] for group in info["planned_groups"]])


if __name__ == "__main__":
    unittest.main()
