"""Player-visible UI strings come from the locale catalogue, never a Rust literal (#196).

CI runs every `test_*.py` in `scripts/` (`.github/workflows/ci.yml`, changes job), so this file is
the hard-coded-string guard: a new English or Japanese sentence passed straight to a text
constructor in a localised module fails the build.

Two halves, both deterministic and offline:

- **Catalogue.** `crates/client/locales/{en,ja}.toml` carry the same flat keys, every key is a
  non-empty string, placeholders (`{name}`) match between the languages, every key is used by the
  client, and every key a `locale::text`/`locale::format` call names exists.
- **Source.** In the modules #196 localises, a string literal is not passed as the text of a UI
  widget. `TEXT_CONSTRUCTORS` below is the whole rule; there is no per-string allowlist. A literal
  with no word (a time placeholder such as `0:00.00`, a bare `—`) is not text and is allowed.

Scope: the modules localised so far (#196, #1626), `controls.rs` and the pause menu's own modules
included. `shell/**` joins `SCANNED` when #1626 localises it.
"""

import re
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CLIENT = ROOT / "crates" / "client"

# The modules whose player-visible text #196 moved into the catalogue. `src/hud` and `src/results`
# are directories (all `.rs` under them).
SCANNED = (
    "src/pause.rs",
    "src/pause",
    "src/controls.rs",
    "src/ui/widgets.rs",
    "src/ui/notice.rs",
    "src/hud",
    "src/results",
    "src/viewer.rs",
    "src/card",
    "src/loading/page.rs",
    "src/ui/toolbar.rs",
)

CATALOGUES = {
    "en": CLIENT / "locales" / "en.toml",
    "ja": CLIENT / "locales" / "ja.toml",
}

# Functions whose first argument is the player-visible text of a widget. `button` here is the
# kit's label button, not `button_with` (whose first arguments are a kind, size and a bundle).
TEXT_CONSTRUCTORS = ("text", "Text::new", "button", "pill", "stat", "head", "label")

_CONSTRUCTOR_RE = re.compile(
    r"(?<![\w:])(?:%s)\s*\(" % "|".join(re.escape(n) for n in TEXT_CONSTRUCTORS)
)
_STRING_RE = re.compile(r'"((?:[^"\\]|\\.)*)"')
# A "word" is two or more Latin letters, or any kana/kanji: a bare number, a time, a dash or an
# ellipsis is not UI text and needs no catalogue entry.
_WORD_RE = re.compile(r"[A-Za-z]{2,}|[぀-ヿ㐀-鿿]")
_LOCALE_CALL_RE = re.compile(r"locale::(?:text|format)\s*\(")


def catalogue(language):
    """The flat key -> string table of a locale file."""
    with CATALOGUES[language].open("rb") as f:
        return tomllib.load(f)


def source_files():
    """The scoped Rust files, sorted."""
    out = []
    for entry in SCANNED:
        path = CLIENT / entry
        out.extend([path] if path.is_file() else sorted(path.rglob("*.rs")))
    return out


def strip_test_modules(text):
    """`text` with inline `#[cfg(test)]` modules blanked out.

    Tests check English outputs, so their literals are not player-facing UI. rustfmt puts a
    top-level module's closing brace at column 0, which is what ends the skip.
    """
    kept = []
    skipping = False
    for line in text.splitlines():
        if not skipping and line.strip().startswith("#[cfg(test)]"):
            skipping = True
            continue
        if skipping:
            if line.rstrip() == "}":
                skipping = False
            continue
        kept.append(line)
    return "\n".join(kept)


def first_argument(text, open_paren):
    """The substring of `text` from `open_paren` (just past the `(`) to its top-level `,` or `)`."""
    depth = 0
    in_string = False
    escaped = False
    out = []
    i = open_paren
    while i < len(text):
        c = text[i]
        if in_string:
            if escaped:
                escaped = False
            elif c == "\\":
                escaped = True
            elif c == '"':
                in_string = False
        elif c == '"':
            in_string = True
        elif c in "([{":
            depth += 1
        elif c in ")]}":
            if depth == 0:
                break
            depth -= 1
        elif c == "," and depth == 0:
            break
        out.append(c)
        i += 1
    return "".join(out)


