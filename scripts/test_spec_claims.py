"""Every claim, register and converter-request id cited in the client must resolve (client#1114).

The namespaces and their homes are in `docs/architecture/testing.md`, "Citation
ids"; research `specs/IDS.md` is the model.

- Claim ids (`TICK-001`, `COL-031`, ...) come from the research GM8 spec, with
  `specs/INDEX.md` as the generated home of every id. The client cannot read
  that private checkout in CI, so `docs/architecture/sim/claim-ids.txt` holds
  the ids alone (no spec text) and this test checks every cited id against it.
- Register rows (`docs/architecture/sim/register-*.md`) keep the register id as
  the row key and name the matching claim in their `Claim` column. A row with no
  matching GM8 claim says so instead.
- Converter requests (`CR7`, ...) are rows of
  `docs/format/converter-requests.md`.

The scan covers every client doc (`docs/**/*.md`) and code comment/file
(`crates/**/*.rs`), not only the sim pages. Trace-request and issue ids are
research/GitHub-owned and are not checked here.

Regenerate the claim list after the research index changes (ids are stable, so
this is rare):

    IWBTG_RESEARCH=../iwbtg-collection-research \
        python3 -I scripts/test_spec_claims.py --write

`IWBTG_RESEARCH` points at the research checkout; the writer records its
`specs/INDEX.md` commit in the list header.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SIM = ROOT / "docs/architecture/sim"
ID_LIST = SIM / "claim-ids.txt"
CODE = ROOT / "crates"
DOCS = ROOT / "docs"
CONVERTER_REQUESTS = ROOT / "docs/format/converter-requests.md"

CLAIM = re.compile(r"\b[A-Z][A-Z0-9]*-\d{3,}\b")
CLAIM_ID = re.compile(r"^[A-Z][A-Z0-9]*-\d{3,}$")
REGISTER = re.compile(r"\b[CIKMNSR]\d{1,2}[a-z]?\b")
REGISTER_ID = re.compile(r"^[CIKMNSR]\d+[a-z]?$")
CR = re.compile(r"\bCR\d+\b")
# Id-shaped words that are not ids: a hash name, a placeholder, an ISO standard, a security
# advisory, the old M* milestone names in the roadmap, and the client UI's icon variants.
IGNORE_CLAIMS = {"SHA-256", "NNN", "ISO-8601", "RUSTSEC-2026"}
IGNORE_REGISTERS = {"M2b", "M9", "M10", "M11", "M12", "M13"}
NO_CLAIM = "—"


def read_ids() -> list[str]:
    lines = ID_LIST.read_text(encoding="utf-8").splitlines()
    return [line.strip() for line in lines if line.strip() and not line.startswith("#")]


def index_ids(research: Path) -> list[str]:
    """Claim ids from the research index, in INDEX.md order."""
    text = (research / "specs/INDEX.md").read_text(encoding="utf-8")
    section = text.split("## Claim ids", 1)
    if len(section) != 2:
        raise SystemExit(f"{research}/specs/INDEX.md has no '## Claim ids' section")
    body = section[1].split("\n## ", 1)[0]
    ids = re.findall(r"^\| \[([A-Z][A-Z0-9]*-\d{3,})\]", body, re.M)
    if not ids:
        raise SystemExit(f"{research}/specs/INDEX.md lists no claim ids")
    return ids


def source_commit(research: Path) -> str:
    try:
        return subprocess.run(
            ["git", "-C", str(research), "rev-parse", "HEAD"],
            check=True, capture_output=True, text=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def write_list(research: Path) -> None:
    ids = index_ids(research)
    header = [
        "# Claim ids cited from the research GM8 spec (ids only; no spec text).",
        f"# Source: iwbtg-collection-research specs/INDEX.md at {source_commit(research)}",
        "# Regenerate: IWBTG_RESEARCH=../iwbtg-collection-research"
        " python3 -I scripts/test_spec_claims.py --write",
        "# Do not edit by hand; scripts/test_spec_claims.py checks every cited id.",
    ]
    ID_LIST.write_text("\n".join(header + ids) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {ID_LIST.relative_to(ROOT)}: {len(ids)} claim ids")


def sim_pages() -> list[Path]:
    return sorted(SIM.rglob("*.md"))


def citation_pages() -> list[Path]:
    """Every client page and source file that may cite an id."""
    return sorted(DOCS.rglob("*.md")) + sorted(CODE.rglob("*.rs"))


def register_rows() -> dict[str, tuple[Path, int, list[str]]]:
    """Register id -> (page, line, cells) for each row of each register table."""
    rows: dict[str, tuple[Path, int, list[str]]] = {}
    for page in sorted(SIM.glob("register-*.md")):
        header: list[str] = []
        for n, line in enumerate(page.read_text(encoding="utf-8").splitlines(), 1):
            if not line.startswith("|"):
                continue
            cells = [cell.strip() for cell in split_row(line)]
            if cells and cells[0] in ("Id", "id"):
                header = cells
                continue
            if not cells or not REGISTER_ID.match(cells[0]):
                continue
            rows[cells[0]] = (page, n, cells)
    return rows


def split_row(line: str) -> list[str]:
    """Split a markdown table row on pipes outside backticks and not escaped."""
    text = line.strip()
    cells: list[str] = []
    cur: list[str] = []
    in_code = False
    i = 0
    while i < len(text):
        ch = text[i]
        if ch == "\\" and i + 1 < len(text) and text[i + 1] == "|":
            cur.append("|")
            i += 2
            continue
        if ch == "`":
            in_code = not in_code
        elif ch == "|" and not in_code:
            cells.append("".join(cur))
            cur = []
            i += 1
            continue
        cur.append(ch)
        i += 1
    cells.append("".join(cur))
    # Drop the empty cells the outer pipes produce.
    if cells and not cells[0].strip():
        cells = cells[1:]
    if cells and not cells[-1].strip():
        cells = cells[:-1]
    return cells


def claim_column() -> dict[str, str]:
    """Register id -> the `Claim` cell, from the tables' last column header."""
    out: dict[str, str] = {}
    for page in sorted(SIM.glob("register-*.md")):
        header: list[str] = []
        for line in page.read_text(encoding="utf-8").splitlines():
            if not line.startswith("|"):
                continue
            cells = [cell.strip() for cell in split_row(line)]
            if cells and cells[0] in ("Id", "id"):
                header = cells
                continue
            if not header or not cells or not REGISTER_ID.match(cells[0]):
                continue
            claim_at = header.index("Claim") if "Claim" in header else None
            out[cells[0]] = cells[claim_at] if claim_at is not None and claim_at < len(cells) else ""
    return out


