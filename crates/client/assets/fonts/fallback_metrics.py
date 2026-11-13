"""Gives the fallback fonts Barlow's vertical metrics (ascent 1.0 em, descent 0.2 em, no line gap),
so a fallback glyph does not move the baseline of the line it sits in (#546).

The text layout (parley) puts a line's baseline at its tallest ascent plus half the leading left
by its tallest ascent and descent. Noto Sans Symbols 2 has a 0.63 em descent, so one `▾` or `★`
in a Barlow label lifted the whole label about 0.18 em (2.5 px at 14 px). A browser keeps the
primary font's baseline; with the fallbacks inside Barlow's box, ours does too. Outlines and
advances are unchanged; only the hhea and OS/2 typo metrics are set (and USE_TYPO_METRICS, so
every reader takes the same pair). Needs fonttools. Idempotent.

  python -I fallback_metrics.py NotoSansSymbols2-subset.ttf InterArrows-*.ttf NotoSansJP-subset.ttf

The subset scripts next to this file call `apply` on what they write.
"""
import sys
from fontTools.ttLib import TTFont

ASCENT = 1.0
DESCENT = 0.2


def apply(font):
    upm = font['head'].unitsPerEm
    ascent = round(ASCENT * upm)
    descent = -round(DESCENT * upm)
    hhea = font['hhea']
    hhea.ascent, hhea.descent, hhea.lineGap = ascent, descent, 0
    os2 = font['OS/2']
    os2.sTypoAscender, os2.sTypoDescender, os2.sTypoLineGap = ascent, descent, 0
    os2.fsSelection |= 1 << 7  # USE_TYPO_METRICS
    if os2.version < 4:
        os2.version = 4


if __name__ == '__main__':
    for path in sys.argv[1:]:
        f = TTFont(path)
        apply(f)
        f.save(path)
        print(path, f['hhea'].ascent, f['hhea'].descent, 'of', f['head'].unitsPerEm)