def without_comments(text):
    """`text` with comments blanked to spaces, newlines kept so line numbers still match.

    A constructor call in a comment (`// text("old", …)`) must not fail the guard, and a call
    whose first argument is on the next line still must be seen.
    """
    out = []
    i = 0
    n = len(text)
    in_string = in_char = line_comment = False
    escaped = False
    block_depth = 0
    while i < n:
        c = text[i]
        nxt = text[i + 1] if i + 1 < n else ""
        if line_comment:
            if c == "\n":
                line_comment = False
                out.append(c)
            else:
                out.append(" ")
        elif block_depth:
            if c == "/" and nxt == "*":
                block_depth += 1
                out.append("  ")
                i += 1
            elif c == "*" and nxt == "/":
                block_depth -= 1
                out.append("  ")
                i += 1
            elif c == "\n":
                out.append(c)
            else:
                out.append(" ")
        elif in_string:
            out.append(c)
            if escaped:
                escaped = False
            elif c == "\\":
                escaped = True
            elif c == '"':
                in_string = False
        elif in_char:
            out.append(c)
            if escaped:
                escaped = False
            elif c == "\\":
                escaped = True
            elif c == "'":
                in_char = False
        elif c == "/" and nxt == "/":
            line_comment = True
            out.append("  ")
            i += 1
        elif c == "/" and nxt == "*":
            block_depth = 1
            out.append("  ")
            i += 1
        elif c == '"':
            in_string = True
            out.append(c)
        elif c == "'":
            # A char literal ('a', '\n') opens a quote; a lifetime ('static) does not.
            closes = (i + 2 < n and text[i + 2] == "'" and text[i + 1] != "\\") or (
                i + 3 < n and text[i + 1] == "\\" and text[i + 3] == "'"
            )
            in_char = closes
            out.append(c)
        else:
            out.append(c)
        i += 1
    return "".join(out)


def hard_coded_literals(text, where="<snippet>"):
    """The player-visible string literals passed to a text constructor, as `(where, literal)`."""
    code = without_comments(strip_test_modules(text))
    found = []
    for match in _CONSTRUCTOR_RE.finditer(code):
        argument = first_argument(code, match.end())
        literal = _STRING_RE.match(argument.lstrip())
        if literal and _WORD_RE.search(literal.group(1)):
            line = code.count("\n", 0, match.start()) + 1
            found.append((f"{where}:{line}", literal.group(1)))
    return found


def locale_keys_in(text):
    """Every string literal named as a key by a `locale::text`/`locale::format` call."""
    keys = []
    for match in _LOCALE_CALL_RE.finditer(text):
        for literal in _STRING_RE.finditer(first_argument(text, match.end())):
            keys.append(literal.group(1))
    return keys


def rust_sources():
    return sorted((CLIENT / "src").rglob("*.rs"))


def japanese_subset_codepoints():
    """The codepoints `NotoSansJP-subset.ttf` carries, as `assets/fonts/make-jp-subset.py` builds it.

    Kana, Japanese punctuation and fullwidth forms, then JIS X 0208 level 1 kanji (lead bytes
    0x88..0x98). Level 2 needs the subset rebuilt (the script says how).
    """
    covered = set()
    for first, last in (
        (0x3000, 0x30FF),
        (0x31F0, 0x31FF),
        (0xFF00, 0xFFEF),
        (0x2010, 0x2027),
        (0x25A0, 0x25EF),
    ):
        covered.update(range(first, last + 1))
    for lead in range(0x88, 0x99):
        for trail in range(0x40, 0xFD):
            if trail == 0x7F:
                continue
            try:
                covered.add(ord(bytes([lead, trail]).decode("shift_jis")))
            except UnicodeDecodeError:
                pass
    return covered


def needs_japanese_subset(char):
    """Whether `char` is a script the Japanese fallback must draw (not a symbol another face has)."""
    codepoint = ord(char)
    return (
        0x3000 <= codepoint <= 0x30FF  # kana and Japanese punctuation
        or 0x31F0 <= codepoint <= 0x31FF  # katakana phonetic extensions
        or 0x4E00 <= codepoint <= 0x9FFF  # CJK unified ideographs
        or 0xFF00 <= codepoint <= 0xFFEF  # fullwidth forms
    )