def cited_claims() -> list[str]:
    return [
        token
        for page in citation_pages()
        for line in page.read_text(encoding="utf-8").splitlines()
        for token in CLAIM.findall(line)
        if token not in IGNORE_CLAIMS and CLAIM_ID.match(token)
    ]


def unknown_claim_ids() -> list[str]:
    known = set(read_ids())
    out = []
    for page in citation_pages():
        for n, line in enumerate(page.read_text(encoding="utf-8").splitlines(), 1):
            for token in CLAIM.findall(line):
                if token in IGNORE_CLAIMS or CLAIM_ID.match(token) is None:
                    continue
                if token not in known:
                    out.append(f"{page.relative_to(ROOT)}:{n}: unknown claim id {token}")
    return out


def unknown_register_ids() -> list[str]:
    defined = set(register_rows())
    out = []
    for page in citation_pages():
        for n, line in enumerate(page.read_text(encoding="utf-8").splitlines(), 1):
            for token in REGISTER.findall(line):
                if token in IGNORE_REGISTERS or not REGISTER_ID.match(token):
                    continue
                if token not in defined:
                    out.append(f"{page.relative_to(ROOT)}:{n}: unknown register id {token}")
    return out


def converter_rows() -> set[str]:
    """The CR ids that have a row in `docs/format/converter-requests.md`."""
    text = CONVERTER_REQUESTS.read_text(encoding="utf-8")
    return set(re.findall(r"^\|\s*(CR\d+)\s*\|", text, re.M))


