"""Rebuilds InterArrows-<Weight>.ttf: the basic arrows (U+2190..U+2199, ← ↑ → ↓ ↔ ↕ ↖ ↗ ↘ ↙),
which Barlow, Noto Sans Symbols 2 and Noto Sans lack, in the four weights the UI sets (400, 500,
600, 700), so an arrow in bold text is as heavy as the digits around it (#720). Static instances
of Inter (github.com/google/fonts, ofl/inter, Inter[opsz,wght].ttf; SIL OFL 1.1, no reserved
font name) at its default optical size. Needs fonttools.

  python -I make-arrows-subset.py <path to Inter[opsz,wght].ttf>
"""
import os
import sys
from fontTools.ttLib import TTFont
from fontTools import subset
from fontTools.varLib import instancer

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fallback_metrics  # noqa: E402

WEIGHTS = {400: 'Regular', 500: 'Medium', 600: 'SemiBold', 700: 'Bold'}

for weight, style in WEIGHTS.items():
    f = TTFont(sys.argv[1])
    # opsz None: pinned at its default.
    f = instancer.instantiateVariableFont(f, {'wght': weight, 'opsz': None}, updateFontNames=True)
    opts = subset.Options()
    opts.layout_features = []
    opts.name_IDs = ['*']
    opts.notdef_outline = True
    s = subset.Subsetter(opts)
    s.populate(unicodes=range(0x2190, 0x219A))
    s.subset(f)
    fallback_metrics.apply(f)  # Barlow's ascent and descent (#546)
    out = f'InterArrows-{style}.ttf'
    f.save(out)
    names = f['name']
    print(out, len(f.getBestCmap()), 'characters,', names.getDebugName(16) or names.getDebugName(1),
          names.getDebugName(17) or names.getDebugName(2), f['OS/2'].usWeightClass)