class ScannerSelfTest(unittest.TestCase):
    """The guard's own test: it catches a literal and lets the localised forms through."""

    def test_catches_a_hard_coded_label(self):
        self.assertEqual(
            hard_coded_literals('c.spawn(text("Personal best", style, ink));'),
            [("<snippet>:1", "Personal best")],
        )
        self.assertEqual(
            hard_coded_literals('Text::new("PAUSE")'),
            [("<snippet>:1", "PAUSE")],
        )
        self.assertEqual(
            hard_coded_literals('children![text("No clears yet", a, b)]'),
            [("<snippet>:1", "No clears yet")],
        )

    def test_catches_a_hard_coded_japanese_label(self):
        self.assertEqual(
            hard_coded_literals('text("保存できません", a, b)'),
            [("<snippet>:1", "保存できません")],
        )

    def test_allows_locale_calls_and_non_words(self):
        for snippet in (
            'text(locale::text("results_back"), style, ink)',
            'Text::new(locale::text("pause_title"))',
            'button(locale::text("pause_continue"), kind)',
            'pill(locale::text("pill_ranked"), style, a, b, 26.0, 12.0)',
            'text("0:00.00", time, ink)',
            'text("", time, ink)',
            'text("—", style, ink)',
            'text(&format!("{}", n), style, ink)',
            'children![text(&locale::format("results_stage_number", &[("n", &6)]), a, b)]',
            '// text("commented out", a, b)',
        ):
            self.assertEqual(hard_coded_literals(snippet), [], snippet)

    def test_catches_a_literal_on_the_next_line(self):
        snippet = (
            'c.spawn((\n'
            '    text(\n'
            '        "Death timeline",\n'
            '        style,\n'
            '        ink,\n'
            '    ),\n'
            '));'
        )
        # The report points at the constructor call, which is where the fix goes.
        self.assertEqual(
            hard_coded_literals(snippet), [("<snippet>:2", "Death timeline")]
        )

    def test_inline_comments_are_ignored(self):
        snippet = 'let a = 1; // text("old", a, b)\nlet b = 2;'
        self.assertEqual(hard_coded_literals(snippet), [])

    def test_test_modules_are_ignored(self):
        snippet = 'fn f() {}\n#[cfg(test)]\nmod tests {\n    fn t() { text("Back", a, b); }\n}\n'
        self.assertEqual(hard_coded_literals(snippet), [])

    def test_first_argument_stops_at_the_comma(self):
        self.assertEqual(
            first_argument('(a, "b", c)', 1), 'a'
        )
        self.assertEqual(
            first_argument('("k", &[("n", &1)])', 1), '"k"'
        )


class CatalogueTest(unittest.TestCase):
    def test_both_languages_have_the_same_non_empty_keys(self):
        en, ja = catalogue("en"), catalogue("ja")
        self.assertEqual(sorted(en), sorted(ja))
        self.assertGreater(len(en), 40, "the catalogue is populated")
        for key, value in {**en, **ja}.items():
            self.assertIsInstance(value, str, f"{key} is not a string")
            self.assertTrue(value.strip(), f"{key} is empty")

    def test_placeholders_match_between_languages(self):
        def placeholders(value):
            return sorted(re.findall(r"\{([^{}]+)\}", value))

        en, ja = catalogue("en"), catalogue("ja")
        for key in en:
            self.assertEqual(placeholders(en[key]), placeholders(ja[key]), key)

    def test_japanese_is_marked_as_machine_translation(self):
        header = CATALOGUES["ja"].read_text(encoding="utf-8")
        self.assertIn("MACHINE TRANSLATION", header)

    def test_japanese_glyphs_are_in_the_bundled_subset(self):
        # A kanji outside JIS level 1 would draw as tofu: rebuild the subset first
        # (crates/client/assets/fonts/make-jp-subset.py).
        covered = japanese_subset_codepoints()
        missing = []
        for key, value in catalogue("ja").items():
            for char in value:
                if needs_japanese_subset(char) and ord(char) not in covered:
                    missing.append(f"{key}: U+{ord(char):04X}")
        self.assertEqual(
            missing,
            [],
            "ja.toml uses glyphs the bundled Noto Sans JP subset lacks "
            "(rebuild it with assets/fonts/make-jp-subset.py):\n" + "\n".join(missing),
        )

    def test_every_catalogue_key_is_used_by_the_client(self):
        # Keys reach the lookup both directly (`locale::text("results_back")`) and through a
        # table (`BUTTONS`, `MONTH_KEYS`), so look for the key as a literal anywhere in the
        # client's source.
        literals = set()
        for path in rust_sources():
            text = path.read_text(encoding="utf-8")
            literals.update(_STRING_RE.findall(text))
        missing = sorted(set(catalogue("en")) - literals)
        self.assertEqual(missing, [], f"catalogue keys no client code reads: {missing}")

    def test_every_locale_call_names_a_real_key(self):
        keys = set(catalogue("en"))
        bad = []
        for path in rust_sources():
            for key in locale_keys_in(path.read_text(encoding="utf-8")):
                if key not in keys:
                    bad.append(f"{path.relative_to(ROOT)}: {key}")
        self.assertEqual(bad, [], f"locale calls naming unknown keys: {bad}")


class HardCodedStringTest(unittest.TestCase):
    def test_localised_modules_pass_text_through_the_catalogue(self):
        violations = []
        for path in source_files():
            found = hard_coded_literals(
                path.read_text(encoding="utf-8"), str(path.relative_to(ROOT))
            )
            violations.extend(f"{where}: {literal!r}" for where, literal in found)
        self.assertEqual(
            violations,
            [],
            "player-visible strings must be keys in crates/client/locales/*.toml "
            "(locale::text / locale::format), not literals:\n" + "\n".join(violations),
        )


if __name__ == "__main__":
    unittest.main()