def unknown_converter_ids() -> list[str]:
    known = converter_rows()
    out = []
    for page in citation_pages():
        for n, line in enumerate(page.read_text(encoding="utf-8").splitlines(), 1):
            for token in CR.findall(line):
                if token not in known:
                    out.append(f"{page.relative_to(ROOT)}:{n}: unknown converter request {token}")
    return out


def claim_cell_problems() -> list[str]:
    known = set(read_ids())
    claims = claim_column()
    problems = []
    for rid, (page, n, cells) in register_rows().items():
        if len(cells) < 4:
            problems.append(f"{page.name}:{n}: {rid} row has no Claim column")
            continue
        cell = claims.get(rid, "")
        if not cell:
            problems.append(f"{page.name}:{n}: {rid} has no Claim cell")
            continue
        if cell.startswith(NO_CLAIM):
            if "(" not in cell:
                problems.append(f"{page.name}:{n}: {rid} says no claim without a reason")
            continue
        for cid in (part.strip() for part in cell.split(",")):
            if not CLAIM_ID.match(cid) or cid not in known:
                problems.append(f"{page.name}:{n}: {rid} names unknown claim {cid!r}")
    return problems


class SpecClaimTests(unittest.TestCase):
    def test_the_id_list_is_well_formed(self):
        ids = read_ids()
        self.assertTrue(ids, "the claim id list is empty")
        for cid in ids:
            self.assertRegex(cid, CLAIM_ID, "claim id list entry")
        self.assertEqual(len(ids), len(set(ids)), "duplicate claim id in the list")
        self.assertIn("iwbtg-collection-research specs/INDEX.md at",
                      ID_LIST.read_text(encoding="utf-8").splitlines()[1],
                      "the list header records where it came from")

    def test_cited_claim_ids_exist(self):
        problems = unknown_claim_ids()
        self.assertEqual(problems, [], "\n".join(problems))

    def test_cited_register_ids_exist(self):
        problems = unknown_register_ids()
        self.assertEqual(problems, [], "\n".join(problems))

    def test_cited_converter_ids_exist(self):
        problems = unknown_converter_ids()
        self.assertEqual(problems, [], "\n".join(problems))

    def test_every_register_row_names_a_claim_or_says_none(self):
        problems = claim_cell_problems()
        self.assertEqual(problems, [], "\n".join(problems))

    @unittest.skipUnless(os.environ.get("IWBTG_RESEARCH"), "needs the research checkout")
    def test_the_list_matches_the_research_index(self):
        research = Path(os.environ["IWBTG_RESEARCH"])
        if not (research / "specs/INDEX.md").exists():
            self.skipTest(f"{research}/specs/INDEX.md is absent")
        self.assertEqual(read_ids(), index_ids(research),
                         "claim-ids.txt is stale; run the --write command in its header")


def main(argv: list[str]) -> int:
    if "--write" in argv:
        research = os.environ.get("IWBTG_RESEARCH")
        if not research:
            raise SystemExit("set IWBTG_RESEARCH to the iwbtg-collection-research checkout")
        write_list(Path(research))
        return 0
    problems = claim_cell_problems() + unknown_claim_ids() + unknown_register_ids() + unknown_converter_ids()
    if problems:
        print("\n".join(problems), file=sys.stderr)
        return 1
    print(f"client docs and code: {len(set(cited_claims()))} claim ids cited, "
          f"{len(register_rows())} register rows, {len(converter_rows())} converter requests, all resolve")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
